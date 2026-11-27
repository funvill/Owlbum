//! The video handler (docs/03-media-and-scanning.md, "The video handler").
//!
//! Owlbum bundles no video decoder. The facts come from the container:
//! `nom-exif` reads the date, place, camera and (for Matroska and WebM)
//! the length and size; a small walk of the MP4 and QuickTime boxes
//! ([`boxes`]) and of the AVI header ([`avi`]) gives the length, the
//! displayed size, the rotation and the codec names, so the facts need no
//! system codec and work the same on every system. The poster frame is
//! the operating system's: on Windows the shell's thumbnail for the file
//! ([`shell`]). Where the system has none, `preview` answers
//! `unsupported`, and the scanner keeps the row with no thumbnail.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use chrono::Local;

use super::exif::{Dir, Exif, Value};
use super::handler::{
    Cost, Layout, MediaError, MediaErrorCode, MediaHandler, Metadata, Pixels, Samples,
    SourcePixels, VideoInfo,
};
use super::{registry, sniff};
use crate::scan::metadata::video_captured_at;
use crate::text;

/// The video handler, one instance for every video row.
#[derive(Debug)]
pub struct Video {
    // Not zero-sized, so the static has an address of its own.
    _addr: u8,
}

pub static VIDEO: Video = Video { _addr: 0 };

/// The longest side asked of the system for a poster frame.
const POSTER_SIDE: u32 = 1600;

fn decode_error(file: &Path) -> MediaError {
    let label = file
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(registry::format_of)
        .map_or("video", |f| f.label);
    MediaError {
        code: MediaErrorCode::Decode,
        message: text::media::video_decode_error(label),
    }
}

fn open(file: &Path) -> Result<BufReader<File>, MediaError> {
    File::open(file)
        .map(BufReader::new)
        .map_err(|e| MediaError::io(&e))
}

/// The container family, from the first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Iso,
    Riff,
    Ebml,
}

fn family(file: &Path) -> Result<Family, MediaError> {
    let mut head = [0u8; 12];
    let n = open(file)?
        .read(&mut head)
        .map_err(|e| MediaError::io(&e))?;
    let head = &head[..n];
    if head.len() >= 8 && &head[4..8] == b"ftyp" {
        Ok(Family::Iso)
    } else if head.starts_with(b"RIFF") {
        Ok(Family::Riff)
    } else if head.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        Ok(Family::Ebml)
    } else if head.len() >= 8 && matches!(&head[4..8], b"moov" | b"mdat" | b"wide" | b"free") {
        // Older QuickTime files have no `ftyp`.
        Ok(Family::Iso)
    } else {
        Err(decode_error(file))
    }
}

/// Reads the container's facts: length, displayed size, rotation, codecs.
fn probe(file: &Path) -> Result<VideoInfo, MediaError> {
    let info = match family(file)? {
        Family::Iso => boxes::probe(&mut open(file)?),
        Family::Riff => avi::probe(&mut open(file)?),
        Family::Ebml => nom_exif::read_track(file).ok().and_then(|t| {
            use nom_exif::TrackInfoTag as T;
            let w = t.get(T::Width).and_then(|v| v.as_u32())?;
            let h = t.get(T::Height).and_then(|v| v.as_u32())?;
            Some(VideoInfo {
                duration_ms: t.get(T::DurationMs).and_then(|v| v.as_u64()).unwrap_or(0),
                width: w,
                height: h,
                rotation: 0,
                video_codec: None,
                audio_codec: None,
            })
        }),
    };
    info.ok_or_else(|| decode_error(file))
}

/// The container's date, place and camera as EXIF tags, so the photo
/// rules (`scan::metadata`) apply to them unchanged.
fn container_tags(file: &Path) -> Option<Exif> {
    use nom_exif::{EntryValue, TrackInfoTag as T};
    let track = nom_exif::read_track(file).ok()?;
    let mut exif = Exif::default();
    let text = |s: &str| Value::Text(s.to_owned());
    if let Some(m) = track.get(T::Make).and_then(EntryValue::as_str) {
        exif.set(Dir::Main, 0x010f, text(m));
    }
    if let Some(m) = track.get(T::Model).and_then(EntryValue::as_str) {
        exif.set(Dir::Main, 0x0110, text(m));
    }
    if let Some(EntryValue::DateTime(at)) = track.get(T::CreateDate) {
        // A date written with its own offset (QuickTime's creationdate) is
        // the wall clock where it was shot; the container's creation time
        // is UTC and is shown in this computer's zone.
        let wall = (at.offset().local_minus_utc() != 0)
            .then(|| at.naive_local().format("%Y-%m-%dT%H:%M:%S").to_string());
        if let Some(when) = video_captured_at(wall.as_deref(), Some(at.timestamp()), &Local) {
            exif.set(Dir::Exif, 0x9003, text(&when));
        }
    }
    if let Some((lat, lon)) = track
        .get(T::GpsIso6709)
        .and_then(EntryValue::as_str)
        .and_then(iso6709)
    {
        let dms = |v: f64| Value::Ratios(vec![((v.abs() * 1e6).round() as i64, 1_000_000)]);
        exif.set(Dir::Gps, 0x0001, text(if lat < 0.0 { "S" } else { "N" }));
        exif.set(Dir::Gps, 0x0002, dms(lat));
        exif.set(Dir::Gps, 0x0003, text(if lon < 0.0 { "W" } else { "E" }));
        exif.set(Dir::Gps, 0x0004, dms(lon));
    }
    Some(exif)
}

/// Latitude and longitude from an ISO 6709 string such as
/// `+48.8577+002.2950+035.000/`.
pub(crate) fn iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim().trim_end_matches('/');
    let mut starts = s
        .char_indices()
        .filter(|&(_, c)| c == '+' || c == '-')
        .map(|(i, _)| i);
    let a = starts.next()?;
    let b = starts.next()?;
    let c = starts.next().unwrap_or(s.len());
    if a != 0 {
        return None;
    }
    Some((s[a..b].parse().ok()?, s[b..c].parse().ok()?))
}

impl MediaHandler for Video {
    fn sniff(&self, head: &[u8]) -> bool {
        sniff::video_family(head)
    }

    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError> {
        let info = probe(file)?;
        Ok(Metadata {
            width: info.width,
            height: info.height,
            duration_ms: Some(info.duration_ms),
            exif: container_tags(file),
            ..Metadata::default()
        })
    }

    fn preview(&self, file: &Path, min_side: u32) -> Result<Pixels, MediaError> {
        let info = probe(file)?;
        // Never ask for more than the video's own size: the system would
        // enlarge it.
        let side = min_side
            .max(POSTER_SIDE)
            .min(info.width.max(info.height))
            .max(1);
        let px =
            poster(file, side).ok_or_else(|| MediaError::unsupported(text::media::NO_POSTER))?;
        Ok(Pixels {
            full_width: info.width,
            full_height: info.height,
            ..px
        })
    }

    fn working(&self, file: &Path) -> Result<Pixels, MediaError> {
        self.preview(file, POSTER_SIDE)
    }

    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError> {
        let px = self.working(file)?;
        Ok(SourcePixels {
            width: px.width,
            height: px.height,
            layout: Layout::Rgba,
            samples: Samples::U8(px.rgba),
            icc: None,
        })
    }

    fn probe(&self, file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        probe(file).map(Some)
    }

    fn cost(&self, _file_size: u64) -> Cost {
        Cost::Heavy
    }
}

/// The poster frame from the operating system, at most `side` on its
/// longest side.
#[cfg(windows)]
fn poster(file: &Path, side: u32) -> Option<Pixels> {
    shell::thumbnail(file, side)
}

/// No poster frames on this system yet (GStreamer is a follow-up).
#[cfg(not(windows))]
fn poster(_file: &Path, _side: u32) -> Option<Pixels> {
    None
}

/// The MP4 and QuickTime box walk.
pub(crate) mod boxes {
    use super::{Read, Seek, SeekFrom, VideoInfo};

    /// The most of one `moov` box read into memory.
    const MAX_MOOV: u64 = 64 << 20;

    fn be32(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
    }

    fn be64(b: &[u8], at: usize) -> Option<u64> {
        Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
    }

    /// The children of a box body: `(kind, body)`.
    fn children(body: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at + 8 <= body.len() {
            let Some(size) = be32(body, at) else { break };
            let kind: [u8; 4] = body[at + 4..at + 8].try_into().unwrap_or([0; 4]);
            let (head, len) = match size {
                0 => (8, body.len() - at),
                1 => match be64(body, at + 8).and_then(|l| usize::try_from(l).ok()) {
                    Some(l) => (16, l),
                    None => break,
                },
                n => (8, n as usize),
            };
            if len < head || at + len > body.len() {
                break;
            }
            out.push((kind, &body[at + head..at + len]));
            at += len;
        }
        out
    }

    fn child<'a>(body: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
        children(body)
            .into_iter()
            .find(|(k, _)| k == kind)
            .map(|(_, b)| b)
    }

    /// Finds the top-level `moov` box by its headers and reads it.
    fn moov<R: Read + Seek>(r: &mut R) -> Option<Vec<u8>> {
        let end = r.seek(SeekFrom::End(0)).ok()?;
        let mut at = 0u64;
        while at + 8 <= end {
            r.seek(SeekFrom::Start(at)).ok()?;
            let mut h = [0u8; 16];
            r.read_exact(&mut h[..8]).ok()?;
            let size = u64::from(be32(&h, 0)?);
            let (head, len) = match size {
                0 => (8, end - at),
                1 => {
                    r.read_exact(&mut h[8..16]).ok()?;
                    (16, be64(&h, 8)?)
                }
                n => (8, n),
            };
            if len < head {
                return None;
            }
            if &h[4..8] == b"moov" {
                let body = len - head;
                if body > MAX_MOOV {
                    return None;
                }
                let mut buf = vec![0u8; usize::try_from(body).ok()?];
                r.read_exact(&mut buf).ok()?;
                return Some(buf);
            }
            at += len;
        }
        None
    }

    /// A codec's short name from its sample entry.
    pub(crate) fn codec_name(fourcc: &[u8; 4], object_type: Option<u8>) -> String {
        match fourcc {
            b"avc1" | b"avc3" => "h264".into(),
            b"hvc1" | b"hev1" => "hevc".into(),
            b"av01" => "av1".into(),
            b"vp08" => "vp8".into(),
            b"vp09" => "vp9".into(),
            b"mp4a" => "aac".into(),
            b"Opus" => "opus".into(),
            b"ac-3" => "ac3".into(),
            b"ec-3" => "eac3".into(),
            b"jpeg" | b"mjpa" | b"mjpb" => "mjpeg".into(),
            b"mp4v" if object_type == Some(0x6c) => "mjpeg".into(),
            b"mp4v" => "mpeg4".into(),
            other => String::from_utf8_lossy(other).trim().to_lowercase(),
        }
    }

    /// The MPEG-4 object type in an `esds` box, if any.
    fn object_type(entry: &[u8], skip: usize) -> Option<u8> {
        let esds = child(entry.get(skip..)?, b"esds")?;
        // version/flags, then ES_Descriptor (tag 3), then the
        // DecoderConfigDescriptor (tag 4) whose first byte is the type.
        let at = esds.iter().position(|&b| b == 0x04)?;
        let mut i = at + 1;
        while esds.get(i)? & 0x80 != 0 {
            i += 1;
        }
        esds.get(i + 1).copied()
    }

    struct Track {
        handler: [u8; 4],
        codec: Option<String>,
        width: u32,
        height: u32,
        rotation: u16,
    }

    fn track(trak: &[u8]) -> Option<Track> {
        let tkhd = child(trak, b"tkhd")?;
        let version = *tkhd.first()?;
        // After version/flags: times and ids (20 or 32 bytes), reserved 8,
        // layer, group, volume, reserved (8), the matrix (36), width,
        // height (16.16).
        let matrix_at = 4 + if version == 1 { 32 } else { 20 } + 8 + 8;
        let a = be32(tkhd, matrix_at)? as i32;
        let b = be32(tkhd, matrix_at + 4)? as i32;
        let rotation = match (a.signum(), b.signum()) {
            (0, 1) => 90,
            (-1, 0) => 180,
            (0, -1) => 270,
            _ => 0,
        };
        let width = be32(tkhd, matrix_at + 36)? >> 16;
        let height = be32(tkhd, matrix_at + 40)? >> 16;
        let mdia = child(trak, b"mdia")?;
        let hdlr = child(mdia, b"hdlr")?;
        let handler: [u8; 4] = hdlr.get(8..12)?.try_into().ok()?;
        let stsd = child(child(child(mdia, b"minf")?, b"stbl")?, b"stsd")?;
        // version/flags, entry count, then the first entry's box.
        let codec = children(stsd.get(8..)?).first().map(|(kind, entry)| {
            // Visual entries have 78 bytes before their child boxes, sound
            // entries 28.
            let skip = if &handler == b"vide" { 78 } else { 28 };
            codec_name(kind, object_type(entry, skip))
        });
        Some(Track {
            handler,
            codec,
            width,
            height,
            rotation,
        })
    }

    /// Length, displayed size, rotation and codecs of an MP4 or QuickTime
    /// file. None when it has no readable `moov` or no video track.
    pub fn probe<R: Read + Seek>(r: &mut R) -> Option<VideoInfo> {
        let moov = moov(r)?;
        let mvhd = child(&moov, b"mvhd")?;
        let (scale, duration) = if *mvhd.first()? == 1 {
            (be32(mvhd, 20)?, be64(mvhd, 24)?)
        } else {
            (be32(mvhd, 12)?, u64::from(be32(mvhd, 16)?))
        };
        let tracks: Vec<Track> = children(&moov)
            .into_iter()
            .filter(|(k, _)| k == b"trak")
            .filter_map(|(_, t)| track(t))
            .collect();
        let video = tracks.iter().find(|t| &t.handler == b"vide")?;
        let audio = tracks.iter().find(|t| &t.handler == b"soun");
        let (width, height) = if video.rotation % 180 == 90 {
            (video.height, video.width)
        } else {
            (video.width, video.height)
        };
        let duration_ms = if scale == 0 {
            0
        } else {
            u64::try_from(u128::from(duration) * 1000 / u128::from(scale)).unwrap_or(u64::MAX)
        };
        Some(VideoInfo {
            duration_ms,
            width,
            height,
            rotation: video.rotation,
            video_codec: video.codec.clone(),
            audio_codec: audio.and_then(|a| a.codec.clone()),
        })
    }
}

/// The AVI header walk.
pub(crate) mod avi {
    use super::{Read, Seek, SeekFrom, VideoInfo};

    fn le32(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
    }

    /// Length, size and video codec from `hdrl` (`avih` and the first
    /// `strh`/`strf` pair of each type).
    pub fn probe<R: Read + Seek>(r: &mut R) -> Option<VideoInfo> {
        r.seek(SeekFrom::Start(0)).ok()?;
        let mut head = [0u8; 12];
        r.read_exact(&mut head).ok()?;
        if &head[..4] != b"RIFF" || &head[8..12] != b"AVI " {
            return None;
        }
        // The first list is `hdrl`; read it whole (it is small).
        let mut list = [0u8; 12];
        r.read_exact(&mut list).ok()?;
        if &list[..4] != b"LIST" || &list[8..12] != b"hdrl" {
            return None;
        }
        let len = le32(&list, 4)?.checked_sub(4)?;
        if len > 1 << 20 {
            return None;
        }
        let mut hdrl = vec![0u8; len as usize];
        r.read_exact(&mut hdrl).ok()?;
        let avih_at = hdrl.windows(4).position(|w| w == b"avih")? + 8;
        let us_per_frame = u64::from(le32(&hdrl, avih_at)?);
        let frames = u64::from(le32(&hdrl, avih_at + 16)?);
        let width = le32(&hdrl, avih_at + 32)?;
        let height = le32(&hdrl, avih_at + 36)?;
        let mut video_codec = None;
        let mut audio_codec = None;
        let mut at = 0;
        while let Some(p) = hdrl[at..].windows(4).position(|w| w == b"strh") {
            let s = at + p + 8;
            match hdrl.get(s..s + 4) {
                Some(b"vids") if video_codec.is_none() => {
                    let fcc: [u8; 4] = hdrl.get(s + 4..s + 8)?.try_into().ok()?;
                    video_codec = Some(match &fcc.to_ascii_lowercase()[..] {
                        b"mjpg" => "mjpeg".to_owned(),
                        b"h264" | b"avc1" | b"x264" => "h264".to_owned(),
                        b"xvid" | b"divx" | b"dx50" | b"fmp4" => "mpeg4".to_owned(),
                        other => String::from_utf8_lossy(other).trim().to_owned(),
                    });
                }
                Some(b"auds") if audio_codec.is_none() => {
                    // The format tag is the first field of the `strf` that
                    // follows.
                    let f = hdrl[s..].windows(4).position(|w| w == b"strf")? + s + 8;
                    let tag = u16::from_le_bytes(hdrl.get(f..f + 2)?.try_into().ok()?);
                    audio_codec = Some(
                        match tag {
                            0x0001 => "pcm",
                            0x0055 => "mp3",
                            0x00ff | 0x1610 => "aac",
                            0x2000 => "ac3",
                            _ => "unknown",
                        }
                        .to_owned(),
                    );
                }
                _ => {}
            }
            at = s;
        }
        Some(VideoInfo {
            duration_ms: frames * us_per_frame / 1000,
            width,
            height,
            rotation: 0,
            video_codec,
            audio_codec,
        })
    }
}

/// The Windows shell's thumbnail of a file.
#[cfg(windows)]
#[allow(unsafe_code)]
mod shell {
    use std::path::Path;

    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
    };
    use windows::core::HSTRING;

    use crate::media::Pixels;

    /// The shell's thumbnail of `file`, at most `side` on its longest
    /// side. Only a stored or extracted thumbnail: never the file type's
    /// icon.
    pub fn thumbnail(file: &Path, side: u32) -> Option<Pixels> {
        let side = i32::try_from(side).ok()?;
        // SAFETY: COM is initialised for this thread and released below
        // only when this call initialised it; every handle made here is
        // released before returning.
        unsafe {
            let init = CoInitializeEx(None, COINIT_MULTITHREADED);
            let out = (|| {
                let factory: IShellItemImageFactory =
                    SHCreateItemFromParsingName(&HSTRING::from(file.as_os_str()), None).ok()?;
                let bitmap = factory
                    .GetImage(SIZE { cx: side, cy: side }, SIIGBF_THUMBNAILONLY)
                    .ok()?;
                let px = read(bitmap);
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                px
            })();
            if init.is_ok() {
                CoUninitialize();
            }
            out
        }
    }

    /// The pixels of a bitmap, top-down RGBA, opaque.
    unsafe fn read(bitmap: HBITMAP) -> Option<Pixels> {
        // SAFETY: `bitmap` is a live bitmap handle; the buffers passed are
        // sized for what is asked.
        unsafe {
            let mut info = BITMAP::default();
            let got = GetObjectW(
                HGDIOBJ(bitmap.0),
                i32::try_from(size_of::<BITMAP>()).ok()?,
                Some(std::ptr::from_mut(&mut info).cast()),
            );
            if got == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
                return None;
            }
            let (w, h) = (info.bmWidth, info.bmHeight.abs());
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).ok()?,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let (wu, hu) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
            let mut bgra = vec![0u8; wu as usize * hu as usize * 4];
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                hu,
                Some(bgra.as_mut_ptr().cast()),
                &raw mut header,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(dc);
            if lines != h {
                return None;
            }
            for p in bgra.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                p[3] = 255;
            }
            Some(Pixels {
                width: wu,
                height: hu,
                full_width: wu,
                full_height: hu,
                rgba: bgra,
                has_alpha: false,
            })
        }
    }
}
