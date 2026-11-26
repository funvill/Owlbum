//! Signature tests on the first bytes of a file
//! (docs/03-media-and-scanning.md, "When the extension lies").
//!
//! RAW and video are recognised by container family only: TIFF-based, ISO
//! base media, RIFF and EBML. ISO base media files are told apart by their
//! brands, so a HEIC or AVIF still is never taken for a video.

pub fn jpeg(head: &[u8]) -> bool {
    head.starts_with(&[0xFF, 0xD8, 0xFF])
}

pub fn png(head: &[u8]) -> bool {
    head.starts_with(b"\x89PNG\r\n\x1a\n")
}

pub fn gif(head: &[u8]) -> bool {
    head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a")
}

/// A RIFF file of the given form type (`WEBP`, `AVI `).
fn riff(head: &[u8], form: &[u8; 4]) -> bool {
    head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == form
}

pub fn webp(head: &[u8]) -> bool {
    riff(head, b"WEBP")
}

/// Classic or BigTIFF, either byte order.
pub fn tiff(head: &[u8]) -> bool {
    head.starts_with(b"II*\0")
        || head.starts_with(b"MM\0*")
        || head.starts_with(b"II+\0")
        || head.starts_with(b"MM\0+")
}

pub fn bmp(head: &[u8]) -> bool {
    head.len() >= 14 && head.starts_with(b"BM")
}

/// A bare JPEG XL codestream or the JPEG XL container.
pub fn jxl(head: &[u8]) -> bool {
    head.starts_with(&[0xFF, 0x0A])
        || head.starts_with(&[
            0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
}

/// The major brand and compatible brands of an ISO base media file's
/// leading `ftyp` box, as far as `head` holds them.
fn ftyp_brands(head: &[u8]) -> Option<Vec<[u8; 4]>> {
    if head.len() < 16 || &head[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
    let end = size.clamp(16, head.len());
    let mut brands = vec![[head[8], head[9], head[10], head[11]]];
    // Bytes 12..16 are the minor version; compatible brands follow.
    let mut i = 16;
    while i + 4 <= end {
        brands.push([head[i], head[i + 1], head[i + 2], head[i + 3]]);
        i += 4;
    }
    Some(brands)
}

const HEIC_BRANDS: [&[u8; 4]; 8] = [
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs",
];
const AVIF_BRANDS: [&[u8; 4]; 2] = [b"avif", b"avis"];
/// Brands of still-image files in ISO base media: HEIF, AVIF, Canon CR3.
const STILL_BRANDS: [&[u8; 4]; 13] = [
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1", b"msf1",
    b"avif", b"avis", b"crx ",
];

pub fn avif(head: &[u8]) -> bool {
    ftyp_brands(head).is_some_and(|b| b.iter().any(|x| AVIF_BRANDS.contains(&x)))
}

/// HEIF with HEVC pictures. An AVIF file (which may also carry `mif1`) is
/// not HEIC.
pub fn heic(head: &[u8]) -> bool {
    !avif(head)
        && ftyp_brands(head).is_some_and(|b| {
            b.iter()
                .any(|x| HEIC_BRANDS.contains(&x) || x == b"mif1" || x == b"msf1")
        })
}

/// Camera RAW containers: TIFF-based (CR2, NEF, ARW, DNG; ORF and RW2 use
/// their own TIFF magic), Fujifilm RAF, and Canon CR3 (ISO base media).
pub fn raw_family(head: &[u8]) -> bool {
    tiff(head)
        || head.starts_with(b"IIRO")
        || head.starts_with(b"IIRS")
        || head.starts_with(b"MMOR")
        || head.starts_with(b"IIU\0")
        || head.starts_with(b"FUJIFILMCCD-RAW")
        || ftyp_brands(head).is_some_and(|b| b[0] == *b"crx ")
}

/// Video containers: ISO base media that is not a still-image brand, RIFF
/// AVI, and EBML (Matroska, WebM).
pub fn video_family(head: &[u8]) -> bool {
    let iso_video = ftyp_brands(head).is_some_and(|b| !b.iter().any(|x| STILL_BRANDS.contains(&x)));
    iso_video || riff(head, b"AVI ") || head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + 4 * compatible.len();
        let mut v = (size as u32).to_be_bytes().to_vec();
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(major);
        v.extend_from_slice(&[0, 0, 0, 0]);
        for c in compatible {
            v.extend_from_slice(*c);
        }
        v.extend_from_slice(&[0; 16]);
        v
    }

    #[test]
    fn heif_brands_are_stills_not_video() {
        let heic_file = ftyp(b"heic", &[b"mif1", b"heic"]);
        assert!(heic(&heic_file) && !avif(&heic_file) && !video_family(&heic_file));
        let avif_file = ftyp(b"avif", &[b"mif1", b"avif"]);
        assert!(avif(&avif_file) && !heic(&avif_file) && !video_family(&avif_file));
        // An AVIF whose major brand is mif1 is still AVIF.
        let mif1_avif = ftyp(b"mif1", &[b"avif", b"mif1"]);
        assert!(avif(&mif1_avif) && !heic(&mif1_avif));
    }

    #[test]
    fn mp4_and_mov_are_video() {
        for brand in [b"isom", b"mp42", b"qt  ", b"M4V "] {
            let f = ftyp(brand, &[b"isom"]);
            assert!(video_family(&f), "{brand:?}");
            assert!(!heic(&f) && !avif(&f) && !raw_family(&f));
        }
    }

    #[test]
    fn cr3_is_raw_not_video() {
        let cr3 = ftyp(b"crx ", &[b"crx ", b"isom"]);
        assert!(raw_family(&cr3));
        assert!(!video_family(&cr3));
    }

    #[test]
    fn riff_form_type_separates_webp_and_avi() {
        let mut webp_head = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        webp_head.extend_from_slice(&[0; 8]);
        let mut avi = b"RIFF\0\0\0\0AVI LIST".to_vec();
        avi.extend_from_slice(&[0; 8]);
        assert!(webp(&webp_head) && !video_family(&webp_head));
        assert!(video_family(&avi) && !webp(&avi));
    }

    #[test]
    fn short_heads_match_nothing() {
        for head in [&b""[..], b"\xFF", b"RIFF", b"\0\0\0\x0cftyp"] {
            assert!(
                !jpeg(head)
                    && !png(head)
                    && !gif(head)
                    && !webp(head)
                    && !tiff(head)
                    && !bmp(head)
                    && !jxl(head)
                    && !heic(head)
                    && !avif(head)
                    && !raw_family(head)
                    && !video_family(head),
                "{head:?}"
            );
        }
    }
}
