//! Handler tests (docs/03-media-and-scanning.md, "Tests": Handlers).
//!
//! Every picture is generated here, except the JPEG XL files under
//! `testdata/media/`: no JPEG XL encoder exists in Rust, so those few
//! hundred bytes were made once with ImageMagick (libjxl) and committed.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::{Delay, Frame, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use tempfile::TempDir;

use super::*;

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255, 255, 255];

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/media")
        .join(name)
}

/// The handler `identify` picks for a file on disk.
fn handler_for(path: &Path) -> &'static MediaFormat {
    let head = std::fs::read(path).unwrap();
    let head = &head[..head.len().min(16 * 1024)];
    let name = path.file_name().unwrap().to_str().unwrap();
    match identify(name, head).expect("a supported name") {
        Identified::Format(f) => f,
        Identified::Unsupported(u) => panic!("{name} identified as {}", u.label),
    }
}

/// 32 × 16: red top left, green top right, blue bottom left, white bottom
/// right.
fn quadrants() -> RgbImage {
    RgbImage::from_fn(32, 16, |x, y| {
        Rgb(match (x < 16, y < 8) {
            (true, true) => RED,
            (false, true) => GREEN,
            (true, false) => BLUE,
            (false, false) => WHITE,
        })
    })
}

/// The colour at the centre of each quadrant: TL, TR, BL, BR.
fn corners(p: &Pixels) -> [[u8; 3]; 4] {
    let (w, h) = (p.width, p.height);
    let at = |x: u32, y: u32| {
        let c = p.pixel(x, y);
        [c[0], c[1], c[2]]
    };
    [
        at(w / 4, h / 4),
        at(3 * w / 4, h / 4),
        at(w / 4, 3 * h / 4),
        at(3 * w / 4, 3 * h / 4),
    ]
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 48)
}

fn assert_corners(p: &Pixels, want: [[u8; 3]; 4], what: &str) {
    let got = corners(p);
    assert!(
        got.iter().zip(want).all(|(g, w)| close(*g, w)),
        "{what}: corners {got:?}, want {want:?}"
    );
}

fn jpeg_bytes(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(img)
        .unwrap();
    out
}

fn encode(img: &image::DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

/// A little-endian EXIF block holding only an Orientation tag.
fn exif_orientation(o: u16) -> Vec<u8> {
    let mut e = b"II*\0".to_vec();
    e.extend_from_slice(&8u32.to_le_bytes());
    e.extend_from_slice(&1u16.to_le_bytes()); // one entry
    e.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
    e.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    e.extend_from_slice(&1u32.to_le_bytes()); // count
    e.extend_from_slice(&o.to_le_bytes());
    e.extend_from_slice(&[0, 0]);
    e.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
    e
}

/// A JPEG with an APP1 Exif segment straight after SOI.
fn jpeg_with_orientation(img: &RgbImage, o: u16) -> Vec<u8> {
    let plain = jpeg_bytes(img);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&exif_orientation(o));
    let mut out = plain[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&plain[2..]);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = (data.len() as u32).to_be_bytes().to_vec();
    c.extend_from_slice(kind);
    c.extend_from_slice(data);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(data);
    c.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    c
}

/// A PNG with `chunk` inserted straight after IHDR.
fn png_with_chunk(png: &[u8], chunk: &[u8]) -> Vec<u8> {
    let ihdr_end = 8 + 8 + 13 + 4;
    let mut out = png[..ihdr_end].to_vec();
    out.extend_from_slice(chunk);
    out.extend_from_slice(&png[ihdr_end..]);
    out
}

// ---- One file per format decodes upright at the expected size ----

#[test]
fn each_still_format_decodes_at_its_size() {
    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(quadrants());
    let files = [
        ("a.jpg", jpeg_bytes(&quadrants()), "jpeg"),
        ("a.png", encode(&img, ImageFormat::Png), "png"),
        ("a.gif", encode(&img, ImageFormat::Gif), "gif"),
        ("a.tif", encode(&img, ImageFormat::Tiff), "tiff"),
        ("a.bmp", encode(&img, ImageFormat::Bmp), "bmp"),
        ("a.webp", encode(&img, ImageFormat::WebP), "webp"),
    ];
    for (name, bytes, id) in files {
        let path = write(&dir, name, &bytes);
        let format = handler_for(&path);
        assert_eq!(format.id, id, "{name}");
        let meta = format.handler.metadata(&path).unwrap();
        assert_eq!((meta.width, meta.height), (32, 16), "{name}");
        assert_eq!(meta.frames, None, "{name}");
        assert_eq!(meta.duration_ms, None, "{name}");
        let px = format.handler.working(&path).unwrap();
        assert_eq!((px.width, px.height), (32, 16), "{name}");
        assert_eq!((px.full_width, px.full_height), (32, 16), "{name}");
        assert_eq!(px.rgba.len(), 32 * 16 * 4, "{name}");
        assert_corners(&px, [RED, GREEN, BLUE, WHITE], name);
        let preview = format.handler.preview(&path, 1600).unwrap();
        assert_eq!(preview, px, "{name}: preview is the same decode");
        assert_eq!(format.handler.probe(&path).unwrap(), None, "{name}");
    }
}

#[test]
fn jpeg_xl_decodes_at_its_size() {
    let path = testdata("quadrants.jxl");
    let format = handler_for(&path);
    assert_eq!(format.id, "jxl");
    let meta = format.handler.metadata(&path).unwrap();
    assert_eq!((meta.width, meta.height), (32, 16));
    let px = format.handler.working(&path).unwrap();
    assert_eq!((px.width, px.height), (32, 16));
    assert!(!px.has_alpha);
    assert_corners(&px, [RED, GREEN, BLUE, WHITE], "quadrants.jxl");
    assert_eq!(format.handler.probe(&path).unwrap(), None);
}

// ---- Orientation ----

/// The upright corners for each EXIF orientation of `quadrants()`.
fn upright_corners(o: u16) -> [[u8; 3]; 4] {
    let (r, g, b, w) = (RED, GREEN, BLUE, WHITE);
    match o {
        1 => [r, g, b, w],
        2 => [g, r, w, b], // mirrored left to right
        3 => [w, b, g, r], // turned 180
        4 => [b, w, r, g], // mirrored top to bottom
        5 => [r, b, g, w], // mirrored about the main diagonal
        6 => [b, r, w, g], // turned 90 clockwise
        7 => [w, g, b, r], // mirrored about the other diagonal
        8 => [g, w, r, b], // turned 90 anticlockwise
        _ => unreachable!(),
    }
}

#[test]
fn all_eight_exif_orientations_are_applied() {
    let dir = TempDir::new().unwrap();
    for o in 1..=8u16 {
        let path = write(
            &dir,
            &format!("o{o}.jpg"),
            &jpeg_with_orientation(&quadrants(), o),
        );
        let format = handler_for(&path);
        let want_size = if o >= 5 { (16, 32) } else { (32, 16) };
        let meta = format.handler.metadata(&path).unwrap();
        assert_eq!((meta.width, meta.height), want_size, "orientation {o}");
        assert!(meta.exif.is_some(), "orientation {o}: EXIF block kept");
        let px = format.handler.working(&path).unwrap();
        assert_eq!((px.width, px.height), want_size, "orientation {o}");
        assert_corners(&px, upright_corners(o), &format!("orientation {o}"));
        let src = format.handler.source(&path).unwrap();
        assert_eq!((src.width, src.height), want_size, "orientation {o}");
    }
}

#[test]
fn png_exif_orientation_is_applied() {
    let dir = TempDir::new().unwrap();
    let png = encode(
        &image::DynamicImage::ImageRgb8(quadrants()),
        ImageFormat::Png,
    );
    let png = png_with_chunk(&png, &png_chunk(b"eXIf", &exif_orientation(6)));
    let path = write(&dir, "turned.png", &png);
    let format = handler_for(&path);
    let meta = format.handler.metadata(&path).unwrap();
    assert_eq!((meta.width, meta.height), (16, 32));
    let px = format.handler.working(&path).unwrap();
    assert_corners(&px, upright_corners(6), "turned.png");
}

#[test]
fn jpeg_xl_orientation_is_applied() {
    let path = testdata("quadrants-orient6.jxl");
    let format = handler_for(&path);
    let meta = format.handler.metadata(&path).unwrap();
    assert_eq!((meta.width, meta.height), (16, 32));
    let px = format.handler.working(&path).unwrap();
    assert_eq!((px.width, px.height), (16, 32));
    assert_corners(&px, upright_corners(6), "quadrants-orient6.jxl");
}

// ---- Alpha ----

fn half_transparent() -> RgbaImage {
    RgbaImage::from_fn(8, 8, |x, _| {
        if x < 4 {
            Rgba([255, 0, 0, 0])
        } else {
            Rgba([0, 0, 255, 255])
        }
    })
}

#[test]
fn alpha_is_reported() {
    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgba8(half_transparent());
    for (name, format) in [("a.png", ImageFormat::Png), ("a.webp", ImageFormat::WebP)] {
        let path = write(&dir, name, &encode(&img, format));
        let f = handler_for(&path);
        assert!(f.handler.metadata(&path).unwrap().has_alpha, "{name}");
        let px = f.handler.working(&path).unwrap();
        assert!(px.has_alpha, "{name}");
        assert_eq!(px.pixel(1, 1)[3], 0, "{name}");
        assert_eq!(px.pixel(6, 1), [0, 0, 255, 255], "{name}");
    }
    let jxl = testdata("alpha.jxl");
    let f = handler_for(&jxl);
    assert!(f.handler.metadata(&jxl).unwrap().has_alpha);
    let px = f.handler.working(&jxl).unwrap();
    assert!(px.has_alpha);
    assert_eq!(px.pixel(1, 1)[3], 0);
    assert_eq!(px.pixel(6, 1), [0, 0, 255, 255]);
}

#[test]
fn a_source_without_alpha_is_opaque() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "a.jpg", &jpeg_bytes(&quadrants()));
    let px = handler_for(&path).handler.working(&path).unwrap();
    assert!(!px.has_alpha);
    assert!(px.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}

// ---- Bit depth: working is 8-bit, source keeps 16 ----

#[test]
fn sixteen_bit_png_keeps_its_depth_in_source_only() {
    let dir = TempDir::new().unwrap();
    let img = image::ImageBuffer::<Rgb<u16>, _>::from_fn(4, 2, |x, _| {
        Rgb([x as u16 * 1000 + 1, 2, 65535])
    });
    let path = write(
        &dir,
        "deep.png",
        &encode(&image::DynamicImage::ImageRgb16(img), ImageFormat::Png),
    );
    let f = handler_for(&path);
    let px = f.handler.working(&path).unwrap();
    assert_eq!(px.rgba.len(), 4 * 2 * 4);
    let src = f.handler.source(&path).unwrap();
    assert_eq!(src.layout, Layout::Rgb);
    assert_eq!(src.samples.bit_depth(), 16);
    let Samples::U16(s) = &src.samples else {
        unreachable!()
    };
    assert_eq!(&s[..6], &[1, 2, 65535, 1001, 2, 65535]);
}

#[test]
fn source_keeps_eight_bit_and_greyscale_as_they_are() {
    let dir = TempDir::new().unwrap();
    let grey = image::GrayImage::from_fn(4, 4, |x, _| image::Luma([x as u8 * 60]));
    let path = write(
        &dir,
        "grey.png",
        &encode(&image::DynamicImage::ImageLuma8(grey), ImageFormat::Png),
    );
    let src = handler_for(&path).handler.source(&path).unwrap();
    assert_eq!(src.layout, Layout::Grey);
    assert_eq!(
        src.samples,
        Samples::U8((0..16).map(|i| (i % 4) as u8 * 60).collect())
    );
    assert_eq!(src.icc, None);
}

#[test]
fn jpeg_xl_source_keeps_sixteen_bits() {
    let path = testdata("grey16.jxl");
    let f = handler_for(&path);
    let src = f.handler.source(&path).unwrap();
    assert_eq!((src.width, src.height), (16, 16));
    assert_eq!(src.samples.bit_depth(), 16);
    assert_eq!(src.samples.len(), 16 * 16 * src.layout.channels());
    let px = f.handler.working(&path).unwrap();
    assert_eq!(px.rgba.len(), 16 * 16 * 4);
}

// ---- Animated pictures: frame count, loop length, frame 1 ----

fn solid(c: [u8; 3]) -> RgbaImage {
    RgbaImage::from_pixel(8, 8, Rgba([c[0], c[1], c[2], 255]))
}

#[test]
fn animated_gif_reports_frames_and_shows_frame_one() {
    let dir = TempDir::new().unwrap();
    let mut bytes = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut bytes);
        enc.encode_frames([
            Frame::from_parts(solid(RED), 0, 0, Delay::from_numer_denom_ms(100, 1)),
            Frame::from_parts(solid(BLUE), 0, 0, Delay::from_numer_denom_ms(200, 1)),
            Frame::from_parts(solid(GREEN), 0, 0, Delay::from_numer_denom_ms(50, 1)),
        ])
        .unwrap();
    }
    let path = write(&dir, "anim.gif", &bytes);
    let f = handler_for(&path);
    let meta = f.handler.metadata(&path).unwrap();
    assert_eq!(meta.frames, Some(3));
    assert_eq!(meta.duration_ms, Some(350));
    let px = f.handler.working(&path).unwrap();
    assert_eq!(px.pixel(3, 3), [255, 0, 0, 255]);
}

/// The `VP8L` chunk's payload of a lossless WebP made by `image`.
fn vp8l_payload(img: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    WebPEncoder::new_lossless(&mut bytes)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
        if &bytes[i..i + 4] == b"VP8L" {
            return bytes[i + 8..i + 8 + size].to_vec();
        }
        i += 8 + size + (size & 1);
    }
    panic!("no VP8L chunk");
}

fn riff_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = kind.to_vec();
    c.extend_from_slice(&(data.len() as u32).to_le_bytes());
    c.extend_from_slice(data);
    if data.len() % 2 == 1 {
        c.push(0);
    }
    c
}

fn u24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// An animated WebP of 8 × 8 frames with the given colours and durations.
fn animated_webp(frames: &[([u8; 3], u32)]) -> Vec<u8> {
    let mut vp8x = vec![0x12, 0, 0, 0]; // animation and alpha flags
    vp8x.extend_from_slice(&u24(7));
    vp8x.extend_from_slice(&u24(7));
    let mut body = b"WEBP".to_vec();
    body.extend(riff_chunk(b"VP8X", &vp8x));
    body.extend(riff_chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
    for (colour, ms) in frames {
        let mut anmf = Vec::new();
        anmf.extend_from_slice(&u24(0));
        anmf.extend_from_slice(&u24(0));
        anmf.extend_from_slice(&u24(7));
        anmf.extend_from_slice(&u24(7));
        anmf.extend_from_slice(&u24(*ms));
        anmf.push(0);
        anmf.extend(riff_chunk(b"VP8L", &vp8l_payload(&solid(*colour))));
        body.extend(riff_chunk(b"ANMF", &anmf));
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

#[test]
fn animated_webp_reports_frames_and_shows_frame_one() {
    let dir = TempDir::new().unwrap();
    let path = write(
        &dir,
        "anim.webp",
        &animated_webp(&[(RED, 100), (BLUE, 250)]),
    );
    let f = handler_for(&path);
    let meta = f.handler.metadata(&path).unwrap();
    assert_eq!(meta.frames, Some(2));
    assert_eq!(meta.duration_ms, Some(350));
    assert_eq!((meta.width, meta.height), (8, 8));
    let px = f.handler.working(&path).unwrap();
    let c = px.pixel(3, 3);
    assert!(close([c[0], c[1], c[2]], RED) && c[3] == 255, "{c:?}");
}

#[test]
fn animated_png_reports_frames_but_stays_a_still() {
    let dir = TempDir::new().unwrap();
    let png = encode(
        &image::DynamicImage::ImageRgba8(solid(RED)),
        ImageFormat::Png,
    );
    let mut actl = 2u32.to_be_bytes().to_vec(); // frames
    actl.extend_from_slice(&0u32.to_be_bytes()); // plays forever
    let path = write(
        &dir,
        "anim.png",
        &png_with_chunk(&png, &png_chunk(b"acTL", &actl)),
    );
    let f = handler_for(&path);
    let meta = f.handler.metadata(&path).unwrap();
    assert_eq!(meta.frames, Some(2));
    assert_eq!(
        meta.duration_ms, None,
        "an animated PNG is shown as a still"
    );
    let px = f.handler.working(&path).unwrap();
    assert_eq!(px.pixel(3, 3), [255, 0, 0, 255]);
}

// ---- When the extension lies ----

#[test]
fn a_png_named_jpg_reads_as_png() {
    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(quadrants());
    let path = write(&dir, "liar.jpg", &encode(&img, ImageFormat::Png));
    let f = handler_for(&path);
    assert_eq!(f.id, "png");
    let px = f.handler.working(&path).unwrap();
    assert_corners(&px, [RED, GREEN, BLUE, WHITE], "liar.jpg");
}

#[test]
fn a_heic_named_jpg_is_unsupported() {
    let mut heic = 24u32.to_be_bytes().to_vec();
    heic.extend_from_slice(b"ftypheic\0\0\0\0mif1heic");
    heic.extend_from_slice(&[0; 64]);
    match identify("IMG_1.jpg", &heic) {
        Some(Identified::Unsupported(u)) => {
            assert_eq!(u.label, "HEIC");
            assert_eq!(u.reason, crate::text::media::HEIC_UNSUPPORTED);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn random_bytes_give_decode() {
    let dir = TempDir::new().unwrap();
    // A fixed pseudo-random sequence that matches no signature.
    let mut x = 0x2545_F491u32;
    let bytes: Vec<u8> = (0..4096)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect();
    for name in [
        "noise.png",
        "noise.jpg",
        "noise.gif",
        "noise.webp",
        "noise.tif",
        "noise.bmp",
        "noise.jxl",
    ] {
        let path = write(&dir, name, &bytes);
        let f = handler_for(&path);
        assert_eq!(
            f.extensions[0],
            extension_of(name),
            "{name}: the extension's own format"
        );
        let e = f.handler.working(&path).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::Decode, "{name}");
        assert_eq!(
            e.message,
            format!("This file is damaged or is not a {} picture.", f.label)
        );
        let e = f.handler.metadata(&path).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::Decode, "{name}");
    }
}

#[test]
fn a_missing_file_gives_io() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("gone.png");
    for f in [format_by_id("png").unwrap(), format_by_id("jxl").unwrap()] {
        let e = f.handler.working(&path).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::Io);
        assert!(
            e.message.starts_with("This file could not be read: "),
            "{}",
            e.message
        );
    }
}

// ---- Size limit: refused on the header, before allocating ----

#[test]
fn a_huge_png_header_gives_too_large() {
    let dir = TempDir::new().unwrap();
    let mut ihdr = 20_000u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &[0x78, 0x9C, 0x03, 0x00]));
    png.extend(png_chunk(b"IEND", &[]));
    let path = write(&dir, "huge.png", &png);
    let f = handler_for(&path);
    for e in [
        f.handler.metadata(&path).unwrap_err(),
        f.handler.working(&path).unwrap_err(),
        f.handler.source(&path).map(|_| ()).unwrap_err(),
    ] {
        assert_eq!(e.code, MediaErrorCode::TooLarge);
        assert_eq!(
            e.message,
            "This picture is too large to open (20000 × 20000)."
        );
    }
}

#[test]
fn a_huge_jpeg_header_gives_too_large() {
    let dir = TempDir::new().unwrap();
    let mut jpeg = vec![
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 8, 0xFF, 0xFF, 0xFF, 0xFF, 3,
    ];
    jpeg.extend_from_slice(&[1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    jpeg.extend_from_slice(&[0xFF, 0xD9]);
    let path = write(&dir, "huge.jpg", &jpeg);
    let f = handler_for(&path);
    for e in [
        f.handler.metadata(&path).unwrap_err(),
        f.handler.working(&path).unwrap_err(),
    ] {
        assert_eq!(e.code, MediaErrorCode::TooLarge, "{e:?}");
    }
}

#[test]
fn wide_pictures_are_not_capped_by_a_decoder() {
    // Only Owlbum's own pixel limit refuses a picture: a long, thin one well
    // under it decodes in full.
    let dir = TempDir::new().unwrap();
    let img = RgbImage::from_fn(20_000, 8, |x, _| Rgb([(x % 256) as u8, 0, 0]));
    let files = [
        ("wide.jpg", jpeg_bytes(&img)),
        (
            "wide.png",
            encode(&image::DynamicImage::ImageRgb8(img), ImageFormat::Png),
        ),
    ];
    for (name, bytes) in files {
        let path = write(&dir, name, &bytes);
        let px = handler_for(&path).handler.working(&path).unwrap();
        assert_eq!((px.width, px.height), (20_000, 8), "{name}");
    }
}

// ---- A large JPEG's preview is decoded at a reduced scale ----

/// `quadrants()` at 3600 × 1800, big enough for a scaled preview.
fn big_quadrants() -> RgbImage {
    RgbImage::from_fn(3600, 1800, |x, y| {
        Rgb(match (x < 1800, y < 900) {
            (true, true) => RED,
            (false, true) => GREEN,
            (true, false) => BLUE,
            (false, false) => WHITE,
        })
    })
}

#[test]
fn a_large_jpeg_preview_is_decoded_at_the_smallest_scale_that_fits() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "big.jpg", &jpeg_bytes(&big_quadrants()));
    let h = handler_for(&path).handler;
    // 1/2 is 1800 px, the smallest eighth still at least 1600.
    let px = h.preview(&path, 1600).unwrap();
    assert_eq!((px.width, px.height), (1800, 900));
    assert_eq!((px.full_width, px.full_height), (3600, 1800));
    assert_eq!(px.rgba.len(), 1800 * 900 * 4);
    assert!(!px.has_alpha);
    assert_corners(&px, [RED, GREEN, BLUE, WHITE], "1/2");
    // 1/4 is 900 px, at least 512; 1/8 would be 450.
    let px = h.preview(&path, 512).unwrap();
    assert_eq!((px.width, px.height), (900, 450));
    assert_eq!((px.full_width, px.full_height), (3600, 1800));
    assert_corners(&px, [RED, GREEN, BLUE, WHITE], "1/4");
    // Too small to scale: the full decode, as `working`.
    assert_eq!(h.preview(&path, 2000).unwrap(), h.working(&path).unwrap());
}

#[test]
fn a_scaled_jpeg_preview_is_upright() {
    let dir = TempDir::new().unwrap();
    for o in [3u16, 5, 6, 8] {
        let path = write(
            &dir,
            &format!("big{o}.jpg"),
            &jpeg_with_orientation(&big_quadrants(), o),
        );
        let px = handler_for(&path).handler.preview(&path, 1600).unwrap();
        let what = format!("orientation {o}");
        if o >= 5 {
            assert_eq!((px.width, px.height), (900, 1800), "{what}");
            assert_eq!((px.full_width, px.full_height), (1800, 3600), "{what}");
        } else {
            assert_eq!((px.width, px.height), (1800, 900), "{what}");
            assert_eq!((px.full_width, px.full_height), (3600, 1800), "{what}");
        }
        assert_corners(&px, upright_corners(o), &what);
    }
}

#[test]
fn a_scaled_jpeg_preview_is_converted_to_srgb() {
    use owlbum_fixtures::icc;

    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(RgbImage::from_pixel(2000, 1000, Rgb(ORANGE)));
    let p3 = icc::DISPLAY_P3.bytes();
    let path = write(&dir, "p3.jpg", &with_icc(&img, ImageFormat::Jpeg, &p3));
    let px = handler_for(&path).handler.preview(&path, 512).unwrap();
    assert_eq!((px.width, px.height), (1000, 500));
    let want = icc::DISPLAY_P3.expected_srgb(ORANGE);
    assert_near(px.pixel(500, 250), want, 5.0, "p3 preview");
}

#[test]
fn a_scaled_grey_jpeg_preview_is_grey() {
    let dir = TempDir::new().unwrap();
    let grey = image::GrayImage::from_fn(2000, 1000, |x, _| {
        image::Luma([if x < 1000 { 40 } else { 200 }])
    });
    let path = write(
        &dir,
        "grey.jpg",
        &encode(&image::DynamicImage::ImageLuma8(grey), ImageFormat::Jpeg),
    );
    let px = handler_for(&path).handler.preview(&path, 512).unwrap();
    assert_eq!((px.width, px.height), (1000, 500));
    assert_eq!((px.full_width, px.full_height), (2000, 1000));
    for (x, v) in [(250, 40u8), (750, 200)] {
        let c = px.pixel(x, 250);
        assert!(c[..3].iter().all(|k| k.abs_diff(v) <= 3), "{x}: {c:?}");
    }
}

#[test]
fn a_truncated_large_jpeg_preview_still_yields_a_picture() {
    let dir = TempDir::new().unwrap();
    let full = jpeg_bytes(&big_quadrants());
    let path = write(&dir, "cut.jpg", &full[..full.len() * 3 / 4]);
    let px = handler_for(&path).handler.preview(&path, 1600).unwrap();
    assert!(px.width.max(px.height) >= 1600);
    assert_eq!((px.full_width, px.full_height), (3600, 1800));
}

// ---- Leniency, files closed, cost ----

#[test]
fn a_truncated_jpeg_still_yields_a_picture() {
    let dir = TempDir::new().unwrap();
    let full = jpeg_bytes(&RgbImage::from_fn(64, 64, |x, y| {
        Rgb([(x * 4) as u8, (y * 4) as u8, 128])
    }));
    let path = write(&dir, "cut.jpg", &full[..full.len() * 3 / 4]);
    let px = handler_for(&path).handler.working(&path).unwrap();
    assert_eq!((px.width, px.height), (64, 64));
}

#[test]
fn files_are_closed_when_a_call_returns() {
    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(quadrants());
    let jpg = write(&dir, "a.jpg", &jpeg_bytes(&quadrants()));
    let gif = write(&dir, "a.gif", &encode(&img, ImageFormat::Gif));
    let jxl = dir.path().join("a.jxl");
    std::fs::copy(testdata("quadrants.jxl"), &jxl).unwrap();
    for path in [jpg, gif, jxl] {
        let f = handler_for(&path);
        f.handler.metadata(&path).unwrap();
        f.handler.working(&path).unwrap();
        f.handler.source(&path).unwrap();
        // Windows refuses to delete a file that is still open.
        std::fs::remove_file(&path).unwrap();
    }
}

#[test]
fn cost_follows_format_and_size() {
    const MB: u64 = 1_000_000;
    let cost = |id: &str, size: u64| format_by_id(id).unwrap().handler.cost(size);
    assert_eq!(cost("jpeg", 50 * MB), Cost::Cheap);
    assert_eq!(cost("png", 7 * MB), Cost::Cheap);
    assert_eq!(cost("png", 9 * MB), Cost::Decode);
    assert_eq!(cost("tiff", 99 * MB), Cost::Decode);
    assert_eq!(cost("tiff", 101 * MB), Cost::Heavy);
    assert_eq!(cost("bmp", 1), Cost::Decode);
    assert_eq!(cost("gif", 1), Cost::Decode);
    assert_eq!(cost("webp", 1), Cost::Decode);
    assert_eq!(cost("jxl", 1), Cost::Decode);
    assert_eq!(cost("cr2", 1), Cost::Heavy);
    assert_eq!(cost("mp4", 1), Cost::Heavy);
}

// ---- Camera RAW ----

fn dng_of(img: &RgbImage, orientation: u16) -> Vec<u8> {
    let meta = owlbum_fixtures::encode::Meta {
        make: Some("Owlbum"),
        model: Some("Test Camera"),
        orientation,
        taken: Some("2024:06:12 14:03:00".into()),
        ..Default::default()
    };
    owlbum_fixtures::dng::dng(img, &meta)
}

#[test]
fn dng_reads_its_size_and_tags_from_the_headers() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "IMG_1.DNG", &dng_of(&quadrants(), 0));
    let f = handler_for(&path);
    assert_eq!(f.id, "dng");
    let m = f.handler.metadata(&path).unwrap();
    assert_eq!((m.width, m.height), (32, 16));
    let exif = m.exif.expect("EXIF");
    assert_eq!(exif.text(exif::Dir::Main, 0x010f), Some("Owlbum"));
    assert_eq!(
        exif.text(exif::Dir::Exif, 0x9003),
        Some("2024:06:12 14:03:00")
    );
    assert_eq!(f.handler.cost(1), Cost::Heavy);
}

#[test]
fn dng_preview_is_the_embedded_picture() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "IMG_1.dng", &dng_of(&quadrants(), 0));
    let px = handler_for(&path).handler.preview(&path, 16).unwrap();
    assert_eq!((px.width, px.height), (32, 16));
    assert_eq!((px.full_width, px.full_height), (32, 16));
    assert_corners(&px, [RED, GREEN, BLUE, WHITE], "embedded preview");
    // Zoom and copy use the embedded picture too: it is the full size.
    let w = handler_for(&path).handler.working(&path).unwrap();
    assert_corners(&w, [RED, GREEN, BLUE, WHITE], "working");
}

#[test]
fn dng_is_developed_when_no_embedded_picture_is_large_enough() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "IMG_1.dng", &dng_of(&quadrants(), 0));
    let px = handler_for(&path).handler.preview(&path, 1600).unwrap();
    assert_eq!((px.width, px.height), (32, 16));
    let s = handler_for(&path).handler.source(&path).unwrap();
    assert_eq!((s.width, s.height, s.layout), (32, 16, Layout::Rgb));
}

#[test]
fn raw_orientation_turns_the_picture_upright() {
    let dir = TempDir::new().unwrap();
    // Orientation 6: turned a quarter clockwise for display.
    let path = write(&dir, "IMG_6.dng", &dng_of(&quadrants(), 6));
    let f = handler_for(&path);
    let m = f.handler.metadata(&path).unwrap();
    assert_eq!((m.width, m.height), (16, 32));
    let px = f.handler.preview(&path, 16).unwrap();
    assert_eq!((px.width, px.height), (16, 32));
    assert_eq!((px.full_width, px.full_height), (16, 32));
    assert_corners(&px, upright_corners(6), "orientation 6");
}

#[test]
fn a_damaged_raw_is_a_decode_error() {
    let dir = TempDir::new().unwrap();
    let mut bytes = b"II*    ".to_vec();
    bytes.extend_from_slice(&[0xff; 64]);
    let path = write(&dir, "IMG_1.CR2", &bytes);
    let f = handler_for(&path);
    assert_eq!(f.id, "cr2");
    let e = f.handler.metadata(&path).unwrap_err();
    assert_eq!(e.code, MediaErrorCode::Decode);
    assert_eq!(
        e.message,
        "This file is damaged or is not a Canon RAW picture."
    );
    assert_eq!(
        f.handler.preview(&path, 512).unwrap_err().code,
        MediaErrorCode::Decode
    );
}

// ---- Video ----

/// An MP4 of four 32 × 16 Motion JPEG frames, 250 ms each.
fn mjpeg_mp4(quarter_turn: bool) -> Vec<u8> {
    let frames: Vec<Vec<u8>> = (0..4).map(|_| jpeg_bytes(&quadrants())).collect();
    owlbum_fixtures::mp4::mjpeg(&frames, 32, 16, 250, quarter_turn)
}

#[test]
fn video_facts_come_from_the_container() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "clip.mp4", &mjpeg_mp4(false));
    let f = handler_for(&path);
    assert_eq!(f.id, "mp4");
    let info = f.handler.probe(&path).unwrap().unwrap();
    assert_eq!(
        info,
        VideoInfo {
            duration_ms: 1000,
            width: 32,
            height: 16,
            rotation: 0,
            video_codec: Some("mjpeg".into()),
            audio_codec: None,
        }
    );
    let m = f.handler.metadata(&path).unwrap();
    assert_eq!((m.width, m.height, m.duration_ms), (32, 16, Some(1000)));
    assert_eq!(f.handler.cost(1), Cost::Heavy);
}

#[test]
fn a_rotated_video_reports_its_displayed_size() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "turned.mov", &mjpeg_mp4(true));
    let f = handler_for(&path);
    assert_eq!(f.id, "mov", "the extension picks the id within a family");
    let info = f.handler.probe(&path).unwrap().unwrap();
    assert_eq!((info.width, info.height, info.rotation), (16, 32, 90));
}

#[test]
fn a_video_container_that_cannot_be_read_is_a_decode_error() {
    let dir = TempDir::new().unwrap();
    let mut mp4 = 20u32.to_be_bytes().to_vec();
    mp4.extend_from_slice(b"ftypisom    isom");
    mp4.extend_from_slice(&[0; 16]);
    let path = write(&dir, "clip.mov", &mp4);
    let f = handler_for(&path);
    let e = f.handler.metadata(&path).unwrap_err();
    assert_eq!(e.code, MediaErrorCode::Decode);
    assert_eq!(
        e.message,
        "This file is damaged or is not a QuickTime video."
    );
}

#[test]
fn a_poster_is_the_system_frame_or_unsupported() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "clip.mp4", &mjpeg_mp4(false));
    match handler_for(&path).handler.preview(&path, 512) {
        // Never enlarged past the video's own size.
        Ok(px) => {
            assert!(px.width <= 32 && px.height <= 16, "{px:?}");
            assert_eq!((px.full_width, px.full_height), (32, 16));
        }
        Err(e) => assert_eq!(e.code, MediaErrorCode::Unsupported),
    }
}

#[test]
fn iso6709_locations() {
    assert_eq!(
        video::iso6709("+48.8577+002.2950+035.000/"),
        Some((48.8577, 2.295))
    );
    assert_eq!(
        video::iso6709("-33.8688+151.2093/"),
        Some((-33.8688, 151.2093))
    );
    assert_eq!(video::iso6709("nonsense"), None);
}

// ---- Colour profiles: preview and working are sRGB, source is not ----

/// A colour inside the sRGB gamut that a wide-gamut profile moves a lot.
const ORANGE: [u8; 3] = [200, 100, 50];

/// `img` encoded as `format` with the ICC profile `icc` embedded.
fn with_icc(img: &image::DynamicImage, format: ImageFormat, icc: &[u8]) -> Vec<u8> {
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use image::codecs::tiff::TiffEncoder;

    let mut out = Cursor::new(Vec::new());
    let (w, h, colour) = (img.width(), img.height(), img.color().into());
    let bytes = img.as_bytes();
    let icc = icc.to_vec();
    match format {
        ImageFormat::Jpeg => {
            let mut e = JpegEncoder::new_with_quality(&mut out, 100);
            e.set_icc_profile(icc).unwrap();
            e.write_image(bytes, w, h, colour).unwrap();
        }
        ImageFormat::Png => {
            let mut e = PngEncoder::new(&mut out);
            e.set_icc_profile(icc).unwrap();
            e.write_image(bytes, w, h, colour).unwrap();
        }
        ImageFormat::WebP => {
            let mut e = WebPEncoder::new_lossless(&mut out);
            e.set_icc_profile(icc).unwrap();
            e.write_image(bytes, w, h, colour).unwrap();
        }
        ImageFormat::Tiff => {
            let mut e = TiffEncoder::new(&mut out);
            e.set_icc_profile(icc).unwrap();
            e.write_image(bytes, w, h, colour).unwrap();
        }
        other => panic!("no ICC writer for {other:?}"),
    }
    out.into_inner()
}

fn assert_near(got: [u8; 4], want: [f64; 3], tolerance: f64, what: &str) {
    for k in 0..3 {
        assert!(
            (f64::from(got[k]) - want[k]).abs() <= tolerance,
            "{what}: got {got:?}, want {want:?}"
        );
    }
}

#[test]
fn every_still_format_converts_an_embedded_profile_for_display_only() {
    use owlbum_fixtures::icc;

    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb(ORANGE)));
    let want = icc::DISPLAY_P3.expected_srgb(ORANGE);
    let p3 = icc::DISPLAY_P3.bytes();
    // JPEG is lossy: its decoded colour may be a level or two off.
    for (name, format, lossy) in [
        ("p3.jpg", ImageFormat::Jpeg, 3.0),
        ("p3.png", ImageFormat::Png, 0.0),
        ("p3.webp", ImageFormat::WebP, 0.0),
        ("p3.tif", ImageFormat::Tiff, 0.0),
    ] {
        let path = write(&dir, name, &with_icc(&img, format, &p3));
        let h = handler_for(&path).handler;
        let working = h.working(&path).unwrap();
        assert_near(working.pixel(8, 8), want, 2.0 + lossy, name);
        assert!(working.pixel(8, 8)[0].abs_diff(ORANGE[0]) >= 10, "{name}");
        assert_eq!(h.preview(&path, 512).unwrap(), working, "{name}");

        let src = h.source(&path).unwrap();
        assert_eq!(
            src.icc.as_deref(),
            Some(&p3[..]),
            "{name}: profile untouched"
        );
        assert_eq!(src.layout, Layout::Rgb, "{name}");
        let Samples::U8(s) = &src.samples else {
            panic!("{name}: 8-bit source")
        };
        let mid = (8 * 16 + 8) * 3;
        let raw = [s[mid], s[mid + 1], s[mid + 2], 255];
        assert_near(raw, ORANGE.map(f64::from), lossy, name);
    }
}

#[test]
fn a_sixteen_bit_png_with_a_profile_keeps_depth_and_profile_in_source() {
    use owlbum_fixtures::icc;

    let dir = TempDir::new().unwrap();
    let deep = ORANGE.map(|v| u16::from(v) * 257);
    let img = image::DynamicImage::ImageRgb16(image::ImageBuffer::from_pixel(4, 4, Rgb(deep)));
    let adobe = icc::ADOBE_RGB.bytes();
    let path = write(&dir, "deep.png", &with_icc(&img, ImageFormat::Png, &adobe));
    let h = handler_for(&path).handler;

    let working = h.working(&path).unwrap();
    let want = icc::ADOBE_RGB.expected_srgb(ORANGE);
    assert_near(working.pixel(1, 1), want, 2.0, "working");
    assert_ne!(&working.pixel(1, 1)[..3], &ORANGE);

    let src = h.source(&path).unwrap();
    assert_eq!(src.icc.as_deref(), Some(&adobe[..]));
    assert_eq!(src.samples, Samples::U16(deep.repeat(16)));
}

#[test]
fn a_grey_png_with_a_grey_profile_is_converted() {
    use owlbum_fixtures::icc;

    let dir = TempDir::new().unwrap();
    let grey = image::GrayImage::from_fn(16, 1, |x, _| image::Luma([x as u8 * 16]));
    let profile = icc::grey(461);
    let path = write(
        &dir,
        "grey.png",
        &with_icc(
            &image::DynamicImage::ImageLuma8(grey),
            ImageFormat::Png,
            &profile,
        ),
    );
    let h = handler_for(&path).handler;
    let working = h.working(&path).unwrap();
    for x in 0..16 {
        let v = x as u8 * 16;
        let want = icc::expected_grey_srgb(461, v);
        assert_near(working.pixel(x, 0), [want; 3], 2.0, &format!("level {v}"));
    }
    assert!(working.pixel(4, 0)[0].abs_diff(64) >= 10);

    let src = h.source(&path).unwrap();
    assert_eq!(src.layout, Layout::Grey);
    assert_eq!(src.icc.as_deref(), Some(&profile[..]));
    assert_eq!(src.samples, Samples::U8((0..16).map(|x| x * 16).collect()));
}

#[test]
fn an_srgb_profile_and_a_broken_profile_leave_the_picture_as_it_is() {
    use owlbum_fixtures::icc;

    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb(ORANGE)));
    let mut broken = icc::DISPLAY_P3.bytes();
    broken.truncate(100);
    for (name, profile) in [("srgb.png", icc::SRGB.bytes()), ("broken.png", broken)] {
        let path = write(&dir, name, &with_icc(&img, ImageFormat::Png, &profile));
        let h = handler_for(&path).handler;
        assert_eq!(
            h.working(&path).unwrap().pixel(3, 3),
            [200, 100, 50, 255],
            "{name}"
        );
        assert_eq!(h.source(&path).unwrap().icc, Some(profile), "{name}");
    }
}

#[test]
fn jpeg_xl_converts_its_profile_for_display_only() {
    use owlbum_fixtures::icc;

    // ImageMagick: an 8 × 8 rgb(200,100,50) picture tagged with
    // `icc::DISPLAY_P3`, written lossless.
    let path = testdata("p3.jxl");
    let h = handler_for(&path).handler;
    let working = h.working(&path).unwrap();
    assert_near(
        working.pixel(4, 4),
        icc::DISPLAY_P3.expected_srgb(ORANGE),
        2.0,
        "p3.jxl",
    );
    let src = h.source(&path).unwrap();
    let Samples::U8(s) = &src.samples else {
        panic!("8-bit source")
    };
    assert_eq!(&s[..3], &ORANGE);
    assert!(!colour::is_srgb(src.icc.as_deref()));
}

#[test]
fn an_srgb_jpeg_xl_is_not_converted() {
    // Every JPEG XL carries a profile, made by `jxl-oxide` when the file
    // has none; an sRGB one is read, recognised and left alone.
    let path = testdata("quadrants.jxl");
    let src = handler_for(&path).handler.source(&path).unwrap();
    let icc = src.icc.expect("a rendered profile");
    assert!(moxcms::ColorProfile::new_from_slice(&icc).is_ok());
    assert!(colour::is_srgb(Some(&icc)));
}

// ---- EXIF from each container (docs/03, "Metadata reading") ----

/// A camera's EXIF block, as the `trip` fixture writes it.
fn camera_exif() -> Vec<u8> {
    owlbum_fixtures::encode::Meta {
        make: Some("Owlbum"),
        model: Some("Fixture One"),
        taken: Some("2025:07:14 09:12:00".into()),
        ..Default::default()
    }
    .exif_block()
    .unwrap()
}

fn assert_camera_exif(meta: &Metadata, what: &str) {
    let exif = meta
        .exif
        .as_ref()
        .unwrap_or_else(|| panic!("{what}: no EXIF"));
    assert_eq!(exif.text(exif::Dir::Main, 0x010f), Some("Owlbum"), "{what}");
    assert_eq!(
        exif.text(exif::Dir::Exif, 0x9003),
        Some("2025:07:14 09:12:00"),
        "{what}"
    );
}

fn box_(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut b = (u32::try_from(data.len() + 8).unwrap())
        .to_be_bytes()
        .to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(data);
    b
}

/// A JPEG XL container: the codestream in `jxlc`, then an `Exif` box.
fn jxl_container(codestream: &[u8], exif: Option<&[u8]>) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 12, b'J', b'X', b'L', b' ', 0x0d, 0x0a, 0x87, 0x0a];
    out.extend(box_(b"ftyp", b"jxl \0\0\0\0jxl "));
    out.extend(box_(b"jxlc", codestream));
    if let Some(block) = exif {
        let mut body = 0u32.to_be_bytes().to_vec();
        body.extend_from_slice(block);
        out.extend(box_(b"Exif", &body));
    }
    out
}

#[test]
fn exif_is_read_from_every_container_that_holds_it() {
    let dir = TempDir::new().unwrap();
    let block = camera_exif();
    let img = quadrants();

    // JPEG: the APP1 segment.
    let meta = owlbum_fixtures::encode::Meta {
        make: Some("Owlbum"),
        model: Some("Fixture One"),
        taken: Some("2025:07:14 09:12:00".into()),
        ..Default::default()
    };
    let jpeg = owlbum_fixtures::encode::jpeg(&img, 90, &meta);

    // PNG: an `eXIf` chunk before the pixels.
    let png = encode(
        &image::DynamicImage::ImageRgb8(img.clone()),
        ImageFormat::Png,
    );
    let png = png_with_chunk(&png, &png_chunk(b"eXIf", &block));

    // WebP: the extended form with an `EXIF` chunk.
    let mut vp8x = vec![0x08, 0, 0, 0]; // EXIF flag
    vp8x.extend_from_slice(&u24(31));
    vp8x.extend_from_slice(&u24(15));
    let rgba = image::DynamicImage::ImageRgb8(img.clone()).to_rgba8();
    let mut body = b"WEBP".to_vec();
    body.extend(riff_chunk(b"VP8X", &vp8x));
    body.extend(riff_chunk(b"VP8L", &vp8l_payload(&rgba)));
    body.extend(riff_chunk(b"EXIF", &block));
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&(body.len() as u32).to_le_bytes());
    webp.extend(body);

    // TIFF: the file's own IFDs, written after the pixels.
    use owlbum_fixtures::tiff::{Ifd, Val};
    let mut exif_ifd = Ifd::new();
    exif_ifd.ascii(0x9003, "2025:07:14 09:12:00");
    let mut ifd0 = Ifd::new();
    ifd0.long(256, 32).long(257, 16);
    ifd0.set(258, Val::Short(vec![8, 8, 8]));
    ifd0.short(259, 1).short(262, 2);
    ifd0.ascii(0x010f, "Owlbum");
    ifd0.set(273, Val::Blob(img.as_raw().clone()));
    ifd0.short(277, 3).long(278, 16).long(279, 32 * 16 * 3);
    ifd0.set(0x8769, Val::Ifds(vec![exif_ifd]));
    let tiff = owlbum_fixtures::tiff::write(&ifd0);

    // JPEG XL: the container's `Exif` box after the codestream.
    let jxl = jxl_container(&owlbum_fixtures::encode::jxl(&img), Some(&block));

    for (name, bytes) in [
        ("e.jpg", jpeg),
        ("e.png", png),
        ("e.webp", webp),
        ("e.tif", tiff),
        ("e.jxl", jxl),
    ] {
        let path = write(&dir, name, &bytes);
        let format = handler_for(&path);
        let meta = format.handler.metadata(&path).unwrap();
        assert_eq!((meta.width, meta.height), (32, 16), "{name}");
        assert_camera_exif(&meta, name);
        // The picture still reads.
        let px = format.handler.working(&path).unwrap();
        assert_corners(&px, [RED, GREEN, BLUE, WHITE], name);
    }
}

#[test]
fn files_without_exif_give_none() {
    let dir = TempDir::new().unwrap();
    let img = image::DynamicImage::ImageRgb8(quadrants());
    let bare_jxl = owlbum_fixtures::encode::jxl(&quadrants());
    for (name, bytes) in [
        ("n.jpg", jpeg_bytes(&quadrants())),
        ("n.png", encode(&img, ImageFormat::Png)),
        ("n.gif", encode(&img, ImageFormat::Gif)),
        ("n.bmp", encode(&img, ImageFormat::Bmp)),
        ("n.webp", encode(&img, ImageFormat::WebP)),
        ("n.jxl", bare_jxl.clone()),
        ("c.jxl", jxl_container(&bare_jxl, None)),
    ] {
        let path = write(&dir, name, &bytes);
        let meta = handler_for(&path).handler.metadata(&path).unwrap();
        assert_eq!(meta.exif, None, "{name}");
    }
}

#[test]
fn a_damaged_jpeg_xl_exif_box_is_no_error() {
    let dir = TempDir::new().unwrap();
    let mut bytes = jxl_container(&owlbum_fixtures::encode::jxl(&quadrants()), None);
    // An `Exif` box whose offset points past its end, and a box that claims
    // more bytes than the file has.
    bytes.extend(box_(b"Exif", &[0, 0, 0, 99, 1, 2]));
    let path = write(&dir, "d.jxl", &bytes);
    let meta = handler_for(&path).handler.metadata(&path).unwrap();
    assert_eq!(meta.exif, None);
    assert_eq!(
        jxl::exif_box(&mut Cursor::new(&bytes[..bytes.len() - 3]))
            .ok()
            .flatten(),
        None
    );
    let mut long = jxl_container(&owlbum_fixtures::encode::jxl(&quadrants()), None);
    long.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, b'j', b'u', b'n', b'k']);
    assert!(jxl::exif_box(&mut Cursor::new(&long)).is_ok());
}
