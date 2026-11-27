//! Tagging for the whole program (docs/07-tagging.md): the chosen models
//! (settings), the install state of each model, the one download thread,
//! and the [`Runtime`] that tags the open album. The shell holds one and
//! its commands call it.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::catalogue::{Catalogue, ModelChoice, Part, Size};
use super::download::{self, Fetch, Job, Outcome};
use super::install::{self, InstallState};
use super::runtime::Runtime;
use crate::appdata::AppData;
use crate::error::AppResult;
use crate::events::{Emitter, Event, emit};
use crate::text::tagging as words;
use crate::{AppError, ErrorCode};

/// One model as Settings shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ModelInfo {
    pub id: String,
    pub part: Part,
    pub size: Size,
    pub label: String,
    pub bytes: u64,
    pub licence: String,
    pub seconds_per_photo: f32,
    pub state: InstallState,
    pub bytes_present: u64,
}

/// The chosen model of each part; null is Off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TaggingChoice {
    pub describe: Option<String>,
    pub keywords: Option<String>,
}

/// A download in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Downloading {
    pub model: String,
    pub received: u64,
    pub total: u64,
}

/// `tagging_models`'s reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ModelsState {
    pub models: Vec<ModelInfo>,
    pub settings: TaggingChoice,
    pub not_working: Vec<Part>,
    pub downloading: Option<Downloading>,
    /// The last download failure.
    pub error: Option<String>,
}

/// The `download_progress` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DownloadProgress {
    pub model: String,
    pub received: u64,
    pub total: u64,
    pub error: Option<String>,
    pub finished: bool,
}

impl Event for DownloadProgress {
    const NAME: &'static str = "download_progress";
}

struct Running {
    model: &'static ModelChoice,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    received: u64,
    total: u64,
}

#[derive(Default)]
struct Downloads {
    running: Option<Running>,
    error: Option<String>,
    checking: BTreeSet<&'static str>,
    damaged: BTreeSet<&'static str>,
}

struct Inner {
    data: AppData,
    models: PathBuf,
    catalogue: Catalogue,
    fetch: Arc<dyn Fetch>,
    emitter: Arc<dyn Emitter>,
    downloads: Mutex<Downloads>,
    stall: Duration,
}

/// Tagging for the program.
#[derive(Clone)]
pub struct Tagging {
    inner: Arc<Inner>,
    runtime: Arc<Runtime>,
}

impl Tagging {
    /// Tagging over `data`'s settings and `<data>/models`.
    pub fn new(
        data: AppData,
        catalogue: Catalogue,
        fetch: Arc<dyn Fetch>,
        emitter: Arc<dyn Emitter>,
        runtime: Runtime,
    ) -> Self {
        let models = data.dir().join("models");
        let t = Self {
            inner: Arc::new(Inner {
                data,
                models,
                catalogue,
                fetch,
                emitter,
                downloads: Mutex::new(Downloads::default()),
                stall: download::STALL,
            }),
            runtime: Arc::new(runtime),
        };
        t.apply_choice();
        t
    }

    /// The same with a shorter stall limit, for tests.
    #[must_use]
    pub fn with_stall(self, stall: Duration) -> Self {
        let inner = Inner {
            data: self.inner.data.clone(),
            models: self.inner.models.clone(),
            catalogue: self.inner.catalogue,
            fetch: Arc::clone(&self.inner.fetch),
            emitter: Arc::clone(&self.inner.emitter),
            downloads: Mutex::new(Downloads::default()),
            stall,
        };
        Self {
            inner: Arc::new(inner),
            runtime: self.runtime,
        }
    }

    /// `<app data>/models`.
    pub fn models_dir(&self) -> &PathBuf {
        &self.inner.models
    }

    /// The catalogue in use.
    pub fn catalogue(&self) -> Catalogue {
        self.inner.catalogue
    }

    /// The queue.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The queue, to hold apart from the service.
    pub fn runtime_arc(&self) -> Arc<Runtime> {
        Arc::clone(&self.runtime)
    }

    fn downloads(&self) -> MutexGuard<'_, Downloads> {
        self.inner.downloads.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn dir(&self, m: &ModelChoice) -> PathBuf {
        install::model_dir(&self.inner.models, m)
    }

    /// The saved choice.
    pub fn choice(&self) -> TaggingChoice {
        self.inner
            .data
            .settings()
            .map(|s| TaggingChoice {
                describe: s.tagging_describe,
                keywords: s.tagging_keywords,
            })
            .unwrap_or_default()
    }

    /// A model's state: the quick reading, with hashing in progress and a
    /// failed check remembered.
    fn state_of(&self, m: &'static ModelChoice) -> InstallState {
        let d = self.downloads();
        if d.checking.contains(m.id) {
            return InstallState::Checking;
        }
        let quick = install::quick_state(&self.dir(m), m);
        if d.damaged.contains(m.id) && quick != InstallState::Ok {
            return InstallState::Damaged;
        }
        quick
    }

    /// The chosen model of a part when it is in the catalogue and `ok`.
    fn chosen_ok(&self, part: Part, choice: &TaggingChoice) -> Option<&'static ModelChoice> {
        let id = match part {
            Part::Describe => choice.describe.as_deref(),
            Part::Keywords => choice.keywords.as_deref(),
        }?;
        let m = self.inner.catalogue.get(id).filter(|m| m.part == part)?;
        (install::quick_state(&self.dir(m), m) == InstallState::Ok).then_some(m)
    }

    /// Tells the runtime which models are active.
    fn apply_choice(&self) {
        let choice = self.choice();
        self.runtime.set_active([
            self.chosen_ok(Part::Describe, &choice),
            self.chosen_ok(Part::Keywords, &choice),
        ]);
    }

    /// `tagging_models`. An `unverified` model is hashed in the background
    /// the first time it is seen.
    pub fn models(&self) -> ModelsState {
        let mut models = Vec::new();
        for m in self.inner.catalogue.all() {
            let mut state = self.state_of(m);
            if state == InstallState::Unverified && self.downloads().running.as_ref().is_none_or(|r| r.model.id != m.id) {
                self.check_in_background(m);
                state = InstallState::Checking;
            }
            models.push(ModelInfo {
                id: m.id.to_owned(),
                part: m.part,
                size: m.size,
                label: m.label.to_owned(),
                bytes: m.bytes(),
                licence: m.licence.as_str().to_owned(),
                seconds_per_photo: m.seconds_per_photo,
                state,
                bytes_present: install::bytes_present(&self.dir(m), m),
            });
        }
        let d = self.downloads();
        ModelsState {
            models,
            settings: self.choice(),
            not_working: self.runtime.status().not_working,
            downloading: d.running.as_ref().map(|r| Downloading {
                model: r.model.id.to_owned(),
                received: r.received,
                total: r.total,
            }),
            error: d.error.clone(),
        }
    }

    fn check_in_background(&self, m: &'static ModelChoice) {
        if !self.downloads().checking.insert(m.id) {
            return;
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("owlbum-model-check".into())
            .spawn(move || {
                let state = install::verify(&this.dir(m), m, false);
                let mut d = this.downloads();
                d.checking.remove(m.id);
                if state == InstallState::Damaged {
                    d.damaged.insert(m.id);
                }
                drop(d);
                this.apply_choice();
            });
        if spawned.is_err() {
            self.downloads().checking.remove(m.id);
        }
    }

    fn model(&self, id: &str) -> AppResult<&'static ModelChoice> {
        self.inner
            .catalogue
            .get(id)
            .ok_or_else(|| AppError::new(ErrorCode::NoSuchModel, words::NO_SUCH_MODEL))
    }

    /// `tagging_download`: starts the download on its own thread.
    pub fn download(&self, id: &str) -> AppResult<()> {
        let m = self.model(id)?;
        let mut d = self.downloads();
        if d.running.as_ref().is_some_and(|r| r.thread.as_ref().is_some_and(|t| !t.is_finished())) {
            return Err(AppError::new(ErrorCode::DownloadBusy, words::DOWNLOAD_BUSY));
        }
        d.error = None;
        d.damaged.remove(m.id);
        let cancel = Arc::new(AtomicBool::new(false));
        let this = self.clone();
        let flag = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("owlbum-model-download".into())
            .spawn(move || this.download_thread(m, &flag))
            .map_err(AppError::internal)?;
        d.running = Some(Running {
            model: m,
            cancel,
            thread: Some(thread),
            received: 0,
            total: m.bytes(),
        });
        Ok(())
    }

    fn download_thread(&self, m: &'static ModelChoice, cancel: &AtomicBool) {
        let dir = self.dir(m);
        // A model that would not load: every file is hashed first.
        if self.runtime.status().not_working.contains(&m.part) {
            install::verify(&dir, m, true);
        }
        let emitter = Arc::clone(&self.inner.emitter);
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut progress = |received: u64, total: u64| {
            if let Some(r) = self.downloads().running.as_mut() {
                r.received = received;
                r.total = total;
            }
            if last.elapsed() >= Duration::from_millis(100) {
                last = Instant::now();
                emit(
                    &*emitter,
                    &DownloadProgress {
                        model: m.id.to_owned(),
                        received,
                        total,
                        error: None,
                        finished: false,
                    },
                );
            }
        };
        let outcome = download::download(&mut Job {
            model: m,
            dir: &dir,
            fetch: &*self.inner.fetch,
            cancel,
            progress: &mut progress,
            stall: self.inner.stall,
        });
        let error = match &outcome {
            Outcome::Failed(reason) => Some(words::download_failed(m.label, reason)),
            _ => None,
        };
        let received = install::bytes_present(&dir, m);
        {
            let mut d = self.downloads();
            d.error.clone_from(&error);
            if let Some(r) = d.running.as_mut() {
                r.received = received;
            }
        }
        if outcome == Outcome::Done {
            self.runtime.clear_not_working();
            self.apply_choice();
        }
        if outcome != Outcome::Cancelled {
            emit(
                &*self.inner.emitter,
                &DownloadProgress {
                    model: m.id.to_owned(),
                    received: if outcome == Outcome::Done { m.bytes() } else { received },
                    total: m.bytes(),
                    error,
                    finished: true,
                },
            );
        }
        // The thread's handle is taken by whoever waits; a finished
        // download leaves no running entry.
        let mut d = self.downloads();
        if d.running.as_ref().is_some_and(|r| r.model.id == m.id && !r.cancel.load(Ordering::Relaxed)) {
            d.running = None;
        }
    }

    /// `tagging_download_cancel`: stops the download and returns when it
    /// has stopped. Cancel is not an error.
    pub fn cancel_download(&self) {
        let running = self.downloads().running.take();
        if let Some(mut r) = running {
            r.cancel.store(true, Ordering::Relaxed);
            if let Some(t) = r.thread.take() {
                let _ = t.join();
            }
        }
    }

    /// `tagging_remove`: cancels its download, unloads it, deletes its
    /// folder, and sets its part to Off if it was chosen. Album results are
    /// kept.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        let m = self.model(id)?;
        if self.downloads().running.as_ref().is_some_and(|r| r.model.id == m.id) {
            self.cancel_download();
        }
        let choice = self.choice();
        let chosen = match m.part {
            Part::Describe => choice.describe.as_deref() == Some(m.id),
            Part::Keywords => choice.keywords.as_deref() == Some(m.id),
        };
        self.runtime.unload(m.part);
        if chosen {
            let mut patch = serde_json::Map::new();
            patch.insert(format!("tagging.{}", m.part.as_str()), serde_json::Value::Null);
            self.inner.data.set_settings(&patch)?;
        }
        install::remove(&self.dir(m)).map_err(|e| AppError::internal(format!("removing {}: {e}", m.label)))?;
        {
            let mut d = self.downloads();
            d.damaged.remove(m.id);
        }
        self.runtime.clear_not_working();
        self.apply_choice();
        Ok(())
    }

    /// `tagging_set`: saves the choice. Each part takes null or a model of
    /// that part whose state is `ok`.
    pub fn set(&self, describe: Option<String>, keywords: Option<String>) -> AppResult<TaggingChoice> {
        let refuse = || AppError::new(ErrorCode::ModelNotReady, words::MODEL_NOT_READY);
        for (part, id) in [(Part::Describe, &describe), (Part::Keywords, &keywords)] {
            if let Some(id) = id {
                let m = self.inner.catalogue.get(id).filter(|m| m.part == part).ok_or_else(refuse)?;
                if self.state_of(m) != InstallState::Ok {
                    return Err(refuse());
                }
            }
        }
        let mut patch = serde_json::Map::new();
        patch.insert("tagging.describe".into(), describe.into());
        patch.insert("tagging.keywords".into(), keywords.into());
        self.inner.data.set_settings(&patch)?;
        self.runtime.clear_not_working();
        self.apply_choice();
        Ok(self.choice())
    }

    /// Owlbum closes: the download is cancelled and the models unloaded.
    pub fn shutdown(&self) {
        self.cancel_download();
        self.runtime.shutdown();
    }
}

#[cfg(test)]
mod tests;
