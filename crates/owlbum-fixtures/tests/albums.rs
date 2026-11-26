//! The generated albums: what is in them, that every picture decodes as
//! what the manifest says, and that the bytes never drift.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use owlbum_fixtures::{Album, Kind, extras, large, trip};

fn files_under(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy();
                out.insert(rel.replace('\\', "/"));
            }
        }
    }
    out
}

/// FNV-1a over every file's path and bytes, in path order. No dependency,
/// so the pinned value means the same thing on every system.
fn digest(root: &Path) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for rel in files_under(root) {
        eat(rel.as_bytes());
        eat(&[0]);
        eat(&fs::read(root.join(&rel)).unwrap());
    }
    hash
}

fn decoded_size(album: &Album, rel: &str) -> (u32, u32) {
    let bytes = fs::read(album.root.join(rel)).unwrap();
    let img = image::load_from_memory(&bytes).unwrap_or_else(|e| panic!("{rel}: {e}"));
    (img.width(), img.height())
}

/// The trip album's files and sizes, pinned. A change to the generator that
/// changes any byte fails here; update the list only on purpose.
const TRIP_FILES: &[(&str, u64)] = &[
    ("copies/lake.jpg", 23400),
    ("day1/IMG_0002.jpg", 26674),
    ("day1/IMG_0003.jpg", 21515),
    ("day1/lake.jpg", 23400),
    ("day1/panorama.jpg", 89230),
    ("day2/blurred.jpg", 20805),
    ("day2/burst/IMG_0010.jpg", 30063),
    ("day2/burst/IMG_0011.jpg", 29382),
    ("day2/burst/IMG_0012.jpg", 28616),
    ("day2/burst/IMG_0013.jpg", 28446),
    ("day2/burst/IMG_0014.jpg", 27771),
    ("day2/harbour.jpg", 22264),
    ("day2/rotated.jpg", 29963),
    ("day3/plain.png", 1776),
    ("day3/sample.dng", 32608),
    ("day3/tiny.jpg", 2460),
    ("formats/sample.bmp", 230454),
    ("formats/sample.gif", 11165),
    ("formats/sample.jxl", 3788),
    ("formats/sample.tif", 230610),
    ("formats/sample.webp", 1738),
    ("problems/corrupt.jpg", 2052),
    ("problems/photo.heic", 536),
    ("video/clip.mp4", 30898),
];
const TRIP_DIGEST: u64 = 0x1412_70e4_c636_26c6;

#[test]
fn trip_is_pinned() {
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    let on_disk: Vec<(String, u64)> = files_under(&album.root)
        .into_iter()
        .map(|rel| {
            let size = fs::metadata(album.root.join(&rel)).unwrap().len();
            (rel, size)
        })
        .collect();
    let pinned: Vec<(String, u64)> = TRIP_FILES
        .iter()
        .map(|(p, s)| ((*p).to_string(), *s))
        .collect();
    assert_eq!(on_disk, pinned, "trip file list or sizes changed");
    assert_eq!(digest(&album.root), TRIP_DIGEST, "trip bytes changed");
}

#[test]
fn trip_manifest_matches_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    let listed: BTreeSet<String> = album.files.iter().map(|f| f.rel_path.clone()).collect();
    assert_eq!(listed, files_under(&album.root));
    assert!(
        (20..=30).contains(&album.files.len()),
        "about two dozen files, got {}",
        album.files.len()
    );
}

#[test]
fn trip_covers_the_spec() {
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    for kind in [
        Kind::Landscape,
        Kind::Portrait,
        Kind::Square,
        Kind::Panorama,
        Kind::Burst,
        Kind::Blurred,
        Kind::ExifRotated,
        Kind::Duplicate,
        Kind::LowRes,
        Kind::NoMetadata,
        Kind::Format,
        Kind::Raw,
        Kind::Video,
        Kind::Corrupt,
        Kind::Unsupported,
    ] {
        assert!(
            album.files.iter().any(|f| f.kind == kind),
            "trip has no {kind:?}"
        );
    }
    assert!(album.of_kind(Kind::Burst).count() >= 3);
    let formats: BTreeSet<&str> = album.files.iter().filter_map(|f| f.format).collect();
    for id in [
        "jpeg", "png", "webp", "gif", "tiff", "bmp", "jxl", "dng", "mp4",
    ] {
        assert!(formats.contains(id), "trip has no {id}");
    }
    // The fake tagger reads words from file names.
    assert!(album.files.iter().any(|f| f.rel_path.contains("lake")));
    assert!(album.files.iter().any(|f| f.rel_path.contains("harbour")));
}

#[test]
fn trip_pictures_decode_at_the_stated_size() {
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    for f in &album.files {
        match f.format {
            Some("jpeg" | "png" | "webp" | "gif" | "tiff" | "bmp") if f.kind != Kind::Corrupt => {
                let (w, h) = decoded_size(&album, &f.rel_path);
                if f.kind == Kind::ExifRotated {
                    // Stored sideways; the manifest gives the upright size.
                    assert_eq!((h, w), (f.width, f.height), "{}", f.rel_path);
                } else {
                    assert_eq!((w, h), (f.width, f.height), "{}", f.rel_path);
                }
            }
            Some("jxl") => {
                let img = jxl_oxide::JxlImage::builder()
                    .open(album.root.join(&f.rel_path))
                    .unwrap();
                assert_eq!((img.width(), img.height()), (f.width, f.height));
                img.render_frame(0).unwrap();
            }
            _ => {}
        }
    }
}

#[test]
fn shapes_are_what_they_say() {
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    for f in &album.files {
        match f.kind {
            Kind::Landscape => assert!(f.width > f.height, "{}", f.rel_path),
            Kind::Portrait | Kind::ExifRotated => assert!(f.width < f.height, "{}", f.rel_path),
            Kind::Square => assert_eq!(f.width, f.height),
            Kind::Panorama => assert!(f.width >= 4 * f.height),
            Kind::LowRes => assert!(f.width * f.height < 10_000),
            _ => {}
        }
    }
}

#[test]
fn trip_metadata_is_readable() {
    use nom_exif::{ExifTag, read_exif};
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    let rotated = album.of_kind(Kind::ExifRotated).next().unwrap();
    let exif = read_exif(album.root.join(&rotated.rel_path)).unwrap();
    assert_eq!(
        exif.get(ExifTag::Orientation).and_then(|v| v.as_u16()),
        Some(6)
    );

    let lake = album.get("day1/lake.jpg").unwrap();
    let exif = read_exif(album.root.join(&lake.rel_path)).unwrap();
    let gps = exif.gps_info().expect("lake has GPS");
    let (lat, lon) = lake.gps.unwrap();
    assert!((gps.latitude_decimal().unwrap() - lat).abs() < 1e-4);
    assert!((gps.longitude_decimal().unwrap() - lon).abs() < 1e-4);
    let taken = exif.get(ExifTag::DateTimeOriginal).unwrap().to_string();
    assert!(taken.starts_with("2025-07-14"), "{taken}");
    assert_eq!(
        exif.get(ExifTag::Make).and_then(|v| v.as_str()),
        Some("Owlbum")
    );
    assert_eq!(lake.taken.as_deref(), Some("2025-07-14T09:12:00"));

    let plain = album.of_kind(Kind::NoMetadata).next().unwrap();
    assert!(read_exif(album.root.join(&plain.rel_path)).is_err());
}

#[test]
fn trip_video_has_a_track() {
    use nom_exif::{TrackInfoTag, read_track};
    let dir = tempfile::tempdir().unwrap();
    let album = trip(dir.path()).unwrap();
    let video = album.of_kind(Kind::Video).next().unwrap();
    let info = read_track(album.root.join(&video.rel_path)).unwrap();
    assert_eq!(
        info.get(TrackInfoTag::Width).and_then(|v| v.as_u32()),
        Some(video.width)
    );
    assert_eq!(
        info.get(TrackInfoTag::Height).and_then(|v| v.as_u32()),
        Some(video.height)
    );
    assert_eq!(
        info.get(TrackInfoTag::DurationMs).and_then(|v| v.as_u64()),
        Some(2000)
    );
}

#[test]
fn same_seed_same_bytes() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    trip(a.path()).unwrap();
    trip(b.path()).unwrap();
    assert_eq!(digest(a.path()), digest(b.path()));
    large(a.path().join("big").as_path(), 200, 7).unwrap();
    large(b.path().join("big").as_path(), 200, 7).unwrap();
    assert_eq!(digest(&a.path().join("big")), digest(&b.path().join("big")));
}

#[test]
fn large_has_n_distinct_photos() {
    let dir = tempfile::tempdir().unwrap();
    let album = large(dir.path(), 300, 1).unwrap();
    assert_eq!(album.files.len(), 300);
    assert_eq!(files_under(&album.root).len(), 300);
    let mut contents = BTreeSet::new();
    for f in &album.files {
        assert_eq!(f.format, Some("jpeg"));
        assert!(contents.insert(fs::read(album.root.join(&f.rel_path)).unwrap()));
    }
    // Spread over years, with some bursts and some photos without a date.
    let years: BTreeSet<&str> = album
        .files
        .iter()
        .filter_map(|f| f.taken.as_deref())
        .map(|t| &t[..4])
        .collect();
    assert!(years.len() >= 8, "{years:?}");
    assert!(album.of_kind(Kind::Burst).count() > 0);
    assert!(album.files.iter().any(|f| f.taken.is_none()));
    assert!(album.files.iter().any(|f| f.gps.is_some()));
    let f = &album.files[0];
    assert_eq!(decoded_size(&album, &f.rel_path), (f.width, f.height));
}

#[test]
fn another_seed_another_album() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    large(a.path(), 50, 1).unwrap();
    large(b.path(), 50, 2).unwrap();
    assert_ne!(digest(a.path()), digest(b.path()));
}

#[test]
fn refuses_a_folder_with_files_in_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("mine.jpg"), b"not yours").unwrap();
    assert!(trip(dir.path()).is_err());
    assert!(large(dir.path(), 5, 1).is_err());
    assert!(extras(dir.path()).is_err());
    assert_eq!(fs::read(dir.path().join("mine.jpg")).unwrap(), b"not yours");
}

#[test]
fn extras_cover_the_handler_and_walker_cases() {
    let dir = tempfile::tempdir().unwrap();
    let album = extras(dir.path()).unwrap();
    let listed: BTreeSet<String> = album.files.iter().map(|f| f.rel_path.clone()).collect();
    assert_eq!(listed, files_under(&album.root));
    for kind in [
        Kind::Orientation,
        Kind::Animated,
        Kind::Transparent,
        Kind::WideGamut,
        Kind::SixteenBit,
        Kind::WrongExtension,
        Kind::Unsupported,
        Kind::Corrupt,
        Kind::TooLarge,
        Kind::Video,
        Kind::Skipped,
        Kind::Nested,
    ] {
        assert!(
            album.files.iter().any(|f| f.kind == kind),
            "extras has no {kind:?}"
        );
    }
    assert_eq!(album.of_kind(Kind::Orientation).count(), 8);
    assert!(album.root.join("nested/.owlbum/owlbum.sqlite").is_file());
}

#[test]
fn orientations_turn_upright() {
    let dir = tempfile::tempdir().unwrap();
    let album = extras(dir.path()).unwrap();
    for n in 1..=8u8 {
        let f = album.get(&format!("orientation/{n}.jpg")).unwrap();
        let mut img = image::load_from_memory(&fs::read(album.root.join(&f.rel_path)).unwrap())
            .unwrap()
            .to_rgb8();
        img = owlbum_fixtures::picture::apply_orientation(img, n);
        assert_eq!((img.width(), img.height()), (f.width, f.height), "{n}");
        // Upright: red top-left, green top-right, blue bottom-left, white
        // bottom-right.
        let at = |x: u32, y: u32| img.get_pixel(x, y).0;
        let near = |p: [u8; 3], q: [u8; 3]| p.iter().zip(q).all(|(a, b)| a.abs_diff(b) < 40);
        let (w, h) = (img.width(), img.height());
        assert!(near(at(4, 4), [255, 0, 0]), "{n} top-left {:?}", at(4, 4));
        assert!(near(at(w - 5, 4), [0, 255, 0]), "{n} top-right");
        assert!(near(at(4, h - 5), [0, 0, 255]), "{n} bottom-left");
        assert!(near(at(w - 5, h - 5), [255, 255, 255]), "{n} bottom-right");
    }
}

#[test]
fn extras_pictures_decode() {
    use image::AnimationDecoder;
    let dir = tempfile::tempdir().unwrap();
    let album = extras(dir.path()).unwrap();
    let read = |rel: &str| fs::read(album.root.join(rel)).unwrap();

    let gif =
        image::codecs::gif::GifDecoder::new(std::io::Cursor::new(read("animated/animated.gif")))
            .unwrap();
    assert_eq!(gif.into_frames().count(), 3);
    let webp =
        image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(read("animated/animated.webp")))
            .unwrap();
    assert!(webp.has_animation());
    assert_eq!(webp.into_frames().count(), 3);
    let png =
        image::codecs::png::PngDecoder::new(std::io::Cursor::new(read("animated/animated.png")))
            .unwrap();
    assert!(png.is_apng().unwrap());
    assert_eq!(png.apng().unwrap().into_frames().count(), 3);

    let transparent = image::load_from_memory(&read("png/transparent.png")).unwrap();
    assert!(transparent.color().has_alpha());
    let deep = image::load_from_memory(&read("png/16-bit.png")).unwrap();
    assert_eq!(deep.color(), image::ColorType::Rgb16);
    let wide = png::Decoder::new(std::io::Cursor::new(read("png/wide-gamut.png")))
        .read_info()
        .unwrap();
    let icc = wide.info().icc_profile.as_ref().expect("an ICC profile");
    assert_eq!(&icc[36..40], b"acsp");

    // A PNG under a .jpg name is still a PNG.
    assert_eq!(
        image::guess_format(&read("lies/png-named.jpg")).unwrap(),
        image::ImageFormat::Png
    );
    // A HEIC signature under a .jpg name.
    assert_eq!(&read("lies/heic-named.jpg")[4..12], b"ftypheic");
    // A header claiming more than 300,000,000 pixels.
    let mut big = png::Decoder::new(std::io::Cursor::new(read("lies/too-large.png")));
    let header = big.read_header_info().unwrap();
    assert!(u64::from(header.width) * u64::from(header.height) > 300_000_000);
    assert!(read("lies/too-large.png").len() < 1024);
    assert!(image::load_from_memory(&read("lies/random.jpg")).is_err());
}

#[test]
fn rotated_video_is_marked_rotated() {
    use nom_exif::{TrackInfoTag, read_track};
    let dir = tempfile::tempdir().unwrap();
    let album = extras(dir.path()).unwrap();
    let f = album.get("video/rotated.mp4").unwrap();
    let info = read_track(album.root.join(&f.rel_path)).unwrap();
    // Stored landscape; the manifest gives the displayed (portrait) size.
    let stored_w = info.get(TrackInfoTag::Width).and_then(|v| v.as_u32());
    assert_eq!(stored_w, Some(f.height));
    let bytes = fs::read(album.root.join(&f.rel_path)).unwrap();
    // The track header's matrix holds a quarter turn: a=0, b=1.0.
    let tkhd = bytes.windows(4).position(|w| w == b"tkhd").unwrap();
    let matrix = tkhd + 4 + 4 + 4 * 5 + 8 + 2 * 4;
    assert_eq!(&bytes[matrix..matrix + 8], &[0, 0, 0, 0, 0, 1, 0, 0]);
}
