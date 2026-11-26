//! The `extras` album: the further files single parts' tests ask for. It is
//! a separate album so the `trip` album stays small and its counts stay
//! simple.

use std::io;
use std::path::Path;

use crate::encode::{self, Meta};
use crate::picture::{self, quadrants, scene};
use crate::rng::Rng;
use crate::{Album, Fixture, Kind, Writer, icc};

/// Write the `extras` album into `root` (which must be missing or empty).
pub fn extras(root: &Path) -> io::Result<Album> {
    let mut w = Writer::new(root)?;

    // All eight EXIF orientations of one upright 96 × 64 picture of four
    // coloured quadrants.
    let upright = quadrants(96, 64);
    for n in 1..=8u8 {
        let stored = picture::store_for_orientation(&upright, n);
        let meta = Meta {
            orientation: u16::from(n),
            pixels: Some(stored.dimensions()),
            ..Meta::default()
        };
        w.put(
            Fixture::new(
                format!("orientation/{n}.jpg"),
                Kind::Orientation,
                Some("jpeg"),
            )
            .size(96, 64),
            &encode::jpeg(&stored, 95, &meta),
        )?;
    }

    // Animations: three frames each; frame 1 is the picture.
    let frames: Vec<_> = (0..3)
        .map(|i| picture::nudge(&scene(0x0100, 120, 90), i * 20, 0, 0))
        .collect();
    let animations: [(&str, &str, Vec<u8>); 3] = [
        (
            "animated/animated.gif",
            "gif",
            encode::gif_animated(&frames, 200),
        ),
        (
            "animated/animated.webp",
            "webp",
            encode::webp_animated(&frames, 200),
        ),
        ("animated/animated.png", "png", encode::apng(&frames, 200)),
    ];
    for (rel, id, bytes) in animations {
        w.put(
            Fixture::new(rel, Kind::Animated, Some(id)).size(120, 90),
            &bytes,
        )?;
    }

    // PNG variants.
    w.put(
        Fixture::new("png/transparent.png", Kind::Transparent, Some("png")).size(200, 150),
        &encode::png_rgba(&picture::transparent_disc(200, 150)),
    )?;
    w.put(
        Fixture::new("png/wide-gamut.png", Kind::WideGamut, Some("png")).size(200, 150),
        &encode::png_with_icc(&scene(0x0110, 200, 150), &icc::display_p3()),
    )?;
    w.put(
        Fixture::new("png/16-bit.png", Kind::SixteenBit, Some("png")).size(200, 150),
        &encode::png_rgb16(&scene(0x0111, 200, 150)),
    )?;

    // Extensions that lie, and bytes nothing reads.
    w.put(
        Fixture::new("lies/png-named.jpg", Kind::WrongExtension, Some("png")).size(160, 120),
        &encode::png_rgb(&scene(0x0120, 160, 120)),
    )?;
    let mut junk = vec![0u8; 4096];
    Rng::new(0x0121).fill(&mut junk);
    w.put(
        Fixture::new("lies/heic-named.jpg", Kind::WrongExtension, None),
        &encode::heic_signature(&junk[..512]),
    )?;
    w.put(Fixture::new("lies/random.jpg", Kind::Corrupt, None), &junk)?;
    w.put(
        Fixture::new("lies/too-large.png", Kind::TooLarge, Some("png")).size(20_000, 20_000),
        &encode::png_header_only(20_000, 20_000),
    )?;

    // Recognised but not supported yet: counted, never given a row.
    let unsupported: [(&str, Vec<u8>); 4] = [
        (
            "unsupported/photo.heic",
            encode::heic_signature(&junk[..256]),
        ),
        (
            "unsupported/photo.avif",
            encode::avif_signature(&junk[..256]),
        ),
        ("unsupported/old-raw.crw", junk[..256].to_vec()),
        ("unsupported/clip.mts", junk[..256].to_vec()),
    ];
    for (rel, bytes) in unsupported {
        w.put(Fixture::new(rel, Kind::Unsupported, None), &bytes)?;
    }

    // A video stored landscape and turned a quarter for display.
    w.put(
        Fixture::new("video/rotated.mp4", Kind::Video, Some("mp4")).size(120, 160),
        &crate::trip::clip(0x0130, true),
    )?;

    // Names the walker skips. Each holds a real photo, so a walker that
    // fails to skip it shows up as an extra row.
    let small = encode::jpeg(&scene(0x0140, 64, 48), 80, &Meta::default());
    for rel in [
        "skipped/.hidden.jpg",
        "skipped/._IMG_0001.jpg",
        "skipped/.dotfolder/inside.jpg",
        "skipped/$RECYCLE.BIN/deleted.jpg",
        "skipped/System Volume Information/inside.jpg",
        "skipped/@eaDir/thumb.jpg",
        "skipped/#recycle/deleted.jpg",
        "skipped/#snapshot/old.jpg",
        "skipped/__MACOSX/inside.jpg",
    ] {
        w.put(
            Fixture::new(rel, Kind::Skipped, Some("jpeg")).size(64, 48),
            &small,
        )?;
    }

    // A nested album root: its own (empty) database, and a photo the outer
    // album must not show.
    w.put(
        Fixture::new("nested/.owlbum/owlbum.sqlite", Kind::Nested, None),
        &[],
    )?;
    w.put(
        Fixture::new("nested/inside.jpg", Kind::Nested, Some("jpeg")).size(64, 48),
        &encode::jpeg(&scene(0x0150, 64, 48), 80, &Meta::default()),
    )?;

    Ok(w.finish())
}
