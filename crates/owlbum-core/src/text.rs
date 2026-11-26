//! Every sentence Rust shows the user: error messages, menu labels, native
//! dialog wording (docs/01-architecture.md, "Interface text has two homes").
//! The page's own text lives in `ui/src/text/`.
//!
//! One submodule per feature area, named like the area's module in `lib.rs`
//! (`text/album.rs` for `album`), so parallel issues add text in different
//! files. Text shared by several areas goes in `errors`.

pub mod album;
pub mod appdata;
pub mod errors;
pub mod media;
pub mod scan;
