//! Every EXIF tag a photo's file carries, for the details panel's EXIF
//! section and its Copy button: `photo_exif` (docs/04-browsing.md, "EXIF
//! section").
//!
//! The tags are read with `nom-exif`, which the scanner already uses for
//! video tracks: it finds the EXIF block in JPEG, HEIF and AVIF, PNG,
//! WebP, TIFF and the TIFF-based RAW files, CR3 and RAF, and it names
//! every tag it knows. When it finds nothing (a format it does not read,
//! such as JPEG XL), the file's own media handler is asked for its EXIF
//! instead, through Owlbum's small reader, so a tag the scan used is never
//! missing here. The tags of the thumbnail IFD (IFD 1) describe the
//! embedded thumbnail, not the photo, and are left out. Nothing is stored;
//! the file is read each time the section asks.

use std::path::Path;
use std::sync::Mutex;

use nom_exif::{EntryValue, ExifTag, IfdIndex, IfdKind, Rational};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::Album;
use crate::error::AppResult;
use crate::info::detail::photo_detail;
use crate::media::exif::{Dir, Value};
use crate::media::format_by_id;
use crate::scan::memory::Overlay;

#[cfg(test)]
mod tests;

/// A value longer than this many characters is cut, with "…" after it.
pub const MAX_CHARS: usize = 300;
/// An unreadable (binary) value shows at most this many bytes, in hex.
const MAX_BYTES: usize = 32;

/// One EXIF tag as the details panel lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExifField {
    /// Where the tag sits: `Image` (IFD 0), `Exif`, `GPS` or `Interop`.
    pub group: String,
    /// The tag's EXIF name, such as `ExposureTime`, or `Tag 0x1234` for
    /// one with no name.
    pub name: String,
    /// The value as text: numbers, `1/200` for a ratio, text as written,
    /// and bytes that are not text in hex.
    pub value: String,
}

/// `photo_exif`: the EXIF tags of photo `id`'s file, in file order. Empty
/// when the file has none. `photo_not_found` as for `photo_detail`.
pub fn photo_exif(album: &Album, overlay: &Mutex<Overlay>, id: i64) -> AppResult<Vec<ExifField>> {
    let detail = photo_detail(album, overlay, id)?;
    Ok(exif_fields(Path::new(&detail.abs_path), &detail.format))
}

/// The EXIF tags of the file at `path`, whose registry format id is
/// `format`. A file that cannot be read gives an empty list.
pub fn exif_fields(path: &Path, format: &str) -> Vec<ExifField> {
    let found = read_with_nom_exif(path);
    if !found.is_empty() {
        return found;
    }
    read_with_handler(path, format)
}

fn read_with_nom_exif(path: &Path) -> Vec<ExifField> {
    let Ok(iter) = nom_exif::read_exif_iter(path) else {
        return Vec::new();
    };
    iter.filter(|e| e.ifd() != IfdIndex::THUMBNAIL)
        .filter_map(|e| {
            let value = e.value()?;
            let tag = e.tag();
            let name = match tag.tag() {
                Some(t) => t.name().to_owned(),
                None => code_name(tag.code()),
            };
            Some(ExifField {
                group: kind_name(e.ifd_kind()).to_owned(),
                name,
                value: cut(entry_text(value)),
            })
        })
        .collect()
}

fn read_with_handler(path: &Path, format: &str) -> Vec<ExifField> {
    let Some(format) = format_by_id(format) else {
        return Vec::new();
    };
    let Some(exif) = format.handler.metadata(path).ok().and_then(|m| m.exif) else {
        return Vec::new();
    };
    exif.tags()
        .iter()
        .map(|(dir, code, value)| {
            let (group, kind) = match dir {
                Dir::Main => ("Image", IfdKind::Tiff),
                Dir::Exif => ("Exif", IfdKind::Exif),
                Dir::Gps => ("GPS", IfdKind::Gps),
            };
            ExifField {
                group: group.to_owned(),
                name: ExifTag::from_code_in(kind, *code).map_or_else(|| code_name(*code), |t| t.name().to_owned()),
                value: cut(value_text(value)),
            }
        })
        .collect()
}

fn kind_name(kind: IfdKind) -> &'static str {
    match kind {
        IfdKind::Tiff => "Image",
        IfdKind::Exif => "Exif",
        IfdKind::Gps => "GPS",
        IfdKind::Interop => "Interop",
        _ => "Other",
    }
}

fn code_name(code: u16) -> String {
    format!("Tag 0x{code:04X}")
}

fn ratio<T: Copy + std::fmt::Display + PartialEq + From<u8>>(r: &Rational<T>) -> String {
    if r.denominator() == T::from(1) {
        r.numerator().to_string()
    } else {
        format!("{}/{}", r.numerator(), r.denominator())
    }
}

fn list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
    items.iter().map(one).collect::<Vec<_>>().join(", ")
}

/// Bytes as text when they are printable ASCII (after any trailing NULs),
/// else as hex, at most [`MAX_BYTES`] of them.
fn bytes_text(bytes: &[u8]) -> String {
    let trimmed = bytes.iter().rposition(|&b| b != 0).map_or(&bytes[..0], |end| &bytes[..=end]);
    if !trimmed.is_empty() && trimmed.iter().all(|&b| b == b' ' || b.is_ascii_graphic()) {
        return String::from_utf8_lossy(trimmed).into_owned();
    }
    let shown = bytes.iter().take(MAX_BYTES).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
    if bytes.len() > MAX_BYTES {
        format!("{shown} … ({} bytes)", bytes.len())
    } else {
        shown
    }
}

fn entry_text(value: &EntryValue) -> String {
    match value {
        EntryValue::Text(s) => s.trim_end_matches('\0').trim().to_owned(),
        EntryValue::URational(r) => ratio(r),
        EntryValue::IRational(r) => ratio(r),
        EntryValue::NaiveDateTime(d) => d.format("%Y-%m-%d %H:%M:%S").to_string(),
        EntryValue::DateTime(d) => d.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
        EntryValue::Undefined(b) => bytes_text(b),
        EntryValue::URationalArray(v) => list(v, ratio),
        EntryValue::IRationalArray(v) => list(v, ratio),
        EntryValue::U8Array(v) => list(v, u8::to_string),
        EntryValue::U16Array(v) => list(v, u16::to_string),
        EntryValue::U32Array(v) => list(v, u32::to_string),
        other => other.to_string(),
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Text(s) => s.trim().to_owned(),
        Value::Ints(v) => list(v, i64::to_string),
        Value::Ratios(v) => list(v, |&(n, d)| if d == 1 { n.to_string() } else { format!("{n}/{d}") }),
    }
}

/// `text` cut to [`MAX_CHARS`] characters, with "…" when cut.
fn cut(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let mut short: String = text.chars().take(MAX_CHARS).collect();
    short.push('…');
    short
}
