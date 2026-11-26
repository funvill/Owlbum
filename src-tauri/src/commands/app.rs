//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use crate::{area, command};
use owlbum_core::app::{self, AppVersion};
use owlbum_core::appdata::{AppData, Recent, Settings};
use owlbum_core::media::{self, FormatInfo};
use owlbum_core::{AppError, text};
use serde_json::{Map, Value};
use tauri::State;

area! {
    prefix: "app",
    commands: [
        app_formats,
        app_recent_list,
        app_recent_remove,
        app_settings_get,
        app_settings_set,
        app_version,
    ],
    events: [],
}

command! {
    /// The media format registry, so the page never hard-codes a format rule
    /// (docs/03-media-and-scanning.md, "Commands").
    pub fn app_formats() -> Vec<FormatInfo> {
        Ok(media::formats())
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
    pub async fn app_settings_set(patch: Map<String, Value>; data: State<'_, AppData>) -> Settings {
        let data = data.inner().clone();
        blocking(move || data.set_settings(&patch)).await
    }
}

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
