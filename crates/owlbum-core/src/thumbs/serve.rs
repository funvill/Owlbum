//! Answers an `owlbum://` URL (docs/01-architecture.md, "The `owlbum://`
//! protocol"; docs/03-media-and-scanning.md, "`owlbum://` URLs served").
//!
//! [`answer`] is a plain function from a URL to a [`Reply`], so it is
//! tested without a webview. The shell's protocol handler calls it off the
//! main thread and turns the reply into an HTTP response.
//!
//! Every URL resolves only to a photo of the open album that is in scope,
//! not missing and without a scan error; anything else answers 404, even
//! when a file for it sits in the shared cache.

use std::sync::Mutex;

use super::{Kind, Thumbs, lookup, store};
use crate::album::Album;
use crate::media::MediaKind;
use crate::scan::memory::Overlay;

/// What a URL asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// `thumb/<thumb_key>/s` or `/m`.
    Thumb { key: String, kind: Kind },
    /// `full/<photo_id>?v=<thumb_key>`: the zoom copy.
    Full { id: i64 },
    /// `original/<photo_id>?v=<thumb_key>`: an animated still's own bytes.
    Original { id: i64 },
    /// `video/<photo_id>`: the video file, in byte ranges.
    Video { id: i64 },
}

/// The hosts under which the route is in the path rather than the host:
/// `owlbum://localhost/thumb/…` and Windows' `http://owlbum.localhost/thumb/…`.
const HOSTS: [&str; 2] = ["localhost", "owlbum.localhost"];

impl Route {
    /// Reads a URL in any of the forms the webviews send:
    /// `owlbum://thumb/<key>/s`, `owlbum://localhost/thumb/<key>/s` and
    /// `http://owlbum.localhost/thumb/<key>/s`. The query (`?v=`) is
    /// ignored; it only makes the webview's own cache drop a changed
    /// picture.
    pub fn parse(url: &str) -> Option<Self> {
        let rest = url.split_once("://").map_or(url, |(_, r)| r);
        let rest = rest.split(['?', '#']).next().unwrap_or("");
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let mut parts: Vec<&str> = Vec::new();
        if !HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
            parts.push(host);
        }
        parts.extend(path.split('/').filter(|p| !p.is_empty()));
        let id = |s: &str| {
            (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .then(|| s.parse::<i64>().ok())
                .flatten()
        };
        match parts.as_slice() {
            ["thumb", key, size] if store::is_thumb_key(key) => {
                let kind = match *size {
                    "s" => Kind::Small,
                    "m" => Kind::Medium,
                    _ => return None,
                };
                Some(Self::Thumb {
                    key: (*key).to_owned(),
                    kind,
                })
            }
            ["full", n] => id(n).map(|id| Self::Full { id }),
            ["original", n] => id(n).map(|id| Self::Original { id }),
            ["video", n] => id(n).map(|id| Self::Video { id }),
            _ => None,
        }
    }
}

/// `Cache-Control` of a thumbnail: its key changes when the file does.
pub const IMMUTABLE: &str = "private, max-age=31536000, immutable";
/// `Cache-Control` of the zoom copy and an original.
pub const ONE_HOUR: &str = "private, max-age=3600";
/// `Cache-Control` of everything else.
pub const NO_STORE: &str = "no-store";

/// The most bytes of a video one reply carries (docs/03, "Video
/// playback").
pub const VIDEO_CHUNK: u64 = 4 << 20;

/// The answer to one URL, before it is HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub cache_control: &'static str,
    pub body: Vec<u8>,
    /// `Content-Range`, for a part of a video.
    pub content_range: Option<String>,
    /// True when the reply says `Accept-Ranges: bytes` (videos).
    pub ranges: bool,
}

impl Reply {
    fn ok(content_type: &'static str, cache_control: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            cache_control,
            body,
            content_range: None,
            ranges: false,
        }
    }

    /// 404 with no body.
    pub fn not_found() -> Self {
        Self::ok("text/plain; charset=utf-8", NO_STORE, Vec::new()).with_status(404)
    }

    fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

/// The first range of a `Range` header (`bytes=a-b`, `bytes=a-` or
/// `bytes=-n`) for a file of `len` bytes, as an inclusive start and end,
/// at most [`VIDEO_CHUNK`] long. No header means `bytes=0-`. None when the
/// range cannot be satisfied.
pub fn byte_range(header: Option<&str>, len: u64) -> Option<(u64, u64)> {
    let spec = header.map_or("bytes=0-", str::trim);
    let first = spec.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = first.split_once('-')?;
    let last = len.checked_sub(1)?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            if n == 0 {
                return None;
            }
            (len.saturating_sub(n), last)
        }
        (a, "") => (a.parse().ok()?, last),
        (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(last)),
    };
    if start > last || end < start {
        return None;
    }
    Some((start, end.min(start + VIDEO_CHUNK - 1)))
}

/// A part of a video file: 206 with the bytes asked for, or 416.
fn video_part(path: &std::path::Path, mime: &'static str, range: Option<&str>) -> Reply {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return Reply::not_found();
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Reply::not_found();
    };
    let Some((start, end)) = byte_range(range, len) else {
        let mut r = Reply::ok(mime, NO_STORE, Vec::new()).with_status(416);
        r.content_range = Some(format!("bytes */{len}"));
        r.ranges = true;
        return r;
    };
    let mut body = vec![0u8; usize::try_from(end - start + 1).unwrap_or(0)];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
        return Reply::not_found();
    }
    let mut r = Reply::ok(mime, NO_STORE, body).with_status(206);
    r.content_range = Some(format!("bytes {start}-{end}/{len}"));
    r.ranges = true;
    r
}

/// The open album, as the handler sees it: the album and the rows the
/// scanner holds in memory for a view-only album.
#[derive(Debug, Clone, Copy)]
pub struct Open<'a> {
    pub album: &'a Album,
    pub overlay: &'a Mutex<Overlay>,
}

/// Answers `url`. `open` is the open album, None when none is open.
/// `range` is the request's `Range` header, used for a video. Runs decodes
/// when a picture has to be made, so call it off the main thread.
pub fn answer(open: Option<Open<'_>>, thumbs: &Thumbs, url: &str, range: Option<&str>) -> Reply {
    let (Some(open), Some(route)) = (open, Route::parse(url)) else {
        return Reply::not_found();
    };
    let found = match &route {
        Route::Thumb { key, .. } => lookup::by_key(open.album, open.overlay, key),
        Route::Full { id } | Route::Original { id } | Route::Video { id } => {
            lookup::by_id(open.album, open.overlay, *id)
        }
    };
    let photo = match found {
        Ok(Some(p)) => p,
        Ok(None) => return Reply::not_found(),
        Err(e) => {
            tracing::warn!(target: "thumbs", "looking up a picture: {}", e.message);
            return Reply::not_found();
        }
    };
    let jpeg = "image/jpeg";
    match route {
        Route::Thumb { kind, .. } => thumbs
            .thumbnail(&photo, kind)
            .map_or_else(Reply::not_found, |b| Reply::ok(jpeg, IMMUTABLE, b)),
        Route::Full { .. } => thumbs
            .full(&photo)
            .map_or_else(Reply::not_found, |b| Reply::ok(jpeg, ONE_HOUR, b)),
        Route::Original { .. } => {
            let animated = photo.kind == MediaKind::Image && photo.duration_ms.is_some();
            match (animated, photo.format) {
                (true, Some(format)) => std::fs::read(&photo.path).map_or_else(
                    |_| Reply::not_found(),
                    |b| Reply::ok(format.mime, ONE_HOUR, b),
                ),
                _ => Reply::not_found(),
            }
        }
        Route::Video { .. } => match photo.format {
            Some(format) if photo.kind == MediaKind::Video => {
                video_part(&photo.path, format.mime, range)
            }
            _ => Reply::not_found(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn every_url_form_is_read() {
        let small = Route::Thumb {
            key: KEY.into(),
            kind: Kind::Small,
        };
        for url in [
            format!("owlbum://thumb/{KEY}/s"),
            format!("owlbum://localhost/thumb/{KEY}/s"),
            format!("http://owlbum.localhost/thumb/{KEY}/s"),
            format!("https://owlbum.localhost/thumb/{KEY}/s?x=1"),
        ] {
            assert_eq!(Route::parse(&url), Some(small.clone()), "{url}");
        }
        assert_eq!(
            Route::parse(&format!("owlbum://thumb/{KEY}/m")),
            Some(Route::Thumb {
                key: KEY.into(),
                kind: Kind::Medium
            })
        );
        assert_eq!(
            Route::parse(&format!("http://owlbum.localhost/full/42?v={KEY}")),
            Some(Route::Full { id: 42 })
        );
        assert_eq!(
            Route::parse(&format!("owlbum://original/7?v={KEY}")),
            Some(Route::Original { id: 7 })
        );
        assert_eq!(
            Route::parse("owlbum://localhost/video/3"),
            Some(Route::Video { id: 3 })
        );
    }

    #[test]
    fn byte_ranges() {
        const MB4: u64 = VIDEO_CHUNK;
        assert_eq!(byte_range(None, 10), Some((0, 9)));
        assert_eq!(byte_range(Some("bytes=0-"), 10), Some((0, 9)));
        assert_eq!(byte_range(Some("bytes=2-5"), 10), Some((2, 5)));
        assert_eq!(byte_range(Some("bytes=2-50"), 10), Some((2, 9)));
        assert_eq!(byte_range(Some("bytes=-3"), 10), Some((7, 9)));
        assert_eq!(byte_range(Some("bytes=0-"), 3 * MB4), Some((0, MB4 - 1)));
        assert_eq!(byte_range(Some("bytes=5-"), 3 * MB4), Some((5, MB4 + 4)));
        assert_eq!(byte_range(Some("bytes=10-"), 10), None);
        assert_eq!(byte_range(Some("bytes=5-2"), 10), None);
        assert_eq!(byte_range(Some("items=0-1"), 10), None);
        assert_eq!(byte_range(None, 0), None);
    }

    #[test]
    fn anything_else_is_not_a_route() {
        for url in [
            "owlbum://thumb/abc/s".to_owned(),
            format!("owlbum://thumb/{KEY}/full"),
            format!("owlbum://thumb/{KEY}"),
            format!("owlbum://thumb/{KEY}/s/extra"),
            format!("owlbum://thumb/{}/s", KEY.to_uppercase()),
            "owlbum://full/-1".to_owned(),
            "owlbum://full/1e3".to_owned(),
            "owlbum://full/".to_owned(),
            "owlbum://full/99999999999999999999999".to_owned(),
            "owlbum://edited/1".to_owned(),
            "owlbum://localhost/".to_owned(),
            "owlbum://thumb/..%2F..%2Fetc/s".to_owned(),
            String::new(),
        ] {
            assert_eq!(Route::parse(&url), None, "{url}");
        }
    }
}
