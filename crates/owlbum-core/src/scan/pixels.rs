//! What the pixels pass makes from its one decode
//! (docs/03-media-and-scanning.md, "Pixels pass").
//!
//! Roadmap #10 replaces [`analysis`] with the pinned image analysis, and
//! raises [`super::SCAN_VERSION`] in the same commit so the rows analysed
//! here with `{}` are read again. Roadmap #11 writes the thumbnails from
//! the same [`Pixels`], next to the call of [`analysis`] in the scanner.

use crate::media::Pixels;

/// The `analysis` JSON of a picture. Until the analysis is built (#10) it is
/// the empty object: the row is complete, and nothing is measured yet.
pub fn analysis(_pixels: &Pixels) -> String {
    "{}".to_owned()
}

/// The upright size to store after the decode, when it should change the
/// row: the header gave none, or the decode reports a different full size.
/// The size of a reduced or embedded preview (`full_*` 0) is never stored.
pub fn corrected_size(header: (u32, u32), pixels: &Pixels) -> Option<(u32, u32)> {
    let full = (pixels.full_width, pixels.full_height);
    if full.0 == 0 || full.1 == 0 || full == header {
        None
    } else {
        Some(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(full: (u32, u32)) -> Pixels {
        Pixels {
            width: 4,
            height: 2,
            full_width: full.0,
            full_height: full.1,
            rgba: vec![0; 32],
            has_alpha: false,
        }
    }

    #[test]
    fn the_decode_corrects_a_missing_or_wrong_header_size() {
        assert_eq!(corrected_size((0, 0), &px((40, 20))), Some((40, 20)));
        assert_eq!(corrected_size((30, 20), &px((40, 20))), Some((40, 20)));
        assert_eq!(corrected_size((40, 20), &px((40, 20))), None);
        // A reduced preview that cannot tell the full size changes nothing.
        assert_eq!(corrected_size((0, 0), &px((0, 0))), None);
    }

    #[test]
    fn analysis_is_an_object() {
        let v: serde_json::Value = serde_json::from_str(&analysis(&px((4, 2)))).unwrap();
        assert!(v.is_object());
    }
}
