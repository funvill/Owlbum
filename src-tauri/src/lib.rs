//! Owlbum's desktop shell, the crate `owlbum`.
//!
//! It is thin: each command validates its arguments, calls one core function
//! and maps the result or error to a reply. It also owns what only a desktop
//! shell can do: windows, menus, dialogs, drag-out, trash, clipboard, reveal,
//! the updater and single-instance handling.

pub mod bindings;
pub mod closing;
pub mod commands;
pub mod desktop;
pub mod events;
pub mod fatal;
pub mod handoff;
pub mod keymap;
pub mod menu;
pub mod navigation;
pub mod protocol;
pub mod session;
pub mod update;
pub mod window;

use std::sync::Arc;

use owlbum_core::app::{DropPhase, FolderDrop};
use owlbum_core::appdata::log::{self, LogFile, Logger};
use owlbum_core::appdata::{self, AppData};
use owlbum_core::thumbs::Thumbs;
use tauri::{AppHandle, DragDropEvent, Manager, Runtime, WindowEvent};

/// Adds Owlbum's commands and the `owlbum://` protocol to a Tauri builder.
/// `run` uses it, and shell tests use it with Tauri's mock runtime.
pub fn setup<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    protocol::register(builder)
        .manage(session::Session::default())
        .manage(handoff::Handoff::default())
        .manage(commands::file::DragPrep::default())
        .manage(closing::Closing::default())
        .manage(update::Updates::default())
        .manage(window::Geometry::default())
        .manage(commands::tagging::TaggingState::default())
        .on_menu_event(|app, event| menu::chosen(app, event.id().as_ref()))
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(true) => window.state::<session::Session>().focused(),
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => self::window::changed(window),
            WindowEvent::CloseRequested { api, .. } => {
                if closing::requested(window.app_handle()) {
                    api.prevent_close();
                }
            }
            WindowEvent::DragDrop(drop) => dropped(window.app_handle(), drop),
            _ => {}
        })
        .invoke_handler(commands::registry::invoke_handler(commands::modules()))
}

/// Files dragged over the window (docs/04-browsing.md, "Opening a
/// folder"): the page shows its drop outline while a drag is over it, and
/// a drop opens the first folder among the dropped items, or the folder of
/// the first file. The webview never navigates to a dropped file.
pub fn dropped<R: Runtime>(app: &AppHandle<R>, drop: &DragDropEvent) {
    let phase = |phase| {
        let _ = events::emit(app, &FolderDrop { phase });
    };
    match drop {
        DragDropEvent::Enter { .. } => phase(DropPhase::Over),
        DragDropEvent::Leave => phase(DropPhase::Left),
        DragDropEvent::Drop { paths, .. } => {
            phase(DropPhase::Left);
            match owlbum_core::app::dropped_folder(paths) {
                Some(folder) => handoff::deliver(app, &folder),
                None => phase(DropPhase::Refused),
            }
        }
        _ => {}
    }
}

/// Gives the app its open app data folder, which the `app_` settings and
/// recent-list commands use, and the thumbnail cache inside it. `run`
/// passes the real folder; shell tests pass one in a temp folder, so no
/// test touches a real install's data.
pub fn with_app_data<R: Runtime>(builder: tauri::Builder<R>, data: AppData) -> tauri::Builder<R> {
    let builder = match Thumbs::open(data.clone()) {
        Ok(thumbs) => builder.manage(thumbs),
        Err(e) => {
            // Pictures answer 404 and the cache commands fail; the rest of
            // the program works.
            tracing::error!(target: "thumbs", "the thumbnail cache could not open: {}", e.message);
            builder
        }
    };
    builder.manage(data)
}

/// The identifier of a development build (`src-tauri/tauri.dev.conf.json`).
pub const DEV_IDENTIFIER: &str = "com.funvill.owlbum.dev";

/// Whether this is a development build: one compiled with debug assertions
/// (docs/08-desktop-and-release.md), or one carrying the development
/// identifier, such as a release-profile build started with the dev config.
/// A development build uses the `Dev` app data folder, so it never touches
/// an installed copy's data.
pub fn is_dev_build(debug_assertions: bool, identifier: &str) -> bool {
    debug_assertions || identifier == DEV_IDENTIFIER || identifier.ends_with(".dev")
}

/// Starts the program. A failure while starting is shown in the native
/// fatal error box (docs/08-desktop-and-release.md, "Failure handling"),
/// which ends the program.
pub fn run() {
    use owlbum_core::app::fatal::Fatal;
    let context = tauri::generate_context!();
    let dev = is_dev_build(cfg!(debug_assertions), &context.config().identifier);
    let package = owlbum_core::app::package_kind(dev, std::env::var_os("APPIMAGE").is_some());
    fatal::install_panic_hook();

    // Startup sequence, docs/08-desktop-and-release.md, steps 1, 2, 4, 5.
    let location = match appdata::locate(appdata::Platform::CURRENT, dev, |k| std::env::var_os(k)) {
        Ok(location) => location,
        Err(e) => {
            fatal::set_facts(fatal::Facts {
                package: package.to_owned(),
                ..fatal::Facts::default()
            });
            fatal::show_native(Fatal::Start, &e.message);
            return;
        }
    };
    let file = Arc::new(LogFile::open(
        &log::folders(&location.dir, &std::env::temp_dir()),
        dev,
    ));
    Logger::new(
        Arc::clone(&file),
        log::debug_requested(std::env::var_os(log::LEVEL_VAR).as_deref()),
    )
    .install();
    fatal::set_facts(fatal::Facts {
        package: package.to_owned(),
        data_dir: location.dir.display().to_string(),
        log_file: file.path().map(std::path::Path::to_path_buf),
    });
    let data = match AppData::open(&location.dir) {
        Ok(data) => data,
        Err(e) => {
            tracing::error!(target: "app", "the app data folder could not be opened: {}", e.message);
            fatal::show_native(
                Fatal::StartDataFolder,
                &format!("{}: {:?}", e.message, e.details),
            );
            return;
        }
    };
    tracing::info!(
        target: "app",
        "start version={} platform={} arch={} dev={} identifier={} data={} data_overridden={}",
        owlbum_core::app::version().version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        dev,
        context.config().identifier,
        data.dir().display(),
        location.overridden,
    );
    if let Some(moved) = data.set_aside() {
        tracing::warn!(
            target: "app",
            "app.sqlite was damaged; set aside as {} and remade",
            moved.display()
        );
    }

    let launch = std::env::current_dir()
        .ok()
        .and_then(|cwd| owlbum_core::app::launch_folder(std::env::args_os(), &cwd));

    let auto = data.settings().map(|s| s.updates_auto).unwrap_or(true);
    let pubkey = update::public_key(context.config());
    let saved_window = data.settings().map(|s| s.window).unwrap_or_default();

    let mut builder = tauri::Builder::default();
    // Step 3: one running copy per app data folder. Tests that set
    // OWLBUM_DATA_DIR run several copies at once, each with its own folder.
    if !location.overridden {
        builder = builder.plugin(tauri_plugin_single_instance::init(second_launch));
    }
    let started = with_app_data(setup(builder), data)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            use_bundled_ort(app.handle());
            // Step 6: the menu.
            let menu = menu::build(app.handle(), dev, auto)?;
            app.set_menu(menu)?;
            // Step 8: the window, hidden, with the restored geometry.
            let window = main_window(app, dev)?;
            self::window::place(&window, &saved_window);
            app.manage(commands::app::FolderPicker(folder_dialog(
                app.handle().clone(),
            )));
            app.manage(commands::app::Browser(Arc::new(|url: &str| {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
            })));
            app.manage(commands::file::DesktopShell(Arc::new(
                desktop::NativeDesktop::new(app.handle().clone()),
            )));
            if let Some(folder) = &launch {
                handoff::deliver(app.handle(), folder);
            }
            configure_updates(app.handle(), dev, pubkey.as_deref());
            let page = app.handle().clone();
            fatal::set_page_report(Box::new(move |e| {
                let _ = events::emit(&page, e);
            }));
            fatal::watch_page_start();
            // Step 10: shown when `app_startup` arrives, or after 3 s.
            let handle = app.handle().clone();
            let _ = std::thread::Builder::new()
                .name("owlbum-show".into())
                .spawn(move || {
                    std::thread::sleep(SHOW_WAIT);
                    show_window(&handle);
                });
            Ok(())
        })
        .run(context);
    if let Err(e) = started {
        let error = e.to_string();
        tracing::error!(target: "app", "Owlbum could not start: {error}");
        fatal::show_native(fatal::start_phase(&error), &error);
    }
}

/// Sets the updater up (docs/08-desktop-and-release.md, "The signing
/// key"): a development build, or one with no public key, stays
/// `unavailable` and never checks. `OWLBUM_UPDATE_FEED` replaces the feed
/// and is logged.
fn configure_updates<R: Runtime>(app: &AppHandle<R>, dev: bool, pubkey: Option<&str>) {
    use owlbum_core::app::update as rules;
    let updates = app.state::<update::Updates>();
    let over = std::env::var(rules::FEED_VAR).ok();
    if let Some(url) = over.as_deref().filter(|u| !u.trim().is_empty()) {
        tracing::info!(target: "update", "the feed is replaced: {url}");
    }
    if pubkey.is_none() {
        tracing::info!(target: "update", "no public key: the updater is not set up");
        updates.configure(None, dev);
        return;
    }
    // A development build may use a test feed (docs/08, "Test feed"),
    // otherwise it never checks.
    if dev && over.is_none() {
        updates.configure(None, dev);
        return;
    }
    let feed = rules::feed(over.as_deref(), dev)
        .and_then(|f| f.parse::<tauri::Url>().map_err(|e| e.to_string()));
    match feed {
        Ok(feed) => {
            let source = update::PluginSource::new(app.clone(), feed);
            updates.configure(Some(Arc::new(source)), false);
        }
        Err(e) => {
            tracing::warn!(target: "update", "{e}");
            updates.configure(None, dev);
        }
    }
}

/// Points tagging at the ONNX Runtime library the installer put among the
/// program's resources (docs/07-tagging.md, "Packaging"). A build without
/// it (`cargo build`, `tauri dev`) keeps the other places: the file named
/// by `OWLBUM_ORT_DYLIB`, then the file next to the program.
fn use_bundled_ort<R: Runtime>(app: &AppHandle<R>) {
    let found = app
        .path()
        .resource_dir()
        .ok()
        .and_then(|dir| owlbum_core::tagging::onnx::library_in(&dir));
    match found {
        Some(path) => {
            tracing::info!(target: "tagging", "onnx runtime library={}", path.display());
            owlbum_core::tagging::onnx::set_library(path);
        }
        None => tracing::info!(target: "tagging", "onnx runtime is not among the resources"),
    }
}

/// How long the hidden window waits for `app_startup` before it is shown
/// anyway.
pub const SHOW_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Shows the main window and gives it focus. Called when `app_startup`
/// arrives, after [`SHOW_WAIT`], and on a second launch, which also
/// restores it from minimised.
pub fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
        if !window.is_visible().unwrap_or(true) {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// A second copy was started (docs/08-desktop-and-release.md, "Single
/// instance and second launches"): it has handed over its arguments and
/// exits. This copy comes to the front and opens the folder it named.
fn second_launch<R: Runtime>(app: &AppHandle<R>, args: Vec<String>, cwd: String) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(folder) = owlbum_core::app::launch_folder(&args, std::path::Path::new(&cwd)) {
        handoff::deliver(app, &folder);
    }
}

/// Builds the main window from its configuration (which has `create:
/// false`), with Owlbum's user agent, so map tile servers can tell which
/// program is asking (docs/08-desktop-and-release.md, "Map-tile consent"),
/// and with navigation away from the app's page and new windows refused.
fn main_window<R: Runtime>(
    app: &tauri::App<R>,
    dev: bool,
) -> tauri::Result<tauri::WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .unwrap_or_default();
    let version = owlbum_core::app::version().version;
    // Under `tauri dev` the page comes from the Vite server; otherwise only
    // the app's own page may be shown (docs/08, "Navigation and new windows").
    let dev_url = tauri::is_dev()
        .then(|| app.config().build.dev_url.clone())
        .flatten();
    let window = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .title(window_title(&version, None))
        .user_agent(&user_agent(std::env::consts::OS, &version))
        .on_navigation(move |url| navigation::check(url, dev_url.as_ref()))
        .on_new_window(|url, _| {
            navigation::refuse_new_window(&url);
            tauri::webview::NewWindowResponse::Deny
        })
        .visible(false)
        .devtools(dev)
        .build()?;
    Ok(window)
}

/// The main window's title: `Owlbum <version>`, so a tester can tell which
/// build is running (CHANGELOG.md says what each version changed), and
/// while an album is open ` - <folder>`: the folder the user opened, the
/// album's root or the scope folder inside it (docs/08, "The window").
pub fn window_title(version: &str, album: Option<&std::path::Path>) -> String {
    match album {
        Some(folder) => format!("Owlbum {version} - {}", folder.display()),
        None => format!("Owlbum {version}"),
    }
}

/// Retitles the main window for the album now open (`None`: none is).
pub fn show_album_in_title<R: Runtime>(app: &AppHandle<R>, album: Option<&std::path::Path>) {
    if let Some(window) = app.get_webview_window("main") {
        let title = window_title(&owlbum_core::app::version().version, album);
        if let Err(e) = window.set_title(&title) {
            tracing::warn!(target: "app", "the window title could not be set: {e}");
        }
    }
}

/// The webview's user agent: `Owlbum/<version>`, the platform and the
/// project's address. OpenStreetMap's tile policy asks for a user agent
/// that names the app and never poses as a browser, so the webview's own
/// is replaced. It keeps the `Windows` token, which the page reads to
/// address its custom scheme.
pub fn user_agent(os: &str, version: &str) -> String {
    let platform = match os {
        "windows" => "Windows NT 10.0; Win64; x64",
        "macos" => "Macintosh; Mac OS X",
        _ => "X11; Linux x86_64",
    };
    format!("Owlbum/{version} ({platform}; +https://github.com/funvill/Owlbum)")
}

/// The native folder dialog, over the main window.
fn folder_dialog<R: Runtime>(
    app: AppHandle<R>,
) -> Arc<dyn Fn() -> Option<std::path::PathBuf> + Send + Sync> {
    use tauri_plugin_dialog::DialogExt;
    Arc::new(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title(owlbum_core::text::album::PICK_FOLDER_TITLE);
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        dialog.blocking_pick_folder()?.into_path().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_title_carries_the_version() {
        assert_eq!(window_title("0.1.1", None), "Owlbum 0.1.1");
    }

    #[test]
    fn the_window_title_carries_the_open_folder() {
        let folder = std::path::Path::new("/example/folder/album");
        assert_eq!(
            window_title("0.1.3", Some(folder)),
            "Owlbum 0.1.3 - /example/folder/album"
        );
    }

    #[test]
    fn the_user_agent_names_owlbum_and_keeps_the_platform() {
        let windows = user_agent("windows", "1.2.3");
        assert!(windows.starts_with("Owlbum/1.2.3 ("));
        assert!(windows.contains("Windows"));
        assert!(!windows.contains("Mozilla"));
        assert!(user_agent("linux", "1.2.3").contains("Linux"));
    }

    #[test]
    fn a_debug_build_is_a_development_build_whatever_its_identifier() {
        assert!(is_dev_build(true, "com.funvill.owlbum"));
        assert!(is_dev_build(true, DEV_IDENTIFIER));
    }

    #[test]
    fn a_release_build_with_the_dev_identifier_is_a_development_build() {
        assert!(is_dev_build(false, DEV_IDENTIFIER));
        assert!(is_dev_build(false, "com.example.other.dev"));
    }

    #[test]
    fn only_an_installed_release_build_uses_the_real_folder() {
        assert!(!is_dev_build(false, "com.funvill.owlbum"));
        assert!(!is_dev_build(false, "com.funvill.owlbum.devtools"));
    }

    #[test]
    fn the_dev_identifier_matches_the_dev_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.dev.conf.json")).unwrap();
        assert_eq!(conf["identifier"], DEV_IDENTIFIER);
        let main: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!is_dev_build(false, main["identifier"].as_str().unwrap()));
    }
}
