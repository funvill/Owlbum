//! Owlbum's desktop shell, the crate `owlbum`.
//!
//! It is thin: each command validates its arguments, calls one core function
//! and maps the result or error to a reply. It also owns what only a desktop
//! shell can do: windows, menus, dialogs, drag-out, trash, clipboard, reveal,
//! the updater and single-instance handling.

pub mod bindings;
pub mod commands;
pub mod events;
pub mod handoff;
pub mod keymap;
pub mod protocol;
pub mod session;

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
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(true) => window.state::<session::Session>().focused(),
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

/// Starts the program.
///
/// # Panics
///
/// When the app data folder cannot be found, created or opened, or Tauri
/// cannot start. Failure handling replaces these with the fatal error
/// dialog (roadmap issue 61).
pub fn run() {
    let context = tauri::generate_context!();
    let dev = is_dev_build(cfg!(debug_assertions), &context.config().identifier);

    // Startup sequence, docs/08-desktop-and-release.md, steps 1, 2, 4, 5.
    let location = appdata::locate(appdata::Platform::CURRENT, dev, |k| std::env::var_os(k))
        .expect("Owlbum could not find a folder for its settings");
    let file = Arc::new(LogFile::open(
        &log::folders(&location.dir, &std::env::temp_dir()),
        dev,
    ));
    Logger::new(
        Arc::clone(&file),
        log::debug_requested(std::env::var_os(log::LEVEL_VAR).as_deref()),
    )
    .install();
    let data = AppData::open(&location.dir).expect("Owlbum could not open its settings");
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

    with_app_data(setup(tauri::Builder::default()), data)
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            main_window(app)?;
            app.manage(commands::app::FolderPicker(folder_dialog(
                app.handle().clone(),
            )));
            if let Some(folder) = &launch {
                handoff::deliver(app.handle(), folder);
            }
            Ok(())
        })
        .run(context)
        .expect("Owlbum could not start");
}

/// Builds the main window from its configuration (which has `create:
/// false`), with Owlbum's user agent, so map tile servers can tell which
/// program is asking (docs/08-desktop-and-release.md, "Map-tile consent").
fn main_window<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .unwrap_or_default();
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .user_agent(&user_agent(
            std::env::consts::OS,
            &owlbum_core::app::version().version,
        ))
        .build()?;
    Ok(())
}

/// The webview's user agent: a browser-like one for the platform, with
/// `Owlbum/<version>` added. Tauri can only replace the user agent, not add
/// to it, so the platform part is written here. It keeps the `Windows`
/// token, which the page reads to address its custom scheme.
pub fn user_agent(os: &str, version: &str) -> String {
    let platform = match os {
        "windows" => "Windows NT 10.0; Win64; x64",
        "macos" => "Macintosh; Intel Mac OS X 10_15_7",
        _ => "X11; Linux x86_64",
    };
    format!("Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Owlbum/{version}")
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
    fn the_user_agent_names_owlbum_and_keeps_the_platform() {
        let windows = user_agent("windows", "1.2.3");
        assert!(windows.ends_with(" Owlbum/1.2.3"));
        assert!(windows.contains("Windows"));
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
