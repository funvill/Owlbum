//! Handlers for the RAW and video rows of the registry whose decoders are
//! not built yet: RAW is roadmap #49 (`rawler`), video #50 (system codecs).
//!
//! Each recognises its container family, so sniffing and the "extension
//! lies" rules already work for these rows, and answers every read with
//! `unsupported`. The scanner therefore records such a file as an
//! unreadable photo with a plain reason instead of skipping it silently.
//! The issue that builds the real handler replaces the static in the
//! registry and deletes this one. It must also raise `SCAN_VERSION`, so
//! that the rows these placeholders gave an `unsupported` error are read
//! again by the next scan.

use std::path::Path;

use super::handler::{Cost, MediaError, MediaHandler, Metadata, Pixels, SourcePixels, VideoInfo};
use super::sniff;
use crate::text;

/// A container family whose decoder is still to come.
#[derive(Debug)]
pub struct Pending {
    sniff: fn(&[u8]) -> bool,
    reason: &'static str,
}

/// Camera RAW (roadmap #49).
pub static RAW: Pending = Pending {
    sniff: sniff::raw_family,
    reason: text::media::RAW_UNSUPPORTED,
};

/// Video (roadmap #50).
pub static VIDEO: Pending = Pending {
    sniff: sniff::video_family,
    reason: text::media::VIDEO_UNSUPPORTED,
};

impl Pending {
    fn refuse<T>(&self) -> Result<T, MediaError> {
        Err(MediaError::unsupported(self.reason))
    }
}

impl MediaHandler for Pending {
    fn sniff(&self, head: &[u8]) -> bool {
        (self.sniff)(head)
    }

    fn metadata(&self, _file: &Path) -> Result<Metadata, MediaError> {
        self.refuse()
    }

    fn preview(&self, _file: &Path, _min_side: u32) -> Result<Pixels, MediaError> {
        self.refuse()
    }

    fn working(&self, _file: &Path) -> Result<Pixels, MediaError> {
        self.refuse()
    }

    fn source(&self, _file: &Path) -> Result<SourcePixels, MediaError> {
        self.refuse()
    }

    fn probe(&self, _file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        self.refuse()
    }

    fn cost(&self, _file_size: u64) -> Cost {
        Cost::Heavy
    }
}
