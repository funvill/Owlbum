//! The `trip` album: three days of a made-up trip, about two dozen files.

use std::io;
use std::path::Path;

use image::RgbImage;

use crate::encode::{self, Meta};
use crate::picture::{self, scene};
use crate::rng::Rng;
use crate::{Album, Fixture, Kind, Writer, degrees, dng, iso, mp4};

const QUALITY: u8 = 88;

struct Camera {
    make: &'static str,
    model: &'static str,
    lens: Option<&'static str>,
}

const CAMERA: Camera = Camera {
    make: "Owlbum",
    model: "Fixture One",
    lens: Some("Fixture 24-70mm F2.8"),
};
const PHONE: Camera = Camera {
    make: "Owlbum",
    model: "Pocket 2",
    lens: None,
};

fn meta(cam: &Camera, taken: &str, gps: Option<(i32, i32)>, (w, h): (u32, u32)) -> Meta {
    Meta {
        make: Some(cam.make),
        model: Some(cam.model),
        lens: cam.lens,
        orientation: 1,
        taken: Some(taken.to_string()),
        offset: Some("-07:00"),
        exposure: Some([1, 250]),
        f_number_x10: Some(if cam.lens.is_some() { 56 } else { 18 }),
        iso: Some(100),
        focal_mm: Some(if cam.lens.is_some() { 35 } else { 5 }),
        flash: Some(16),
        gps,
        pixels: Some((w, h)),
    }
}

/// A JPEG photo with camera metadata; returns the fixture and its bytes.
fn photo(
    rel: &str,
    kind: Kind,
    img: &RgbImage,
    cam: &Camera,
    taken: &str,
    gps: Option<(i32, i32)>,
) -> (Fixture, Vec<u8>) {
    let m = meta(cam, taken, gps, img.dimensions());
    let bytes = encode::jpeg(img, QUALITY, &m);
    let mut f = Fixture::new(rel, kind, Some("jpeg")).size(img.width(), img.height());
    f.taken = Some(iso(taken));
    f.gps = degrees(gps);
    (f, bytes)
}

const LAKE_GPS: (i32, i32) = (49_341_210, -123_002_330);
const HARBOUR_GPS: (i32, i32) = (49_288_400, -123_111_300);
const PARK_GPS: (i32, i32) = (49_300_000, -123_140_000);
const RIDGE_GPS: (i32, i32) = (49_350_000, -122_950_000);

/// Write the `trip` album into `root` (which must be missing or empty).
pub fn trip(root: &Path) -> io::Result<Album> {
    let mut w = Writer::new(root)?;

    // Day 1: mixed shapes, a panorama.
    let lake = picture::lake(640, 480);
    let (f, lake_bytes) = photo(
        "day1/lake.jpg",
        Kind::Landscape,
        &lake,
        &CAMERA,
        "2025:07:14 09:12:00",
        Some(LAKE_GPS),
    );
    w.put(f, &lake_bytes)?;
    let (f, b) = photo(
        "day1/IMG_0002.jpg",
        Kind::Portrait,
        &scene(0x0002, 480, 640),
        &CAMERA,
        "2025:07:14 11:30:00",
        None,
    );
    w.put(f, &b)?;
    let (f, b) = photo(
        "day1/IMG_0003.jpg",
        Kind::Square,
        &scene(0x0003, 480, 480),
        &PHONE,
        "2025:07:14 14:05:00",
        Some(PARK_GPS),
    );
    w.put(f, &b)?;
    let (f, b) = photo(
        "day1/panorama.jpg",
        Kind::Panorama,
        &scene(0x0004, 2400, 400),
        &PHONE,
        "2025:07:14 17:40:00",
        Some(RIDGE_GPS),
    );
    w.put(f, &b)?;

    // Day 2: a burst of five, a harbour, a blurred frame, a sideways photo.
    let base = scene(0x0010, 640, 480);
    for i in 0..5u8 {
        let shot = picture::nudge(&base, i64::from(i) * 3, i64::from(i), i * 2);
        let (mut f, b) = photo(
            &format!("day2/burst/IMG_00{}.jpg", 10 + i),
            Kind::Burst,
            &shot,
            &CAMERA,
            &format!("2025:07:15 10:00:0{i}"),
            None,
        );
        f.burst = Some(1);
        w.put(f, &b)?;
    }
    let (f, b) = photo(
        "day2/harbour.jpg",
        Kind::Landscape,
        &picture::harbour(640, 480),
        &PHONE,
        "2025:07:15 13:20:00",
        Some(HARBOUR_GPS),
    );
    w.put(f, &b)?;
    let (f, b) = photo(
        "day2/blurred.jpg",
        Kind::Blurred,
        &picture::blur(&scene(0x0020, 640, 480), 8),
        &CAMERA,
        "2025:07:15 16:45:00",
        None,
    );
    w.put(f, &b)?;
    let upright = scene(0x0021, 480, 640);
    let stored = picture::store_for_orientation(&upright, 6);
    let mut m = meta(&PHONE, "2025:07:15 19:10:00", None, stored.dimensions());
    m.orientation = 6;
    let mut f = Fixture::new("day2/rotated.jpg", Kind::ExifRotated, Some("jpeg")).size(480, 640);
    f.taken = Some(iso("2025:07:15 19:10:00"));
    w.put(f, &encode::jpeg(&stored, QUALITY, &m))?;

    // Day 3: a tiny photo, a PNG with nothing but pixels, the DNG.
    let (f, b) = photo(
        "day3/tiny.jpg",
        Kind::LowRes,
        &scene(0x0030, 64, 48),
        &PHONE,
        "2025:07:16 08:00:00",
        None,
    );
    w.put(f, &b)?;
    let plain = scene(0x0031, 320, 240);
    w.put(
        Fixture::new("day3/plain.png", Kind::NoMetadata, Some("png")).size(320, 240),
        &encode::png_rgb(&plain),
    )?;
    let raw_meta = meta(&CAMERA, "2025:07:16 10:30:00", None, (128, 96));
    let mut f = Fixture::new("day3/sample.dng", Kind::Raw, Some("dng")).size(128, 96);
    f.taken = Some(iso("2025:07:16 10:30:00"));
    w.put(f, &dng::dng(&scene(0x0032, 128, 96), &raw_meta))?;

    // An exact copy of the first photo, in another folder.
    let mut f = Fixture::new("copies/lake.jpg", Kind::Duplicate, Some("jpeg")).size(640, 480);
    f.taken = Some(iso("2025:07:14 09:12:00"));
    f.gps = degrees(Some(LAKE_GPS));
    w.put(f, &lake_bytes)?;

    // One still per remaining supported format, no metadata.
    type Encoder = fn(&RgbImage) -> Vec<u8>;
    let formats: [(&str, &str, Encoder); 5] = [
        ("formats/sample.webp", "webp", encode::webp),
        ("formats/sample.gif", "gif", encode::gif),
        ("formats/sample.tif", "tiff", encode::tiff),
        ("formats/sample.bmp", "bmp", encode::bmp),
        ("formats/sample.jxl", "jxl", encode::jxl),
    ];
    for (i, (rel, id, enc)) in formats.into_iter().enumerate() {
        let img = scene(0x0040 + i as u64, 320, 240);
        w.put(
            Fixture::new(rel, Kind::Format, Some(id)).size(320, 240),
            &enc(&img),
        )?;
    }

    // A two-second video.
    w.put(
        Fixture::new("video/clip.mp4", Kind::Video, Some("mp4")).size(160, 120),
        &clip(0x0050, false),
    )?;

    // Files that cannot be shown: undecodable bytes, and a HEIC.
    let mut junk = vec![0u8; 2048];
    Rng::new(0x0060).fill(&mut junk);
    let mut corrupt = vec![0xff, 0xd8, 0xff, 0xe0];
    corrupt.extend_from_slice(&junk);
    w.put(
        Fixture::new("problems/corrupt.jpg", Kind::Corrupt, None),
        &corrupt,
    )?;
    w.put(
        Fixture::new("problems/photo.heic", Kind::Unsupported, None),
        &encode::heic_signature(&junk[..512]),
    )?;

    Ok(w.finish())
}

/// Eight 160 × 120 frames of a panning scene, 250 ms each.
pub(crate) fn clip(seed: u64, quarter_turn: bool) -> Vec<u8> {
    let base = scene(seed, 160, 120);
    let frames: Vec<Vec<u8>> = (0..8)
        .map(|i| {
            let f = picture::nudge(&base, i * 6, 0, 0);
            encode::jpeg(&f, 80, &Meta::default())
        })
        .collect();
    mp4::mjpeg(&frames, 160, 120, 250, quarter_turn)
}
