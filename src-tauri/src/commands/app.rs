//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use std::path::PathBuf;
use std::sync::Arc;

use crate::handoff::Handoff;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::reopen;
use owlbum_core::app::assoc::{self, Association};
use owlbum_core::app::fatal::AppFatal;
use owlbum_core::app::update::{self as update_rules, UpdateMessage, UpdateState};
use owlbum_core::app::whats_new::{self, WhatsNewEntry};
use owlbum_core::app::{self, About, AppClosing, AppVersion, FolderDrop, OpenFolder, Startup};
use owlbum_core::appdata::{AppData, Recent, Settings};
use owlbum_core::files::Capabilities;
use owlbum_core::info::country;
use owlbum_core::media::{self, FormatInfo};
use owlbum_core::thumbs::{CacheCleared, CacheUsage, Thumbs};
use owlbum_core::{AppError, text};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "app",
    commands: [
        app_about,
        app_associate,
        app_associate_status,
        app_cache_clear,
        app_cache_usage,
        app_capabilities,
        app_check_updates,
        app_close_ready,
        app_copy_details,
        app_country_at,
        app_developer_tools,
        app_formats,
        app_full_screen,
        app_log,
        app_open_feedback,
        app_open_data_folder,
        app_open_licence,
        app_open_log_folder,
        app_open_notices,
        app_open_release_notes,
        app_open_release_page,
        app_open_repository,
        app_pick_folder,
        app_quit,
        app_recent_list,
        app_recent_remove,
        app_restart,
        app_session_save,
        app_settings_get,
        app_settings_set,
        app_startup,
        app_update_choose,
        app_update_download,
        app_update_install,
        app_update_notice_answer,
        app_update_state,
        app_version,
        app_whats_new,
        app_whats_new_seen,
    ],
    events: [AppClosing, AppFatal, FolderDrop, OpenFolder, UpdateMessage, UpdateState],
}

command! {
    /// The About box's facts (docs/08-desktop-and-release.md, "About box").
    pub fn app_about<R: Runtime>(; app: AppHandle<R>, data: State<'_, AppData>) -> About {
        let dev = crate::is_dev_build(cfg!(debug_assertions), &app.config().identifier);
        let package = app::package_kind(dev, std::env::var_os("APPIMAGE").is_some());
        let has_notices = resource(&app, NOTICES_FILE).is_some_and(|p| p.is_file());
        Ok(app::about(data.dir(), package, has_notices))
    }
}

/// The licence, among the program's resources.
pub const LICENCE_FILE: &str = "LICENSE.txt";
/// The generated third-party notices, among the program's resources.
pub const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.txt";

/// A file among the program's resources.
fn resource<R: Runtime>(app: &AppHandle<R>, name: &str) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join(name))
}

/// The error of a file or folder that could not be opened.
fn cannot_open(e: &dyn std::fmt::Display) -> AppError {
    AppError::internal(text::appdata::OPENING_FILE)
        .with_details(serde_json::json!({ "cause": e.to_string() }))
}

/// Opens a text file in the user's own program; a file that cannot be
/// opened is shown in its folder instead, and the failure is logged
/// (docs/08-desktop-and-release.md, "About box").
fn open_text(path: &std::path::Path) -> Result<(), AppError> {
    if let Err(e) = tauri_plugin_opener::open_path(path, None::<&str>) {
        tracing::warn!(target: "app", "{} could not be opened: {e}", path.display());
        let folder = path.parent().unwrap_or(path);
        tauri_plugin_opener::open_path(folder, None::<&str>).map_err(|e| cannot_open(&e))?;
    }
    Ok(())
}

command! {
    /// The About box's Licence: `LICENSE.txt` from the program's resources.
    /// When the resource is missing (a development build), the licence
    /// compiled into the program is written to the app data folder and
    /// opened from there.
    pub fn app_open_licence<R: Runtime>(; app: AppHandle<R>, data: State<'_, AppData>) -> () {
        let path = match resource(&app, LICENCE_FILE).filter(|p| p.is_file()) {
            Some(p) => p,
            None => {
                let p = data.dir().join(LICENCE_FILE);
                std::fs::write(&p, app::LICENCE).map_err(|e| cannot_open(&e))?;
                p
            }
        };
        open_text(&path)
    }
}

command! {
    /// The About box's Third-party notices: `THIRD-PARTY-NOTICES.txt` from
    /// the program's resources. The button shows only when it exists.
    pub fn app_open_notices<R: Runtime>(; app: AppHandle<R>) -> () {
        let path = resource(&app, NOTICES_FILE)
            .ok_or_else(|| AppError::internal(text::appdata::OPENING_FILE))?;
        open_text(&path)
    }
}

command! {
    /// Help > Check for updates…, the About box or Settings: the manual
    /// check. Its result arrives as `update_state` and `update_message`
    /// (docs/08-desktop-and-release.md, "Updates").
    pub fn app_check_updates<R: Runtime>(; app: AppHandle<R>) -> () {
        crate::update::check(&app, true);
        Ok(())
    }
}

command! {
    /// The updater's state now, for the page's first look.
    pub fn app_update_state<R: Runtime>(; app: AppHandle<R>) -> UpdateState {
        Ok(app
            .try_state::<crate::update::Updates>()
            .map_or(
                UpdateState {
                    state: update_rules::UpdateStateKind::Unavailable,
                    version: None,
                    percent: None,
                },
                |u| u.current(),
            ))
    }
}

command! {
    /// Download: starts the download of the offered version.
    pub fn app_update_download<R: Runtime>(; app: AppHandle<R>) -> () {
        crate::update::download(&app);
        Ok(())
    }
}

command! {
    /// Restart now: the install steps, through the closing steps.
    pub fn app_update_install<R: Runtime>(; app: AppHandle<R>) -> () {
        crate::update::install(&app);
        Ok(())
    }
}

command! {
    /// The answer to the first-start notice (docs/08, "What the first start
    /// says"): OK keeps the daily check on, Turn off turns it off. Either
    /// way the notice is never shown again, and after OK the quiet check
    /// may run in this session.
    pub async fn app_update_notice_answer<R: Runtime>(
        keep_on: bool;
        app: AppHandle<R>,
        data: State<'_, AppData>
    ) -> Settings {
        let data = data.inner().clone();
        let settings = blocking(move || {
            let patch = serde_json::json!({ "updates.notice_seen": true, "updates.auto": keep_on });
            data.set_settings(patch.as_object().expect("an object"))
        })
        .await?;
        crate::update::start_quiet(&app);
        Ok(settings)
    }
}

command! {
    /// The newest release's page, from the update messages.
    pub fn app_open_release_page<R: Runtime>(; app: AppHandle<R>) -> () {
        open_fixed(&app, update_rules::RELEASE_PAGE_URL)
    }
}

command! {
    /// Release notes of the offered version. The page passes no address
    /// and no version: Rust holds the offered one.
    pub fn app_open_release_notes<R: Runtime>(; app: AppHandle<R>) -> () {
        let version = app
            .try_state::<crate::update::Updates>()
            .and_then(|u| u.current().version)
            .ok_or_else(|| AppError::internal(text::appdata::OPENING_ADDRESS))?;
        open_address(&app, &update_rules::release_notes_url(&version))
    }
}

command! {
    /// Help > What's new: the running version's notes, or none
    /// (docs/08-desktop-and-release.md, "What's new"). Never changes
    /// `whats_new.last_seen`.
    pub fn app_whats_new() -> Vec<WhatsNewEntry> {
        let entries = whats_new::parse(whats_new::CHANGELOG);
        Ok(whats_new::for_version(&app::version().version, &entries))
    }
}

command! {
    /// The What's new dialog shown at start was closed: records the
    /// running version as `whats_new.last_seen`.
    pub async fn app_whats_new_seen(; data: State<'_, AppData>) -> () {
        let data = data.inner().clone();
        blocking(move || {
            let patch = serde_json::json!({ "whats_new.last_seen": app::version().version });
            data.set_settings(patch.as_object().expect("an object")).map(|_| ())
        })
        .await
    }
}

command! {
    /// The page reports an error or warning for the log (area `page`). At
    /// most 2,000 characters a line and 100 lines a minute; more are
    /// dropped. `level` is `error`, `warn` or anything else for `info`.
    pub fn app_log(level: String, message: String) -> () {
        if PAGE_LOG.allow(std::time::Instant::now()) {
            let message: String = message.chars().take(PAGE_LOG_MAX_CHARS).collect();
            match level.as_str() {
                "error" => tracing::error!(target: "page", "{message}"),
                "warn" => tracing::warn!(target: "page", "{message}"),
                _ => tracing::info!(target: "page", "{message}"),
            }
        }
        Ok(())
    }
}

/// The longest line the page may log.
pub const PAGE_LOG_MAX_CHARS: usize = 2_000;
/// The most lines the page may log in a minute.
pub const PAGE_LOG_PER_MINUTE: u32 = 100;

static PAGE_LOG: PageLog = PageLog::new();

/// Counts the page's log lines per minute.
pub struct PageLog(std::sync::Mutex<Option<(std::time::Instant, u32)>>);

impl PageLog {
    pub const fn new() -> Self {
        Self(std::sync::Mutex::new(None))
    }

    /// Whether one more line may be written at `now`.
    pub fn allow(&self, now: std::time::Instant) -> bool {
        let mut w = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &mut *w {
            Some((start, n)) if now.duration_since(*start) < std::time::Duration::from_secs(60) => {
                *n += 1;
                *n <= PAGE_LOG_PER_MINUTE
            }
            _ => {
                *w = Some((now, 1));
                true
            }
        }
    }
}

impl Default for PageLog {
    fn default() -> Self {
        Self::new()
    }
}

command! {
    /// The fatal dialog's Copy details: the technical block on the
    /// clipboard. The page is never sent the block itself.
    pub fn app_copy_details() -> () {
        crate::fatal::copy_details();
        Ok(())
    }
}

command! {
    /// The fatal dialog's Open the log folder.
    pub fn app_open_log_folder() -> () {
        let folder = crate::fatal::log_folder()
            .ok_or_else(|| AppError::internal(text::appdata::OPENING_FILE))?;
        tauri_plugin_opener::open_path(&folder, None::<&str>).map_err(|e| cannot_open(&e))
    }
}

command! {
    /// The fatal dialog's Restart: starts Owlbum again and ends this copy.
    /// The closing steps are not run: the part that failed cannot be
    /// trusted to close.
    pub fn app_restart<R: Runtime>(; app: AppHandle<R>) -> () {
        crate::fatal::quitting();
        app.restart()
    }
}

/// Opens an address in the user's own browser. `run` installs the real
/// one; shell tests install a fake that records what it was given.
#[derive(Clone)]
pub struct Browser(pub Arc<OpenAddress>);

/// Opens one address; an error says why it could not.
pub type OpenAddress = dyn Fn(&str) -> Result<(), String> + Send + Sync;

/// Opens one of the program's own fixed addresses: the page never passes
/// an address (docs/08-desktop-and-release.md, "Navigation and new
/// windows").
pub fn open_fixed<R: Runtime>(app: &AppHandle<R>, url: &'static str) -> Result<(), AppError> {
    open_address(app, url)
}

/// Opens an address the program made itself: a fixed address, or the
/// release notes of the version Rust holds.
fn open_address<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<(), AppError> {
    let browser = app
        .try_state::<Browser>()
        .ok_or_else(|| AppError::internal(text::appdata::OPENING_ADDRESS))?;
    (browser.0)(url).map_err(|e| {
        tracing::warn!(target: "app", "{url} could not be opened: {e}");
        AppError::internal(text::appdata::OPENING_ADDRESS)
            .with_details(serde_json::json!({ "cause": e }))
    })
}

command! {
    /// Help > Give feedback (F2): the project's issues page in the user's
    /// browser (docs/08-desktop-and-release.md, "Feedback").
    pub fn app_open_feedback<R: Runtime>(; app: AppHandle<R>) -> () {
        open_fixed(&app, app::FEEDBACK_URL)
    }
}

command! {
    /// More > Open the data folder: the program's own folder in the file
    /// manager (docs/08-desktop-and-release.md, "The More menu").
    pub fn app_open_data_folder(; data: State<'_, AppData>) -> () {
        tauri_plugin_opener::open_path(data.dir(), None::<&str>).map_err(|e| {
            tracing::warn!(target: "app", "the data folder could not be opened: {e}");
            cannot_open(&e)
        })
    }
}

command! {
    /// More > Full screen (F11): the window fills the screen, or leaves
    /// full screen.
    pub fn app_full_screen<R: Runtime>(; app: AppHandle<R>) -> () {
        if let Some(w) = app.get_webview_window("main") {
            let full = w.is_fullscreen().unwrap_or(false);
            w.set_fullscreen(!full)
                .map_err(|e| AppError::internal(text::appdata::FULL_SCREEN).with_details(serde_json::json!({ "cause": e.to_string() })))?;
        }
        Ok(())
    }
}

command! {
    /// More > Developer tools (Ctrl+Shift+I), development builds only:
    /// opens or closes the webview's developer tools. A release build does
    /// nothing.
    pub fn app_developer_tools<R: Runtime>(; app: AppHandle<R>) -> () {
        #[cfg(debug_assertions)]
        if let Some(w) = app.get_webview_window("main") {
            if w.is_devtools_open() {
                w.close_devtools();
            } else {
                w.open_devtools();
            }
        }
        #[cfg(not(debug_assertions))]
        let _ = app;
        Ok(())
    }
}

command! {
    /// More > the update item: check, download or install, by the state
    /// (docs/08-desktop-and-release.md, "States").
    pub fn app_update_choose<R: Runtime>(; app: AppHandle<R>) -> () {
        crate::update::menu_chosen(&app);
        Ok(())
    }
}

command! {
    /// The About box's link: the project's page on GitHub.
    pub fn app_open_repository<R: Runtime>(; app: AppHandle<R>) -> () {
        open_fixed(&app, app::REPOSITORY_URL)
    }
}

command! {
    /// The country that holds a point, from the bundled outlines, or null
    /// (at sea): the place name a pick on the map gives (docs/04-browsing.md,
    /// "The place picker").
    pub async fn app_country_at(lat: f64, lon: f64) -> Option<String> {
        Ok(country::country_at(lat, lon).map(str::to_owned))
    }
}

command! {
    /// Clear cache: deletes `thumbs/`, `full/` and their `cache_entries`
    /// rows, and answers how many bytes they held (docs/02-data.md,
    /// "Thumbnail keys and the cache"). Models, settings and albums are not
    /// touched. Pictures are made again as the page asks for them.
    pub async fn app_cache_clear(; thumbs: State<'_, Thumbs>) -> CacheCleared {
        let thumbs = thumbs.inner().clone();
        blocking(move || Ok(thumbs.cache().clear())).await
    }
}

/// Whether Owlbum opens pictures, by the registry: off where it is not
/// offered.
fn association<R: Runtime>(app: &AppHandle<R>) -> Association {
    match app.try_state::<crate::assoc::FileTypes>() {
        Some(types) => Association {
            available: true,
            on: assoc::is_registered(types.registry.as_ref(), &types.exe),
        },
        None => Association {
            available: false,
            on: false,
        },
    }
}

command! {
    /// Whether Owlbum is the program that opens pictures, for Settings
    /// (docs/08-desktop-and-release.md, "Opening pictures"). Read from the
    /// registry each time, so a change made outside Owlbum shows.
    pub fn app_associate_status<R: Runtime>(; app: AppHandle<R>) -> Association {
        Ok(association(&app))
    }
}

command! {
    /// Makes Owlbum a program that opens pictures for this user, then opens
    /// Windows' Default apps page, where the user makes it the default
    /// (Windows does not let a program do that itself); or, with `on`
    /// false, removes the registration. Needs no administrator rights.
    /// `internal` when the registry refused a change.
    pub async fn app_associate<R: Runtime>(on: bool; app: AppHandle<R>) -> Association {
        let Some(types) = app.try_state::<crate::assoc::FileTypes>().map(|t| t.inner().clone()) else {
            return Ok(association(&app));
        };
        blocking(move || {
            let reg = types.registry.as_ref();
            let done = if on {
                assoc::register(reg, &types.exe)
            } else {
                assoc::unregister(reg, &types.exe)
            };
            if let Err(e) = done {
                tracing::warn!(target: "app", "associating pictures on={on}: {e}");
                return Err(AppError::internal(text::appdata::ASSOCIATING)
                    .with_details(serde_json::json!({ "cause": e })));
            }
            tracing::info!(target: "app", "associating pictures on={on} exe={}", types.exe.display());
            if on {
                // The page says what to do there; a page that did not open
                // leaves the registration in place.
                let _ = open_address(&app, assoc::DEFAULT_APPS);
            }
            Ok(association(&app))
        })
        .await
    }
}

command! {
    /// The bytes the thumbnail cache holds and its limit, for Settings.
    pub fn app_cache_usage(; thumbs: State<'_, Thumbs>) -> CacheUsage {
        Ok(thumbs.cache().usage())
    }
}

command! {
    /// What this platform offers for file actions: dragging out, and
    /// whether copying several photos copies files or paths (docs/06,
    /// "Dragging photos out"). The page asks this, not the user agent.
    pub fn app_capabilities() -> Capabilities {
        Ok(Capabilities::current())
    }
}

command! {
    /// The page's answer to `app_closing`: edit mode has ended as Done and
    /// the session is saved, so the close can go on
    /// (docs/08-desktop-and-release.md, "Closing and quitting").
    pub fn app_close_ready<R: Runtime>(; app: AppHandle<R>) -> () {
        if let Some(closing) = app.try_state::<crate::closing::Closing>() {
            closing.ready();
        }
        Ok(())
    }
}

command! {
    /// The media format registry, so the page never hard-codes a format rule
    /// (docs/03-media-and-scanning.md, "Commands").
    pub fn app_formats() -> Vec<FormatInfo> {
        Ok(media::formats())
    }
}

command! {
    /// Quits through the closing steps, as File > Quit does
    /// (docs/08-desktop-and-release.md, "Closing and quitting").
    pub fn app_quit<R: Runtime>(; app: AppHandle<R>) -> () {
        if !crate::closing::requested(&app) {
            app.exit(0);
        }
        Ok(())
    }
}

command! {
    /// The recent list, newest first. Each folder is checked to exist, all at
    /// once, each check given at most 1.5 seconds.
    pub async fn app_recent_list(; data: State<'_, AppData>) -> Vec<Recent> {
        let data = data.inner().clone();
        blocking(move || data.recent()).await
    }
}

command! {
    /// Removes one folder from the recent list and answers the list.
    pub async fn app_recent_remove(root: String; data: State<'_, AppData>) -> Vec<Recent> {
        let data = data.inner().clone();
        blocking(move || data.remove_recent(&root)).await
    }
}

command! {
    /// Saves where the person is in the open album: its view, sort, filter,
    /// search, current photo (`selected_id`) and scroll index, against the
    /// folder it was opened at (docs/04-browsing.md, "Saving"). The current
    /// photo is kept as its path, so the id is turned into the path the row
    /// has now. Fields of the stored session that this one lacks are kept,
    /// and the folder moves to the top of the recent list. Nothing happens
    /// when no album is open.
    pub async fn app_session_save(
        session: Value;
        data: State<'_, AppData>,
        open: State<'_, Session>
    ) -> () {
        let data = data.inner().clone();
        let Some(scanner) = open.scanner() else {
            return Ok(());
        };
        blocking(move || {
            let album = scanner.album();
            let opened = album.location().opened().display().to_string();
            let stored = album.db().read(|c| reopen::to_stored(c, &session))?;
            data.save_session(&opened, &stored, crate::commands::album::now_ms())
        })
        .await
    }
}

command! {
    /// Every setting, with defaults filled in (docs/02-data.md,
    /// "`app.sqlite`").
    pub async fn app_settings_get(; data: State<'_, AppData>) -> Settings {
        let data = data.inner().clone();
        blocking(move || data.settings()).await
    }
}

command! {
    /// Saves some settings and answers every setting. Unknown keys are kept.
    /// A value outside its setting's range answers `setting_out_of_range`
    /// and changes nothing.
    pub async fn app_settings_set<R: Runtime>(
        patch: Map<String, Value>;
        app: AppHandle<R>,
        data: State<'_, AppData>,
        thumbs: State<'_, Thumbs>
    ) -> Settings {
        let data = data.inner().clone();
        let thumbs = thumbs.inner().clone();
        let theme_changed = patch.contains_key("theme");
        let settings = blocking(move || {
            let settings = data.set_settings(&patch)?;
            // Lowering the limit prunes at once.
            thumbs.cache().set_limit_gb(settings.cache_limit_gb);
            Ok(settings)
        })
        .await?;
        // A new theme colours the title bar and the window at once (issue 75).
        if theme_changed && let Some(window) = app.get_webview_window("main") {
            crate::theme::apply(&window, settings.theme);
        }
        Ok(settings)
    }
}

command! {
    /// The native "Open a folder of photos" dialog, for the page's Open
    /// folder button. Answers the chosen folder, or null when it was
    /// cancelled.
    pub async fn app_pick_folder<R: Runtime>(; app: AppHandle<R>) -> Option<String> {
        let picker = app
            .try_state::<FolderPicker>()
            .map(|p| Arc::clone(&p.0))
            .ok_or_else(|| AppError::internal(text::appdata::PICKING_FOLDER))?;
        blocking(move || Ok(picker().map(|p| p.display().to_string()))).await
    }
}

command! {
    /// Called once when the page has drawn: everything it needs to begin
    /// (docs/08-desktop-and-release.md, "Commands and events this part
    /// adds"). A folder that arrived before this call is handed over here
    /// as `launch_folder`; later ones arrive as `open_folder`.
    pub async fn app_startup<R: Runtime>(; app: AppHandle<R>) -> Startup {
        blocking(move || {
            let data = app.state::<AppData>();
            let settings = data.settings()?;
            let recent = data.recent()?;
            let launch = app.try_state::<Handoff>().and_then(|h| h.start());
            let (launch_folder, launch_photo) = match launch {
                Some(open) => (Some(open.path), open.photo),
                None => (None, None),
            };
            let reopen = app::reopen(launch_folder.as_deref(), settings.reopen_last, &recent);
            let dev = crate::is_dev_build(cfg!(debug_assertions), &app.config().identifier);
            // What's new (docs/08, "What's new"): shown now and recorded
            // when its dialog closes, or recorded at once.
            let version = app::version().version;
            let off = dev || whats_new::env_off(std::env::var_os(whats_new::OFF_VAR).as_deref());
            let entries = whats_new::parse(whats_new::CHANGELOG);
            let news =
                whats_new::at_start(&version, settings.whats_new_last_seen.as_deref(), off, &entries);
            let settings = match &news.record {
                Some(seen) => data.set_settings(
                    serde_json::json!({ "whats_new.last_seen": seen })
                        .as_object()
                        .expect("an object"),
                )?,
                None => settings,
            };
            // Startup step 10: the page has drawn.
            crate::fatal::page_started();
            crate::show_window(&app);
            crate::update::start_quiet(&app);
            // An install the copy before could not run (issue 178).
            crate::update::report_failed_install(&app);
            Ok(Startup {
                version: app::version().version,
                platform: std::env::consts::OS.to_owned(),
                arch: std::env::consts::ARCH.to_owned(),
                package: app::package_kind(dev, std::env::var_os("APPIMAGE").is_some()).to_owned(),
                dev,
                show_update_notice: !settings.updates_notice_seen,
                whats_new: news.show,
                settings,
                recent,
                launch_folder,
                launch_photo,
                reopen,
            })
        })
        .await
    }
}

/// Shows the native folder dialog and answers the chosen folder. `run`
/// installs the real dialog; shell tests install a fake.
#[derive(Clone)]
pub struct FolderPicker(pub Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>);

command! {
    /// The program's version.
    pub fn app_version() -> AppVersion {
        Ok(app::version())
    }
}

/// Runs `job` on the blocking pool: it touches `app.sqlite`, and checking
/// the recent folders can take a moment.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| {
            AppError::internal(text::appdata::READING_SETTINGS)
                .with_details(serde_json::json!({ "cause": e.to_string() }))
        })?
}
