//! Events pushed from Rust to the page (docs/01-architecture.md, "Events").
//!
//! An event is a payload type that implements [`Event`]. The shell emits it
//! by name and the binding generator gives the page a typed `listen`. Core
//! code that produces events (the scanner, the tagging queue) takes an
//! emitter from the shell rather than depending on Tauri.

use serde::Serialize;
use ts_rs::TS;

/// A payload pushed from Rust to the page.
pub trait Event: Serialize + TS + 'static {
    /// The event's name on the wire, `snake_case`, for example
    /// `"album_changed"`.
    const NAME: &'static str;
}

/// Something that can push events to the page. The shell implements it over
/// Tauri; tests implement it with a recorder.
pub trait Emitter: Send + Sync {
    /// Sends `payload` (already serialised) under `name`.
    fn emit_json(&self, name: &'static str, payload: serde_json::Value);
}

/// Sends a typed event through an emitter.
pub fn emit<E: Event>(emitter: &dyn Emitter, event: &E) {
    match serde_json::to_value(event) {
        Ok(value) => emitter.emit_json(E::NAME, value),
        // A payload that cannot be serialised is a programming error.
        Err(e) => debug_assert!(false, "event {} did not serialise: {e}", E::NAME),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Serialize, TS)]
    struct Ping {
        n: u32,
    }
    impl Event for Ping {
        const NAME: &'static str = "ping";
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(&'static str, serde_json::Value)>>);
    impl Emitter for Recorder {
        fn emit_json(&self, name: &'static str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((name, payload));
        }
    }

    #[test]
    fn emit_sends_the_name_and_serialised_payload() {
        let r = Recorder::default();
        emit(&r, &Ping { n: 3 });
        assert_eq!(
            r.0.lock().unwrap().as_slice(),
            &[("ping", serde_json::json!({ "n": 3 }))]
        );
    }
}
