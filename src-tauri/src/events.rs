//! Sending events to the page. Payload types and their names live in
//! `owlbum-core` (`owlbum_core::Event`); each is listed in the `events` of
//! the area that sends it (see `commands`), which gives the page its type.

use owlbum_core::Event;
use owlbum_core::events::Emitter;
use tauri::{AppHandle, Emitter as _, Runtime};

/// Sends a typed event to the page.
pub fn emit<R: Runtime, E: Event>(app: &AppHandle<R>, event: &E) -> tauri::Result<()> {
    app.emit(E::NAME, event)
}

/// Lets core code (the scanner, the tagging queue) send events without
/// depending on Tauri.
pub struct TauriEmitter<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Emitter for TauriEmitter<R> {
    fn emit_json(&self, name: &'static str, payload: serde_json::Value) {
        // A failed emit means the window is gone; there is no one to tell.
        let _ = self.0.emit(name, payload);
    }
}
