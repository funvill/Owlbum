//! The still handler: JPEG, PNG, WebP, GIF, TIFF and BMP, decoded with the
//! `image` crate (docs/03-media-and-scanning.md, "The still handler").
//!
//! `image` decodes JPEG with `zune-jpeg`, which has no reduced-size
//! decode. A JPEG's `preview` is therefore decoded by `jpeg-decoder`,
//! scaled in the DCT to 1/2, 1/4 or 1/8 of its size: the smallest that is
//! still at least `min_side` on its longest side (#114). When no scale fits
//! below full size, when the file is CMYK, 16-bit or lossless, or when
//! `jpeg-decoder` cannot read it, `preview` is the full decode. For every
//! other format `preview` and `working` are the same decode. A CMYK JPEG
//! with a CMYK profile is converted through it by `cmyk` (#82).

use std::fs::File;
use std::io::BufReader;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use image::codecs::tiff::TiffDecoder;
use image::metadata::Orientation;
use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, RgbImage};
use jpeg_decoder::{CodingProcess, PixelFormat};

use super::colour;
use super::exif::Exif;
use super::handler::{
    Cost, Layout, MediaError, MediaHandler, Metadata, Pixels, Samples, SourcePixels, VideoInfo,
    check_size,
};
use super::{cmyk, headers, registry, sniff};

/// One format the still handler reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StillFormat {
    Jpeg,
    Png,
    Webp,
    Gif,
    Tiff,
    Bmp,
}

/// The still handler for one format. Each registry row has its own instance,
/// so sniffing tells a PNG named `.jpg` from a JPEG.
#[derive(Debug)]
pub struct Still {
    format: StillFormat,
}

pub static JPEG: Still = Still::new(StillFormat::Jpeg);
pub static PNG: Still = Still::new(StillFormat::Png);
pub static WEBP: Still = Still::new(StillFormat::Webp);
pub static GIF: Still = Still::new(StillFormat::Gif);
pub static TIFF: Still = Still::new(StillFormat::Tiff);
pub static BMP: Still = Still::new(StillFormat::Bmp);

const MB: u64 = 1_000_000;

impl Still {
    const fn new(format: StillFormat) -> Self {
        Self { format }
    }

    fn image_format(&self) -> ImageFormat {
        match self.format {
            StillFormat::Jpeg => ImageFormat::Jpeg,
            StillFormat::Png => ImageFormat::Png,
            StillFormat::Webp => ImageFormat::WebP,
            StillFormat::Gif => ImageFormat::Gif,
            StillFormat::Tiff => ImageFormat::Tiff,
            StillFormat::Bmp => ImageFormat::Bmp,
        }
    }

    fn open(file: &Path) -> Result<BufReader<File>, MediaError> {
        File::open(file)
            .map(BufReader::new)
            .map_err(|e| MediaError::io(&e))
    }

    fn decode_err<E>(&self) -> impl FnOnce(E) -> MediaError + '_ {
        move |_| MediaError::decode(registry::label_of(self))
    }

    /// For JPEG, reads the size from the markers. `image` needs the whole
    /// stream to be well formed before it reports a size, and the pixel
    /// limit must hold even for a file whose header alone is readable.
    /// A marker walk that fails is not an error here: the decoder, which is
    /// more lenient, tries anyway.
    fn check_jpeg_header(&self, file: &Path) -> Result<(), MediaError> {
        if self.format == StillFormat::Jpeg
            && let Ok(h) = headers::jpeg(&mut Self::open(file)?)
        {
            check_size(h.width, h.height)?;
        }
        Ok(())
    }

    /// Decodes the first picture, upright, at its own depth, with its ICC
    /// profile. Refuses an oversized picture before decoding.
    fn decode(&self, file: &Path) -> Result<(DynamicImage, Option<Vec<u8>>), MediaError> {
        self.check_jpeg_header(file)?;
        let mut reader = ImageReader::with_format(Self::open(file)?, self.image_format());
        // The pixel limit below replaces `image`'s default allocation cap.
        reader.no_limits();
        let mut decoder = reader.into_decoder().map_err(self.decode_err())?;
        let (w, h) = decoder.dimensions();
        check_size(w, h)?;
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let mut icc = decoder.icc_profile().ok().flatten();
        if icc.is_none() && self.format == StillFormat::Tiff {
            icc = Self::tiff_icc(file);
        }
        let mut img = DynamicImage::from_decoder(decoder).map_err(self.decode_err())?;
        // `zune-jpeg` has turned a CMYK JPEG into RGB with a formula; with
        // a CMYK profile it is decoded again and converted through the
        // profile (#82), and is then sRGB.
        if self.format == StillFormat::Jpeg
            && let Some(profile) = icc.as_deref()
            && cmyk::is_cmyk_profile(profile)
            && let Some(rgb) = std::fs::read(file)
                .ok()
                .and_then(|bytes| cmyk::jpeg_to_srgb(&bytes, profile))
        {
            img = DynamicImage::ImageRgb8(rgb);
            icc = None;
        }
        img.apply_orientation(orientation);
        Ok((img, icc))
    }

    /// A JPEG's preview decoded at the smallest DCT scale (1/2, 1/4, 1/8)
    /// that is still at least `min_side` on its longest side, in sRGB, with
    /// `full_width` and `full_height` the full upright size. None when no
    /// scale below full size fits or `jpeg-decoder` cannot give the picture
    /// as 8-bit grey or RGB: the caller then decodes it in full, which also
    /// keeps a damaged file as lenient as `zune-jpeg` makes it.
    fn scaled_jpeg(file: &Path, min_side: u32) -> Option<Pixels> {
        let mut decoder = jpeg_decoder::Decoder::new(Self::open(file).ok()?);
        decoder.read_info().ok()?;
        // `info` panics on a component count JPEG allows but
        // `jpeg-decoder` does not handle, as can its decode on a hostile
        // file; either way the full decode takes over.
        catch_unwind(AssertUnwindSafe(move || {
            let info = decoder.info()?;
            if info.coding_process == CodingProcess::Lossless
                || !matches!(info.pixel_format, PixelFormat::L8 | PixelFormat::RGB24)
            {
                return None;
            }
            let full = (u32::from(info.width), u32::from(info.height));
            let side = u16::try_from(min_side).unwrap_or(u16::MAX);
            let (w, h) = decoder.scale(side, side).ok()?;
            let (w, h) = (u32::from(w), u32::from(h));
            if (w, h) == full {
                return None;
            }
            let raw = decoder.decode().ok()?;
            let mut img = match info.pixel_format {
                PixelFormat::L8 => DynamicImage::ImageLuma8(GrayImage::from_raw(w, h, raw)?),
                _ => DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, raw)?),
            };
            let orientation = decoder
                .exif_data()
                .and_then(Orientation::from_exif_chunk)
                .unwrap_or(Orientation::NoTransforms);
            let icc = decoder.icc_profile();
            img.apply_orientation(orientation);
            let mut px = to_pixels(&img);
            (px.full_width, px.full_height) = if swaps_sides(orientation) {
                (full.1, full.0)
            } else {
                full
            };
            Some(colour::to_srgb(px, icc.as_deref()))
        }))
        .ok()
        .flatten()
    }

    /// A TIFF's ICC profile, read by a decoder with no limits set. Once
    /// `image` sets limits on its TIFF decoder, a tag of more values than
    /// the picture's byte count divided by 32 cannot be read, so a small
    /// picture loses its profile (`image` 0.25.10 over version 0.11.3 of
    /// the TIFF crate).
    fn tiff_icc(file: &Path) -> Option<Vec<u8>> {
        let mut decoder = TiffDecoder::new(Self::open(file).ok()?).ok()?;
        decoder.icc_profile().ok().flatten()
    }

    fn metadata_inner(&self, file: &Path) -> Result<Metadata, MediaError> {
        let mut meta = Metadata::default();
        if self.format == StillFormat::Jpeg {
            // `image` reads a whole JPEG into memory before it parses the
            // header, so the size and EXIF come from a marker walk instead.
            let h = headers::jpeg(&mut Self::open(file)?).map_err(self.decode_err())?;
            check_size(h.width, h.height)?;
            let turned = h
                .exif
                .as_deref()
                .and_then(Orientation::from_exif_chunk)
                .is_some_and(swaps_sides);
            (meta.width, meta.height) = if turned {
                (h.height, h.width)
            } else {
                (h.width, h.height)
            };
            meta.exif = h.exif.as_deref().and_then(Exif::parse);
            return Ok(meta);
        }

        let mut reader = ImageReader::with_format(Self::open(file)?, self.image_format());
        reader.no_limits();
        let mut decoder = reader.into_decoder().map_err(self.decode_err())?;
        let (w, h) = decoder.dimensions();
        check_size(w, h)?;
        meta.has_alpha = decoder.color_type().has_alpha();
        meta.exif = match self.format {
            // A TIFF file is itself the TIFF structure EXIF uses: its tags
            // are read from the file, by seeking to its IFDs.
            StillFormat::Tiff => Exif::read(&mut Self::open(file)?),
            _ => decoder
                .exif_metadata()
                .ok()
                .flatten()
                .as_deref()
                .and_then(Exif::parse),
        };
        let turned = decoder.orientation().is_ok_and(swaps_sides);
        (meta.width, meta.height) = if turned { (h, w) } else { (w, h) };
        drop(decoder);

        let animation = match self.format {
            StillFormat::Gif => Some(headers::gif(&mut Self::open(file)?)),
            StillFormat::Webp => Some(headers::webp(&mut Self::open(file)?)),
            StillFormat::Png => {
                let frames = headers::png_frames(&mut Self::open(file)?).unwrap_or(1);
                if frames > 1 {
                    meta.frames = Some(frames);
                }
                None
            }
            _ => None,
        };
        // A frame count that cannot be read leaves the file a still.
        if let Some(Ok(a)) = animation
            && a.frames > 1
        {
            meta.frames = Some(a.frames);
            meta.duration_ms = Some(a.duration_ms);
        }
        Ok(meta)
    }
}

/// True for the EXIF orientations 5 to 8, which turn the picture a quarter.
fn swaps_sides(o: Orientation) -> bool {
    matches!(
        o,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// An upright decoded picture as 8-bit RGBA.
pub(super) fn to_pixels(img: &DynamicImage) -> Pixels {
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Pixels {
        width,
        height,
        full_width: width,
        full_height: height,
        rgba: rgba.into_raw(),
        has_alpha,
    }
}

/// An upright decoded picture at its own depth. Floating-point and other
/// deeper samples become 16 bits.
fn to_source(img: DynamicImage, icc: Option<Vec<u8>>) -> SourcePixels {
    let (width, height) = (img.width(), img.height());
    let (layout, samples) = match img {
        DynamicImage::ImageLuma8(b) => (Layout::Grey, Samples::U8(b.into_raw())),
        DynamicImage::ImageLumaA8(b) => (Layout::GreyAlpha, Samples::U8(b.into_raw())),
        DynamicImage::ImageRgb8(b) => (Layout::Rgb, Samples::U8(b.into_raw())),
        DynamicImage::ImageRgba8(b) => (Layout::Rgba, Samples::U8(b.into_raw())),
        DynamicImage::ImageLuma16(b) => (Layout::Grey, Samples::U16(b.into_raw())),
        DynamicImage::ImageLumaA16(b) => (Layout::GreyAlpha, Samples::U16(b.into_raw())),
        DynamicImage::ImageRgb16(b) => (Layout::Rgb, Samples::U16(b.into_raw())),
        DynamicImage::ImageRgba16(b) => (Layout::Rgba, Samples::U16(b.into_raw())),
        other if other.color().has_alpha() => {
            (Layout::Rgba, Samples::U16(other.to_rgba16().into_raw()))
        }
        other => (Layout::Rgb, Samples::U16(other.to_rgb16().into_raw())),
    };
    SourcePixels {
        width,
        height,
        layout,
        samples,
        icc,
    }
}

impl MediaHandler for Still {
    fn sniff(&self, head: &[u8]) -> bool {
        match self.format {
            StillFormat::Jpeg => sniff::jpeg(head),
            StillFormat::Png => sniff::png(head),
            StillFormat::Webp => sniff::webp(head),
            StillFormat::Gif => sniff::gif(head),
            StillFormat::Tiff => sniff::tiff(head),
            StillFormat::Bmp => sniff::bmp(head),
        }
    }

    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError> {
        self.metadata_inner(file)
    }

    fn preview(&self, file: &Path, min_side: u32) -> Result<Pixels, MediaError> {
        if self.format == StillFormat::Jpeg {
            // The size limit holds before any pixel memory is taken.
            self.check_jpeg_header(file)?;
            if let Some(px) = Self::scaled_jpeg(file, min_side) {
                return Ok(px);
            }
        }
        self.working(file)
    }

    fn working(&self, file: &Path) -> Result<Pixels, MediaError> {
        let (img, icc) = self.decode(file)?;
        Ok(colour::to_srgb(to_pixels(&img), icc.as_deref()))
    }

    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError> {
        let (img, icc) = self.decode(file)?;
        Ok(to_source(img, icc))
    }

    fn probe(&self, _file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        Ok(None)
    }

    fn cost(&self, file_size: u64) -> Cost {
        match self.format {
            StillFormat::Jpeg => Cost::Cheap,
            StillFormat::Png if file_size < 8 * MB => Cost::Cheap,
            StillFormat::Tiff if file_size > 100 * MB => Cost::Heavy,
            _ => Cost::Decode,
        }
    }
}
