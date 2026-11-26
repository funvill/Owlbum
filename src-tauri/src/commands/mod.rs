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

use registry::Module;
use tauri::Runtime;

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
