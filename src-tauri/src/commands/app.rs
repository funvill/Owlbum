//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use crate::{area, command};
use owlbum_core::app::{self, AppVersion};
use owlbum_core::appdata::{AppData, Recent, Settings};
use owlbum_core::info::country::{self, CountryMatch};
use owlbum_core::media::{self, FormatInfo};
use owlbum_core::thumbs::{CacheCleared, CacheUsage, Thumbs};
use owlbum_core::{AppError, text};
use serde_json::{Map, Value};
use tauri::State;

area! {
    prefix: "app",
    commands: [
        app_cache_clear,
        app_cache_usage,
        app_formats,
        app_place_search,
        app_recent_list,
        app_recent_remove,
        app_settings_get,
        app_settings_set,
        app_version,
    ],
    events: [],
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
    pub async fn app_settings_set(
        patch: Map<String, Value>;
        data: State<'_, AppData>,
        thumbs: State<'_, Thumbs>
    ) -> Settings {
        let data = data.inner().clone();
        let thumbs = thumbs.inner().clone();
        blocking(move || {
            let settings = data.set_settings(&patch)?;
            // Lowering the limit prunes at once.
            thumbs.cache().set_limit_gb(settings.cache_limit_gb);
            Ok(settings)
        })
        .await
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
