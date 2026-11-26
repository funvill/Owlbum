//! Small readers for the header facts the decoders do not give without a
//! full read: JPEG size and EXIF, and the frame counts of animated GIF,
//! WebP and PNG. Each reads block headers only and skips pixel data.

use std::io::{self, BufRead, Read, Seek, SeekFrom};

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_be16(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_be_bytes(b))
}

fn skip(r: &mut impl Seek, n: u64) -> io::Result<()> {
    let n = i64::try_from(n).map_err(|_| invalid("block too long"))?;
    r.seek(SeekFrom::Current(n)).map(|_| ())
}

/// A JPEG's stored size (before orientation) and its EXIF block (from the
/// TIFF header on).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct JpegHeader {
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub exif: Option<Vec<u8>>,
}

/// Walks a JPEG's markers up to the first frame header.
pub fn jpeg(r: &mut (impl BufRead + Seek)) -> io::Result<JpegHeader> {
    if read_be16(r)? != 0xFFD8 {
        return Err(invalid("not a JPEG"));
    }
    let mut out = JpegHeader::default();
    loop {
        // Markers may be preceded by any number of 0xFF fill bytes.
        let mut marker = read_u8(r)?;
        if marker != 0xFF {
            return Err(invalid("expected a marker"));
        }
        while marker == 0xFF {
            marker = read_u8(r)?;
        }
        match marker {
            // Stand-alone markers carry no length.
            0x01 | 0xD0..=0xD7 => continue,
            0xD9 | 0xDA => return Err(invalid("no frame header")),
            _ => {}
        }
        let len = read_be16(r)?;
        if len < 2 {
            return Err(invalid("bad segment length"));
        }
        let body = u64::from(len - 2);
        let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            let mut b = [0u8; 6];
            r.read_exact(&mut b)?;
            out.height = u32::from(u16::from_be_bytes([b[1], b[2]]));
            out.width = u32::from(u16::from_be_bytes([b[3], b[4]]));
            out.components = b[5];
            return Ok(out);
        }
        if marker == 0xE1 && out.exif.is_none() && body >= 6 {
            let mut data = vec![0u8; body as usize];
            r.read_exact(&mut data)?;
            if let Some(tiff) = data.strip_prefix(b"Exif\0\0") {
                out.exif = Some(tiff.to_vec());
            }
            continue;
        }
        skip(r, body)?;
    }
}

/// Frame count and loop length in milliseconds.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Animation {
    pub frames: u32,
    pub duration_ms: u64,
}

/// Skips a chain of GIF data sub-blocks.
fn gif_skip_sub_blocks(r: &mut (impl Read + Seek)) -> io::Result<()> {
    loop {
        let n = read_u8(r)?;
        if n == 0 {
            return Ok(());
        }
        skip(r, u64::from(n))?;
    }
}

/// Counts a GIF's frames and adds up their delays. Image data is skipped,
/// not decompressed. A file cut short counts the frames seen so far.
pub fn gif(r: &mut (impl Read + Seek)) -> io::Result<Animation> {
    let mut header = [0u8; 13];
    r.read_exact(&mut header)?;
    if !header.starts_with(b"GIF8") {
        return Err(invalid("not a GIF"));
    }
    let flags = header[10];
    if flags & 0x80 != 0 {
        skip(r, 3 * (1u64 << ((flags & 7) + 1)))?;
    }
    let mut out = Animation::default();
    let mut delay_cs: u64 = 0;
    let result = (|| -> io::Result<()> {
        loop {
            match read_u8(r)? {
                0x3B => return Ok(()),
                0x21 => {
                    let label = read_u8(r)?;
                    if label == 0xF9 {
                        let mut gce = [0u8; 6];
                        r.read_exact(&mut gce)?;
                        // size(4), flags, delay lo, delay hi, transparent, terminator
                        delay_cs = u64::from(u16::from_le_bytes([gce[2], gce[3]]));
                        if gce[5] != 0 {
                            gif_skip_sub_blocks(r)?;
                        }
                    } else {
                        gif_skip_sub_blocks(r)?;
                    }
                }
                0x2C => {
                    let mut desc = [0u8; 9];
                    r.read_exact(&mut desc)?;
                    let lflags = desc[8];
                    if lflags & 0x80 != 0 {
                        skip(r, 3 * (1u64 << ((lflags & 7) + 1)))?;
                    }
                    read_u8(r)?; // LZW minimum code size
                    gif_skip_sub_blocks(r)?;
                    out.frames += 1;
                    out.duration_ms += delay_cs * 10;
                    delay_cs = 0;
                }
                _ => return Err(invalid("unknown GIF block")),
            }
        }
    })();
    match result {
        Ok(()) => Ok(out),
        Err(_) if out.frames > 0 => Ok(out),
        Err(e) => Err(e),
    }
}

/// Counts a WebP's `ANMF` chunks and adds up their durations. A still WebP
/// has one frame and no duration.
pub fn webp(r: &mut (impl Read + Seek)) -> io::Result<Animation> {
    let mut header = [0u8; 12];
    r.read_exact(&mut header)?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WEBP" {
        return Err(invalid("not a WebP"));
    }
    let mut out = Animation::default();
    let mut animated = false;
    loop {
        let mut ch = [0u8; 8];
        match r.read_exact(&mut ch) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let size = u64::from(u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]));
        let padded = size + (size & 1);
        match &ch[0..4] {
            b"ANIM" => {
                animated = true;
                skip(r, padded)?;
            }
            b"ANMF" if size >= 16 => {
                let mut f = [0u8; 16];
                if r.read_exact(&mut f).is_err() {
                    break;
                }
                out.frames += 1;
                out.duration_ms += u64::from(u32::from_le_bytes([f[12], f[13], f[14], 0]));
                skip(r, padded - 16)?;
            }
            b"VP8 " | b"VP8L" if !animated => {
                out.frames = 1;
                break;
            }
            _ => skip(r, padded)?,
        }
    }
    if !animated {
        out.frames = out.frames.max(1);
        out.duration_ms = 0;
    }
    Ok(out)
}

/// The frame count in an animated PNG's `acTL` chunk, or 1. Stops at the
/// first `IDAT`, which `acTL` must precede.
pub fn png_frames(r: &mut (impl Read + Seek)) -> io::Result<u32> {
    let mut sig = [0u8; 8];
    r.read_exact(&mut sig)?;
    if &sig != b"\x89PNG\r\n\x1a\n" {
        return Err(invalid("not a PNG"));
    }
    loop {
        let mut ch = [0u8; 8];
        r.read_exact(&mut ch)?;
        let len = u64::from(u32::from_be_bytes([ch[0], ch[1], ch[2], ch[3]]));
        match &ch[4..8] {
            b"acTL" => {
                let mut b = [0u8; 4];
                r.read_exact(&mut b)?;
                return Ok(u32::from_be_bytes(b).max(1));
            }
            b"IDAT" | b"IEND" => return Ok(1),
            _ => skip(r, len + 4)?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn jpeg_header_reads_size_after_app_segments_and_fill_bytes() {
        let mut f = vec![0xFF, 0xD8];
        // APP0 JFIF
        f.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x06, b'J', b'F', b'I', b'F']);
        // APP1 Exif with a 4-byte TIFF body
        f.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x0C]);
        f.extend_from_slice(b"Exif\0\0II*\0");
        // fill bytes, then SOF0: precision 8, height 300, width 400, 3 components
        f.extend_from_slice(&[0xFF, 0xFF, 0xC0, 0x00, 0x11, 8, 0x01, 0x2C, 0x01, 0x90, 3]);
        f.extend_from_slice(&[0; 9]);
        let h = jpeg(&mut Cursor::new(f)).unwrap();
        assert_eq!((h.width, h.height, h.components), (400, 300, 3));
        assert_eq!(h.exif.as_deref(), Some(&b"II*\0"[..]));
    }

    #[test]
    fn jpeg_header_without_frame_is_an_error() {
        let f = vec![0xFF, 0xD8, 0xFF, 0xD9];
        assert!(jpeg(&mut Cursor::new(f)).is_err());
        assert!(jpeg(&mut Cursor::new(vec![0x89, b'P'])).is_err());
    }

    #[test]
    fn webp_still_has_one_frame_and_no_duration() {
        let mut f = b"RIFF\x14\0\0\0WEBPVP8L\x08\0\0\0".to_vec();
        f.extend_from_slice(&[0; 8]);
        assert_eq!(
            webp(&mut Cursor::new(f)).unwrap(),
            Animation {
                frames: 1,
                duration_ms: 0
            }
        );
    }

    #[test]
    fn png_without_actl_has_one_frame() {
        let mut f = b"\x89PNG\r\n\x1a\n".to_vec();
        f.extend_from_slice(&[0, 0, 0, 13]);
        f.extend_from_slice(b"IHDR");
        f.extend_from_slice(&[0; 17]);
        f.extend_from_slice(&[0, 0, 0, 0]);
        f.extend_from_slice(b"IDAT");
        assert_eq!(png_frames(&mut Cursor::new(f)).unwrap(), 1);
    }
}
