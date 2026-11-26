//! Writing pictures as files. Only pure-Rust encoders, so the bytes are the
//! same on every system.

use std::io::Cursor;

use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::{
    Delay, ExtendedColorType, Frame, ImageEncoder, ImageFormat, RgbImage, RgbaImage,
    buffer::ConvertBuffer,
};

use crate::tiff::{Ifd, Val};

/// What a camera writes about a photo. `None` fields are left out.
#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub make: Option<&'static str>,
    pub model: Option<&'static str>,
    pub lens: Option<&'static str>,
    /// EXIF orientation, 1 to 8. 0 leaves the tag out.
    pub orientation: u16,
    /// `YYYY:MM:DD HH:MM:SS`, as EXIF writes it.
    pub taken: Option<String>,
    /// `+HH:MM`.
    pub offset: Option<&'static str>,
    /// Exposure as a fraction of a second.
    pub exposure: Option<[u32; 2]>,
    /// f-number times ten.
    pub f_number_x10: Option<u32>,
    pub iso: Option<u16>,
    pub focal_mm: Option<u32>,
    pub flash: Option<u16>,
    /// Latitude and longitude in millionths of a degree.
    pub gps: Option<(i32, i32)>,
    /// Stored pixel size, for PixelXDimension and PixelYDimension.
    pub pixels: Option<(u32, u32)>,
}

impl Meta {
    pub fn is_empty(&self) -> bool {
        self.tiff_ifd().is_empty()
    }

    fn tiff_ifd(&self) -> Ifd {
        let mut ifd0 = Ifd::new();
        if let Some(m) = self.make {
            ifd0.ascii(0x010f, m);
        }
        if let Some(m) = self.model {
            ifd0.ascii(0x0110, m);
        }
        if self.orientation != 0 {
            ifd0.short(0x0112, self.orientation);
        }
        if let Some(t) = &self.taken {
            ifd0.ascii(0x0132, t);
        }

        let mut exif = Ifd::new();
        if let Some(e) = self.exposure {
            exif.set(0x829a, Val::Rational(vec![e]));
        }
        if let Some(f) = self.f_number_x10 {
            exif.rational(0x829d, f, 10);
        }
        if let Some(iso) = self.iso {
            exif.short(0x8827, iso);
        }
        if self.taken.is_some() || self.exposure.is_some() {
            exif.set(0x9000, Val::Undefined(b"0232".to_vec()));
        }
        if let Some(t) = &self.taken {
            exif.ascii(0x9003, t).ascii(0x9004, t);
        }
        if let Some(o) = self.offset {
            exif.ascii(0x9011, o);
        }
        if let Some(f) = self.flash {
            exif.short(0x9209, f);
        }
        if let Some(f) = self.focal_mm {
            exif.rational(0x920a, f, 1);
        }
        if let Some((w, h)) = self.pixels {
            exif.long(0xa002, w).long(0xa003, h);
        }
        if let Some(l) = self.lens {
            exif.ascii(0xa434, l);
        }
        if !exif.is_empty() {
            ifd0.set(0x8769, Val::Ifds(vec![exif]));
        }

        if let Some((lat, lon)) = self.gps {
            let mut gps = Ifd::new();
            gps.set(0x0000, Val::Byte(vec![2, 3, 0, 0]));
            gps.ascii(0x0001, if lat < 0 { "S" } else { "N" });
            gps.set(0x0002, Val::Rational(dms(lat.unsigned_abs())));
            gps.ascii(0x0003, if lon < 0 { "W" } else { "E" });
            gps.set(0x0004, Val::Rational(dms(lon.unsigned_abs())));
            ifd0.set(0x8825, Val::Ifds(vec![gps]));
        }
        ifd0
    }

    /// The EXIF block (a TIFF structure), or nothing when there is no field.
    pub fn exif_block(&self) -> Option<Vec<u8>> {
        let ifd = self.tiff_ifd();
        (!ifd.is_empty()).then(|| crate::tiff::write(&ifd))
    }
}

/// Millionths of a degree as degrees, minutes and hundredths of seconds.
fn dms(micro: u32) -> Vec<[u32; 2]> {
    let deg = micro / 1_000_000;
    let rest = u64::from(micro % 1_000_000) * 60; // millionths of a minute
    let min = (rest / 1_000_000) as u32;
    let sec_x100 = ((rest % 1_000_000) * 60 * 100 / 1_000_000) as u32;
    vec![[deg, 1], [min, 1], [sec_x100, 100]]
}

/// A JPEG with the EXIF block from `meta`, if it has any field.
pub fn jpeg(img: &RgbImage, quality: u8, meta: &Meta) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgb8,
        )
        .expect("JPEG encode");
    match meta.exif_block() {
        Some(tiff) => insert_exif(out, &tiff),
        None => out,
    }
}

/// Put an APP1 EXIF segment straight after SOI (and after a JFIF APP0).
fn insert_exif(jpeg: Vec<u8>, tiff: &[u8]) -> Vec<u8> {
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    let mut at = 2;
    if jpeg[2..4] == [0xff, 0xe0] {
        at = 4 + usize::from(u16::from_be_bytes([jpeg[4], jpeg[5]]));
    }
    let len = u16::try_from(2 + 6 + tiff.len()).expect("EXIF block too large");
    let mut out = Vec::with_capacity(jpeg.len() + tiff.len() + 10);
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(&[0xff, 0xe1]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(tiff);
    out.extend_from_slice(&jpeg[at..]);
    out
}

fn png_info(w: u32, h: u32, colour: png::ColorType, depth: png::BitDepth) -> png::Info<'static> {
    let mut info = png::Info::with_size(w, h);
    info.color_type = colour;
    info.bit_depth = depth;
    info
}

fn png_with(info: png::Info<'_>, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::with_info(&mut out, info).expect("PNG info");
        enc.set_compression(png::Compression::Balanced);
        let mut w = enc.write_header().expect("PNG header");
        w.write_image_data(data).expect("PNG data");
    }
    out
}

/// A plain 8-bit RGB PNG with no metadata.
pub fn png_rgb(img: &RgbImage) -> Vec<u8> {
    let info = png_info(
        img.width(),
        img.height(),
        png::ColorType::Rgb,
        png::BitDepth::Eight,
    );
    png_with(info, img.as_raw())
}

/// An 8-bit RGBA PNG.
pub fn png_rgba(img: &RgbaImage) -> Vec<u8> {
    let info = png_info(
        img.width(),
        img.height(),
        png::ColorType::Rgba,
        png::BitDepth::Eight,
    );
    png_with(info, img.as_raw())
}

/// A 16-bit RGB PNG: each 8-bit value `v` becomes `v * 257` plus a little
/// low-order detail, so the extra bits are really used.
pub fn png_rgb16(img: &RgbImage) -> Vec<u8> {
    let info = png_info(
        img.width(),
        img.height(),
        png::ColorType::Rgb,
        png::BitDepth::Sixteen,
    );
    let mut data = Vec::with_capacity(img.as_raw().len() * 2);
    for (i, v) in img.as_raw().iter().enumerate() {
        let wide = (u16::from(*v) * 257) ^ (i as u16 & 0x7f);
        data.extend_from_slice(&wide.to_be_bytes());
    }
    png_with(info, &data)
}

/// An 8-bit RGB PNG with an embedded ICC profile.
pub fn png_with_icc(img: &RgbImage, icc: &[u8]) -> Vec<u8> {
    let mut info = png_info(
        img.width(),
        img.height(),
        png::ColorType::Rgb,
        png::BitDepth::Eight,
    );
    info.icc_profile = Some(icc.to_vec().into());
    png_with(info, img.as_raw())
}

/// An animated PNG (APNG) whose first frame is also the still picture.
pub fn apng(frames: &[RgbImage], delay_ms: u16) -> Vec<u8> {
    let (w, h) = frames[0].dimensions();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Balanced);
        enc.set_animated(u32::try_from(frames.len()).expect("frames"), 0)
            .expect("APNG");
        enc.set_frame_delay(delay_ms, 1000).expect("delay");
        let mut wr = enc.write_header().expect("APNG header");
        for f in frames {
            wr.write_image_data(f.as_raw()).expect("APNG frame");
        }
        wr.finish().expect("APNG end");
    }
    out
}

/// An animated GIF that loops forever.
pub fn gif_animated(frames: &[RgbImage], delay_ms: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(Repeat::Infinite).expect("GIF repeat");
        for f in frames {
            let rgba: RgbaImage = f.convert();
            let frame = Frame::from_parts(rgba, 0, 0, Delay::from_numer_denom_ms(delay_ms, 1));
            enc.encode_frame(frame).expect("GIF frame");
        }
    }
    out
}

/// A still GIF (one frame).
pub fn gif(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        let rgba: RgbaImage = img.convert();
        enc.encode_frame(Frame::new(rgba)).expect("GIF frame");
    }
    out
}

/// A lossless WebP (the only kind the pure-Rust encoder writes).
pub fn webp(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    WebPEncoder::new_lossless(&mut out)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgb8,
        )
        .expect("WebP encode");
    out
}

fn riff_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&u32::try_from(data.len()).expect("chunk").to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

fn u24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// An animated WebP, built from one lossless still per frame: each still's
/// `VP8L` chunk becomes the image data of an `ANMF` chunk.
pub fn webp_animated(frames: &[RgbImage], delay_ms: u32) -> Vec<u8> {
    let (w, h) = frames[0].dimensions();
    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![0x02, 0, 0, 0]; // animation flag
    vp8x.extend_from_slice(&u24(w - 1));
    vp8x.extend_from_slice(&u24(h - 1));
    riff_chunk(&mut body, b"VP8X", &vp8x);
    riff_chunk(&mut body, b"ANIM", &[255, 255, 255, 255, 0, 0]);
    for f in frames {
        let still = webp(f);
        // RIFF header (12 bytes), then the VP8L chunk.
        assert_eq!(&still[12..16], b"VP8L");
        let vp8l = &still[12..];
        let mut anmf = Vec::new();
        anmf.extend_from_slice(&u24(0)); // x / 2
        anmf.extend_from_slice(&u24(0)); // y / 2
        anmf.extend_from_slice(&u24(f.width() - 1));
        anmf.extend_from_slice(&u24(f.height() - 1));
        anmf.extend_from_slice(&u24(delay_ms));
        anmf.push(0b10); // no blending, no disposal
        anmf.extend_from_slice(vp8l);
        riff_chunk(&mut body, b"ANMF", &anmf);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&u32::try_from(body.len()).expect("RIFF").to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn with_image_crate(img: &RgbImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).expect("encode");
    out.into_inner()
}

pub fn tiff(img: &RgbImage) -> Vec<u8> {
    with_image_crate(img, ImageFormat::Tiff)
}

pub fn bmp(img: &RgbImage) -> Vec<u8> {
    with_image_crate(img, ImageFormat::Bmp)
}

/// A lossless JPEG XL.
pub fn jxl(img: &RgbImage) -> Vec<u8> {
    use zune_core::bit_depth::BitDepth;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::EncoderOptions;
    let opts = EncoderOptions::new(
        img.width() as usize,
        img.height() as usize,
        ColorSpace::RGB,
        BitDepth::Eight,
    )
    .set_num_threads(1);
    let mut out = Vec::new();
    zune_jpegxl::JxlSimpleEncoder::new(img.as_raw(), opts)
        .encode(&mut out)
        .expect("JPEG XL encode");
    out
}

/// CRC-32 as PNG uses it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("chunk").to_be_bytes());
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc_input);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// A PNG whose header claims `w` × `h` pixels but which holds almost no
/// data: a decoder must refuse it from the header, without allocating.
pub fn png_header_only(w: u32, h: u32) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
    png_chunk(&mut out, b"IHDR", &ihdr);
    // A valid empty zlib stream, then the end.
    png_chunk(
        &mut out,
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    );
    png_chunk(&mut out, b"IEND", &[]);
    out
}

/// The first bytes of a HEIC file (an ISO base media `ftyp` box with the
/// `heic` brand), followed by filler. Enough to be recognised, not decoded.
pub fn heic_signature(filler: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0x18];
    out.extend_from_slice(b"ftypheic");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(b"mif1heic");
    out.extend_from_slice(filler);
    out
}

/// The first bytes of an AVIF file, followed by filler.
pub fn avif_signature(filler: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0x1c];
    out.extend_from_slice(b"ftypavif");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(b"avifmif1miaf");
    out.extend_from_slice(filler);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_png() {
        // The IEND chunk's well-known CRC.
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn dms_splits_degrees() {
        // 49.5 degrees = 49° 30' 0".
        assert_eq!(dms(49_500_000), vec![[49, 1], [30, 1], [0, 100]]);
        // 0.0125 degrees = 0° 0' 45".
        assert_eq!(dms(12_500), vec![[0, 1], [0, 1], [4500, 100]]);
    }

    #[test]
    fn exif_sits_after_soi() {
        let img = RgbImage::new(8, 8);
        let meta = Meta {
            orientation: 6,
            ..Meta::default()
        };
        let b = jpeg(&img, 90, &meta);
        let app1 = b.windows(2).position(|w| w == [0xff, 0xe1]).unwrap();
        assert!(app1 < 30);
        assert_eq!(&b[app1 + 4..app1 + 10], b"Exif\0\0");
    }

    #[test]
    fn no_fields_no_exif() {
        let img = RgbImage::new(8, 8);
        let b = jpeg(&img, 90, &Meta::default());
        assert!(!b.windows(6).any(|w| w == b"Exif\0\0"));
    }
}
