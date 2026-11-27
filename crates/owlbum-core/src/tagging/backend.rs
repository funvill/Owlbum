//! The backend trait (docs/07-tagging.md, "The backend trait"): everything
//! that runs a model sits behind [`TagBackend`], so the queue, the commands
//! and search are tested with the fake in [`super::fake`].

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use image::RgbImage;

use super::catalogue::{ModelChoice, Part};
use super::results::{Described, Keyworded};

/// The picture given to a model.
pub struct TagImage<'a> {
    pub rgb: &'a RgbImage,
    /// The file name, for logs and for the fake.
    pub name: &'a str,
}

/// Why a model call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The model would not load.
    Load(String),
    /// This picture could not be handled; it becomes a failed row.
    Photo(String),
    /// The model is not loaded, or the run panicked.
    Lost(String),
    /// The run passed its time limit.
    Timeout,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Load(m) => write!(f, "load failed: {m}"),
            BackendError::Photo(m) => write!(f, "{m}"),
            BackendError::Lost(m) => write!(f, "lost: {m}"),
            BackendError::Timeout => write!(f, "timed out"),
        }
    }
}

/// When a run must end: a time, or earlier when it is told to stop (the
/// album closed). Checked between steps; the ONNX backend also ends a run
/// in progress through ONNX Runtime's run options.
#[derive(Debug, Clone)]
pub struct Deadline {
    end: Instant,
    stop: Arc<AtomicBool>,
}

impl Deadline {
    /// A deadline `limit` from now that `stop` can bring forward.
    pub fn new(limit: Duration, stop: Arc<AtomicBool>) -> Self {
        Self {
            end: Instant::now() + limit,
            stop,
        }
    }

    /// A deadline nothing else can stop.
    pub fn after(limit: Duration) -> Self {
        Self::new(limit, Arc::new(AtomicBool::new(false)))
    }

    /// True once the time has passed or the run was told to stop.
    pub fn expired(&self) -> bool {
        self.stopped() || Instant::now() >= self.end
    }

    /// True when the run was told to stop (not a timeout).
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// `Err(Timeout)` once expired, for `?` between steps.
    pub fn check(&self) -> Result<(), BackendError> {
        if self.expired() {
            Err(BackendError::Timeout)
        } else {
            Ok(())
        }
    }

    /// How long is left.
    pub fn remaining(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }
}

/// Turns search text into a unit vector. Safe to call from another thread
/// while the tagging thread runs a picture.
pub trait TextEmbedder: Send + Sync {
    fn embed(&self, text: &str, deadline: &Deadline) -> Result<Vec<f32>, BackendError>;
}

/// Runs models. One slot per part; a slot holds at most one loaded model.
pub trait TagBackend: Send {
    /// Loads `model` from the files in `dir` into its part's slot, dropping
    /// what the slot held. A keywords model embeds `vocabulary` here.
    fn load(
        &mut self,
        model: &ModelChoice,
        dir: &Path,
        threads: usize,
        vocabulary: &[&str],
        deadline: &Deadline,
    ) -> Result<(), BackendError>;
    /// Empties a part's slot.
    fn unload(&mut self, part: Part);
    /// The id of the model in a part's slot.
    fn loaded(&self, part: Part) -> Option<&'static str>;
    /// Greedy caption, then objects.
    fn describe(&mut self, image: &TagImage, deadline: &Deadline) -> Result<Described, BackendError>;
    /// A sampled caption only, for Regenerate.
    fn caption(&mut self, image: &TagImage, deadline: &Deadline) -> Result<String, BackendError>;
    /// Keywords and the vector.
    fn keywords(&mut self, image: &TagImage, deadline: &Deadline) -> Result<Keyworded, BackendError>;
    /// The loaded keywords model's text side, for search by meaning.
    fn text_embedder(&self) -> Option<Arc<dyn TextEmbedder>>;
}

/// Makes a backend for a new tagging thread. Fails when the runtime itself
/// cannot start; the reason is shown as "The model runtime would not
/// start: {reason}".
pub type BackendFactory = Arc<dyn Fn() -> Result<Box<dyn TagBackend>, String> + Send + Sync>;
