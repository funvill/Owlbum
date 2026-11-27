//! The metadata an edited file keeps (docs/06-editing-and-file-actions.md,
//! "Output"): the original's EXIF block with Orientation 1, the new pixel
//! size and no embedded thumbnail, and its XMP packet as it is.
//!
//! Pure Rust, no extra crate (decided on #150): `little_exif`, named in the
//! architecture, could not drop the embedded thumbnail, and an EXIF block
//! is a small TIFF structure that this module rewrites in two ways.
//!
//! - **Kept in place** for a block that a JPEG, PNG or WebP carries. The
//!   original bytes stay where they are, so every value that lives at an
//!   offset (the maker note above all) still reads correctly; a new IFD 0
//!   and a new Exif IFD are appended, the header is pointed at the new IFD
//!   0, and IFD 1 (the thumbnail) is unlinked and its bytes are cleared.
//! - **Compact** for a TIFF-structured file (TIFF, DNG and the TIFF-based
//!   RAW formats), whose "EXIF" is the whole file. Only IFD 0's descriptive
//!   tags, the Exif IFD (without the maker note, whose offsets cannot be
//!   moved) and the GPS IFD are copied into a fresh block. A kept-in-place
//!   block too large for a JPEG segment is made compact too.

use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

/// What an edited file keeps from its original.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The original's EXIF block (a TIFF structure, without `Exif\0\0`).
    exif: Option<Exif>,
    /// The original's XMP packet, unchanged.
    pub xmp: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Exif {
    /// A block carried by a JPEG, PNG or WebP: rewritten in place.
    Block(Vec<u8>),
    /// A block already made compact from a TIFF-structured file.
    Compact(Tree),
}

/// What the edit changed, for the EXIF rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    /// The written picture's size.
    pub width: u32,
    pub height: u32,
    /// True when the pixels were converted to sRGB (a colour edit): the
    /// ColorSpace tag is set to sRGB.
    pub srgb: bool,
}

const TAG_IMAGE_WIDTH: u16 = 0x0100;
const TAG_IMAGE_LENGTH: u16 = 0x0101;
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_XMP: u16 = 0x02bc;
const TAG_EXIF_IFD: u16 = 0x8769;
const TAG_GPS_IFD: u16 = 0x8825;
const TAG_MAKER_NOTE: u16 = 0x927c;
const TAG_COLOR_SPACE: u16 = 0xa001;
const TAG_PIXEL_X: u16 = 0xa002;
const TAG_PIXEL_Y: u16 = 0xa003;
const TAG_INTEROP_IFD: u16 = 0xa005;
const TAG_THUMB_OFFSET: u16 = 0x0201;
const TAG_THUMB_LENGTH: u16 = 0x0202;

const SHORT: u16 = 3;
const LONG: u16 = 4;

/// IFD 0 tags a compact block keeps: what describes the photo, not how a
/// TIFF file stores its pixels.
const IFD0_KEEP: [u16; 13] = [
    0x010e, // ImageDescription
    0x010f, // Make
    0x0110, // Model
    0x011a, // XResolution
    0x011b, // YResolution
    0x0128, // ResolutionUnit
    0x0131, // Software
    0x0132, // DateTime
    0x013b, // Artist
    0x0213, // YCbCrPositioning
    0x8298, // Copyright
    0x9c9b, // XPTitle
    0x9c9f, // XPSubject
];

/// The largest single value copied into a compact block.
const MAX_VALUE: usize = 64 * 1024;
/// The largest XMP packet read from a TIFF file's tag 700.
const MAX_XMP: usize = 4 * 1024 * 1024;
/// The most entries an IFD may have before it is taken to be damaged.
const MAX_ENTRIES: u16 = 1024;
/// The largest EXIF block a JPEG APP1 segment holds (with `Exif\0\0`).
pub const JPEG_EXIF_MAX: usize = 65533 - 6;
const XMP_JPEG_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_PNG_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// Reads what an edited copy of `file` keeps. Never fails: a file whose
/// metadata cannot be read gives nothing to keep.
pub fn read(file: &Path) -> Kept {
    let Ok(mut f) = fs::File::open(file) else {
        return Kept::default();
    };
    let mut head = [0u8; 12];
    if f.read_exact(&mut head).is_err() {
        return Kept::default();
    }
    if head.starts_with(b"II") || head.starts_with(b"MM") {
        // TIFF, DNG and the TIFF-based RAW formats (whose magic number
        // differs, so it is not checked).
        let mut r = std::io::BufReader::new(f);
        return compact_from(&mut r).unwrap_or_default();
    }
    let is_jpeg = head.starts_with(&[0xff, 0xd8]);
    let is_png = head.starts_with(b"\x89PNG");
    let is_webp = head.starts_with(b"RIFF") && &head[8..12] == b"WEBP";
    if !(is_jpeg || is_png || is_webp) {
        return Kept::default();
    }
    drop(f);
    let Ok(bytes) = fs::read(file) else {
        return Kept::default();
    };
    let (exif, xmp) = if is_jpeg {
        jpeg_blocks(&bytes)
    } else if is_png {
        png_blocks(&bytes)
    } else {
        webp_blocks(&bytes)
    };
    Kept {
        exif: exif.filter(|b| is_tiff(b)).map(Exif::Block),
        xmp,
    }
}

fn is_tiff(b: &[u8]) -> bool {
    b.len() >= 8 && (b.starts_with(b"II*\0") || b.starts_with(b"MM\0*"))
}

impl Kept {
    /// True when there is nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.exif.is_none() && self.xmp.is_none()
    }

    /// The EXIF block for the edited file, rewritten for `changes`, at
    /// most `limit` bytes. None when the original has none, or when even a
    /// compact block does not fit.
    pub fn exif_for(&self, changes: Changes, limit: usize) -> Option<Vec<u8>> {
        match self.exif.as_ref()? {
            Exif::Block(block) => {
                if let Some(out) = rewrite_in_place(block, changes)
                    && out.len() <= limit
                {
                    return Some(out);
                }
                let tree = compact_from(&mut Cursor::new(block))?.exif?;
                match tree {
                    Exif::Compact(t) => t.write(changes).filter(|b| b.len() <= limit),
                    Exif::Block(_) => None,
                }
            }
            Exif::Compact(t) => t.write(changes).filter(|b| b.len() <= limit),
        }
    }
}

// ---------------------------------------------------------------------------
// Finding the blocks
// ---------------------------------------------------------------------------

/// The first EXIF block and XMP packet of a JPEG's APP1 segments.
fn jpeg_blocks(b: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (mut exif, mut xmp) = (None, None);
    let mut at = 2;
    while at + 4 <= b.len() {
        if b[at] != 0xff {
            break;
        }
        let marker = b[at + 1];
        if marker == 0xff {
            at += 1; // a fill byte
            continue;
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            at += 2;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            break; // pixel data starts: no more metadata segments
        }
        let len = usize::from(u16::from_be_bytes([b[at + 2], b[at + 3]]));
        let Some(data) = b.get(at + 4..at + 2 + len) else {
            break;
        };
        if marker == 0xe1 {
            if exif.is_none()
                && let Some(rest) = data.strip_prefix(b"Exif\0\0")
            {
                exif = Some(rest.to_vec());
            } else if xmp.is_none()
                && let Some(rest) = data.strip_prefix(XMP_JPEG_PREFIX)
            {
                xmp = Some(rest.to_vec());
            }
        }
        at += 2 + len.max(2);
    }
    (exif, xmp)
}

/// A PNG's `eXIf` chunk and its XMP packet (an uncompressed `iTXt` chunk
/// with the keyword `XML:com.adobe.xmp`).
fn png_blocks(b: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (mut exif, mut xmp) = (None, None);
    let mut at = 8;
    while at + 12 <= b.len() {
        let len = u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
        let kind = &b[at + 4..at + 8];
        let Some(data) = b.get(at + 8..at + 8 + len) else {
            break;
        };
        match kind {
            b"eXIf" if exif.is_none() => exif = Some(data.to_vec()),
            b"iTXt" if xmp.is_none() => xmp = png_xmp(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    (exif, xmp)
}

fn png_xmp(data: &[u8]) -> Option<Vec<u8>> {
    let rest = data.strip_prefix(XMP_PNG_KEYWORD)?.strip_prefix(b"\0")?;
    // Compression flag and method: only an uncompressed packet is read.
    let (&flag, rest) = rest.split_first()?;
    if flag != 0 {
        return None;
    }
    let rest = &rest[1..];
    let lang_end = rest.iter().position(|&c| c == 0)?;
    let rest = &rest[lang_end + 1..];
    let key_end = rest.iter().position(|&c| c == 0)?;
    Some(rest[key_end + 1..].to_vec())
}

/// A WebP's `EXIF` and `XMP ` chunks.
fn webp_blocks(b: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let (mut exif, mut xmp) = (None, None);
    let mut at = 12;
    while at + 8 <= b.len() {
        let kind = &b[at..at + 4];
        let len = u32::from_le_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        let Some(data) = b.get(at + 8..at + 8 + len) else {
            break;
        };
        match kind {
            // Some writers keep the JPEG segment's `Exif\0\0` in the chunk.
            b"EXIF" => exif = Some(data.strip_prefix(b"Exif\0\0").unwrap_or(data).to_vec()),
            b"XMP " => xmp = Some(data.to_vec()),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    (exif, xmp)
}

// ---------------------------------------------------------------------------
// Reading IFDs
// ---------------------------------------------------------------------------

/// One 12-byte IFD entry as stored: the value field holds the value when
/// it fits in 4 bytes, else its offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Raw {
    tag: u16,
    kind: u16,
    count: u32,
    field: [u8; 4],
}

impl Raw {
    fn size(&self) -> Option<usize> {
        let unit = match self.kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        (self.count as usize).checked_mul(unit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Order {
    le: bool,
}

impl Order {
    fn u16(self, b: [u8; 2]) -> u16 {
        if self.le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    }
    fn u32(self, b: [u8; 4]) -> u32 {
        if self.le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    }
    fn put16(self, v: u16) -> [u8; 2] {
        if self.le {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    fn put32(self, v: u32) -> [u8; 4] {
        if self.le {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    }
    /// A SHORT or LONG value, left-justified in a value field.
    fn field(self, kind: u16, v: u32) -> [u8; 4] {
        if kind == SHORT {
            let s = self.put16(u16::try_from(v).unwrap_or(u16::MAX));
            [s[0], s[1], 0, 0]
        } else {
            self.put32(v)
        }
    }
}

fn order_of<R: Read + Seek>(r: &mut R) -> Option<(Order, u32)> {
    let mut h = [0u8; 8];
    r.seek(SeekFrom::Start(0)).ok()?;
    r.read_exact(&mut h).ok()?;
    let o = match &h[..2] {
        b"II" => Order { le: true },
        b"MM" => Order { le: false },
        _ => return None,
    };
    Some((o, o.u32([h[4], h[5], h[6], h[7]])))
}

/// The entries of the IFD at `at` and the offset of the next IFD.
fn read_ifd<R: Read + Seek>(r: &mut R, o: Order, at: u32) -> Option<(Vec<Raw>, u32)> {
    r.seek(SeekFrom::Start(u64::from(at))).ok()?;
    let mut n = [0u8; 2];
    r.read_exact(&mut n).ok()?;
    let n = o.u16(n);
    if n > MAX_ENTRIES {
        return None;
    }
    let mut buf = vec![0u8; usize::from(n) * 12 + 4];
    r.read_exact(&mut buf).ok()?;
    let entries = buf
        .chunks_exact(12)
        .map(|e| Raw {
            tag: o.u16([e[0], e[1]]),
            kind: o.u16([e[2], e[3]]),
            count: o.u32([e[4], e[5], e[6], e[7]]),
            field: [e[8], e[9], e[10], e[11]],
        })
        .collect();
    let t = &buf[buf.len() - 4..];
    Some((entries, o.u32([t[0], t[1], t[2], t[3]])))
}

/// The bytes of an entry's value, at most `cap` long.
fn value_of<R: Read + Seek>(r: &mut R, o: Order, e: &Raw, cap: usize) -> Option<Vec<u8>> {
    let size = e.size()?;
    if size > cap {
        return None;
    }
    if size <= 4 {
        return Some(e.field[..size].to_vec());
    }
    r.seek(SeekFrom::Start(u64::from(o.u32(e.field)))).ok()?;
    let mut v = vec![0u8; size];
    r.read_exact(&mut v).ok()?;
    Some(v)
}

fn pointer(o: Order, entries: &[Raw], tag: u16) -> Option<u32> {
    entries
        .iter()
        .find(|e| e.tag == tag && (e.kind == LONG || e.kind == 13) && e.count == 1)
        .map(|e| o.u32(e.field))
}

// ---------------------------------------------------------------------------
// Kept in place
// ---------------------------------------------------------------------------

/// Sets (or adds) an entry, keeping the list sorted by tag as TIFF wants.
fn set(entries: &mut Vec<Raw>, o: Order, tag: u16, kind: u16, v: u32) {
    let e = Raw {
        tag,
        kind,
        count: 1,
        field: o.field(kind, v),
    };
    match entries.binary_search_by_key(&tag, |e| e.tag) {
        Ok(i) => entries[i] = e,
        Err(i) => entries.insert(i, e),
    }
}

/// Sets `tag` only when it is already there.
fn replace(entries: &mut [Raw], o: Order, tag: u16, v: u32) {
    if let Some(e) = entries.iter_mut().find(|e| e.tag == tag) {
        let kind = if e.kind == SHORT { SHORT } else { LONG };
        *e = Raw {
            tag,
            kind,
            count: 1,
            field: o.field(kind, v),
        };
    }
}

fn exif_changes(o: Order, exif: &mut Vec<Raw>, c: Changes) {
    set(exif, o, TAG_PIXEL_X, LONG, c.width);
    set(exif, o, TAG_PIXEL_Y, LONG, c.height);
    if c.srgb {
        set(exif, o, TAG_COLOR_SPACE, SHORT, 1);
    }
}

/// Appends an IFD of `entries` (each value field already final) to `out`,
/// at an even offset. Answers its offset.
fn push_ifd(out: &mut Vec<u8>, o: Order, entries: &[Raw], next: u32) -> Option<u32> {
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let at = u32::try_from(out.len()).ok()?;
    out.extend(o.put16(u16::try_from(entries.len()).ok()?));
    for e in entries {
        out.extend(o.put16(e.tag));
        out.extend(o.put16(e.kind));
        out.extend(o.put32(e.count));
        out.extend(e.field);
    }
    out.extend(o.put32(next));
    Some(at)
}

/// The block with a new IFD 0 and Exif IFD appended (see the module notes).
fn rewrite_in_place(block: &[u8], c: Changes) -> Option<Vec<u8>> {
    let mut r = Cursor::new(block);
    let (o, ifd0_at) = order_of(&mut r)?;
    let (mut ifd0, ifd1_at) = read_ifd(&mut r, o, ifd0_at)?;
    let exif_at = pointer(o, &ifd0, TAG_EXIF_IFD);
    let mut exif = match exif_at {
        Some(at) => read_ifd(&mut r, o, at)?.0,
        None => Vec::new(),
    };
    let mut out = block.to_vec();
    // The embedded thumbnail: cleared, or cut off when it ends the block.
    if ifd1_at != 0
        && let Some((ifd1, _)) = read_ifd(&mut r, o, ifd1_at)
        && let (Some(off), Some(len)) = (
            ifd1.iter().find(|e| e.tag == TAG_THUMB_OFFSET),
            ifd1.iter().find(|e| e.tag == TAG_THUMB_LENGTH),
        )
    {
        let start = o.u32(off.field) as usize;
        let end = start.saturating_add(o.u32(len.field) as usize);
        if end == out.len() && start <= out.len() {
            out.truncate(start);
        } else if end <= out.len() {
            out[start..end].fill(0);
        }
    }
    ifd0.sort_by_key(|e| e.tag);
    exif.sort_by_key(|e| e.tag);
    set(&mut ifd0, o, TAG_ORIENTATION, SHORT, 1);
    replace(&mut ifd0, o, TAG_IMAGE_WIDTH, c.width);
    replace(&mut ifd0, o, TAG_IMAGE_LENGTH, c.height);
    exif_changes(o, &mut exif, c);
    let new_exif = push_ifd(&mut out, o, &exif, 0)?;
    set(&mut ifd0, o, TAG_EXIF_IFD, LONG, new_exif);
    let new_ifd0 = push_ifd(&mut out, o, &ifd0, 0)?;
    out[4..8].copy_from_slice(&o.put32(new_ifd0));
    Some(out)
}

// ---------------------------------------------------------------------------
// Compact
// ---------------------------------------------------------------------------

/// One entry of a compact block with its value bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    value: Vec<u8>,
}

/// The tags a compact block copies, values in the source's byte order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tree {
    order: Order,
    ifd0: Vec<Entry>,
    exif: Vec<Entry>,
    gps: Vec<Entry>,
}

fn entries<R: Read + Seek>(
    r: &mut R,
    o: Order,
    raw: &[Raw],
    keep: impl Fn(u16) -> bool,
) -> Vec<Entry> {
    let mut out: Vec<Entry> = raw
        .iter()
        .filter(|e| keep(e.tag))
        .filter_map(|e| {
            Some(Entry {
                tag: e.tag,
                kind: e.kind,
                count: e.count,
                value: value_of(r, o, e, MAX_VALUE)?,
            })
        })
        .collect();
    out.sort_by_key(|e| e.tag);
    out.dedup_by_key(|e| e.tag);
    out
}

/// Reads the tags a compact block keeps, and IFD 0's XMP (tag 700).
fn compact_from<R: Read + Seek>(r: &mut R) -> Option<Kept> {
    let (o, at) = order_of(r)?;
    let (ifd0, _) = read_ifd(r, o, at)?;
    let xmp = ifd0
        .iter()
        .find(|e| e.tag == TAG_XMP)
        .and_then(|e| value_of(r, o, e, MAX_XMP));
    let pointers = [TAG_EXIF_IFD, TAG_GPS_IFD, TAG_INTEROP_IFD];
    let exif = match pointer(o, &ifd0, TAG_EXIF_IFD).and_then(|p| read_ifd(r, o, p)) {
        Some((raw, _)) => entries(r, o, &raw, |t| {
            t != TAG_MAKER_NOTE && !pointers.contains(&t)
        }),
        None => Vec::new(),
    };
    let gps = match pointer(o, &ifd0, TAG_GPS_IFD).and_then(|p| read_ifd(r, o, p)) {
        Some((raw, _)) => entries(r, o, &raw, |_| true),
        None => Vec::new(),
    };
    let main = entries(r, o, &ifd0, |t| IFD0_KEEP.contains(&t));
    let tree = Tree {
        order: o,
        ifd0: main,
        exif,
        gps,
    };
    let empty = tree.ifd0.is_empty() && tree.exif.is_empty() && tree.gps.is_empty();
    Some(Kept {
        exif: (!empty).then_some(Exif::Compact(tree)),
        xmp,
    })
}

impl Tree {
    /// A fresh block: header, IFD 0, Exif IFD, GPS IFD, each followed by
    /// its values.
    fn write(&self, c: Changes) -> Option<Vec<u8>> {
        let o = self.order;
        let mut out = Vec::new();
        out.extend(if o.le { b"II*\0" } else { b"MM\0*" });
        out.extend(o.put32(8));
        let entry = |tag, kind, v: u32| Entry {
            tag,
            kind,
            count: 1,
            value: o.field(kind, v)[..if kind == SHORT { 2 } else { 4 }].to_vec(),
        };
        let mut ifd0 = self.ifd0.clone();
        let mut exif = self.exif.clone();
        let upsert =
            |list: &mut Vec<Entry>, e: Entry| match list.binary_search_by_key(&e.tag, |x| x.tag) {
                Ok(i) => list[i] = e,
                Err(i) => list.insert(i, e),
            };
        upsert(&mut ifd0, entry(TAG_ORIENTATION, SHORT, 1));
        upsert(&mut exif, entry(TAG_PIXEL_X, LONG, c.width));
        upsert(&mut exif, entry(TAG_PIXEL_Y, LONG, c.height));
        if c.srgb {
            upsert(&mut exif, entry(TAG_COLOR_SPACE, SHORT, 1));
        }
        // Pointers are placeholders until the sub-IFDs are written.
        upsert(&mut ifd0, entry(TAG_EXIF_IFD, LONG, 0));
        if !self.gps.is_empty() {
            upsert(&mut ifd0, entry(TAG_GPS_IFD, LONG, 0));
        }
        let ifd0_fields = write_ifd(&mut out, o, &ifd0)?;
        let exif_at = write_ifd(&mut out, o, &exif)?.0;
        patch(
            &mut out,
            &ifd0,
            &ifd0_fields.1,
            TAG_EXIF_IFD,
            o.put32(exif_at),
        );
        if !self.gps.is_empty() {
            let gps_at = write_ifd(&mut out, o, &self.gps)?.0;
            patch(
                &mut out,
                &ifd0,
                &ifd0_fields.1,
                TAG_GPS_IFD,
                o.put32(gps_at),
            );
        }
        Some(out)
    }
}

fn patch(out: &mut [u8], list: &[Entry], fields: &[usize], tag: u16, v: [u8; 4]) {
    if let Some(i) = list.iter().position(|e| e.tag == tag) {
        out[fields[i]..fields[i] + 4].copy_from_slice(&v);
    }
}

/// Writes an IFD and its out-of-line values. Answers its offset and where
/// each entry's value field is, for pointers patched later.
fn write_ifd(out: &mut Vec<u8>, o: Order, list: &[Entry]) -> Option<(u32, Vec<usize>)> {
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let at = out.len();
    let mut data_at = at + 2 + list.len() * 12 + 4;
    let mut data = Vec::new();
    let mut fields = Vec::with_capacity(list.len());
    out.extend(o.put16(u16::try_from(list.len()).ok()?));
    for e in list {
        out.extend(o.put16(e.tag));
        out.extend(o.put16(e.kind));
        out.extend(o.put32(e.count));
        fields.push(out.len());
        if e.value.len() <= 4 {
            let mut f = [0u8; 4];
            f[..e.value.len()].copy_from_slice(&e.value);
            out.extend(f);
        } else {
            out.extend(o.put32(u32::try_from(data_at).ok()?));
            data.extend_from_slice(&e.value);
            if e.value.len() % 2 == 1 {
                data.push(0);
            }
            data_at += e.value.len().div_ceil(2) * 2;
        }
    }
    out.extend(o.put32(0));
    out.extend(data);
    Some((u32::try_from(at).ok()?, fields))
}

// ---------------------------------------------------------------------------
// Writing into a PNG
// ---------------------------------------------------------------------------

/// CRC-32 as PNG uses it.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for part in parts {
        for &b in *part {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
    }
    !c
}

/// Inserts an uncompressed `iTXt` chunk holding `xmp` after a PNG's IHDR.
pub fn png_with_xmp(png: Vec<u8>, xmp: &[u8]) -> Vec<u8> {
    // Signature (8) and IHDR (4 length, 4 type, 13 data, 4 CRC).
    const AFTER_IHDR: usize = 33;
    if png.len() < AFTER_IHDR || &png[12..16] != b"IHDR" {
        return png;
    }
    let mut data = Vec::with_capacity(xmp.len() + 24);
    data.extend_from_slice(XMP_PNG_KEYWORD);
    data.extend_from_slice(&[0, 0, 0, 0, 0]); // NUL, no compression, method, no language, no keyword
    data.extend_from_slice(xmp);
    let Ok(len) = u32::try_from(data.len()) else {
        return png;
    };
    let mut out = Vec::with_capacity(png.len() + data.len() + 12);
    out.extend_from_slice(&png[..AFTER_IHDR]);
    out.extend(len.to_be_bytes());
    out.extend_from_slice(b"iTXt");
    out.extend_from_slice(&data);
    out.extend(crc32(&[b"iTXt", &data]).to_be_bytes());
    out.extend_from_slice(&png[AFTER_IHDR..]);
    out
}

/// The XMP packet of a PNG, JPEG or WebP in memory, for tests and checks.
pub fn xmp_of(bytes: &[u8]) -> Option<Vec<u8>> {
    blocks_of(bytes).1
}

/// The EXIF block of a PNG, JPEG or WebP in memory, for tests and checks.
pub fn exif_of(bytes: &[u8]) -> Option<Vec<u8>> {
    blocks_of(bytes).0
}

fn blocks_of(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_blocks(bytes)
    } else if bytes.starts_with(b"\x89PNG") {
        png_blocks(bytes)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        webp_blocks(bytes)
    } else {
        (None, None)
    }
}

/// The JPEG thumbnail IFD 1 points to, if any; for tests.
pub fn has_thumbnail(block: &[u8]) -> bool {
    let mut r = Cursor::new(block);
    let Some((o, at)) = order_of(&mut r) else {
        return false;
    };
    let Some((_, ifd1)) = read_ifd(&mut r, o, at) else {
        return false;
    };
    ifd1 != 0
        && read_ifd(&mut r, o, ifd1)
            .is_some_and(|(e, _)| e.iter().any(|e| e.tag == TAG_THUMB_OFFSET))
}

#[cfg(test)]
mod tests;
