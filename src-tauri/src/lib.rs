//! Owlbum's desktop shell, the crate `owlbum`.
//!
//! It is thin: each command validates its arguments, calls one core function
//! and maps the result or error to a reply. It also owns what only a desktop
//! shell can do: windows, menus, dialogs, drag-out, trash, clipboard, reveal,
//! the updater and single-instance handling.

pub mod bindings;
pub mod commands;
pub mod events;

use tauri::Runtime;

/// Adds Owlbum's commands to a Tauri builder. `run` uses it, and shell tests
/// use it with Tauri's mock runtime.
pub fn setup<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(commands::registry::invoke_handler(commands::modules()))
}

/// Starts the program.
///
/// # Panics
///
/// When Tauri cannot start. Failure handling replaces this (roadmap issue 61).
pub fn run() {
    setup(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("Owlbum could not start");
}
