//! The JPEG XL handler, decoding with `jxl-oxide`
//! (docs/03-media-and-scanning.md, "The JPEG XL handler").
//!
//! Orientation comes from the codestream and `jxl-oxide` applies it when it
//! hands out pixels. An animated JPEG XL is shown as a still: frame 1.
//! EXIF sits in the container's `Exif` box, which [`exif_box`] finds by
//! walking box headers.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use jxl_oxide::{
    EnumColourEncoding, InitializeResult, JxlImage, Moxcms, PixelFormat, Render, RenderingIntent,
};

use super::colour;
use super::exif::Exif;
use super::handler::{
    Cost, Layout, MediaError, MediaHandler, Metadata, Pixels, Samples, SourcePixels, VideoInfo,
    check_size,
};
use super::{registry, sniff};

/// The JPEG XL handler. It holds a byte only so that its static has an
/// address of its own: the registry tells handlers apart by address, and a
/// zero-sized static need not have a unique one.
#[derive(Debug)]
pub struct Jxl {
    _addr: u8,
}

pub static JXL: Jxl = Jxl { _addr: 0 };

fn decode_error() -> MediaError {
    MediaError::decode(registry::label_of(&JXL))
}

fn decode_err<E>(_: E) -> MediaError {
    decode_error()
}

fn open(file: &Path) -> Result<BufReader<File>, MediaError> {
    File::open(file)
        .map(BufReader::new)
        .map_err(|e| MediaError::io(&e))
}

/// Reads only as far as the image header.
fn read_header(file: &Path) -> Result<JxlImage, MediaError> {
    let mut reader = open(file)?;
    let mut uninit = JxlImage::builder().build_uninit();
    // The same feeding loop as `JxlImageBuilder::read`, stopping at the
    // header: bytes the parser has not consumed are kept for the next feed.
    let mut buf = vec![0u8; 16 * 1024];
    let mut valid = 0usize;
    loop {
        if valid == buf.len() {
            buf.resize(buf.len() * 2, 0);
        }
        let n = reader
            .read(&mut buf[valid..])
            .map_err(|e| MediaError::io(&e))?;
        if n == 0 {
            return Err(decode_error());
        }
        valid += n;
        let consumed = uninit.feed_bytes(&buf[..valid]).map_err(decode_err)?;
        buf.copy_within(consumed..valid, 0);
        valid -= consumed;
        match uninit.try_init().map_err(decode_err)? {
            InitializeResult::NeedMoreData(u) => uninit = u,
            InitializeResult::Initialized(image) => return Ok(image),
        }
    }
}

/// The signature box that starts a JPEG XL container.
const CONTAINER: [u8; 12] = [0, 0, 0, 12, b'J', b'X', b'L', b' ', 0x0d, 0x0a, 0x87, 0x0a];

/// Larger `Exif` boxes are taken to be damaged.
const MAX_EXIF: u64 = 16 * 1024 * 1024;

/// The EXIF block (from the TIFF header on) in a JPEG XL container's `Exif`
/// box. Box headers are read and every other box is skipped with a seek,
/// so the codestream is never read. A bare codestream has no box, and a
/// Brotli-compressed `brob` box is not read. The box body starts with a
/// 4-byte big-endian offset from its end to the TIFF header.
pub fn exif_box<R: Read + Seek>(r: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut sig = [0u8; 12];
    if r.read_exact(&mut sig).is_err() || sig != CONTAINER {
        return Ok(None);
    }
    loop {
        let mut head = [0u8; 8];
        if r.read_exact(&mut head).is_err() {
            return Ok(None);
        }
        let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        // The body's length; None runs to the end of the file.
        let body = match size {
            0 => None,
            1 => {
                let mut big = [0u8; 8];
                r.read_exact(&mut big)?;
                match u64::from_be_bytes(big).checked_sub(16) {
                    Some(n) => Some(n),
                    None => return Ok(None),
                }
            }
            2..=7 => return Ok(None),
            n => Some(u64::from(n) - 8),
        };
        if &head[4..8] == b"Exif" {
            let len = body.unwrap_or(MAX_EXIF).min(MAX_EXIF);
            let mut data = Vec::new();
            r.take(len).read_to_end(&mut data)?;
            if data.len() < 4 {
                return Ok(None);
            }
            let offset = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
            return Ok(offset
                .checked_add(4)
                .and_then(|start| data.get(start..))
                .map(<[u8]>::to_vec));
        }
        match body {
            Some(n) => {
                let n = i64::try_from(n).map_err(|_| std::io::ErrorKind::InvalidData)?;
                r.seek(SeekFrom::Current(n))?;
            }
            None => return Ok(None),
        }
    }
}

/// Reads the whole file, checking the size on the header before any pixel
/// is decoded, and renders frame 1.
fn render(file: &Path) -> Result<(JxlImage, Render), MediaError> {
    let header = read_header(file)?;
    check_size(header.width(), header.height())?;
    drop(header);
    let mut image = JxlImage::builder().read(open(file)?).map_err(decode_err)?;
    if image.pixel_format().has_black() {
        // A CMYK picture is rendered through its CMYK profile into sRGB by
        // `moxcms` (#82), since `Layout` has no CMYK.
        image.set_cms(Moxcms);
        image.request_color_encoding(EnumColourEncoding::srgb(RenderingIntent::Perceptual));
    }
    let render = image.render_frame(0).map_err(decode_err)?;
    Ok((image, render))
}

fn layout_of(image: &JxlImage) -> Result<Layout, MediaError> {
    match image.pixel_format() {
        PixelFormat::Gray => Ok(Layout::Grey),
        PixelFormat::Graya => Ok(Layout::GreyAlpha),
        PixelFormat::Rgb => Ok(Layout::Rgb),
        PixelFormat::Rgba => Ok(Layout::Rgba),
        // `render` asks for sRGB when the picture is CMYK, so CMYK is left
        // only when that request failed.
        PixelFormat::Cmyk | PixelFormat::Cmyka => Err(decode_error()),
    }
}

/// Interleaved samples of the colour and alpha channels, upright.
fn samples<S: jxl_oxide::FrameBufferSample + Default + Clone>(
    render: &Render,
) -> (u32, u32, usize, Vec<S>) {
    let mut stream = render.stream();
    let (w, h, c) = (stream.width(), stream.height(), stream.channels() as usize);
    let mut buf = vec![S::default(); w as usize * h as usize * c];
    stream.write_to_buffer(&mut buf);
    (w, h, c, buf)
}

impl MediaHandler for Jxl {
    fn sniff(&self, head: &[u8]) -> bool {
        sniff::jxl(head)
    }

    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError> {
        let image = read_header(file)?;
        let (width, height) = (image.width(), image.height());
        check_size(width, height)?;
        Ok(Metadata {
            width,
            height,
            has_alpha: image.pixel_format().has_alpha(),
            // A damaged box leaves the picture without EXIF, never an error.
            exif: exif_box(&mut open(file)?)
                .ok()
                .flatten()
                .as_deref()
                .and_then(Exif::parse),
            ..Metadata::default()
        })
    }

    fn preview(&self, file: &Path, _min_side: u32) -> Result<Pixels, MediaError> {
        self.working(file)
    }

    fn working(&self, file: &Path) -> Result<Pixels, MediaError> {
        let (image, render) = render(file)?;
        let layout = layout_of(&image)?;
        let (width, height, channels, buf) = samples::<u8>(&render);
        if channels != layout.channels() {
            return Err(decode_error());
        }
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for px in buf.chunks_exact(channels) {
            match layout {
                Layout::Grey => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
                Layout::GreyAlpha => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
                Layout::Rgb => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
                Layout::Rgba => rgba.extend_from_slice(&px[..4]),
            }
        }
        let pixels = Pixels {
            width,
            height,
            full_width: width,
            full_height: height,
            rgba,
            has_alpha: layout.has_alpha(),
        };
        let icc = image.rendered_icc();
        Ok(colour::to_srgb(pixels, Some(&icc)))
    }

    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError> {
        let (image, render) = render(file)?;
        let layout = layout_of(&image)?;
        let deep = image.image_header().metadata.bit_depth.bits_per_sample() > 8;
        let (width, height, samples) = if deep {
            let (w, h, _, buf) = samples::<u16>(&render);
            (w, h, Samples::U16(buf))
        } else {
            let (w, h, _, buf) = samples::<u8>(&render);
            (w, h, Samples::U8(buf))
        };
        Ok(SourcePixels {
            width,
            height,
            layout,
            samples,
            icc: Some(image.rendered_icc()),
        })
    }

    fn probe(&self, _file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        Ok(None)
    }

    fn cost(&self, _file_size: u64) -> Cost {
        Cost::Decode
    }
}
