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
            assert_eq!(u.reason, "HEIC photos are not supported yet.");
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

// ---- RAW and video wait for their handlers ----

#[test]
fn raw_and_video_rows_are_recognised_but_not_read_yet() {
    let dir = TempDir::new().unwrap();
    let tiff_head = b"II*\0\x08\0\0\0".to_vec();
    let path = write(&dir, "IMG_1.CR2", &tiff_head);
    let f = handler_for(&path);
    assert_eq!(f.id, "cr2");
    let e = f.handler.metadata(&path).unwrap_err();
    assert_eq!(e.code, MediaErrorCode::Unsupported);
    assert_eq!(e.message, "This camera's RAW format is not supported yet.");

    let mut mp4 = 20u32.to_be_bytes().to_vec();
    mp4.extend_from_slice(b"ftypisom\0\0\0\0isom");
    mp4.extend_from_slice(&[0; 16]);
    let path = write(&dir, "clip.mov", &mp4);
    let f = handler_for(&path);
    assert_eq!(f.id, "mov", "the extension picks the id within a family");
    let e = f.handler.probe(&path).unwrap_err();
    assert_eq!(e.code, MediaErrorCode::Unsupported);
    assert_eq!(e.message, "This video format is not supported yet.");
}
