//! Tagging, "What's in it" (docs/07-tagging.md): the model catalogue, model
//! downloads and their checks, the runtime that runs the models, the queue
//! that tags the open album, and the results it stores.
//!
//! - [`catalogue`], [`vocabulary`]: what can be installed, compiled in.
//! - [`hosts`], [`download`], [`install`]: getting a model onto the disk
//!   and knowing it is sound.
//! - [`backend`]: the trait every model runner sits behind; [`onnx`] runs
//!   the real models, [`fake`] stands in for them in tests.
//! - [`results`]: turning raw model output into what is stored.
//! - [`store`]: the `tags`, `vectors` and `photo_labels` rows.
//! - [`runtime`]: the tagging thread, its queue and its status.
//! - [`tags`]: one photo's tags as the details panel shows them.
//! - [`meaning`]: search by meaning and its vector cache.
//! - [`service`]: everything above for one program: settings, the
//!   download thread, the runtime.

pub mod backend;
pub mod catalogue;
pub mod download;
pub mod fake;
pub mod hosts;
pub mod install;
pub mod meaning;
pub mod onnx;
pub mod results;
pub mod runtime;
pub mod service;
pub mod store;
pub mod tags;
pub mod vocabulary;
