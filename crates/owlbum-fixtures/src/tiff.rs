//! A small little-endian TIFF writer: enough for an EXIF block inside a
//! JPEG and for a DNG. Entries are written in tag order; sub-IFDs and data
//! blobs are laid out after the IFD that points at them.

/// One value of an IFD entry.
#[derive(Clone, Debug)]
pub enum Val {
    Byte(Vec<u8>),
    Ascii(String),
    Short(Vec<u16>),
    Long(Vec<u32>),
    Rational(Vec<[u32; 2]>),
    SRational(Vec<[i32; 2]>),
    Undefined(Vec<u8>),
    /// Child IFDs, written as LONG offsets (EXIF, GPS, SubIFDs).
    Ifds(Vec<Ifd>),
    /// Bytes stored elsewhere in the file, written as one LONG offset
    /// (StripOffsets). The matching byte count is a separate entry.
    Blob(Vec<u8>),
}

#[derive(Clone, Debug, Default)]
pub struct Ifd {
    entries: Vec<(u16, Val)>,
}

impl Ifd {
    pub fn new() -> Self {
        Ifd::default()
    }

    pub fn set(&mut self, tag: u16, val: Val) -> &mut Self {
        self.entries.retain(|(t, _)| *t != tag);
        self.entries.push((tag, val));
        self
    }

    pub fn ascii(&mut self, tag: u16, text: &str) -> &mut Self {
        self.set(tag, Val::Ascii(text.to_string()))
    }

    pub fn short(&mut self, tag: u16, v: u16) -> &mut Self {
        self.set(tag, Val::Short(vec![v]))
    }

    pub fn long(&mut self, tag: u16, v: u32) -> &mut Self {
        self.set(tag, Val::Long(vec![v]))
    }

    pub fn rational(&mut self, tag: u16, num: u32, den: u32) -> &mut Self {
        self.set(tag, Val::Rational(vec![[num, den]]))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A whole TIFF file (or EXIF block) with `ifd0` as its first IFD.
pub fn write(ifd0: &Ifd) -> Vec<u8> {
    let mut out = b"II*\0".to_vec();
    out.extend_from_slice(&8u32.to_le_bytes());
    write_ifd(&mut out, ifd0);
    out
}

fn pad(out: &mut Vec<u8>) {
    if out.len() % 2 == 1 {
        out.push(0);
    }
}

fn put_u32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn offset(out: &[u8]) -> u32 {
    u32::try_from(out.len()).expect("TIFF over 4 GiB")
}

/// Type code, count and payload bytes of a plain value.
fn encode(val: &Val) -> (u16, u32, Vec<u8>) {
    fn many<T: Copy, const N: usize>(v: &[T], f: impl Fn(T) -> [u8; N]) -> Vec<u8> {
        v.iter().flat_map(|x| f(*x)).collect()
    }
    let n = |len: usize| u32::try_from(len).expect("count");
    match val {
        Val::Byte(v) => (1, n(v.len()), v.clone()),
        Val::Ascii(s) => {
            let mut b = s.as_bytes().to_vec();
            b.push(0);
            (2, n(b.len()), b)
        }
        Val::Short(v) => (3, n(v.len()), many(v, u16::to_le_bytes)),
        Val::Long(v) => (4, n(v.len()), many(v, u32::to_le_bytes)),
        Val::Rational(v) => (
            5,
            n(v.len()),
            v.iter()
                .flat_map(|[a, b]| [a.to_le_bytes(), b.to_le_bytes()].concat())
                .collect(),
        ),
        Val::SRational(v) => (
            10,
            n(v.len()),
            v.iter()
                .flat_map(|[a, b]| [a.to_le_bytes(), b.to_le_bytes()].concat())
                .collect(),
        ),
        Val::Undefined(v) => (7, n(v.len()), v.clone()),
        Val::Ifds(_) | Val::Blob(_) => unreachable!("laid out separately"),
    }
}

/// Writes `ifd` at the end of `out` and returns its offset.
fn write_ifd(out: &mut Vec<u8>, ifd: &Ifd) -> u32 {
    pad(out);
    let start = out.len();
    let mut entries: Vec<&(u16, Val)> = ifd.entries.iter().collect();
    entries.sort_by_key(|(tag, _)| *tag);
    let count = u16::try_from(entries.len()).expect("entries");
    out.extend_from_slice(&count.to_le_bytes());
    out.resize(start + 2 + 12 * entries.len() + 4, 0);

    let mut deferred = Vec::new();
    for (i, (tag, val)) in entries.iter().enumerate() {
        let at = start + 2 + 12 * i;
        out[at..at + 2].copy_from_slice(&tag.to_le_bytes());
        match val {
            Val::Ifds(children) => {
                out[at + 2..at + 4].copy_from_slice(&4u16.to_le_bytes());
                let n = u32::try_from(children.len()).expect("count");
                put_u32(out, at + 4, n);
                deferred.push((at, val));
            }
            Val::Blob(_) => {
                out[at + 2..at + 4].copy_from_slice(&4u16.to_le_bytes());
                put_u32(out, at + 4, 1);
                deferred.push((at, val));
            }
            _ => {
                let (ty, n, bytes) = encode(val);
                out[at + 2..at + 4].copy_from_slice(&ty.to_le_bytes());
                put_u32(out, at + 4, n);
                if bytes.len() <= 4 {
                    out[at + 8..at + 8 + bytes.len()].copy_from_slice(&bytes);
                } else {
                    pad(out);
                    let off = offset(out);
                    out.extend_from_slice(&bytes);
                    put_u32(out, at + 8, off);
                }
            }
        }
    }
    for (at, val) in deferred {
        match val {
            Val::Blob(bytes) => {
                pad(out);
                let off = offset(out);
                out.extend_from_slice(bytes);
                put_u32(out, at + 8, off);
            }
            Val::Ifds(children) => {
                let offs: Vec<u32> = children.iter().map(|c| write_ifd(out, c)).collect();
                if offs.len() == 1 {
                    put_u32(out, at + 8, offs[0]);
                } else {
                    pad(out);
                    let off = offset(out);
                    for o in offs {
                        out.extend_from_slice(&o.to_le_bytes());
                    }
                    put_u32(out, at + 8, off);
                }
            }
            _ => unreachable!(),
        }
    }
    u32::try_from(start).expect("offset")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn entries_are_sorted_and_long_values_out_of_line() {
        let mut ifd = Ifd::new();
        ifd.ascii(0x0110, "Model name").short(0x0112, 6);
        let b = write(&ifd);
        assert_eq!(&b[..4], b"II*\0");
        assert_eq!(u16_at(&b, 8), 2);
        assert_eq!(u16_at(&b, 10), 0x0110);
        assert_eq!(u16_at(&b, 22), 0x0112);
        assert_eq!(u16_at(&b, 30), 6);
        let off = u32_at(&b, 18) as usize;
        assert_eq!(&b[off..off + 11], b"Model name\0");
    }

    #[test]
    fn child_ifds_are_pointed_at() {
        let mut child = Ifd::new();
        child.short(0x9000, 7);
        let mut ifd = Ifd::new();
        ifd.set(0x8769, Val::Ifds(vec![child]));
        let b = write(&ifd);
        let off = u32_at(&b, 18) as usize;
        assert_eq!(u16_at(&b, off), 1);
        assert_eq!(u16_at(&b, off + 2), 0x9000);
    }
}
