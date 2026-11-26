//! The synthetic test albums (docs/02-data.md, "Test fixtures"):
//!
//! - [`trip`]: about two dozen files covering the cases every part needs.
//! - [`large`]: `n` small photos from a seed, spread over ten years.
//! - [`extras`]: the further files single parts' tests ask for (all eight
//!   EXIF orientations, animations, lying extensions, skipped names, a
//!   nested album root and so on).
//!
//! Integration tests call these as a library and generate what they need
//! into a temp folder. Everything is painted and encoded here, with
//! integer arithmetic and pure-Rust encoders, so the same call gives the
//! same bytes on every system. No personal photos are involved.
//!
//! ```text
//! cargo run -p owlbum-fixtures -- trip <folder>
//! cargo run -p owlbum-fixtures -- large <count> <folder> [--seed <n>]
//! cargo run -p owlbum-fixtures -- extras <folder>
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub mod cli;
pub mod dng;
pub mod encode;
pub mod icc;
pub mod mp4;
pub mod picture;
pub mod rng;
pub mod tiff;

mod extras;
mod large;
mod trip;

pub use extras::extras;
pub use large::{DEFAULT_SEED, large};
pub use trip::trip;

/// The `image` crate version the pictures are made with, for callers that
/// want to build their own small pictures the same way.
pub use image;

/// What a fixture file is there to test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// An ordinary landscape photo.
    Landscape,
    Portrait,
    Square,
    Panorama,
    /// One shot of a burst of near-duplicates (see [`Fixture::burst`]).
    Burst,
    /// A badly blurred frame.
    Blurred,
    /// Stored sideways with an EXIF orientation that turns it upright.
    ExifRotated,
    /// A byte-for-byte copy of another file.
    Duplicate,
    /// A very small picture.
    LowRes,
    /// A picture with no metadata at all.
    NoMetadata,
    /// One still picture in a format the other files do not use.
    Format,
    /// A camera RAW (DNG) file.
    Raw,
    Video,
    /// A file named as a picture whose bytes cannot be decoded.
    Corrupt,
    /// A picture format Owlbum recognises but does not support yet.
    Unsupported,
    /// One of the eight EXIF orientations (the number is in the name).
    Orientation,
    /// An animated GIF, WebP or PNG.
    Animated,
    /// A picture with transparent pixels.
    Transparent,
    /// A PNG with a wide-gamut ICC profile.
    WideGamut,
    /// A 16-bit PNG.
    SixteenBit,
    /// A file whose extension names another format than its bytes.
    WrongExtension,
    /// A header that claims more pixels than Owlbum will decode.
    TooLarge,
    /// A file or folder the walker must skip (dot names, `._` files,
    /// system folders).
    Skipped,
    /// Inside a nested album root, which the outer album skips.
    Nested,
}

/// One generated file.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixture {
    /// Path from the album root, with `/` separators.
    pub rel_path: String,
    pub kind: Kind,
    /// The format registry id its bytes really are (`jpeg`, `png`, `dng`,
    /// `mp4`, …), or `None` for bytes no format reads.
    pub format: Option<&'static str>,
    /// The upright size: after the EXIF orientation or the video's
    /// rotation. 0 when not a picture.
    pub width: u32,
    pub height: u32,
    /// The date and time the metadata gives, `YYYY-MM-DDTHH:MM:SS`.
    pub taken: Option<String>,
    /// Latitude and longitude the metadata gives, in degrees.
    pub gps: Option<(f64, f64)>,
    /// The burst this shot belongs to, when it is one.
    pub burst: Option<u32>,
}

impl Fixture {
    fn new(rel_path: impl Into<String>, kind: Kind, format: Option<&'static str>) -> Self {
        Fixture {
            rel_path: rel_path.into(),
            kind,
            format,
            width: 0,
            height: 0,
            taken: None,
            gps: None,
            burst: None,
        }
    }

    fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
}

/// A generated album: its folder and what was written into it.
#[derive(Clone, Debug)]
pub struct Album {
    pub root: PathBuf,
    /// Every file written, in the order written.
    pub files: Vec<Fixture>,
}

impl Album {
    /// The file at `rel_path`, if there is one.
    pub fn get(&self, rel_path: &str) -> Option<&Fixture> {
        self.files.iter().find(|f| f.rel_path == rel_path)
    }

    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Fixture> {
        self.files.iter().filter(move |f| f.kind == kind)
    }

    pub fn path(&self, f: &Fixture) -> PathBuf {
        self.root.join(&f.rel_path)
    }
}

/// Writes files into a folder that was empty, refusing to touch one that
/// was not, so a mistyped path never overwrites someone's photos.
struct Writer {
    root: PathBuf,
    files: Vec<Fixture>,
}

impl Writer {
    fn new(root: &Path) -> io::Result<Self> {
        if root.exists() {
            if !root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} is not a folder", root.display()),
                ));
            }
            if fs::read_dir(root)?.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "{} is not empty; fixtures are only written into an empty folder",
                        root.display()
                    ),
                ));
            }
        }
        fs::create_dir_all(root)?;
        Ok(Writer {
            root: root.to_path_buf(),
            files: Vec::new(),
        })
    }

    fn put(&mut self, f: Fixture, bytes: &[u8]) -> io::Result<()> {
        let path = self.root.join(&f.rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        self.files.push(f);
        Ok(())
    }

    fn finish(self) -> Album {
        Album {
            root: self.root,
            files: self.files,
        }
    }
}

/// `YYYY:MM:DD HH:MM:SS` (EXIF) as `YYYY-MM-DDTHH:MM:SS`.
fn iso(exif_time: &str) -> String {
    let b = exif_time.as_bytes();
    format!(
        "{}-{}-{}T{}",
        &exif_time[0..4],
        &exif_time[5..7],
        &exif_time[8..10],
        std::str::from_utf8(&b[11..]).expect("ascii")
    )
}

/// Millionths of a degree as degrees.
fn degrees(gps: Option<(i32, i32)>) -> Option<(f64, f64)> {
    gps.map(|(a, b)| (f64::from(a) / 1e6, f64::from(b) / 1e6))
}
