//! Owlbum's logic. Everything that can be tested with `cargo test` and a
//! temp folder lives here. This crate has no dependency on Tauri.
//!
//! One module per feature area. A feature's issue adds its code inside its
//! area's module (as submodules when it grows), so parallel work does not
//! edit the same lines. The list below is meant to be fixed; add an area
//! only when no existing one fits.

// Shared foundations.
pub mod error;
pub mod events;
pub mod text;

// Feature areas. The numbers are roadmap numbers (docs/roadmap.md).
/// The album database, album root discovery, lock and view-only, writer
/// thread, backups (#3, #4).
pub mod album;
/// Start-up facts and the program's version.
pub mod app;
/// The app data folder: `app.sqlite`, settings, recent albums, logs (#5).
pub mod appdata;
/// Desktop rules that need no window: update rules, What's new rules, window
/// restore, feedback address, log rotation (#58 to #63).
pub mod desktop;
/// The edit model and renderer, saving edits (#43 to #47).
pub mod edit;
/// The in-memory album index (#12).
pub mod index;
/// Typed info: description, notes, people, date, place, country outlines
/// (#24, #25).
pub mod info;
/// The media format registry, decoders, colour profiles, RAW, video (#6, #7,
/// #48 to #51).
pub mod media;
/// Star, hide, photo score, bursts and stacks, undo, tidy (#27, #28, #30, #33,
/// #34).
pub mod organise;
/// The query engine: filters, sorting, query words, text search, tokens
/// (#13, #29, #31, #32).
pub mod query;
/// The scanner, walker rules, metadata reading, folder watching (#8 to #10,
/// #14).
pub mod scan;
/// Tagging: models, downloads, the queue, search by meaning (#52 to #57).
pub mod tagging;
/// Thumbnails, the zoom copy and the size-limited cache (#11).
pub mod thumbs;

pub use error::{AppError, ErrorCode};
pub use events::Event;
