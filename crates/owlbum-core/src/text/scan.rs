//! Sentences of the scan report (docs/03-media-and-scanning.md, "Nested
//! album roots").

/// A subfolder that is an album of its own. `path` is relative to the
/// album root.
pub fn nested_album(path: &str) -> String {
    format!("{path} is its own album and is not shown here.")
}

/// Said once per session in the status line when the album's folder is not
/// watched (docs/03-media-and-scanning.md, "When watching cannot be
/// trusted").
pub const NOT_WATCHING: &str = "Owlbum can't watch this folder for changes. It checks when you come back to the window; use Rescan to check now.";
