//! Sentences of the scan report (docs/03-media-and-scanning.md, "Nested
//! album roots").

/// A subfolder that is an album of its own. `path` is relative to the
/// album root.
pub fn nested_album(path: &str) -> String {
    format!("{path} is its own album and is not shown here.")
}
