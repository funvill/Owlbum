//! How the scanner reads files. Every read goes through [`Reader`], so tests
//! can count them (a "counting handler") and roadmap #10 and #11 can see
//! where reading happens. [`MediaReader`] is the real one: the format
//! registry's handlers.

use std::path::Path;

use super::keys::{self, Sample};
use crate::media::{MediaError, MediaFormat, Metadata, Pixels, VideoInfo};

/// The longest side the pixels pass asks a handler for: the medium
/// thumbnail's.
pub const PREVIEW_SIDE: u32 = 1600;

/// The scanner's reads of one file.
pub trait Reader: Send + Sync {
    /// Reads the two ends of the file for its `file_key` and its head for
    /// sniffing (at most 32 KiB).
    fn sample(&self, path: &Path) -> std::io::Result<Sample> {
        keys::sample(path)
    }

    /// Facts from headers only (the facts pass).
    fn metadata(&self, format: &'static MediaFormat, path: &Path) -> Result<Metadata, MediaError> {
        format.handler.metadata(path)
    }

    /// Video facts (the facts pass, video only).
    fn probe(
        &self,
        format: &'static MediaFormat,
        path: &Path,
    ) -> Result<Option<VideoInfo>, MediaError> {
        format.handler.probe(path)
    }

    /// The one decode of the pixels pass: `preview(1600)`.
    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        format.handler.preview(path, PREVIEW_SIDE)
    }
}

/// Reads with the format registry's handlers.
#[derive(Debug, Clone, Copy, Default)]
pub struct MediaReader;

impl Reader for MediaReader {}
