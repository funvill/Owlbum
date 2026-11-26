//! Thumbnails, the zoom copy and the size-limited cache
//! (docs/03-media-and-scanning.md, "Thumbnails and the zoom copy";
//! docs/02-data.md, "Thumbnail keys and the cache").
//!
//! - [`make`] turns decoded pixels into thumbnail buffers and JPEG bytes.
//! - [`store`] is the folder layout under app data and the
//!   temporary-then-rename writes.
//! - [`cache`] is `cache_entries`, the size limit, pruning and Clear cache.
//! - [`lookup`] finds the open album's photo a URL names.
//! - [`serve`] answers an `owlbum://` URL as a plain [`serve::Reply`]; the
//!   shell's protocol handler only converts it to HTTP.
//!
//! [`Thumbs`] ties them together. The scanner writes thumbnails from its
//! one decode ([`Thumbs::write_from_scan`]) and makes missing small ones
//! after a scan ([`Thumbs::warm`]); the protocol handler makes them on
//! demand ([`Thumbs::thumbnail`], [`Thumbs::full`]).

pub mod cache;
mod flight;
pub mod lookup;
pub mod make;
pub mod serve;
pub mod store;

#[cfg(test)]
mod tests;

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

use image::ImageDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::metadata::Orientation;

use crate::appdata::AppData;
use crate::error::AppResult;
use crate::media::{MediaError, MediaFormat, MediaKind, Pixels, colour};

pub use cache::{Cache, CacheCleared, CacheUsage};
pub use lookup::Photo;

/// What a cached picture is: `cache_entries.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The 512 px thumbnail, `s`.
    Small,
    /// The 1600 px thumbnail, `m`.
    Medium,
    /// The full-size zoom copy, `full`.
    Full,
}

impl Kind {
    /// As stored in `cache_entries.kind` and written in a thumbnail URL.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Small => "s",
            Self::Medium => "m",
            Self::Full => "full",
        }
    }

    /// The kind stored as `text`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "s" => Some(Self::Small),
            "m" => Some(Self::Medium),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// How pictures are decoded for the cache. Tests count decodes through it.
pub trait Source: Send + Sync {
    /// The thumbnails' decode: the handler's `preview(1600)`.
    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        format.handler.preview(path, make::MEDIUM_SIDE)
    }

    /// The zoom copy's decode: the handler's `working`.
    fn working(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        format.handler.working(path)
    }
}

/// Decodes with the format registry's handlers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Handlers;

impl Source for Handlers {}

/// Both thumbnails made by one on-demand job; None for a size it did not
/// make.
#[derive(Debug, Clone, Default)]
struct Made {
    small: Option<Arc<Vec<u8>>>,
    medium: Option<Arc<Vec<u8>>>,
}

/// Thumbnails and zoom copies for the app. Cloning is cheap.
#[derive(Clone)]
pub struct Thumbs {
    inner: Arc<Inner>,
}

struct Inner {
    cache: Cache,
    source: Arc<dyn Source>,
    /// On-demand thumbnail jobs by `thumb_key`: a request for either size
    /// joins a job already making that key.
    thumbs: flight::Flights<Option<Made>>,
    /// Zoom copy jobs by `thumb_key`.
    fulls: flight::Flights<Option<Arc<Vec<u8>>>>,
    /// On-demand decodes running at once.
    permits: Permits,
}

impl std::fmt::Debug for Thumbs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbs")
            .field("cache", &self.inner.cache)
            .finish_non_exhaustive()
    }
}

impl Thumbs {
    /// The thumbnails of the app data folder `data`, decoding with the
    /// handlers, with as many on-demand decodes at once as the scan pool
    /// has threads.
    pub fn open(data: AppData) -> AppResult<Self> {
        let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Ok(Self::with(
            Cache::open(data)?,
            Arc::new(Handlers),
            cpus.saturating_sub(1).max(1),
        ))
    }

    /// Thumbnails over `cache`, decoding with `source`, at most `workers`
    /// on-demand decodes at once.
    pub fn with(cache: Cache, source: Arc<dyn Source>, workers: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                cache,
                source,
                thumbs: flight::Flights::default(),
                fulls: flight::Flights::default(),
                permits: Permits::new(workers.max(1)),
            }),
        }
    }

    /// The cache.
    pub fn cache(&self) -> &Cache {
        &self.inner.cache
    }

    fn dir(&self) -> &Path {
        self.inner.cache.dir()
    }

    /// The scan's thumbnails for `key` from its one decode (docs/03,
    /// "Pixels pass" step 3): the small one always, the medium one while
    /// the cache is under 70% of its limit. A thumbnail already on disk is
    /// not encoded again.
    pub fn write_from_scan(&self, key: &str, px: &Pixels) {
        if !store::is_thumb_key(key) {
            return;
        }
        let small = store::path(self.dir(), key, Kind::Small);
        let medium = store::path(self.dir(), key, Kind::Medium);
        let want_small = !store::is_whole_file(&small);
        let want_medium = self.inner.cache.has_room_for_medium() && !store::is_whole_file(&medium);
        if !want_small && !want_medium {
            return;
        }
        let b = make::buffers(px);
        if want_small {
            self.put(
                key,
                Kind::Small,
                &make::encode(&b.small, make::SMALL_QUALITY),
            );
        }
        if want_medium {
            self.put(
                key,
                Kind::Medium,
                &make::encode(&b.medium, make::MEDIUM_QUALITY),
            );
        }
    }

    /// Writes one picture and records its entry. A failed write is logged
    /// and forgotten: the picture is made again when asked for.
    fn put(&self, key: &str, kind: Kind, bytes: &[u8]) -> bool {
        match store::write(&store::path(self.dir(), key, kind), bytes) {
            Ok(()) => {
                self.inner.cache.record(key, kind, bytes.len() as u64);
                true
            }
            Err(e) => {
                tracing::warn!(target: "thumbs", "writing a {} picture: {e}", kind.as_str());
                false
            }
        }
    }

    /// The JPEG bytes of `photo`'s thumbnail of `kind` (small or medium),
    /// made on demand when it is not on disk (docs/03, "On demand"). None
    /// when the photo cannot be decoded, or for [`Kind::Full`].
    pub fn thumbnail(&self, photo: &Photo, kind: Kind) -> Option<Vec<u8>> {
        self.thumbnail_with(photo, kind, true)
    }

    /// [`Thumbs::thumbnail`]; `extra_medium` lets a job for the small size
    /// make the medium one too while the cache has room for it.
    fn thumbnail_with(&self, photo: &Photo, kind: Kind, extra_medium: bool) -> Option<Vec<u8>> {
        if kind == Kind::Full || !store::is_thumb_key(&photo.thumb_key) {
            return None;
        }
        let key = &photo.thumb_key;
        loop {
            if let Some(bytes) = store::read(&store::path(self.dir(), key, kind)) {
                self.inner.cache.touch(key, kind);
                return Some(bytes);
            }
            let made = self
                .inner
                .thumbs
                .share(key, || self.make_on_demand(photo, kind, extra_medium))?;
            let bytes = match kind {
                Kind::Small => made.small,
                _ => made.medium,
            };
            if let Some(bytes) = bytes {
                return Some(bytes.as_ref().clone());
            }
            // A job for the other size made only that one: look again, and
            // start a job for this size if it is still not there.
        }
    }

    /// One on-demand decode: the requested size, the small one, and, when
    /// `extra_medium`, the medium one too while the cache has room for it.
    fn make_on_demand(&self, photo: &Photo, kind: Kind, extra_medium: bool) -> Option<Made> {
        // A job that finished between the caller's read and this one
        // starting has made it already.
        if let Some(bytes) = store::read(&store::path(self.dir(), &photo.thumb_key, kind)) {
            let bytes = Some(Arc::new(bytes));
            return Some(match kind {
                Kind::Small => Made {
                    small: bytes,
                    medium: None,
                },
                _ => Made {
                    small: None,
                    medium: bytes,
                },
            });
        }
        let format = photo.format?;
        let px = self.inner.permits.run(|| {
            self.inner
                .source
                .preview(format, &photo.path)
                .map_err(|e| {
                    tracing::debug!(target: "thumbs", "no thumbnail for photo {}: {}", photo.id, e.code.as_str());
                })
                .ok()
        })?;
        let b = make::buffers(&px);
        let key = &photo.thumb_key;
        let small = make::encode(&b.small, make::SMALL_QUALITY);
        self.put(key, Kind::Small, &small);
        let medium = (kind == Kind::Medium
            || (extra_medium && self.inner.cache.has_room_for_medium()))
        .then(|| {
            let bytes = make::encode(&b.medium, make::MEDIUM_QUALITY);
            self.put(key, Kind::Medium, &bytes);
            Arc::new(bytes)
        });
        Some(Made {
            small: Some(Arc::new(small)),
            medium,
        })
    }

    /// The zoom copy of `photo` (docs/03, "The zoom copy"): the original
    /// bytes of a JPEG that is already upright and sRGB, else
    /// `full/<thumb_key>.jpg`, made once. None for a video or a picture
    /// that cannot be decoded.
    pub fn full(&self, photo: &Photo) -> Option<Vec<u8>> {
        let format = photo.format?;
        if photo.kind == MediaKind::Video || !store::is_thumb_key(&photo.thumb_key) {
            return None;
        }
        if format.id == "jpeg"
            && let Ok(bytes) = std::fs::read(&photo.path)
            && is_upright_srgb_jpeg(&bytes)
        {
            return Some(bytes);
        }
        let key = &photo.thumb_key;
        let path = store::path(self.dir(), key, Kind::Full);
        if let Some(bytes) = store::read(&path) {
            self.inner.cache.touch(key, Kind::Full);
            return Some(bytes);
        }
        let made = self.inner.fulls.share(key, || {
            // Another job may have finished between the read above and
            // this one starting.
            if let Some(bytes) = store::read(&path) {
                return Some(Arc::new(bytes));
            }
            let px = self
                .inner
                .permits
                .run(|| self.inner.source.working(format, &photo.path).ok())?;
            let bytes = make::encode(&make::flatten(&px), make::FULL_QUALITY);
            self.put(key, Kind::Full, &bytes);
            Some(Arc::new(bytes))
        })?;
        Some(made.as_ref().clone())
    }

    /// The warm pass (docs/03, "Finish"): makes the small thumbnail of
    /// every photo in `photos` that has none in the cache, found from
    /// `cache_entries` in one query. Stops as soon as `stop` says so.
    /// Answers how many it made.
    pub fn warm(&self, photos: &[Photo], stop: &dyn Fn() -> bool) -> usize {
        let have = self.inner.cache.small_keys();
        let mut made = 0;
        for photo in photos.iter().filter(|p| !have.contains(&p.thumb_key)) {
            if stop() {
                break;
            }
            // A file with no entry (app.sqlite was set aside, or the app
            // ended before a batch was written) is counted again, not
            // read or made again.
            let path = store::path(self.dir(), &photo.thumb_key, Kind::Small);
            if store::is_whole_file(&path) {
                let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
                self.inner
                    .cache
                    .record(&photo.thumb_key, Kind::Small, bytes);
                continue;
            }
            if self.thumbnail_with(photo, Kind::Small, false).is_some() {
                made += 1;
            }
        }
        self.inner.cache.flush();
        made
    }
}

/// True when `bytes` is a JPEG the webview can show as it is: upright
/// (orientation 1 or none) and with no colour profile other than sRGB.
pub fn is_upright_srgb_jpeg(bytes: &[u8]) -> bool {
    let Ok(mut decoder) = JpegDecoder::new(Cursor::new(bytes)) else {
        return false;
    };
    let upright = matches!(decoder.orientation(), Ok(Orientation::NoTransforms));
    let icc = decoder.icc_profile().ok().flatten();
    upright && colour::is_srgb(icc.as_deref())
}

/// A counting semaphore: at most `n` jobs inside [`Permits::run`] at once.
struct Permits {
    free: Mutex<usize>,
    freed: Condvar,
}

impl Permits {
    fn new(n: usize) -> Self {
        Self {
            free: Mutex::new(n),
            freed: Condvar::new(),
        }
    }

    fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        {
            let mut free = self.free.lock().unwrap_or_else(|p| p.into_inner());
            while *free == 0 {
                free = self.freed.wait(free).unwrap_or_else(|p| p.into_inner());
            }
            *free -= 1;
        }
        struct Release<'a>(&'a Permits);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                *self.0.free.lock().unwrap_or_else(|p| p.into_inner()) += 1;
                self.0.freed.notify_one();
            }
        }
        let _release = Release(self);
        job()
    }
}
