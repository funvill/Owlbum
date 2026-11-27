//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use std::path::PathBuf;
use std::sync::Arc;

use crate::handoff::Handoff;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::reopen;
use owlbum_core::app::{self, AppClosing, AppMenu, AppVersion, FolderDrop, OpenFolder, Startup};
use owlbum_core::appdata::{AppData, Recent, Settings};
use owlbum_core::files::Capabilities;
use owlbum_core::info::country::{self, CountryMatch};
use owlbum_core::media::{self, FormatInfo};
use owlbum_core::thumbs::{CacheCleared, CacheUsage, Thumbs};
use owlbum_core::{AppError, text};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "app",
    commands: [
        app_cache_clear,
        app_cache_usage,
        app_capabilities,
        app_close_ready,
        app_formats,
        app_pick_folder,
        app_place_search,
        app_quit,
        app_recent_list,
        app_recent_remove,
        app_session_save,
        app_settings_get,
        app_settings_set,
        app_startup,
        app_version,
    ],
    events: [AppClosing, AppMenu, FolderDrop, OpenFolder],
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
    /// Countries whose name contains the text, at most 6, each with its
    /// central point, from the bundled outlines (docs/04-browsing.md, "How
    /// typed fields save"). Async so the first call, which reads the
    /// outlines, runs off the main thread.
    pub async fn app_place_search(text: String) -> Vec<CountryMatch> {
        Ok(country::place_search(&text))
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
        let settings = blocking(move || {
            let settings = data.set_settings(&patch)?;
            // Lowering the limit prunes at once.
            thumbs.cache().set_limit_gb(settings.cache_limit_gb);
            Ok(settings)
        })
        .await?;
        // Help > Check automatically is the same setting.
        crate::menu::updates_auto(&app, settings.updates_auto);
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
            let launch_folder = app.try_state::<Handoff>().and_then(|h| h.start());
            let reopen = app::reopen(launch_folder.as_deref(), settings.reopen_last, &recent);
            let dev = crate::is_dev_build(cfg!(debug_assertions), &app.config().identifier);
            // Startup step 10: the page has drawn.
            crate::show_window(&app);
            Ok(Startup {
                version: app::version().version,
                platform: std::env::consts::OS.to_owned(),
                arch: std::env::consts::ARCH.to_owned(),
                package: app::package_kind(dev, std::env::var_os("APPIMAGE").is_some()).to_owned(),
                dev,
                show_update_notice: !settings.updates_notice_seen,
                settings,
                recent,
                launch_folder,
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
