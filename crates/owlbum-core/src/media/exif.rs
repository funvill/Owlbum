//! A small EXIF reader (docs/03-media-and-scanning.md, "Metadata reading").
//!
//! EXIF is a TIFF structure: a byte-order mark, an offset to IFD 0, and
//! IFDs of 12-byte entries whose values sit inline or at an offset. This
//! reads IFD 0 and the two sub-IFDs it points to (Exif and GPS) and keeps
//! every numeric and text value of at most [`MAX_VALUE`] bytes. It reads
//! with seeks, so the same code reads an EXIF block in memory and a whole
//! TIFF file on disk without loading the file.
//!
//! Why not `nom-exif`: it parses the three date tags with a strict
//! `%Y:%m:%d %H:%M:%S` and drops a value that does not match, so the date
//! rule's other forms (dashes, the `T` form, trailing text) could never be
//! seen. It also wants a TIFF file in memory. The rules applied to these
//! values are in `scan::metadata`.

use std::io::{Cursor, Read, Seek, SeekFrom};

/// Values longer than this are skipped (maker notes, thumbnails).
pub const MAX_VALUE: u32 = 4096;
/// IFDs with more entries than this are taken to be damaged.
const MAX_ENTRIES: u16 = 1024;

/// Which IFD a tag was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    /// IFD 0: Make, Model, DateTime, Orientation.
    Main,
    /// The Exif sub-IFD (tag 0x8769): exposure, dates, lens.
    Exif,
    /// The GPS sub-IFD (tag 0x8825).
    Gps,
}

/// One tag's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// ASCII, up to the first NUL. Bytes that are not UTF-8 are replaced.
    Text(String),
    /// BYTE, SHORT, LONG and their signed forms.
    Ints(Vec<i64>),
    /// RATIONAL and SRATIONAL as numerator and denominator.
    Ratios(Vec<(i64, i64)>),
}

impl Value {
    /// The value at `i` as a number. A rational with a zero denominator is
    /// nonsense and gives None.
    pub fn number(&self, i: usize) -> Option<f64> {
        match self {
            Value::Text(_) => None,
            #[allow(clippy::cast_precision_loss)]
            Value::Ints(v) => v.get(i).map(|&n| n as f64),
            #[allow(clippy::cast_precision_loss)]
            Value::Ratios(v) => v
                .get(i)
                .and_then(|&(n, d)| (d != 0).then(|| n as f64 / d as f64)),
        }
    }

    /// Every value as a number (None for a zero denominator).
    pub fn numbers(&self) -> Vec<Option<f64>> {
        let n = match self {
            Value::Text(_) => 0,
            Value::Ints(v) => v.len(),
            Value::Ratios(v) => v.len(),
        };
        (0..n).map(|i| self.number(i)).collect()
    }

    /// The first integer, for SHORT and LONG tags.
    pub fn int(&self) -> Option<i64> {
        match self {
            Value::Ints(v) => v.first().copied(),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// The tags read from one EXIF block or TIFF file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exif {
    tags: Vec<(Dir, u16, Value)>,
}

impl Exif {
    /// Reads an EXIF block: a TIFF structure from its header on. A leading
    /// `Exif\0\0` (as some PNG writers leave it) is skipped. None when the
    /// bytes do not start with a TIFF header.
    pub fn parse(block: &[u8]) -> Option<Exif> {
        let block = block.strip_prefix(b"Exif\0\0").unwrap_or(block);
        Self::read(&mut Cursor::new(block))
    }

    /// Reads the TIFF structure that starts at the reader's position 0: an
    /// EXIF block, or a TIFF file. Only the IFD entries and the values kept
    /// are read. A damaged entry or IFD ends that IFD; what was read stays.
    pub fn read<R: Read + Seek>(r: &mut R) -> Option<Exif> {
        let mut head = [0u8; 8];
        r.seek(SeekFrom::Start(0)).ok()?;
        r.read_exact(&mut head).ok()?;
        let order = match &head[..4] {
            b"II*\0" => Order::Little,
            b"MM\0*" => Order::Big,
            _ => return None,
        };
        let mut reader = Ifds { r, order };
        let mut exif = Exif::default();
        let ifd0 = order.u32(&head[4..8]);
        let pointers = reader.ifd(ifd0, Dir::Main, &mut exif.tags);
        for (tag, dir) in [(0x8769, Dir::Exif), (0x8825, Dir::Gps)] {
            if let Some(&(_, at)) = pointers.iter().find(|(t, _)| *t == tag) {
                reader.ifd(at, dir, &mut exif.tags);
            }
        }
        Some(exif)
    }

    pub fn get(&self, dir: Dir, tag: u16) -> Option<&Value> {
        self.tags
            .iter()
            .find(|(d, t, _)| *d == dir && *t == tag)
            .map(|(_, _, v)| v)
    }

    /// A text tag, trimmed; None when absent or empty.
    pub fn text(&self, dir: Dir, tag: u16) -> Option<&str> {
        self.get(dir, tag)
            .and_then(Value::text)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// The first value of a numeric tag.
    pub fn number(&self, dir: Dir, tag: u16) -> Option<f64> {
        self.get(dir, tag).and_then(|v| v.number(0))
    }

    /// The first value of an integer tag.
    pub fn int(&self, dir: Dir, tag: u16) -> Option<i64> {
        self.get(dir, tag).and_then(Value::int)
    }

    /// A tag set by hand: for tests and for handlers that read a field from
    /// another place than a TIFF structure.
    pub fn set(&mut self, dir: Dir, tag: u16, value: Value) {
        self.tags.retain(|(d, t, _)| !(*d == dir && *t == tag));
        self.tags.push((dir, tag, value));
    }
}

#[derive(Debug, Clone, Copy)]
enum Order {
    Little,
    Big,
}

impl Order {
    fn u16(self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        match self {
            Order::Little => u16::from_le_bytes(b),
            Order::Big => u16::from_be_bytes(b),
        }
    }

    fn u32(self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        match self {
            Order::Little => u32::from_le_bytes(b),
            Order::Big => u32::from_be_bytes(b),
        }
    }
}

struct Ifds<'a, R> {
    r: &'a mut R,
    order: Order,
}

/// The byte size of one value of a TIFF field type, for the types kept.
fn type_size(kind: u16) -> Option<u32> {
    match kind {
        1 | 2 | 6 => Some(1),  // BYTE, ASCII, SBYTE
        3 | 8 => Some(2),      // SHORT, SSHORT
        4 | 9 => Some(4),      // LONG, SLONG
        5 | 10 => Some(8),     // RATIONAL, SRATIONAL
        _ => None,             // UNDEFINED, FLOAT, DOUBLE, IFD: not used
    }
}

impl<R: Read + Seek> Ifds<'_, R> {
    /// Reads one IFD at `at` into `out`. Returns the LONG values of its
    /// sub-IFD pointer tags (0x8769 and 0x8825), which are not kept as
    /// values.
    fn ifd(&mut self, at: u32, dir: Dir, out: &mut Vec<(Dir, u16, Value)>) -> Vec<(u16, u32)> {
        let mut pointers = Vec::new();
        let Some(entries) = self.entries(at) else {
            return pointers;
        };
        for e in entries.chunks_exact(12) {
            let o = self.order;
            let (tag, kind, count) = (o.u16(&e[0..2]), o.u16(&e[2..4]), o.u32(&e[4..8]));
            if dir == Dir::Main && (tag == 0x8769 || tag == 0x8825) {
                if count >= 1 && (kind == 4 || kind == 13) {
                    pointers.push((tag, o.u32(&e[8..12])));
                }
                continue;
            }
            let Some(size) = type_size(kind) else {
                continue;
            };
            let Some(len) = size.checked_mul(count).filter(|&l| l > 0 && l <= MAX_VALUE) else {
                continue;
            };
            let bytes = if len <= 4 {
                e[8..8 + len as usize].to_vec()
            } else {
                match self.bytes(o.u32(&e[8..12]), len) {
                    Some(b) => b,
                    None => continue,
                }
            };
            if let Some(v) = self.value(kind, &bytes) {
                out.push((dir, tag, v));
            }
        }
        pointers
    }

    /// The raw 12-byte entries of the IFD at `at`.
    fn entries(&mut self, at: u32) -> Option<Vec<u8>> {
        let count = self.bytes(at, 2)?;
        let count = self.order.u16(&count);
        if count == 0 || count > MAX_ENTRIES {
            return None;
        }
        // A short read keeps the whole entries that were read.
        let mut buf = Vec::with_capacity(usize::from(count) * 12);
        self.r
            .by_ref()
            .take(u64::from(count) * 12)
            .read_to_end(&mut buf)
            .ok()?;
        buf.truncate(buf.len() / 12 * 12);
        Some(buf)
    }

    fn bytes(&mut self, at: u32, len: u32) -> Option<Vec<u8>> {
        self.r.seek(SeekFrom::Start(u64::from(at))).ok()?;
        let mut buf = vec![0u8; len as usize];
        self.r.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn value(&self, kind: u16, b: &[u8]) -> Option<Value> {
        let o = self.order;
        Some(match kind {
            1 => Value::Ints(b.iter().map(|&x| i64::from(x)).collect()),
            6 => Value::Ints(b.iter().map(|&x| i64::from(x as i8)).collect()),
            2 => {
                let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                Value::Text(String::from_utf8_lossy(&b[..end]).into_owned())
            }
            3 => Value::Ints(b.chunks_exact(2).map(|c| i64::from(o.u16(c))).collect()),
            8 => Value::Ints(
                b.chunks_exact(2)
                    .map(|c| i64::from(o.u16(c).cast_signed()))
                    .collect(),
            ),
            4 => Value::Ints(b.chunks_exact(4).map(|c| i64::from(o.u32(c))).collect()),
            9 => Value::Ints(
                b.chunks_exact(4)
                    .map(|c| i64::from(o.u32(c).cast_signed()))
                    .collect(),
            ),
            5 => Value::Ratios(
                b.chunks_exact(8)
                    .map(|c| (i64::from(o.u32(&c[..4])), i64::from(o.u32(&c[4..]))))
                    .collect(),
            ),
            10 => Value::Ratios(
                b.chunks_exact(8)
                    .map(|c| {
                        (
                            i64::from(o.u32(&c[..4]).cast_signed()),
                            i64::from(o.u32(&c[4..]).cast_signed()),
                        )
                    })
                    .collect(),
            ),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use owlbum_fixtures::encode::Meta;
    use owlbum_fixtures::tiff::{self, Ifd, Val};

    use super::*;

    fn trip_meta() -> Meta {
        Meta {
            make: Some("Owlbum"),
            model: Some("Fixture One"),
            lens: Some("Fixture 24-70mm F2.8"),
            orientation: 6,
            taken: Some("2025:07:14 09:12:00".into()),
            offset: Some("-07:00"),
            exposure: Some([1, 250]),
            f_number_x10: Some(56),
            iso: Some(100),
            focal_mm: Some(35),
            flash: Some(16),
            gps: Some((49_341_210, -123_002_330)),
            pixels: Some((640, 480)),
        }
    }

    #[test]
    fn reads_ifd0_exif_and_gps_tags() {
        let exif = Exif::parse(&trip_meta().exif_block().unwrap()).unwrap();
        assert_eq!(exif.text(Dir::Main, 0x010f), Some("Owlbum"));
        assert_eq!(exif.text(Dir::Main, 0x0110), Some("Fixture One"));
        assert_eq!(exif.int(Dir::Main, 0x0112), Some(6));
        assert_eq!(exif.text(Dir::Main, 0x0132), Some("2025:07:14 09:12:00"));
        assert_eq!(exif.text(Dir::Exif, 0x9003), Some("2025:07:14 09:12:00"));
        assert_eq!(exif.text(Dir::Exif, 0x9004), Some("2025:07:14 09:12:00"));
        assert_eq!(exif.number(Dir::Exif, 0x829a), Some(0.004));
        assert_eq!(exif.number(Dir::Exif, 0x829d), Some(5.6));
        assert_eq!(exif.int(Dir::Exif, 0x8827), Some(100));
        assert_eq!(exif.int(Dir::Exif, 0x9209), Some(16));
        assert_eq!(exif.number(Dir::Exif, 0x920a), Some(35.0));
        assert_eq!(exif.text(Dir::Exif, 0xa434), Some("Fixture 24-70mm F2.8"));
        assert_eq!(exif.text(Dir::Gps, 0x0001), Some("N"));
        assert_eq!(exif.text(Dir::Gps, 0x0003), Some("W"));
        let lat = exif.get(Dir::Gps, 0x0002).unwrap().numbers();
        assert_eq!(lat.len(), 3);
        assert_eq!(lat[0], Some(49.0));
        // Sub-IFD pointers are not values.
        assert_eq!(exif.get(Dir::Main, 0x8769), None);
    }

    /// The same IFD written big-endian by hand.
    #[test]
    fn reads_big_endian() {
        let mut b = b"MM\0*".to_vec();
        b.extend_from_slice(&8u32.to_be_bytes());
        b.extend_from_slice(&2u16.to_be_bytes());
        // Make: ASCII, 4 bytes inline ("Owl\0").
        b.extend_from_slice(&0x010fu16.to_be_bytes());
        b.extend_from_slice(&2u16.to_be_bytes());
        b.extend_from_slice(&4u32.to_be_bytes());
        b.extend_from_slice(b"Owl\0");
        // ExposureBiasValue in IFD 0 here: SRATIONAL -2/3 at offset 38.
        b.extend_from_slice(&0x9204u16.to_be_bytes());
        b.extend_from_slice(&10u16.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&38u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes()); // next IFD
        assert_eq!(b.len(), 38);
        b.extend_from_slice(&(-2i32).to_be_bytes());
        b.extend_from_slice(&3i32.to_be_bytes());
        let exif = Exif::parse(&b).unwrap();
        assert_eq!(exif.text(Dir::Main, 0x010f), Some("Owl"));
        let bias = exif.number(Dir::Main, 0x9204).unwrap();
        assert!((bias + 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_zero_denominator_is_no_number() {
        let mut ifd0 = Ifd::new();
        ifd0.set(0x829a, Val::Rational(vec![[1, 0]]));
        let exif = Exif::parse(&tiff::write(&ifd0)).unwrap();
        assert!(exif.get(Dir::Main, 0x829a).is_some());
        assert_eq!(exif.number(Dir::Main, 0x829a), None);
    }

    #[test]
    fn several_values_are_kept_in_order() {
        let mut exif_ifd = Ifd::new();
        exif_ifd.set(0x8827, Val::Short(vec![200, 400]));
        let mut ifd0 = Ifd::new();
        ifd0.set(0x8769, Val::Ifds(vec![exif_ifd]));
        let exif = Exif::parse(&tiff::write(&ifd0)).unwrap();
        assert_eq!(
            exif.get(Dir::Exif, 0x8827),
            Some(&Value::Ints(vec![200, 400]))
        );
    }

    #[test]
    fn a_leading_exif_marker_is_skipped() {
        let mut b = b"Exif\0\0".to_vec();
        b.extend_from_slice(&trip_meta().exif_block().unwrap());
        assert_eq!(
            Exif::parse(&b).unwrap().text(Dir::Main, 0x010f),
            Some("Owlbum")
        );
    }

    #[test]
    fn bytes_that_are_not_tiff_give_none() {
        assert_eq!(Exif::parse(b""), None);
        assert_eq!(Exif::parse(b"II"), None);
        assert_eq!(Exif::parse(b"hello world"), None);
    }

    /// Every truncation and every single-byte change reads without a panic.
    #[test]
    fn damaged_blocks_never_panic() {
        let block = trip_meta().exif_block().unwrap();
        for n in 0..block.len() {
            let _ = Exif::parse(&block[..n]);
        }
        for i in 0..block.len() {
            for x in [0x00, 0xff, 0x80, 0x01] {
                let mut b = block.clone();
                b[i] ^= x;
                let _ = Exif::parse(&b);
            }
        }
    }

    #[test]
    fn a_sub_ifd_pointing_at_itself_ends() {
        // IFD 0 at 8 holds one entry: the Exif pointer back to 8.
        let mut b = b"II*\0".to_vec();
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&0x8769u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        assert!(Exif::parse(&b).is_some());
    }
}
