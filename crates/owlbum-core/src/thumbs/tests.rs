//! Thumbnails, the zoom copy and the URLs against real albums
//! (docs/03-media-and-scanning.md, "Tests": "Thumbnails and URLs").

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use owlbum_fixtures::encode::{self, Meta};
use owlbum_fixtures::image::{Rgb, RgbImage, Rgba, RgbaImage};
use serde_json::Value;

use super::serve::{self, IMMUTABLE, NO_STORE, ONE_HOUR, Reply};
use super::*;
use crate::album::{Album, OpenOptions};
use crate::appdata::AppData;
use crate::events::Emitter;
use crate::media::{MediaError, MediaFormat, Pixels};
use crate::scan::{Request, ScanEnv, Scanner};

/// Drops every event.
struct Quiet;

impl Emitter for Quiet {
    fn emit_json(&self, _: &'static str, _: Value) {}
}

/// Decodes with the handlers, counting each decode, each taking at least
/// `delay`.
#[derive(Default)]
struct Counting {
    previews: AtomicUsize,
    workings: AtomicUsize,
    delay: Mutex<Duration>,
}

impl Source for Counting {
    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        self.previews.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.delay.lock().unwrap());
        Handlers.preview(format, path)
    }

    fn working(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        self.workings.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.delay.lock().unwrap());
        Handlers.working(format, path)
    }
}

/// A picture of four coloured quarters, so a turn shows.
fn quarters(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| match (x < w / 2, y < h / 2) {
        (true, true) => Rgb([220, 30, 30]),
        (false, true) => Rgb([30, 200, 30]),
        (true, false) => Rgb([30, 30, 220]),
        (false, false) => Rgb([240, 240, 240]),
    })
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// The album every test starts from.
fn make_album(root: &Path) {
    write(
        &root.join("big.jpg"),
        &encode::jpeg(&quarters(2000, 1000), 90, &Meta::default()),
    );
    write(
        &root.join("turned.jpg"),
        &encode::jpeg(
            &quarters(300, 200),
            90,
            &Meta {
                orientation: 6,
                ..Meta::default()
            },
        ),
    );
    write(
        &root.join("small.png"),
        &encode::png_rgb(&quarters(300, 200)),
    );
    let clear = RgbaImage::from_fn(40, 20, |x, _| {
        if x < 20 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    write(&root.join("clear.png"), &encode::png_rgba(&clear));
    let frames = [
        quarters(32, 32),
        RgbImage::from_pixel(32, 32, Rgb([9, 9, 9])),
    ];
    write(
        &root.join("moving.gif"),
        &encode::gif_animated(&frames, 100),
    );
    write(
        &root.join("sub/inner.png"),
        &encode::png_rgb(&quarters(64, 48)),
    );
}

struct Fx {
    root: PathBuf,
    data: AppData,
    thumbs: Thumbs,
    source: Arc<Counting>,
    scanner: Option<Scanner>,
    /// Last, so it is dropped last: fields drop in order, and on Windows
    /// the folder cannot be deleted while a database in it is still open.
    _tmp: tempfile::TempDir,
}

impl Fx {
    fn new() -> Self {
        Self::open(&OpenOptions::default(), "")
    }

    /// The album opened at `sub` (a scope) with `options`, scanned.
    fn open(options: &OpenOptions, sub: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("album");
        make_album(&root);
        let data = AppData::open(&tmp.path().join("data")).unwrap();
        let source = Arc::new(Counting::default());
        let thumbs = Thumbs::with(Cache::open(data.clone()).unwrap(), source.clone(), 4);
        let mut fx = Self {
            _tmp: tmp,
            root,
            data,
            thumbs,
            source,
            scanner: None,
        };
        fx.reopen(options, sub);
        fx
    }

    fn reopen(&mut self, options: &OpenOptions, sub: &str) {
        self.scanner = None;
        let folder = if sub.is_empty() {
            self.root.clone()
        } else {
            self.root.join(sub)
        };
        let album = Album::open(&folder, options).unwrap();
        let mut env = ScanEnv::new(Arc::new(Quiet));
        env.threads = 2;
        env.thumbs = Some(self.thumbs.clone());
        let scanner = Scanner::new(Arc::new(album), env);
        let summary = scanner.scan_and_wait(Request::default());
        assert!(summary.error.is_none(), "{:?}", summary.error);
        self.scanner = Some(scanner);
    }

    fn scanner(&self) -> &Scanner {
        self.scanner.as_ref().unwrap()
    }

    fn get(&self, url: &str) -> Reply {
        serve::answer(Some(self.scanner().open()), &self.thumbs, url)
    }

    /// `(id, thumb_key)` of the photo at `rel_path`, from the database or
    /// from memory.
    fn row(&self, rel_path: &str) -> (i64, String) {
        let memory = self.scanner().overlay();
        if let Some(r) = memory.rows().find(|r| r.rel_path == rel_path) {
            return (r.id, r.facts.thumb_key.clone());
        }
        drop(memory);
        self.scanner()
            .album()
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id, thumb_key FROM photos WHERE rel_path = ?1",
                    [rel_path],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap())
            })
            .unwrap()
    }

    fn thumb(&self, rel_path: &str, size: &str) -> Reply {
        self.get(&format!("owlbum://thumb/{}/{size}", self.row(rel_path).1))
    }

    fn file(&self, rel_path: &str, kind: Kind) -> PathBuf {
        store::path(self.data.dir(), &self.row(rel_path).1, kind)
    }

    fn previews(&self) -> usize {
        self.source.previews.swap(0, Ordering::SeqCst)
    }
}

fn size_of(jpeg: &[u8]) -> (u32, u32) {
    let img = image::load_from_memory(jpeg).unwrap();
    (img.width(), img.height())
}

fn pixel(jpeg: &[u8], x: u32, y: u32) -> [u8; 3] {
    image::load_from_memory(jpeg)
        .unwrap()
        .to_rgb8()
        .get_pixel(x, y)
        .0
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 24)
}

#[test]
fn the_scan_writes_both_thumbnails_and_their_entries() {
    let fx = Fx::new();
    let s = std::fs::read(fx.file("big.jpg", Kind::Small)).unwrap();
    let m = std::fs::read(fx.file("big.jpg", Kind::Medium)).unwrap();
    assert_eq!(size_of(&s), (512, 256));
    assert_eq!(size_of(&m), (1600, 800));
    // Never enlarged.
    let small_png = std::fs::read(fx.file("small.png", Kind::Medium)).unwrap();
    assert_eq!(size_of(&small_png), (300, 200));
    // The turned JPEG is upright.
    let turned = std::fs::read(fx.file("turned.jpg", Kind::Small)).unwrap();
    assert_eq!(size_of(&turned), (200, 300));
    fx.thumbs.cache().flush();
    let n: i64 = fx
        .data
        .conn()
        .query_row("SELECT count(*) FROM cache_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2 * 6, "s and m for each of six pictures");
    assert_eq!(
        fx.thumbs.cache().usage().bytes,
        (s.len() + m.len()) as u64
            + [
                "turned.jpg",
                "small.png",
                "clear.png",
                "moving.gif",
                "sub/inner.png"
            ]
            .iter()
            .map(|p| {
                std::fs::metadata(fx.file(p, Kind::Small)).unwrap().len()
                    + std::fs::metadata(fx.file(p, Kind::Medium)).unwrap().len()
            })
            .sum::<u64>()
    );
}

#[test]
fn transparency_is_flattened_on_white() {
    let fx = Fx::new();
    let r = fx.thumb("clear.png", "s");
    assert_eq!(r.status, 200);
    assert!(close(pixel(&r.body, 2, 10), [255, 255, 255]));
    assert!(close(pixel(&r.body, 37, 10), [0, 0, 255]));
}

#[test]
fn a_thumbnail_url_serves_the_jpeg_with_immutable_caching() {
    let fx = Fx::new();
    let r = fx.thumb("big.jpg", "m");
    assert_eq!(r.status, 200);
    assert_eq!(r.content_type, "image/jpeg");
    assert_eq!(r.cache_control, IMMUTABLE);
    assert_eq!(
        r.body,
        std::fs::read(fx.file("big.jpg", Kind::Medium)).unwrap()
    );
    // Served from disk: no decode.
    fx.previews();
    fx.thumb("big.jpg", "s");
    assert_eq!(fx.previews(), 0);
}

#[test]
fn a_missing_thumbnail_is_made_on_demand_and_concurrent_requests_decode_once() {
    let fx = Fx::new();
    fx.thumbs.cache().clear();
    fx.previews();
    *fx.source.delay.lock().unwrap() = Duration::from_millis(300);
    let key = fx.row("big.jpg").1;
    let replies: Vec<Reply> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let url = format!(
                    "owlbum://thumb/{key}/{}",
                    if i % 2 == 0 { "s" } else { "m" }
                );
                let fx = &fx;
                s.spawn(move || fx.get(&url))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(replies.iter().all(|r| r.status == 200));
    assert_eq!(fx.previews(), 1, "one decode for every request");
    assert_eq!(size_of(&replies[0].body), (512, 256));
    assert_eq!(size_of(&replies[1].body), (1600, 800));
    assert!(fx.file("big.jpg", Kind::Small).is_file());
    assert!(fx.file("big.jpg", Kind::Medium).is_file());
}

#[test]
fn a_thumbnail_cut_short_is_made_again() {
    let fx = Fx::new();
    let path = fx.file("small.png", Kind::Small);
    let whole = std::fs::read(&path).unwrap();
    std::fs::write(&path, &whole[..whole.len() / 2]).unwrap();
    fx.previews();
    let r = fx.thumb("small.png", "s");
    assert_eq!(r.status, 200);
    assert!(store::is_whole_jpeg(&r.body));
    assert_eq!(fx.previews(), 1);
    assert!(store::is_whole_file(&path));
}

#[test]
fn a_raised_scan_version_encodes_no_thumbnail_that_exists() {
    let mut fx = Fx::new();
    let path = fx.file("big.jpg", Kind::Small);
    // A marker: a different whole JPEG in its place.
    let marker = encode::jpeg(&quarters(8, 8), 50, &Meta::default());
    std::fs::write(&path, &marker).unwrap();
    let scanner = fx.scanner.take().unwrap();
    let album = Arc::clone(scanner.album());
    drop(scanner);
    let mut env = ScanEnv::new(Arc::new(Quiet));
    env.scan_version += 1;
    env.thumbs = Some(fx.thumbs.clone());
    let scanner = Scanner::new(album, env);
    let summary = scanner.scan_and_wait(Request::default());
    assert!(summary.changed > 0, "every row was read again");
    assert_eq!(std::fs::read(&path).unwrap(), marker);
}

#[test]
fn the_scan_stops_making_medium_thumbnails_at_seventy_percent() {
    let mut fx = Fx::new();
    fx.thumbs.cache().clear();
    // Already 70% full of something else.
    fx.thumbs.cache().set_limit_bytes(1_000_000);
    fx.thumbs
        .cache()
        .record(&"f".repeat(40), Kind::Small, 700_000);
    fx.scanner = None;
    std::fs::remove_dir_all(fx.root.join(".owlbum")).unwrap();
    fx.reopen(&OpenOptions::default(), "");
    assert!(fx.file("big.jpg", Kind::Small).is_file());
    assert!(!fx.file("big.jpg", Kind::Medium).exists());
    // Asked for, it is made.
    assert_eq!(fx.thumb("big.jpg", "m").status, 200);
    assert!(fx.file("big.jpg", Kind::Medium).is_file());
}

#[test]
fn the_warm_pass_makes_small_thumbnails_the_cache_lost() {
    let mut fx = Fx::new();
    fx.thumbs.cache().clear();
    fx.reopen(&OpenOptions::default(), "");
    for p in [
        "big.jpg",
        "turned.jpg",
        "small.png",
        "clear.png",
        "moving.gif",
    ] {
        assert!(fx.file(p, Kind::Small).is_file(), "{p}");
        assert!(!fx.file(p, Kind::Medium).exists(), "{p}: small only");
    }
}

#[test]
fn the_warm_pass_counts_files_that_lost_their_entries_without_reading_them() {
    let mut fx = Fx::new();
    fx.thumbs.cache().flush();
    fx.data
        .conn()
        .execute("DELETE FROM cache_entries", [])
        .unwrap();
    fx.previews();
    fx.reopen(&OpenOptions::default(), "");
    assert_eq!(fx.previews(), 0, "nothing decoded");
    fx.thumbs.cache().flush();
    let n: i64 = fx
        .data
        .conn()
        .query_row(
            "SELECT count(*) FROM cache_entries WHERE kind = 's'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 6, "every small thumbnail is counted again");
}

#[test]
fn the_zoom_copy_of_an_upright_srgb_jpeg_is_the_file_itself() {
    let fx = Fx::new();
    let (id, key) = fx.row("big.jpg");
    let r = fx.get(&format!("owlbum://full/{id}?v={key}"));
    assert_eq!(r.status, 200);
    assert_eq!(r.cache_control, ONE_HOUR);
    assert_eq!(r.body, std::fs::read(fx.root.join("big.jpg")).unwrap());
    assert!(!fx.file("big.jpg", Kind::Full).exists(), "nothing cached");
    assert_eq!(fx.source.workings.load(Ordering::SeqCst), 0);
}

#[test]
fn the_zoom_copy_of_anything_else_is_made_once_and_kept() {
    let fx = Fx::new();
    let (id, _) = fx.row("turned.jpg");
    *fx.source.delay.lock().unwrap() = Duration::from_millis(200);
    let replies: Vec<Reply> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let fx = &fx;
                s.spawn(move || fx.get(&format!("owlbum://full/{id}")))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(replies.iter().all(|r| r.status == 200));
    assert_eq!(fx.source.workings.load(Ordering::SeqCst), 1);
    assert_eq!(size_of(&replies[0].body), (200, 300), "upright, full size");
    assert!(fx.file("turned.jpg", Kind::Full).is_file());
    let again = fx.get(&format!("owlbum://full/{id}"));
    assert_eq!(again.body, replies[0].body);
    assert_eq!(fx.source.workings.load(Ordering::SeqCst), 1, "kept");
    fx.thumbs.cache().flush();
    let kinds: Vec<String> = fx
        .data
        .conn()
        .prepare("SELECT kind FROM cache_entries WHERE kind = 'full'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(kinds, ["full"]);
    // A PNG with transparency is flattened on white.
    let (clear, _) = fx.row("clear.png");
    let png = fx.get(&format!("owlbum://full/{clear}"));
    assert_eq!(size_of(&png.body), (40, 20));
    assert!(close(pixel(&png.body, 2, 10), [255, 255, 255]));
}

#[test]
fn original_serves_only_an_animated_still() {
    let fx = Fx::new();
    let (gif, key) = fx.row("moving.gif");
    let r = fx.get(&format!("http://owlbum.localhost/original/{gif}?v={key}"));
    assert_eq!(r.status, 200);
    assert_eq!(r.content_type, "image/gif");
    assert_eq!(r.cache_control, ONE_HOUR);
    assert_eq!(r.body, std::fs::read(fx.root.join("moving.gif")).unwrap());
    let (still, _) = fx.row("small.png");
    assert_eq!(fx.get(&format!("owlbum://original/{still}")).status, 404);
}

#[test]
fn video_of_a_picture_is_not_found() {
    let fx = Fx::new();
    let (id, _) = fx.row("big.jpg");
    assert_eq!(fx.get(&format!("owlbum://video/{id}")).status, 404);
}

#[test]
fn every_url_answers_404_for_an_unknown_id_or_key() {
    let fx = Fx::new();
    let key = "0".repeat(40);
    for url in [
        format!("owlbum://thumb/{key}/s"),
        "owlbum://full/9999".to_owned(),
        "owlbum://original/9999".to_owned(),
        "owlbum://video/9999".to_owned(),
        "owlbum://nothing/1".to_owned(),
    ] {
        let r = fx.get(&url);
        assert_eq!(r.status, 404, "{url}");
        assert_eq!(r.cache_control, NO_STORE);
    }
    // No album open.
    let (id, _) = fx.row("big.jpg");
    assert_eq!(
        serve::answer(None, &fx.thumbs, &format!("owlbum://full/{id}")).status,
        404
    );
}

#[test]
fn a_missing_row_answers_404_even_with_its_thumbnail_on_disk() {
    let mut fx = Fx::new();
    let (id, key) = fx.row("small.png");
    let file = fx.file("small.png", Kind::Small);
    std::fs::remove_file(fx.root.join("small.png")).unwrap();
    fx.reopen(&OpenOptions::default(), "");
    assert!(file.is_file(), "still in the shared cache");
    assert_eq!(fx.get(&format!("owlbum://thumb/{key}/s")).status, 404);
    assert_eq!(fx.get(&format!("owlbum://full/{id}")).status, 404);
}

#[test]
fn a_key_from_another_album_answers_404() {
    let fx = Fx::new();
    let other = Fx::new();
    let foreign = other.row("big.jpg").1;
    assert_ne!(foreign, fx.row("big.jpg").1, "keys include the album id");
    // Even with its file in this cache.
    let bytes = std::fs::read(other.file("big.jpg", Kind::Small)).unwrap();
    store::write(&store::path(fx.data.dir(), &foreign, Kind::Small), &bytes).unwrap();
    assert_eq!(fx.get(&format!("owlbum://thumb/{foreign}/s")).status, 404);
}

#[test]
fn a_photo_outside_the_scope_answers_404() {
    let mut fx = Fx::new();
    let (id, key) = fx.row("big.jpg");
    let (inner, inner_key) = fx.row("sub/inner.png");
    fx.reopen(&OpenOptions::default(), "sub");
    assert_eq!(fx.get(&format!("owlbum://thumb/{key}/s")).status, 404);
    assert_eq!(fx.get(&format!("owlbum://full/{id}")).status, 404);
    assert_eq!(fx.get(&format!("owlbum://thumb/{inner_key}/s")).status, 200);
    assert_eq!(fx.get(&format!("owlbum://full/{inner}")).status, 200);
}

#[test]
fn a_view_only_album_serves_the_rows_it_holds_in_memory() {
    let mut fx = Fx::new();
    write(
        &fx.root.join("later.png"),
        &encode::png_rgb(&quarters(50, 40)),
    );
    fx.reopen(
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
        "",
    );
    assert!(fx.scanner().album().is_view_only());
    let (id, key) = fx.row("later.png");
    assert!(
        fx.scanner().overlay().get(id).is_some(),
        "held in memory only"
    );
    let r = fx.get(&format!("owlbum://thumb/{key}/s"));
    assert_eq!(r.status, 200);
    assert_eq!(size_of(&r.body), (50, 40));
}

#[test]
fn a_photo_that_cannot_be_decoded_answers_404() {
    let mut fx = Fx::new();
    // Damaged after its row was complete; the next scan has not run.
    let (id, key) = fx.row("small.png");
    fx.thumbs.cache().clear();
    std::fs::write(fx.root.join("small.png"), b"not a png at all").unwrap();
    assert_eq!(fx.get(&format!("owlbum://thumb/{key}/s")).status, 404);
    assert_eq!(fx.get(&format!("owlbum://full/{id}")).status, 404);
    fx.reopen(&OpenOptions::default(), "");
}

#[test]
fn an_upright_srgb_jpeg_is_recognised() {
    let plain = encode::jpeg(&quarters(16, 16), 80, &Meta::default());
    assert!(is_upright_srgb_jpeg(&plain));
    let one = encode::jpeg(
        &quarters(16, 16),
        80,
        &Meta {
            orientation: 1,
            ..Meta::default()
        },
    );
    assert!(is_upright_srgb_jpeg(&one));
    let turned = encode::jpeg(
        &quarters(16, 16),
        80,
        &Meta {
            orientation: 3,
            ..Meta::default()
        },
    );
    assert!(!is_upright_srgb_jpeg(&turned));
    assert!(!is_upright_srgb_jpeg(b"not a jpeg"));
    // A Display P3 profile: the webview would show it wrong.
    let mut p3 = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut p3, 80);
    image::ImageEncoder::set_icc_profile(&mut encoder, owlbum_fixtures::icc::display_p3()).unwrap();
    let img = quarters(16, 16);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        16,
        16,
        image::ExtendedColorType::Rgb8,
    )
    .unwrap();
    assert!(!is_upright_srgb_jpeg(&p3));
}

// ---- The page's requests and the scan share decodes (#106, #91) ----

/// The scan's reader: counts the scan's decodes by file name, and holds one
/// decode (of `held`, or the first one when `held` is None) until let go.
#[derive(Default)]
struct Gate {
    held: Mutex<Option<String>>,
    /// The file the held decode is of, once it is held.
    holding: Mutex<Option<String>>,
    open: Mutex<bool>,
    opened: std::sync::Condvar,
    decodes: Mutex<Vec<String>>,
}

impl Gate {
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }

    /// Waits until a decode is held, and answers its file name.
    fn until_holding(&self) -> String {
        until(|| self.holding.lock().unwrap().clone())
    }
}

impl crate::scan::Reader for Gate {
    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.decodes.lock().unwrap().push(name.clone());
        let hold = {
            let held = self.held.lock().unwrap();
            let mut holding = self.holding.lock().unwrap();
            let hold = holding.is_none() && held.as_ref().is_none_or(|h| *h == name);
            if hold {
                *holding = Some(name.clone());
            }
            hold
        };
        if hold {
            let mut open = self.open.lock().unwrap();
            while !*open {
                open = self.opened.wait(open).unwrap();
            }
        }
        crate::scan::MediaReader.preview(format, path)
    }
}

/// Polls `f` until it answers something, for at most 30 seconds.
fn until<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let start = std::time::Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// An album scanned by one scan thread reading with `gate`, the scan
/// started but not waited for.
struct Live {
    thumbs: Thumbs,
    source: Arc<Counting>,
    gate: Arc<Gate>,
    scanner: Scanner,
    _tmp: tempfile::TempDir,
}

impl Live {
    fn start(held: Option<&str>) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("album");
        make_album(&root);
        let data = AppData::open(&tmp.path().join("data")).unwrap();
        let source = Arc::new(Counting::default());
        let thumbs = Thumbs::with(Cache::open(data).unwrap(), source.clone(), 4);
        let gate = Arc::new(Gate::default());
        *gate.held.lock().unwrap() = held.map(str::to_owned);
        let album = Album::open(&root, &OpenOptions::default()).unwrap();
        let mut env = ScanEnv::new(Arc::new(Quiet));
        env.threads = 1;
        env.reader = gate.clone();
        env.thumbs = Some(thumbs.clone());
        let scanner = Scanner::new(Arc::new(album), env);
        scanner.request(Request::default());
        Self {
            thumbs,
            source,
            gate,
            scanner,
            _tmp: tmp,
        }
    }

    /// `(thumb_key, analysis)` of the row of `name`, once it is written.
    fn row(&self, name: &str) -> Option<(String, Option<String>)> {
        self.scanner
            .album()
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT thumb_key, analysis FROM photos WHERE file_name = ?1",
                    [name],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok())
            })
            .unwrap()
    }

    fn get(&self, url: &str) -> Reply {
        serve::answer(Some(self.scanner.open()), &self.thumbs, url)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.gate.release();
        self.scanner.cancel();
        self.scanner.wait();
    }
}

#[test]
fn a_page_request_makes_a_waiting_photos_pass_and_the_scan_does_not_decode_it_again() {
    let live = Live::start(None);
    // The scan's only thread is held in its first decode.
    let held = live.gate.until_holding();
    let target = if held == "turned.jpg" {
        "big.jpg"
    } else {
        "turned.jpg"
    };
    let (key, analysis) = until(|| live.row(target));
    assert_eq!(analysis, None, "still waiting for its pixels pass");
    let reply = live.get(&format!("owlbum://thumb/{key}/s"));
    assert_eq!(reply.status, 200, "made at once, ahead of the scan");
    assert_eq!(live.source.previews.load(Ordering::SeqCst), 1);
    live.gate.release();
    live.scanner.wait();
    let decoded = live.gate.decodes.lock().unwrap().clone();
    assert!(
        !decoded.iter().any(|n| n == target),
        "the scan wrote the page's pass without decoding again: {decoded:?}"
    );
    assert_eq!(decoded.len(), 5, "every other picture once: {decoded:?}");
    let (_, analysis) = live.row(target).unwrap();
    assert!(analysis.is_some_and(|a| a.starts_with('{')));
    assert!(
        live.thumbs.inner.stash.lock().unwrap().is_empty(),
        "nothing left waiting for the scan"
    );
}

#[test]
fn a_page_request_joins_the_scans_decode_of_the_same_photo() {
    let live = Live::start(Some("turned.jpg"));
    assert_eq!(live.gate.until_holding(), "turned.jpg");
    let (key, _) = until(|| live.row("turned.jpg"));
    std::thread::scope(|s| {
        let request = s.spawn(|| live.get(&format!("owlbum://thumb/{key}/s")));
        until(|| (live.thumbs.inner.thumbs.joined(&key) == 1).then_some(()));
        live.gate.release();
        let reply = request.join().unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(size_of(&reply.body), (200, 300), "upright");
    });
    assert_eq!(
        live.source.previews.load(Ordering::SeqCst),
        0,
        "one decode, the scan's"
    );
    live.scanner.wait();
    assert!(live.row("turned.jpg").unwrap().1.is_some());
}
