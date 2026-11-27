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
mod slots;
pub mod store;

#[cfg(test)]
mod profile;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};

use image::ImageDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::metadata::Orientation;

use crate::appdata::AppData;
use crate::error::AppResult;
use crate::media::{MediaError, MediaFormat, MediaKind, Pixels, colour};

pub use cache::{Cache, CacheCleared, CacheUsage};
pub use lookup::Photo;
pub use slots::Priority;

/// How many pixels passes made on demand wait for the scan to write them
/// ([`Thumbs::scan_pixels`]). Beyond that a request's analysis is dropped
/// and the scan decodes that photo again.
const STASH_MAX: usize = 4096;

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

    /// The page's fast decode for a small thumbnail alone: the handler's
    /// `preview(512)`, which a large JPEG gives at 1/4 or 1/8 scale (#120).
    fn small(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        format.handler.preview(path, make::SMALL_SIDE)
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

/// What the pixels pass keeps from its one decode besides the thumbnails
/// (docs/03, "Pixels pass" step 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelsDone {
    /// The `analysis` JSON.
    pub analysis: String,
    /// The decode's full upright size (`full_width`, `full_height`; 0 for
    /// a reduced or embedded preview).
    pub full: (u32, u32),
}

impl PixelsDone {
    fn of(px: &Pixels, b: &make::Buffers) -> Self {
        Self {
            analysis: crate::scan::pixels::analysis_of(
                b,
                crate::scan::pixels::small_alpha(px).as_ref(),
            ),
            full: (px.full_width, px.full_height),
        }
    }
}

/// What one job for a key made: both thumbnails (None for a size it did
/// not make), and the rest of the pixels pass when the photo was still
/// waiting for it.
#[derive(Debug, Clone, Default)]
struct Made {
    small: Option<Arc<Vec<u8>>>,
    medium: Option<Arc<Vec<u8>>>,
    pixels: Option<Arc<PixelsDone>>,
}

/// Thumbnails and zoom copies for the app. Cloning is cheap.
#[derive(Clone)]
pub struct Thumbs {
    inner: Arc<Inner>,
}

struct Inner {
    cache: Cache,
    source: Arc<dyn Source>,
    /// Thumbnail jobs by `thumb_key`, the page's and the scan's: a request
    /// for either size, or the scan's pixels pass, joins a job already
    /// making that key.
    thumbs: flight::Flights<Option<Made>>,
    /// Zoom copy jobs by `thumb_key`.
    fulls: flight::Flights<Option<Arc<Vec<u8>>>>,
    /// Decodes running at once, the page's before the scan's.
    slots: slots::Slots,
    /// Pixels passes the page's requests made for photos still waiting,
    /// by `thumb_key`, until the scan writes them.
    stash: Mutex<HashMap<String, Arc<PixelsDone>>>,
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
    /// decodes at once (the page's and the scan's together).
    pub fn with(cache: Cache, source: Arc<dyn Source>, workers: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                cache,
                source,
                thumbs: flight::Flights::default(),
                fulls: flight::Flights::default(),
                slots: slots::Slots::new(workers),
                stash: Mutex::new(HashMap::new()),
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

    /// Runs `job`, a decode of the scan's and what is made from it, in one
    /// of the decode slots, after every request from the page that waits
    /// for one (docs/03, "Order of work").
    fn background<T>(&self, job: impl FnOnce() -> T) -> T {
        self.inner.slots.run(Priority::Background, job)
    }

    /// The scan's pixels pass for `key` (docs/03, "Pixels pass"): one
    /// decode with `decode`, the thumbnails written from it, and what the
    /// row needs besides. It is one job with the page's requests for the
    /// same key: when the page already made the pass for this photo, or is
    /// making it, nothing is decoded again. The decode and everything made
    /// from it run in one decode slot, after the page's requests.
    pub fn scan_pixels(
        &self,
        key: &str,
        decode: &dyn Fn() -> Result<Pixels, MediaError>,
    ) -> Result<Arc<PixelsDone>, MediaError> {
        if !store::is_thumb_key(key) {
            return self.background(|| {
                let px = decode()?;
                Ok(Arc::new(PixelsDone::of(&px, &make::buffers(&px))))
            });
        }
        // At most twice: a request's job this joined may have failed, or
        // made only thumbnails (the row was not waiting when it looked).
        for _ in 0..2 {
            let mut failed = None;
            let made = self.inner.thumbs.share(key, || {
                if let Some(done) = self.take_stash(key) {
                    return Some(Made {
                        pixels: Some(done),
                        ..Made::default()
                    });
                }
                match self.background(|| decode().map(|px| self.write_from_scan(key, &px))) {
                    Ok(made) => Some(made),
                    Err(e) => {
                        failed = Some(e);
                        None
                    }
                }
            });
            if let Some(e) = failed {
                return Err(e);
            }
            if let Some(done) = made.and_then(|m| m.pixels) {
                // A request's job this joined kept its pass for the scan too.
                self.take_stash(key);
                return Ok(done);
            }
        }
        self.background(|| {
            let px = decode()?;
            Ok(self
                .write_from_scan(key, &px)
                .pixels
                .unwrap_or_else(|| Arc::new(PixelsDone::of(&px, &make::buffers(&px)))))
        })
    }

    /// The pixels pass's analysis that a request from the page made for
    /// `key`, taken out.
    fn take_stash(&self, key: &str) -> Option<Arc<PixelsDone>> {
        self.inner
            .stash
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key)
    }

    /// The scan's thumbnails for `key` from its one decode (docs/03,
    /// "Pixels pass" step 3): the small one always, the medium one while
    /// the cache is under 70% of its limit. A thumbnail already on disk is
    /// not encoded again. The same buffers give the analysis.
    fn write_from_scan(&self, key: &str, px: &Pixels) -> Made {
        let b = make::buffers(px);
        let small = store::path(self.dir(), key, Kind::Small);
        let medium = store::path(self.dir(), key, Kind::Medium);
        let small = (!store::is_whole_file(&small)).then(|| {
            let bytes = make::encode(&b.small, make::SMALL_QUALITY);
            self.put(key, Kind::Small, &bytes);
            Arc::new(bytes)
        });
        let medium = (self.inner.cache.has_room_for_medium() && !store::is_whole_file(&medium))
            .then(|| {
                let bytes = make::encode(&b.medium, make::MEDIUM_QUALITY);
                self.put(key, Kind::Medium, &bytes);
                Arc::new(bytes)
            });
        Made {
            small,
            medium,
            pixels: Some(Arc::new(PixelsDone::of(px, &b))),
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
    /// A request from the page goes before the scan's decodes, newest
    /// first. The small size is made alone from a decode at about its own
    /// size (#120); the scan writes the rest later under the same key. For
    /// the medium size of a photo still waiting for its pixels pass the job
    /// makes the whole pass, which the scan then writes without decoding
    /// again.
    pub fn thumbnail(&self, photo: &Photo, kind: Kind) -> Option<Vec<u8>> {
        self.thumbnail_with(photo, kind, Priority::Page)
    }

    /// [`Thumbs::thumbnail`] for background work (tagging): its decode
    /// waits behind every picture the page asked for.
    pub fn background_thumbnail(&self, photo: &Photo, kind: Kind) -> Option<Vec<u8>> {
        self.thumbnail_with(photo, kind, Priority::Background)
    }

    /// [`Thumbs::thumbnail`] with its decode at `priority`.
    fn thumbnail_with(&self, photo: &Photo, kind: Kind, priority: Priority) -> Option<Vec<u8>> {
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
                .share(key, || self.make_on_demand(photo, kind, priority))?;
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

    /// One on-demand decode. For the small size, that size alone from the
    /// fast decode. For the medium size, both sizes from `preview(1600)`,
    /// and for a photo still waiting the rest of its pixels pass too, kept
    /// for the scan.
    fn make_on_demand(&self, photo: &Photo, kind: Kind, priority: Priority) -> Option<Made> {
        // A job that finished between the caller's read and this one
        // starting has made it already.
        if let Some(bytes) = store::read(&store::path(self.dir(), &photo.thumb_key, kind)) {
            let bytes = Some(Arc::new(bytes));
            return Some(match kind {
                Kind::Small => Made {
                    small: bytes,
                    ..Made::default()
                },
                _ => Made {
                    medium: bytes,
                    ..Made::default()
                },
            });
        }
        let format = photo.format?;
        // The whole job runs in the slot, so no more full-size pictures are
        // held at once than there are slots.
        self.inner.slots.run(priority, || {
            let source = &self.inner.source;
            let decoded = match kind {
                Kind::Small => source.small(format, &photo.path),
                _ => source.preview(format, &photo.path),
            };
            let px = decoded
                .map_err(|e| {
                    tracing::debug!(target: "thumbs", "no thumbnail for photo {}: {}", photo.id, e.code.as_str());
                })
                .ok()?;
            Some(match kind {
                Kind::Small => self.make_small(&photo.thumb_key, &px),
                _ => self.make_from(photo, &px),
            })
        })
    }

    /// The page's fast path (#120): the small thumbnail alone, resized
    /// straight from the fast decode. Nothing else is made, so a photo
    /// still waiting gets its medium thumbnail and analysis from the scan,
    /// which finds this file whole and leaves it as it is: the tile never
    /// changes under the same key.
    fn make_small(&self, key: &str, px: &Pixels) -> Made {
        let small = make::encode(&make::small(px), make::SMALL_QUALITY);
        self.put(key, Kind::Small, &small);
        Made {
            small: Some(Arc::new(small)),
            ..Made::default()
        }
    }

    /// The thumbnails of an on-demand job for the medium size from its
    /// decode: the medium one, the small one when it is not on disk (one
    /// the fast path made stays as it is), and the rest of the pixels pass
    /// for a photo still waiting.
    fn make_from(&self, photo: &Photo, px: &Pixels) -> Made {
        let b = make::buffers(px);
        let key = &photo.thumb_key;
        let small =
            (!store::is_whole_file(&store::path(self.dir(), key, Kind::Small))).then(|| {
                let bytes = make::encode(&b.small, make::SMALL_QUALITY);
                self.put(key, Kind::Small, &bytes);
                Arc::new(bytes)
            });
        let medium = {
            let bytes = make::encode(&b.medium, make::MEDIUM_QUALITY);
            self.put(key, Kind::Medium, &bytes);
            Some(Arc::new(bytes))
        };
        let pixels = photo.waiting.then(|| {
            let done = Arc::new(PixelsDone::of(px, &b));
            let mut stash = self.inner.stash.lock().unwrap_or_else(|p| p.into_inner());
            if stash.len() < STASH_MAX {
                stash.insert(key.clone(), Arc::clone(&done));
            }
            done
        });
        Made {
            small,
            medium,
            pixels,
        }
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
            let px = self.inner.slots.run(Priority::Page, || {
                self.inner.source.working(format, &photo.path).ok()
            })?;
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
            if self
                .thumbnail_with(photo, Kind::Small, Priority::Background)
                .is_some()
            {
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
