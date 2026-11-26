//! The app's commands, one module per command prefix
//! (docs/01-architecture.md, "Commands").
//!
//! To add a command: declare it with `command!` in its area's module and add
//! its name to that module's `area!` list. To add an event: give its payload
//! type an `owlbum_core::Event` impl and add the type to the `events` list of
//! the area that sends it. Then run `scripts/bindings.sh`. Nothing else
//! changes, so parallel issues do not edit the same lines.

pub mod registry;

pub mod album;
pub mod app;
pub mod edit;
pub mod file;
pub mod photo;
pub mod query;
pub mod search;
pub mod tagging;

use owlbum_core::AppError;
use registry::Module;
use tauri::Runtime;

/// Runs `job` on the blocking pool, off the thread that answers the page:
/// for work that touches a database or the disk, or waits. `what` names
/// what was being done, for the error when the job panics.
pub(crate) async fn off_main<T: Send + 'static>(
    what: &'static str,
    job: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| {
            AppError::internal(what).with_details(serde_json::json!({ "cause": e.to_string() }))
        })?
}

/// Every area, in prefix order. This list is fixed: one entry per prefix in
/// the architecture's command table.
pub fn modules<R: Runtime>() -> Vec<Module<R>> {
    vec![
        album::module(),
        app::module(),
        edit::module(),
        file::module(),
        photo::module(),
        query::module(),
        search::module(),
        tagging::module(),
    ]
}
