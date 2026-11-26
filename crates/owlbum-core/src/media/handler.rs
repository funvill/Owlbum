//! The media handler trait and the types its calls return
//! (docs/03-media-and-scanning.md, "Media handlers").

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::text;

/// The largest picture a handler opens, in pixels.
pub const MAX_PIXELS: u64 = 300_000_000;

/// How expensive a handler's `preview` is, for scheduling the pixels pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cost {
    /// JPEG, small PNG: read first.
    Cheap,
    /// A full decode of a format with no fast path.
    Decode,
    /// RAW, video, very large TIFF: at most half the scan pool at once.
    Heavy,
}

/// An 8-bit sRGB picture, always upright.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    /// Width of this buffer.
    pub width: u32,
    /// Height of this buffer.
    pub height: u32,
    /// Upright width of the full picture. Equals `width` from `working`; 0
    /// when a reduced or embedded preview cannot tell.
    pub full_width: u32,
    /// Upright height of the full picture, as `full_width`.
    pub full_height: u32,
    /// 4 bytes per pixel, row-major. Alpha is 255 when the source has none.
    pub rgba: Vec<u8>,
    /// True when the source has an alpha channel.
    pub has_alpha: bool,
}

impl fmt::Debug for Pixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pixels")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("full_width", &self.full_width)
            .field("full_height", &self.full_height)
            .field("rgba", &format_args!("[{} bytes]", self.rgba.len()))
            .field("has_alpha", &self.has_alpha)
            .finish()
    }
}

impl Pixels {
    /// The RGBA value of one pixel. Panics outside the buffer.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "({x}, {y}) is outside");
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

/// The channels of a [`SourcePixels`] buffer, in sample order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// One grey sample per pixel.
    Grey,
    /// Grey, then alpha.
    GreyAlpha,
    /// Red, green, blue.
    Rgb,
    /// Red, green, blue, alpha.
    Rgba,
}

impl Layout {
    /// Samples per pixel.
    pub fn channels(self) -> usize {
        match self {
            Self::Grey => 1,
            Self::GreyAlpha => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    /// True when the last sample of each pixel is alpha.
    pub fn has_alpha(self) -> bool {
        matches!(self, Self::GreyAlpha | Self::Rgba)
    }
}

/// The samples of a [`SourcePixels`] buffer at the source's own depth.
#[derive(Clone, PartialEq, Eq)]
pub enum Samples {
    /// 8 bits per sample.
    U8(Vec<u8>),
    /// 16 bits per sample: a 16-bit source, or a deeper or floating-point
    /// one scaled to 16 bits.
    U16(Vec<u16>),
}

impl Samples {
    /// Bits per sample: 8 or 16.
    pub fn bit_depth(&self) -> u8 {
        match self {
            Self::U8(_) => 8,
            Self::U16(_) => 16,
        }
    }

    /// The number of samples.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(v) => v.len(),
            Self::U16(v) => v.len(),
        }
    }

    /// True when there are no samples.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Debug for Samples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U{}[{} samples]", self.bit_depth(), self.len())
    }
}

/// The full picture at the source's own bit depth with its colour profile
/// untouched, for the edit renderer (docs/06-editing-and-file-actions.md,
/// "Rendering"). Upright, like every handler result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePixels {
    /// Upright width.
    pub width: u32,
    /// Upright height.
    pub height: u32,
    /// Which channels each pixel has.
    pub layout: Layout,
    /// `width × height × layout.channels()` samples, row-major.
    pub samples: Samples,
    /// The embedded ICC profile's bytes, unconverted. None means sRGB.
    pub icc: Option<Vec<u8>>,
}

/// Facts a handler reads from a file's headers, without decoding pixels.
///
/// The EXIF-derived scan facts (capture date, camera, GPS, the `exif` JSON)
/// are parsed from `exif` by the metadata reader (roadmap #10), not here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Upright width; 0 when the headers cannot tell.
    pub width: u32,
    /// Upright height; 0 when the headers cannot tell.
    pub height: u32,
    /// True when the picture has an alpha channel.
    pub has_alpha: bool,
    /// The number of frames, only when there is more than one. Set for
    /// animated GIF, WebP and PNG (`exif.frames`).
    pub frames: Option<u32>,
    /// The length of one loop in milliseconds, for animated GIF and WebP
    /// (`photos.duration_ms`). None for a still, and for an animated PNG,
    /// which Owlbum shows as a still.
    pub duration_ms: Option<u64>,
    /// The raw EXIF block, from the TIFF header on, when the container has
    /// one.
    pub exif: Option<Vec<u8>>,
}

/// Facts about a video from its container and streams.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoInfo {
    /// Length in milliseconds.
    pub duration_ms: u64,
    /// Displayed width: after rotation and pixel aspect ratio.
    pub width: u32,
    /// Displayed height.
    pub height: u32,
    /// Clockwise rotation in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Short codec name such as `h264`, when known.
    pub video_codec: Option<String>,
    /// Short codec name such as `aac`, when known.
    pub audio_codec: Option<String>,
}

/// Reads one family of formats. Handlers are compiled in; every registry row
/// names one (docs/03-media-and-scanning.md, "Media handlers").
///
/// Every handler:
/// - returns upright pixels (orientation applied);
/// - returns sRGB from `preview` and `working` (colour conversion is
///   roadmap #7) and 8 bits per sample;
/// - opens files read-only and closes them before returning;
/// - refuses a picture over [`MAX_PIXELS`] with `too_large`, checked on the
///   header before pixel memory is allocated;
/// - accepts a damaged file that still yields a picture;
/// - uses no network and starts no child process.
pub trait MediaHandler: Send + Sync {
    /// True when the first bytes of the file belong to this handler.
    fn sniff(&self, head: &[u8]) -> bool;
    /// Facts from headers only: no pixel decode.
    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError>;
    /// Fast picture, at least `min_side` on its longest side when the file
    /// allows. Good enough for thumbnails.
    fn preview(&self, file: &Path, min_side: u32) -> Result<Pixels, MediaError>;
    /// Full quality, full size, 8-bit sRGB. Used for zoom, copy and tagging.
    fn working(&self, file: &Path) -> Result<Pixels, MediaError>;
    /// Full size at the source's own bit depth with its colour profile
    /// untouched. Used only by the edit renderer.
    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError>;
    /// Video only: duration, dimensions, rotation, codecs. None for stills.
    fn probe(&self, file: &Path) -> Result<Option<VideoInfo>, MediaError>;
    /// How expensive `preview` is, for scheduling.
    fn cost(&self, file_size: u64) -> Cost;
}

/// What went wrong reading a file. Stored in `photos.scan_error` as
/// `<code>: <sentence>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaErrorCode {
    /// The file could not be opened or read.
    Io,
    /// The bytes are not a picture the handler can read.
    Decode,
    /// The bytes are a format Owlbum does not support yet.
    Unsupported,
    /// The picture is over the pixel limit.
    TooLarge,
}

impl MediaErrorCode {
    /// The code as stored in `scan_error`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Decode => "decode",
            Self::Unsupported => "unsupported",
            Self::TooLarge => "too_large",
        }
    }
}

/// A handler's error: a code and a sentence for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct MediaError {
    /// What kind of failure.
    pub code: MediaErrorCode,
    /// A sentence for the user, from the `text` module.
    pub message: String,
}

impl MediaError {
    /// The file could not be opened or read.
    pub fn io(err: &std::io::Error) -> Self {
        Self {
            code: MediaErrorCode::Io,
            message: text::media::io_error(err),
        }
    }

    /// The bytes are not a `label` picture.
    pub fn decode(label: &str) -> Self {
        Self {
            code: MediaErrorCode::Decode,
            message: text::media::decode_error(label),
        }
    }

    /// The bytes are a format Owlbum does not support yet; `reason` is the
    /// sentence shown for it.
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            code: MediaErrorCode::Unsupported,
            message: reason.into(),
        }
    }

    /// The picture is over [`MAX_PIXELS`].
    pub fn too_large(width: u32, height: u32) -> Self {
        Self {
            code: MediaErrorCode::TooLarge,
            message: text::media::too_large(width, height),
        }
    }

    /// The `photos.scan_error` value: `<code>: <sentence>`.
    pub fn scan_error(&self) -> String {
        format!("{}: {}", self.code.as_str(), self.message)
    }
}

/// Refuses a picture over [`MAX_PIXELS`]. Call it with the header's size
/// before allocating pixel memory.
pub fn check_size(width: u32, height: u32) -> Result<(), MediaError> {
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        Err(MediaError::too_large(width, height))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_limit_is_three_hundred_million_pixels() {
        assert!(check_size(20_000, 15_000).is_ok());
        let e = check_size(20_000, 15_001).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::TooLarge);
        assert_eq!(
            e.message,
            "This picture is too large to open (20000 × 15001)."
        );
        assert!(check_size(u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn scan_error_is_code_colon_sentence() {
        assert_eq!(
            MediaError::decode("PNG").scan_error(),
            "decode: This file is damaged or is not a PNG picture."
        );
        assert_eq!(
            MediaError::too_large(40_000, 30_000).scan_error(),
            "too_large: This picture is too large to open (40000 × 30000)."
        );
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access is denied.");
        assert_eq!(
            MediaError::io(&io).scan_error(),
            "io: This file could not be read: Access is denied."
        );
    }

    #[test]
    fn codes_have_their_documented_names() {
        for (code, name) in [
            (MediaErrorCode::Io, "io"),
            (MediaErrorCode::Decode, "decode"),
            (MediaErrorCode::Unsupported, "unsupported"),
            (MediaErrorCode::TooLarge, "too_large"),
        ] {
            assert_eq!(code.as_str(), name);
            assert_eq!(serde_json::to_value(code).unwrap(), name);
        }
    }
}
