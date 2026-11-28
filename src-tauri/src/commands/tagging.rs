//! Tagging models, the queue and Regenerate (`tagging_`; docs/07-tagging.md,
//! "Commands and events").

use std::sync::{Arc, OnceLock};

use owlbum_core::appdata::AppData;
use owlbum_core::tagging::backend::{BackendFactory, TagBackend};
use owlbum_core::tagging::catalogue::Catalogue;
use owlbum_core::tagging::download::HttpFetch;
use owlbum_core::tagging::onnx::{self, OnnxBackend};
use owlbum_core::tagging::runtime::{Limits, Runtime as TagRuntime, TaggingProgress};
use owlbum_core::tagging::service::{DownloadProgress, ModelsState, Tagging, TaggingChoice};
use owlbum_core::tagging::tags::PhotoTags;
use owlbum_core::{AppError, ErrorCode, text};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use ts_rs::TS;

use crate::commands::off_main;
use crate::events::TauriEmitter;
use crate::session::Session;
use crate::{area, command};

area! {
    prefix: "tagging",
    commands: [
        tagging_models,
        tagging_download,
        tagging_download_cancel,
        tagging_remove,
        tagging_set,
        tagging_status,
        tagging_pause,
        tagging_restart,
        tagging_retry_failed,
        tagging_focus,
        tagging_regenerate,
    ],
    events: [TaggingProgress, DownloadProgress],
}

/// The program's tagging, made the first time a command or an album needs
/// it (it sends events, so it needs the app).
#[derive(Default)]
pub struct TaggingState(OnceLock<Tagging>);

/// The program's tagging. None when the app data folder is not managed
/// (a shell test that did not give one).
pub fn tagging<R: Runtime>(app: &AppHandle<R>) -> Option<Tagging> {
    let state = app.try_state::<TaggingState>()?;
    let data = app.try_state::<AppData>()?;
    Some(
        state
            .0
            .get_or_init(|| {
                let emitter = Arc::new(TauriEmitter(app.clone()));
                let factory: BackendFactory =
                    Arc::new(|| OnnxBackend::new().map(|b| Box::new(b) as Box<dyn TagBackend>));
                let runtime = TagRuntime::new(
                    factory,
                    data.dir().join("models"),
                    onnx::default_threads(),
                    emitter.clone(),
                    Limits::default(),
                );
                Tagging::new(
                    data.inner().clone(),
                    Catalogue::real(),
                    Arc::new(HttpFetch::new()),
                    emitter,
                    runtime,
                )
            })
            .clone(),
    )
}

/// Owlbum closes: the download is cancelled, the queue stops and the
/// models are unloaded. Nothing to do when tagging was never started.
pub fn shutdown<R: Runtime>(app: &AppHandle<R>) {
    if let Some(t) = app.try_state::<TaggingState>().and_then(|s| s.0.get().cloned()) {
        t.shutdown();
    }
}

fn need<R: Runtime>(app: &AppHandle<R>) -> Result<Tagging, AppError> {
    tagging(app).ok_or_else(|| AppError::internal(text::appdata::READING_SETTINGS))
}

/// `view_only` when the open album is view-only.
fn writable(session: &Session) -> Result<(), AppError> {
    let message = session
        .scanner()
        .and_then(|s| s.album().db().view_only_message());
    match message {
        Some(m) => Err(AppError::new(ErrorCode::ViewOnly, m)),
        None => Ok(()),
    }
}

/// `tagging_retry_failed`'s reply.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct RetryCleared {
    /// Photos whose failed rows were deleted.
    pub cleared: u64,
}

command! {
    /// The models, their install state, the choice, and any download.
    pub async fn tagging_models<R: Runtime>(; app: AppHandle<R>) -> ModelsState {
        let t = need(&app)?;
        off_main(text::appdata::READING_SETTINGS, move || Ok(t.models())).await
    }
}

command! {
    /// Starts downloading a model; progress comes as `download_progress`.
    pub fn tagging_download<R: Runtime>(model: String; app: AppHandle<R>) -> () {
        need(&app)?.download(&model)
    }
}

command! {
    /// Stops the download; returns when it has stopped.
    pub async fn tagging_download_cancel<R: Runtime>(; app: AppHandle<R>) -> () {
        let t = need(&app)?;
        off_main(text::appdata::READING_SETTINGS, move || {
            t.cancel_download();
            Ok(())
        })
        .await
    }
}

command! {
    /// Deletes a model's files; its part goes Off if it was chosen.
    pub async fn tagging_remove<R: Runtime>(model: String; app: AppHandle<R>) -> () {
        let t = need(&app)?;
        off_main(text::appdata::READING_SETTINGS, move || t.remove(&model)).await
    }
}

command! {
    /// Saves the chosen model of each part (null is Off).
    pub fn tagging_set<R: Runtime>(
        describe: Option<String>,
        keywords: Option<String>;
        app: AppHandle<R>
    ) -> TaggingChoice {
        need(&app)?.set(describe, keywords)
    }
}

command! {
    /// The `tagging_progress` payload, for when the page loads.
    pub fn tagging_status<R: Runtime>(; app: AppHandle<R>) -> TaggingProgress {
        Ok(need(&app)?.runtime().status())
    }
}

command! {
    /// Pauses or resumes the queue. Never saved.
    pub fn tagging_pause<R: Runtime>(paused: bool; app: AppHandle<R>, session: State<'_, Session>) -> () {
        writable(&session)?;
        need(&app)?.runtime().pause(paused);
        Ok(())
    }
}

command! {
    /// Try again: clears the problem, the not-working set and the loss
    /// counts, and starts the queue.
    pub fn tagging_restart<R: Runtime>(; app: AppHandle<R>, session: State<'_, Session>) -> () {
        writable(&session)?;
        need(&app)?.runtime().restart();
        Ok(())
    }
}

command! {
    /// Try failed photos again.
    pub async fn tagging_retry_failed<R: Runtime>(; app: AppHandle<R>, session: State<'_, Session>) -> RetryCleared {
        writable(&session)?;
        let t = need(&app)?;
        off_main(text::album::WRITING_DATABASE, move || {
            Ok(RetryCleared { cleared: t.runtime().retry_failed()? })
        })
        .await
    }
}

command! {
    /// The photos to tag first: the selected one, then those on screen.
    pub fn tagging_focus<R: Runtime>(ids: Vec<i64>; app: AppHandle<R>) -> () {
        need(&app)?.runtime().focus(&ids);
        Ok(())
    }
}

command! {
    /// A new description for one photo (docs/07-tagging.md, "Regenerate").
    /// Answers the photo's new tags; on every error the old sentence stays.
    pub async fn tagging_regenerate<R: Runtime>(id: i64; app: AppHandle<R>, session: State<'_, Session>) -> PhotoTags {
        writable(&session)?;
        let (scanner, _) = session.queries()?;
        let t = need(&app)?;
        off_main(text::album::WRITING_DATABASE, move || t.regenerate(scanner.album(), id)).await
    }
}
