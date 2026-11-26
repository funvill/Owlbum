//! The one place decoded pixels are brought to sRGB
//! (docs/03-media-and-scanning.md, "Media handlers": the sRGB rule).
//!
//! Every handler's `preview` and `working` result passes through
//! [`to_srgb`]. Converting an embedded profile is roadmap #7, which chooses
//! the colour library by measurement; until then the pixels are passed
//! through unchanged, which is right for every file without a profile or
//! with an sRGB one.

use super::handler::Pixels;

/// Converts 8-bit pixels described by the ICC profile `icc` to sRGB. With no
/// profile the pixels are taken as sRGB already.
pub fn to_srgb(pixels: Pixels, icc: Option<&[u8]>) -> Pixels {
    let _ = icc;
    pixels
}
