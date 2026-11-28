//! Pictures decoded by the operating system's codecs: HEIC and HEIF (#70)
//! and AVIF (#71) (docs/03-media-and-scanning.md, "The system image
//! handler").
//!
//! Owlbum bundles no patent-encumbered decoder (docs/README.md, principle
//! 6). On Windows the Windows Imaging Component reads these files with the
//! HEIF decoder of the Microsoft Store's "HEIF Image Extensions", which in
//! turn needs the HEVC or AV1 video decoder ("HEVC Video Extensions from
//! Device Manufacturer", "AV1 Video Extension"). The program asks once at
//! start which of them are there ([`install`]); a format whose codec is
//! missing stays "not supported yet", with a reason that names the
//! extension to install. Elsewhere (Linux) nothing is installed, so both
//! stay unsupported.
//!
//! The operating system sits behind [`SystemDecoder`], so tests use a fake
//! and never need the codecs. Until [`install`] is called (every test, and
//! every platform but Windows) no system codec is available.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;

use image::metadata::Orientation;
use image::{DynamicImage, RgbaImage};

use super::exif::{Dir, Exif};
use super::handler::{
    Cost, Layout, MediaError, MediaErrorCode, MediaHandler, Metadata, Pixels, Samples,
    SourcePixels, VideoInfo, check_size,
};
use super::{colour, heif, registry, sniff};
use crate::text;

/// A picture format that only the operating system can decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemCodec {
    /// HEIF with HEVC pictures: HEIC.
    Heic,
    /// HEIF with AV1 pictures: AVIF.
    Avif,
}

impl SystemCodec {
    /// The reason shown for a file of this format while its codec is
    /// missing: on Windows it names the free Store extensions to install.
    pub fn missing(self) -> &'static str {
        match self {
            Self::Heic => text::media::HEIC_UNSUPPORTED,
            Self::Avif => text::media::AVIF_UNSUPPORTED,
        }
    }
}

/// Which system codecs this computer has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Codecs {
    pub heic: bool,
    pub avif: bool,
}

impl Codecs {
    /// No system codec: every platform until [`install`] is called.
    pub const NONE: Self = Self {
        heic: false,
        avif: false,
    };
    /// Every system codec, for tests.
    pub const ALL: Self = Self {
        heic: true,
        avif: true,
    };

    /// True when `codec` can be decoded.
    pub fn has(self, codec: SystemCodec) -> bool {
        match codec {
            SystemCodec::Heic => self.heic,
            SystemCodec::Avif => self.avif,
        }
    }
}

/// A picture's size and orientation as the system reads them from its
/// headers, without decoding pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Stored width, before orientation.
    pub width: u32,
    /// Stored height, before orientation.
    pub height: u32,
    /// True when the picture has an alpha channel.
    pub has_alpha: bool,
    /// The orientation the system reports (1 to 8, as EXIF), or None when
    /// it reports none.
    pub orientation: Option<u16>,
}

/// A picture as the system decodes it: stored orientation, 8-bit RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub header: Header,
    /// `width × height × 4` bytes, non-premultiplied.
    pub rgba: Vec<u8>,
    /// The embedded ICC profile, if any.
    pub icc: Option<Vec<u8>>,
}

/// The operating system's image decoders. The real one is
/// `media::wic::Wic` on Windows; tests use a fake.
pub trait SystemDecoder: Send + Sync {
    /// Which codecs are there.
    fn codecs(&self) -> Codecs;
    /// Size, alpha and orientation from the headers only.
    fn header(&self, file: &Path) -> Result<Header, MediaError>;
    /// The first picture, at full size. Called only after [`Self::header`]
    /// passed the size check.
    fn decode(&self, file: &Path) -> Result<Decoded, MediaError>;
}

static DECODER: OnceLock<Box<dyn SystemDecoder>> = OnceLock::new();

/// Makes `decoder` the program's system decoder. The first call wins; the
/// program calls it once at start, tests never do.
pub fn install(decoder: Box<dyn SystemDecoder>) -> Codecs {
    let _ = DECODER.set(decoder);
    codecs()
}

/// Asks the operating system which codecs it has and installs its
/// decoder. Nothing on platforms without one.
pub fn install_detected() -> Codecs {
    #[cfg(windows)]
    {
        install(Box::new(super::wic::Wic::detect()))
    }
    #[cfg(not(windows))]
    {
        Codecs::NONE
    }
}

/// The codecs of the installed decoder; none when none is installed.
pub fn codecs() -> Codecs {
    DECODER.get().map_or(Codecs::NONE, |d| d.codecs())
}

/// The system image handler for one format.
#[derive(Debug)]
pub struct SystemImage {
    codec: SystemCodec,
}

pub static HEIC: SystemImage = SystemImage {
    codec: SystemCodec::Heic,
};
pub static AVIF: SystemImage = SystemImage {
    codec: SystemCodec::Avif,
};

impl SystemImage {
    /// The installed decoder, when it has this format's codec.
    fn decoder(&self) -> Result<&'static dyn SystemDecoder, MediaError> {
        match DECODER.get() {
            Some(d) if d.codecs().has(self.codec) => Ok(d.as_ref()),
            _ => Err(MediaError::unsupported(self.codec.missing())),
        }
    }

    fn label(&self) -> &'static str {
        registry::label_of(self)
    }

    /// A decoder's `decode` error, with this format's label in it.
    fn named(&self, e: MediaError) -> MediaError {
        if e.code == MediaErrorCode::Decode {
            MediaError::decode(self.label())
        } else {
            e
        }
    }

    /// Facts from the headers, read through `decoder`.
    pub fn metadata_with(
        &self,
        decoder: &dyn SystemDecoder,
        file: &Path,
    ) -> Result<Metadata, MediaError> {
        let header = decoder.header(file).map_err(|e| self.named(e))?;
        check_size(header.width, header.height)?;
        let exif = read_exif(file);
        let turned = swaps_sides(orientation(&header, exif.as_ref()));
        let (width, height) = if turned {
            (header.height, header.width)
        } else {
            (header.width, header.height)
        };
        Ok(Metadata {
            width,
            height,
            has_alpha: header.has_alpha,
            exif,
            ..Metadata::default()
        })
    }

    /// The upright picture in sRGB, decoded through `decoder`.
    pub fn working_with(
        &self,
        decoder: &dyn SystemDecoder,
        file: &Path,
    ) -> Result<Pixels, MediaError> {
        let (img, icc, has_alpha) = self.upright(decoder, file)?;
        let (width, height) = (img.width(), img.height());
        let pixels = Pixels {
            width,
            height,
            full_width: width,
            full_height: height,
            rgba: img.into_raw(),
            has_alpha,
        };
        Ok(colour::to_srgb(pixels, icc.as_deref()))
    }

    /// The upright picture with its profile untouched.
    pub fn source_with(
        &self,
        decoder: &dyn SystemDecoder,
        file: &Path,
    ) -> Result<SourcePixels, MediaError> {
        let (img, icc, _) = self.upright(decoder, file)?;
        Ok(SourcePixels {
            width: img.width(),
            height: img.height(),
            layout: Layout::Rgba,
            samples: Samples::U8(img.into_raw()),
            icc,
        })
    }

    fn upright(
        &self,
        decoder: &dyn SystemDecoder,
        file: &Path,
    ) -> Result<(RgbaImage, Option<Vec<u8>>, bool), MediaError> {
        let header = decoder.header(file).map_err(|e| self.named(e))?;
        check_size(header.width, header.height)?;
        let decoded = decoder.decode(file).map_err(|e| self.named(e))?;
        let h = decoded.header;
        let buffer = RgbaImage::from_raw(h.width, h.height, decoded.rgba)
            .ok_or_else(|| MediaError::decode(self.label()))?;
        let turn = orientation(&h, read_exif(file).as_ref());
        let mut img = DynamicImage::ImageRgba8(buffer);
        img.apply_orientation(turn);
        let img = match img {
            DynamicImage::ImageRgba8(b) => b,
            other => other.to_rgba8(),
        };
        Ok((img, decoded.icc, h.has_alpha))
    }
}

/// The file's EXIF tags, read from its container; a damaged one is none.
fn read_exif(file: &Path) -> Option<Exif> {
    let mut reader = BufReader::new(File::open(file).ok()?);
    heif::exif_block(&mut reader)
        .as_deref()
        .and_then(Exif::parse)
}

/// How to turn the decoded picture upright: the orientation the system
/// reports, else the EXIF Orientation tag, else none.
fn orientation(header: &Header, exif: Option<&Exif>) -> Orientation {
    header
        .orientation
        .map(i64::from)
        .or_else(|| exif.and_then(|e| e.int(Dir::Main, 0x0112)))
        .and_then(|o| u8::try_from(o).ok())
        .and_then(Orientation::from_exif)
        .unwrap_or(Orientation::NoTransforms)
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

impl MediaHandler for SystemImage {
    fn sniff(&self, head: &[u8]) -> bool {
        match self.codec {
            SystemCodec::Heic => sniff::heic(head),
            SystemCodec::Avif => sniff::avif(head),
        }
    }

    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError> {
        self.metadata_with(self.decoder()?, file)
    }

    fn preview(&self, file: &Path, _min_side: u32) -> Result<Pixels, MediaError> {
        self.working(file)
    }

    fn working(&self, file: &Path) -> Result<Pixels, MediaError> {
        self.working_with(self.decoder()?, file)
    }

    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError> {
        self.source_with(self.decoder()?, file)
    }

    fn probe(&self, _file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        Ok(None)
    }

    fn cost(&self, _file_size: u64) -> Cost {
        Cost::Decode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system decoder that "decodes" any file as a 3 × 2 picture: red,
    /// green, blue on the top row, white, black, grey below, with the
    /// orientation it is given.
    struct Fake {
        codecs: Codecs,
        orientation: Option<u16>,
        size: (u32, u32),
    }

    impl Fake {
        fn new() -> Self {
            Self {
                codecs: Codecs::ALL,
                orientation: None,
                size: (3, 2),
            }
        }
    }

    const TOP_LEFT: [u8; 4] = [255, 0, 0, 255];
    const TOP_RIGHT: [u8; 4] = [0, 0, 255, 255];
    const BOTTOM_LEFT: [u8; 4] = [255, 255, 255, 255];

    impl SystemDecoder for Fake {
        fn codecs(&self) -> Codecs {
            self.codecs
        }
        fn header(&self, _file: &Path) -> Result<Header, MediaError> {
            Ok(Header {
                width: self.size.0,
                height: self.size.1,
                has_alpha: false,
                orientation: self.orientation,
            })
        }
        fn decode(&self, file: &Path) -> Result<Decoded, MediaError> {
            let rgba = [
                TOP_LEFT,
                [0, 255, 0, 255],
                TOP_RIGHT,
                BOTTOM_LEFT,
                [0, 0, 0, 255],
                [128, 128, 128, 255],
            ]
            .concat();
            Ok(Decoded {
                header: self.header(file)?,
                rgba,
                icc: None,
            })
        }
    }

    fn file_with(bytes: &[u8]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), bytes).unwrap();
        f
    }

    #[test]
    fn pixels_come_through_as_decoded_when_nothing_turns_them() {
        let f = file_with(b"");
        let px = HEIC.working_with(&Fake::new(), f.path()).unwrap();
        assert_eq!((px.width, px.height, px.full_width), (3, 2, 3));
        assert_eq!(px.pixel(0, 0), TOP_LEFT);
        assert_eq!(px.pixel(2, 0), TOP_RIGHT);
        let src = HEIC.source_with(&Fake::new(), f.path()).unwrap();
        assert_eq!((src.width, src.height, src.layout), (3, 2, Layout::Rgba));
    }

    #[test]
    fn the_reported_orientation_turns_the_picture_upright() {
        let f = file_with(b"");
        let fake = Fake {
            orientation: Some(6),
            ..Fake::new()
        };
        let px = HEIC.working_with(&fake, f.path()).unwrap();
        // Orientation 6: turned a quarter clockwise, so the bottom-left
        // pixel is now top-left.
        assert_eq!((px.width, px.height), (2, 3));
        assert_eq!(px.pixel(0, 0), BOTTOM_LEFT);
        assert_eq!(px.pixel(1, 0), TOP_LEFT);
        let meta = HEIC.metadata_with(&fake, f.path()).unwrap();
        assert_eq!((meta.width, meta.height), (2, 3));
    }

    /// A TIFF-structured EXIF block holding only Orientation.
    fn exif_orientation(o: u16) -> Vec<u8> {
        let mut t = b"II*\0\x08\0\0\0".to_vec();
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend_from_slice(&0x0112u16.to_le_bytes());
        t.extend_from_slice(&3u16.to_le_bytes());
        t.extend_from_slice(&1u32.to_le_bytes());
        t.extend_from_slice(&u32::from(o).to_le_bytes());
        t.extend_from_slice(&0u32.to_le_bytes());
        t
    }

    #[test]
    fn without_a_reported_orientation_the_exif_tag_is_used() {
        let f = file_with(&heif::tests::heif_with_exif(&exif_orientation(8), false));
        let meta = HEIC.metadata_with(&Fake::new(), f.path()).unwrap();
        assert_eq!((meta.width, meta.height), (2, 3));
        assert_eq!(
            meta.exif.unwrap().int(Dir::Main, 0x0112),
            Some(8),
            "the EXIF tags reach the metadata reader"
        );
        // A reported orientation wins over the tag.
        let upright = Fake {
            orientation: Some(1),
            ..Fake::new()
        };
        let px = AVIF.working_with(&upright, f.path()).unwrap();
        assert_eq!((px.width, px.height), (3, 2));
    }

    #[test]
    fn the_size_limit_holds_before_decoding() {
        let f = file_with(b"");
        let huge = Fake {
            size: (20_000, 20_000),
            ..Fake::new()
        };
        let e = HEIC.working_with(&huge, f.path()).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::TooLarge);
        assert_eq!(
            HEIC.metadata_with(&huge, f.path()).unwrap_err().code,
            MediaErrorCode::TooLarge
        );
    }

    #[test]
    fn without_an_installed_decoder_the_handler_says_what_to_install() {
        // Tests never install a decoder, so the handler has no codec.
        let f = file_with(b"");
        let e = HEIC.working(f.path()).unwrap_err();
        assert_eq!(e.code, MediaErrorCode::Unsupported);
        assert_eq!(e.message, text::media::HEIC_UNSUPPORTED);
        let e = AVIF.metadata(f.path()).unwrap_err();
        assert_eq!(e.message, text::media::AVIF_UNSUPPORTED);
        assert_eq!(codecs(), Codecs::NONE);
    }

    #[test]
    fn each_codec_is_told_apart() {
        let only_heic = Codecs {
            heic: true,
            avif: false,
        };
        assert!(only_heic.has(SystemCodec::Heic));
        assert!(!only_heic.has(SystemCodec::Avif));
        assert!(!Codecs::NONE.has(SystemCodec::Heic));
    }
}
