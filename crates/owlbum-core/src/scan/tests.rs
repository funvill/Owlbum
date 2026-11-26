//! The scanner against albums made by `owlbum-fixtures`
//! (docs/03-media-and-scanning.md, "Tests": walker, change detection,
//! identity, errors, running).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use owlbum_fixtures::Kind;
use owlbum_fixtures::image::{Rgb, RgbImage};
use serde_json::Value;

use super::keys::{self, Sample};
use super::memory::Overlay;
use super::progress::{Progress, ScanPhase, ScanProgress};
use super::reader::{MediaReader, Reader};
use super::walk::absolute;
use super::*;
use crate::album::open::Env;
use crate::album::{Album, AlbumChanged, OpenOptions, sqlite_error};
use crate::events::Emitter;
use crate::media::{MediaError, MediaFormat, Metadata, Pixels, VideoInfo};
use crate::{ErrorCode, text};

mod targeted;
mod watching;

// ---- Helpers ----

/// Records every event with the moment it was sent.
#[derive(Default)]
struct Recorder(Mutex<Vec<(&'static str, Value, Instant)>>);

impl Emitter for Recorder {
    fn emit_json(&self, name: &'static str, payload: Value) {
        self.0.lock().unwrap().push((name, payload, Instant::now()));
    }
}

impl Recorder {
    fn progress(&self) -> Vec<(ScanProgress, Instant)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, ..)| *n == "scan_progress")
            .map(|(_, v, t)| (serde_json::from_value(v.clone()).unwrap(), *t))
            .collect()
    }

    fn changes(&self) -> Vec<(AlbumChanged, Instant)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, ..)| *n == "album_changed")
            .map(|(_, v, t)| (serde_json::from_value(v.clone()).unwrap(), *t))
            .collect()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// A counting handler: records every read, by path relative to the album
/// root, and can cancel the scan at its first decode.
#[derive(Default)]
struct Counting {
    root: Mutex<PathBuf>,
    reads: Mutex<Vec<(String, &'static str)>>,
    cancel_at_preview: Mutex<Option<Arc<AtomicBool>>>,
}

impl Counting {
    fn note(&self, path: &Path, what: &'static str) {
        let root = self.root.lock().unwrap().clone();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        self.reads.lock().unwrap().push((rel, what));
    }

    fn take(&self) -> Vec<(String, &'static str)> {
        std::mem::take(&mut *self.reads.lock().unwrap())
    }
}

impl Reader for Counting {
    fn sample(&self, path: &Path) -> std::io::Result<Sample> {
        self.note(path, "sample");
        MediaReader.sample(path)
    }

    fn metadata(&self, format: &'static MediaFormat, path: &Path) -> Result<Metadata, MediaError> {
        self.note(path, "metadata");
        MediaReader.metadata(format, path)
    }

    fn probe(
        &self,
        format: &'static MediaFormat,
        path: &Path,
    ) -> Result<Option<VideoInfo>, MediaError> {
        self.note(path, "probe");
        MediaReader.probe(format, path)
    }

    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        self.note(path, "preview");
        if let Some(flag) = self.cancel_at_preview.lock().unwrap().as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
        MediaReader.preview(format, path)
    }
}

/// One `photos` row, as the tests look at it.
#[derive(Debug, Clone, PartialEq)]
struct Photo {
    id: i64,
    rel_path: String,
    rel_dir: String,
    file_name: String,
    missing: bool,
    size: i64,
    mtime_ms: i64,
    file_key: String,
    thumb_key: String,
    kind: String,
    format: String,
    width: i64,
    height: i64,
    duration_ms: Option<i64>,
    analysis: Option<String>,
    scan_version: i64,
    scan_error: Option<String>,
    star: i64,
}

/// An album on disk, open, with a scan environment that records.
struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    manifest: owlbum_fixtures::Album,
    album: Option<Album>,
    overlay: Mutex<Overlay>,
    events: Arc<Recorder>,
    reads: Arc<Counting>,
    env: ScanEnv,
    progress: Progress,
    cancel: Arc<AtomicBool>,
}

impl Fx {
    fn new(make: impl FnOnce(&Path) -> std::io::Result<owlbum_fixtures::Album>) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("album");
        let manifest = make(&root).unwrap();
        let root = crate::album::locate(&root).unwrap().root;
        let events = Arc::new(Recorder::default());
        let reads = Arc::new(Counting::default());
        *reads.root.lock().unwrap() = root.clone();
        let mut env = ScanEnv::new(events.clone());
        env.reader = reads.clone();
        env.threads = 3;
        let progress = Progress::new(events.clone(), env.progress_every);
        let album = Album::open(&root, &OpenOptions::default()).unwrap();
        Self {
            _tmp: tmp,
            root,
            manifest,
            album: Some(album),
            overlay: Mutex::new(Overlay::default()),
            events,
            reads,
            env,
            progress,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn trip() -> Self {
        Self::new(owlbum_fixtures::trip)
    }

    fn extras() -> Self {
        Self::new(owlbum_fixtures::extras)
    }

    fn large(n: usize) -> Self {
        Self::new(|root| owlbum_fixtures::large(root, n, owlbum_fixtures::DEFAULT_SEED))
    }

    /// An album with no files yet.
    fn empty() -> Self {
        Self::new(|root| {
            std::fs::create_dir_all(root)?;
            Ok(owlbum_fixtures::Album {
                root: root.to_path_buf(),
                files: Vec::new(),
            })
        })
    }

    fn album(&self) -> &Album {
        self.album.as_ref().unwrap()
    }

    /// The open album, for a [`Scanner`] to own.
    fn take_album(&mut self) -> Arc<Album> {
        Arc::new(self.album.take().unwrap())
    }

    /// Closes the album and opens `folder` (the root or a subfolder) with
    /// `options`, in `env`.
    fn reopen_with(&mut self, folder: &Path, options: &OpenOptions, env: &Env) {
        self.album = None;
        self.album = Some(Album::open_with(folder, options, env).unwrap());
        self.overlay = Mutex::new(Overlay::default());
    }

    fn reopen(&mut self, folder: &Path, options: &OpenOptions) {
        self.reopen_with(folder, options, &Env::real());
    }

    fn run(&self, request: Request) -> ScanSummary {
        run(
            &Scan {
                album: self.album(),
                overlay: &self.overlay,
                env: &self.env,
                cancel: &self.cancel,
                progress: &self.progress,
            },
            request,
        )
    }

    /// An automatic scan.
    fn scan(&self) -> ScanSummary {
        self.cancel.store(false, Ordering::SeqCst);
        self.run(Request::default())
    }

    /// A manual Rescan.
    fn rescan(&self) -> ScanSummary {
        self.cancel.store(false, Ordering::SeqCst);
        self.run(Request::manual())
    }

    fn path(&self, rel: &str) -> PathBuf {
        absolute(&self.root, rel)
    }

    fn photos(&self) -> Vec<Photo> {
        self.album()
            .db()
            .read(|c| {
                let mut st = c
                    .prepare(
                        "SELECT id, rel_path, rel_dir, file_name, missing, size, mtime_ms, file_key,
                                thumb_key, kind, format, width, height, duration_ms, analysis,
                                scan_version, scan_error, star
                         FROM photos ORDER BY id",
                    )
                    .unwrap();
                Ok(st
                    .query_map([], |r| {
                        Ok(Photo {
                            id: r.get(0)?,
                            rel_path: r.get(1)?,
                            rel_dir: r.get(2)?,
                            file_name: r.get(3)?,
                            missing: r.get(4)?,
                            size: r.get(5)?,
                            mtime_ms: r.get(6)?,
                            file_key: r.get(7)?,
                            thumb_key: r.get(8)?,
                            kind: r.get(9)?,
                            format: r.get(10)?,
                            width: r.get(11)?,
                            height: r.get(12)?,
                            duration_ms: r.get(13)?,
                            analysis: r.get(14)?,
                            scan_version: r.get(15)?,
                            scan_error: r.get(16)?,
                            star: r.get(17)?,
                        })
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect())
            })
            .unwrap()
    }

    fn photo(&self, rel: &str) -> Option<Photo> {
        self.photos().into_iter().find(|p| p.rel_path == rel)
    }

    fn by_id(&self, id: i64) -> Photo {
        self.photos().into_iter().find(|p| p.id == id).unwrap()
    }

    fn sql(&self, sql: &'static str) {
        self.album()
            .db()
            .write(move |tx| {
                tx.execute_batch(sql)
                    .map_err(sqlite_error(text::album::WRITING_DATABASE))
            })
            .unwrap();
    }

    fn star(&self, id: i64) {
        self.album()
            .db()
            .write(move |tx| {
                tx.execute("UPDATE photos SET star = 1 WHERE id = ?1", [id])
                    .map_err(sqlite_error(text::album::WRITING_DATABASE))
            })
            .unwrap();
    }

    /// Paths read by the scan, of one kind of read.
    fn read(&self, what: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .reads
            .reads
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, w)| *w == what)
            .map(|(p, _)| p.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Writes a small picture whose bytes depend on `seed`, in the format its
/// extension names.
fn picture(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = RgbImage::from_fn(40, 30, |x, y| {
        Rgb([seed, (x * 6) as u8, (y * 8) as u8 ^ seed])
    });
    img.save(path).unwrap();
}

/// Sets a file's modified time.
fn set_mtime(path: &Path, t: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// Moves a file's modified time one minute later.
fn touch_later(path: &Path) {
    let now = std::fs::metadata(path).unwrap().modified().unwrap();
    set_mtime(path, now + Duration::from_secs(60));
}

fn rename(fx: &Fx, from: &str, to: &str) {
    let to = fx.path(to);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::rename(fx.path(from), to).unwrap();
}

/// The files a fixture made that should become rows: everything but the
/// recognised-but-unsupported ones.
fn expected_rows(m: &owlbum_fixtures::Album) -> Vec<&owlbum_fixtures::Fixture> {
    m.files
        .iter()
        .filter(|f| f.kind != Kind::Unsupported)
        .collect()
}

fn code(p: &Photo) -> Option<&str> {
    p.scan_error.as_deref().and_then(|e| e.split(':').next())
}

// ---- First scan ----

#[test]
fn a_first_scan_gives_every_supported_trip_file_a_row() {
    let fx = Fx::trip();
    let summary = fx.scan();
    assert!(summary.error.is_none(), "{:?}", summary.error);
    assert!(!summary.cancelled);

    let photos = fx.photos();
    let expected = expected_rows(&fx.manifest);
    assert_eq!(photos.len(), expected.len(), "{photos:#?}");
    let album_id = fx.album().db().album_id().to_owned();
    for f in expected {
        let p = fx
            .photo(&f.rel_path)
            .unwrap_or_else(|| panic!("no row for {}", f.rel_path));
        let (dir, name) = f.rel_path.rsplit_once('/').unwrap();
        assert_eq!((p.rel_dir.as_str(), p.file_name.as_str()), (dir, name));
        assert!(!p.missing);
        assert_eq!(p.scan_version, SCAN_VERSION);
        let bytes = std::fs::read(fx.manifest.path(f)).unwrap();
        assert_eq!(p.size, bytes.len() as i64);
        let key = keys::sample(&fx.manifest.path(f)).unwrap().file_key;
        assert_eq!(p.file_key, key, "{}", f.rel_path);
        assert_eq!(p.thumb_key, keys::thumb_key(&album_id, &key, p.mtime_ms));
        match f.kind {
            Kind::Corrupt => {
                assert_eq!(code(&p), Some("decode"), "{p:?}");
                assert_eq!((p.width, p.height), (0, 0));
                assert_eq!(p.analysis, None);
            }
            // RAW and video read with placeholder handlers until #49, #50.
            Kind::Raw | Kind::Video => {
                assert_eq!(code(&p), Some("unsupported"), "{p:?}");
            }
            _ => {
                assert_eq!(p.scan_error, None, "{}", f.rel_path);
                assert_eq!(p.format, f.format.unwrap(), "{}", f.rel_path);
                assert_eq!(p.kind, "image");
                assert_eq!(
                    (p.width, p.height),
                    (i64::from(f.width), i64::from(f.height)),
                    "{}",
                    f.rel_path
                );
                let a: Value = serde_json::from_str(p.analysis.as_deref().unwrap()).unwrap();
                assert_eq!(
                    a["dhash"].as_str().map(str::len),
                    Some(16),
                    "{}",
                    f.rel_path
                );
            }
        }
    }

    let n = u32::try_from(fx.photos().len()).unwrap();
    assert_eq!(summary.found, n);
    assert_eq!(summary.done, n);
    assert_eq!(summary.failed, 3, "corrupt, RAW and video");
    assert_eq!(summary.added, n);
    let unsupported: Vec<_> = summary
        .walk
        .unsupported
        .iter()
        .map(|u| u.rel_path.as_str())
        .collect();
    assert_eq!(unsupported, ["problems/photo.heic"]);
    assert_eq!(
        summary.walk.unsupported[0].label,
        crate::media::unsupported_of("photo.heic").unwrap().label
    );
}

#[test]
fn file_names_are_searchable_after_a_scan() {
    let fx = Fx::trip();
    fx.scan();
    let hits: i64 = fx
        .album()
        .db()
        .read(|c| {
            c.query_row(
                "SELECT count(*) FROM search WHERE search MATCH 'file_name:harbour'",
                [],
                |r| r.get(0),
            )
            .map_err(sqlite_error(text::album::READING_DATABASE))
        })
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn extras_rows_follow_what_each_file_is() {
    let fx = Fx::extras();
    let summary = fx.scan();
    assert!(summary.error.is_none());
    for f in &fx.manifest.files {
        let row = fx.photo(&f.rel_path);
        match f.kind {
            Kind::Unsupported => {
                assert!(row.is_none(), "{}", f.rel_path);
                assert!(
                    summary
                        .walk
                        .unsupported
                        .iter()
                        .any(|u| u.rel_path == f.rel_path),
                    "{} is counted",
                    f.rel_path
                );
            }
            // Dot names, `._` files, system folders, a nested album.
            Kind::Skipped | Kind::Nested => {
                assert!(row.is_none(), "{} is skipped", f.rel_path);
            }
            Kind::Corrupt => assert_eq!(code(&row.unwrap()), Some("decode")),
            Kind::TooLarge => assert_eq!(code(&row.unwrap()), Some("too_large")),
            Kind::WrongExtension => {
                let row = row.unwrap();
                match f.format {
                    // A PNG named as a JPEG reads as what it is.
                    Some(real) => {
                        assert_eq!(row.format, real);
                        assert_eq!(row.scan_error, None);
                    }
                    // A HEIC named as a JPEG gets a row with `unsupported`.
                    None => {
                        assert_eq!(code(&row), Some("unsupported"));
                        assert_eq!(
                            row.scan_error.as_deref(),
                            Some("unsupported: HEIC photos are not supported yet.")
                        );
                    }
                }
            }
            Kind::Video => assert_eq!(code(&row.unwrap()), Some("unsupported")),
            Kind::Animated => {
                let row = row.unwrap();
                assert_eq!(row.scan_error, None, "{}", f.rel_path);
                // GIF and WebP loop; an animated PNG is shown as a still.
                if crate::media::format_by_id(&row.format)
                    .unwrap()
                    .caps
                    .animated
                {
                    assert!(row.duration_ms.is_some(), "{} loops", f.rel_path);
                }
            }
            _ => {
                let row = row.unwrap_or_else(|| panic!("no row for {}", f.rel_path));
                assert_eq!(row.scan_error, None, "{}", f.rel_path);
                assert_eq!(
                    (row.width, row.height),
                    (i64::from(f.width), i64::from(f.height)),
                    "{}",
                    f.rel_path
                );
            }
        }
    }
}

// ---- What the walker leaves out ----

#[test]
fn extras_skipped_names_are_never_read_and_the_nested_album_is_reported() {
    let fx = Fx::extras();
    let summary = fx.scan();
    let read: Vec<String> = fx.reads.take().into_iter().map(|(p, _)| p).collect();
    for f in &fx.manifest.files {
        if matches!(f.kind, Kind::Skipped | Kind::Nested) {
            assert!(!read.contains(&f.rel_path), "{} was read", f.rel_path);
        }
    }
    assert_eq!(summary.walk.nested, ["nested"]);
    let report = report::report(Some(&summary.walk));
    assert_eq!(
        report.nested_albums,
        [report::NestedAlbum {
            rel_path: "nested".into(),
            message: "nested is its own album and is not shown here.".into(),
        }]
    );
    assert_eq!(report.nested_album_count, 1);
}

#[test]
fn rows_under_a_folder_that_becomes_an_album_go_missing() {
    let fx = Fx::empty();
    picture(&fx.path("inner/a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    assert!(!fx.photo("inner/a.png").unwrap().missing);
    std::fs::create_dir_all(fx.path("inner/.owlbum")).unwrap();
    std::fs::write(fx.path("inner/.owlbum/owlbum.sqlite"), b"").unwrap();
    fx.reads.take();
    let summary = fx.scan();
    assert_eq!(fx.reads.take(), [], "nothing under it is read");
    assert!(fx.photo("inner/a.png").unwrap().missing);
    assert!(!fx.photo("b.png").unwrap().missing);
    assert_eq!(summary.missing, 1);
}

#[cfg(windows)]
#[test]
fn an_online_only_file_is_never_read_and_its_row_is_kept() {
    use super::walk::tests::{clear_offline, set_offline};
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    let b = fx.photo("b.png").unwrap();
    // "Free up space" on b, and a new file that has never been downloaded.
    picture(&fx.path("c.png"), 3);
    set_offline(&fx.path("b.png"));
    set_offline(&fx.path("c.png"));
    fx.reads.take();

    let summary = fx.scan();
    let reads = fx.reads.take();
    clear_offline(&fx.path("b.png"));
    clear_offline(&fx.path("c.png"));
    assert_eq!(reads, [], "no online-only file is read");
    assert_eq!(fx.photo("b.png").unwrap(), b, "its row is kept as it was");
    assert!(fx.photo("c.png").is_none());
    assert_eq!(summary.walk.online_only.len(), 2);
    assert_eq!(summary.missing, 0);
    assert_eq!(report::report(Some(&summary.walk)).online_only, 2);
}

// ---- The scan report ----

#[test]
fn the_report_counts_what_the_walk_left_out() {
    let fx = Fx::extras();
    let summary = fx.scan();
    let report = report::report(Some(&summary.walk));
    // Unsupported files by label, in the order of the registry's table.
    let mut expected: Vec<report::UnsupportedCount> = Vec::new();
    for u in crate::media::UNSUPPORTED {
        let count = fx
            .manifest
            .files
            .iter()
            .filter(|f| {
                f.kind == Kind::Unsupported
                    && crate::media::unsupported_of(rows::file_name(&f.rel_path))
                        .is_some_and(|x| x.label == u.label)
            })
            .count();
        if count > 0 {
            expected.push(report::UnsupportedCount {
                label: u.label.to_owned(),
                count: u32::try_from(count).unwrap(),
            });
        }
    }
    assert!(!expected.is_empty());
    assert_eq!(report.unsupported, expected);
    assert_eq!(report.links, 0);
    assert_eq!(report.online_only, 0);
    assert_eq!(report.unreadable_names, 0);
    assert!(report.unreadable_folders.is_empty());
    assert!(report.notices.is_empty());
}

#[test]
fn before_any_walk_the_report_is_empty() {
    assert_eq!(report::report(None), report::ScanReport::default());
    assert_eq!(report::unsupported(None, 0, 10).total, 0);
}

#[test]
fn the_report_lists_are_short_and_say_how_many_there_were() {
    let walk = walk::Walk {
        complete: true,
        nested: (0..150).map(|i| format!("n{i}")).collect(),
        unreadable: (0..120)
            .map(|i| walk::UnreadableFolder {
                rel_path: format!("u{i}"),
                message: "denied".into(),
            })
            .collect(),
        ..walk::Walk::default()
    };
    let report = report::report(Some(&walk));
    assert_eq!(report.nested_albums.len(), report::REPORT_LIST_MAX);
    assert_eq!(report.nested_album_count, 150);
    assert_eq!(report.unreadable_folders.len(), report::REPORT_LIST_MAX);
    assert_eq!(report.unreadable_folder_count, 120);
}

#[test]
fn unsupported_files_are_paged_in_walker_order() {
    let fx = Fx::empty();
    for name in ["b/10.heic", "b/2.avif", "a.heic"] {
        std::fs::create_dir_all(fx.path(name).parent().unwrap()).unwrap();
        std::fs::write(fx.path(name), b"x").unwrap();
    }
    let summary = fx.scan();
    let all = report::unsupported(Some(&summary.walk), 0, 500);
    assert_eq!(all.total, 3);
    let paths: Vec<&str> = all.items.iter().map(|u| u.rel_path.as_str()).collect();
    assert_eq!(paths, ["a.heic", "b/2.avif", "b/10.heic"]);
    assert_eq!(all.items[0].reason, "HEIC photos are not supported yet.");
    let page = report::unsupported(Some(&summary.walk), 1, 1);
    assert_eq!(page.total, 3);
    assert_eq!(page.items, [all.items[1].clone()]);
    assert!(
        report::unsupported(Some(&summary.walk), 5, 10)
            .items
            .is_empty()
    );
}

#[test]
fn a_page_holds_at_most_500() {
    let walk = walk::Walk {
        complete: true,
        unsupported: (0..600)
            .map(|i| walk::UnsupportedFile {
                rel_path: format!("{i}.heic"),
                label: "HEIC".into(),
                reason: "HEIC photos are not supported yet.".into(),
            })
            .collect(),
        ..walk::Walk::default()
    };
    let page = report::unsupported(Some(&walk), 0, 10_000);
    assert_eq!(page.total, 600);
    assert_eq!(page.items.len(), 500);
}

#[test]
fn scan_errors_are_listed_in_walker_order_without_missing_rows() {
    let fx = Fx::empty();
    std::fs::create_dir_all(fx.path("sub")).unwrap();
    std::fs::write(fx.path("sub/10.png"), b"not a picture").unwrap();
    std::fs::write(fx.path("sub/2.png"), b"not a picture").unwrap();
    std::fs::write(fx.path("gone.png"), b"not a picture").unwrap();
    picture(&fx.path("good.png"), 1);
    fx.scan();
    let errors = |offset, limit| {
        let overlay = fx.overlay.lock().unwrap();
        report::scan_errors(fx.album(), &overlay, offset, limit).unwrap()
    };
    let all = errors(0, 500);
    assert_eq!(all.total, 3);
    let paths: Vec<&str> = all.items.iter().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(paths, ["gone.png", "sub/2.png", "sub/10.png"]);
    let first = &all.items[0];
    assert_eq!(first.id, fx.photo("gone.png").unwrap().id);
    assert_eq!(first.code, "decode");
    assert_eq!(
        first.message,
        "This file is damaged or is not a PNG picture."
    );
    assert_eq!(errors(2, 5).items, [all.items[2].clone()]);
    assert_eq!(errors(0, 0).items, []);

    std::fs::remove_file(fx.path("gone.png")).unwrap();
    fx.scan();
    assert_eq!(errors(0, 500).total, 2, "a missing row is not listed");
}

#[test]
fn scan_errors_keep_to_the_scope() {
    let mut fx = Fx::empty();
    std::fs::create_dir_all(fx.path("in")).unwrap();
    std::fs::write(fx.path("in/a.png"), b"not a picture").unwrap();
    std::fs::write(fx.path("out.png"), b"not a picture").unwrap();
    fx.scan();
    let sub = fx.path("in");
    fx.reopen(&sub, &OpenOptions::default());
    let overlay = fx.overlay.lock().unwrap();
    let all = report::scan_errors(fx.album(), &overlay, 0, 500).unwrap();
    let paths: Vec<&str> = all.items.iter().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(paths, ["in/a.png"]);
}

#[test]
fn scan_errors_of_a_view_only_album_come_from_memory() {
    let mut fx = Fx::empty();
    std::fs::write(fx.path("a.png"), b"not a picture").unwrap();
    picture(&fx.path("b.png"), 1);
    fx.scan();
    let root = fx.root.clone();
    fx.reopen(
        &root,
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    );
    // In this session a breaks less and b breaks.
    picture(&fx.path("a.png"), 2);
    std::fs::write(fx.path("b.png"), b"broken now").unwrap();
    fx.scan();
    let overlay = fx.overlay.lock().unwrap();
    let all = report::scan_errors(fx.album(), &overlay, 0, 500).unwrap();
    let paths: Vec<&str> = all.items.iter().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(paths, ["b.png"]);
}

#[test]
fn the_scanner_reports_its_last_whole_walk() {
    let mut fx = Fx::empty();
    std::fs::create_dir_all(fx.path("sub/nested/.owlbum")).unwrap();
    std::fs::write(fx.path("sub/nested/.owlbum/owlbum.sqlite"), b"").unwrap();
    std::fs::write(fx.path("sub/a.heic"), b"x").unwrap();
    picture(&fx.path("sub/b.png"), 1);
    let sub = fx.path("sub");
    fx.reopen(&sub, &OpenOptions::default());
    let album = fx.take_album();
    let scanner = Scanner::new(Arc::clone(&album), fx.env.clone());
    assert_eq!(scanner.report(), report::ScanReport::default());
    scanner.scan_and_wait(Request::default());
    let report = scanner.report();
    assert_eq!(report.nested_album_count, 1);
    assert_eq!(scanner.unsupported(0, 500).total, 1);
    let status = scanner.status();
    assert_eq!((status.unsupported, status.skipped), (1, 0));

    // The scoped folder goes away (an unplugged drive): the last whole
    // walk's report stays.
    std::fs::remove_dir_all(&sub).unwrap();
    let failed = scanner.scan_and_wait(Request::default());
    assert!(failed.error.is_some());
    assert_eq!(scanner.report(), report);
    assert_eq!(scanner.unsupported(0, 500).total, 1);
    assert_eq!(scanner.status().unsupported, 1);
}

/// A reader that holds the pixels pass at its first decode until told to
/// go on.
struct Holding {
    entered: Mutex<std::sync::mpsc::Sender<()>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Reader for Holding {
    fn sample(&self, path: &Path) -> std::io::Result<Sample> {
        MediaReader.sample(path)
    }

    fn metadata(&self, format: &'static MediaFormat, path: &Path) -> Result<Metadata, MediaError> {
        MediaReader.metadata(format, path)
    }

    fn probe(
        &self,
        format: &'static MediaFormat,
        path: &Path,
    ) -> Result<Option<VideoInfo>, MediaError> {
        MediaReader.probe(format, path)
    }

    fn preview(&self, format: &'static MediaFormat, path: &Path) -> Result<Pixels, MediaError> {
        let _ = self.entered.lock().unwrap().send(());
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(30));
        MediaReader.preview(format, path)
    }
}

#[test]
fn the_report_answers_while_the_passes_still_run() {
    let mut fx = Fx::empty();
    std::fs::create_dir_all(fx.path("inner/.owlbum")).unwrap();
    std::fs::write(fx.path("inner/.owlbum/owlbum.sqlite"), b"").unwrap();
    picture(&fx.path("a.png"), 1);
    let (entered_tx, entered) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let mut env = fx.env.clone();
    env.reader = Arc::new(Holding {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(release_rx),
    });
    let album = fx.take_album();
    let scanner = Scanner::new(album, env);
    scanner.request(Request::default());
    entered
        .recv_timeout(Duration::from_secs(30))
        .expect("the pixels pass starts");
    assert!(scanner.is_running());
    assert_eq!(scanner.report().nested_album_count, 1);
    release.send(()).unwrap();
    scanner.wait();
}

#[test]
fn skipped_counts_links_online_only_files_and_unreadable_names() {
    let walk = walk::Walk {
        complete: true,
        links: 2,
        online_only: ["a.jpg".to_owned(), "b.jpg".to_owned()]
            .into_iter()
            .collect(),
        unreadable_names: 3,
        nested: vec!["n".into()],
        ..walk::Walk::default()
    };
    assert_eq!(walk.skipped(), 7);
}

// ---- Change detection ----

#[test]
fn a_second_scan_reads_no_file() {
    let fx = Fx::trip();
    let first = fx.scan();
    let before = fx.photos();
    fx.reads.take();

    let second = fx.scan();
    assert_eq!(fx.reads.take(), [], "nothing is read");
    assert_eq!(fx.photos(), before);
    assert_eq!(second.found, first.found);
    assert_eq!(second.done, second.found, "unchanged files are done");
    assert_eq!(
        second.failed, 0,
        "errors from earlier scans are not this scan's"
    );
    assert_eq!(second.batches, 0, "nothing is written");
}

#[test]
fn only_a_changed_file_is_read_again_and_keeps_its_id_and_typed_info() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    let a = fx.photo("a.png").unwrap();
    fx.star(a.id);
    fx.sql(
        "INSERT INTO tags (photo_id, part, model_id, status, made_at) VALUES (1, 'describe', 'm', 'done', 1);
         INSERT INTO vectors (photo_id, model_id, dim, vector) VALUES (1, 'm', 1, x'0000803f');
         INSERT INTO photo_labels VALUES ('keyword', 'sky', 1);",
    );
    assert_eq!(a.id, 1);
    fx.reads.take();
    fx.events.clear();

    picture(&fx.path("a.png"), 9);
    touch_later(&fx.path("a.png"));
    let summary = fx.scan();

    assert_eq!(fx.read("metadata"), ["a.png"]);
    assert_eq!(fx.read("preview"), ["a.png"]);
    let after = fx.photo("a.png").unwrap();
    assert_eq!(after.id, a.id);
    assert_eq!(after.star, 1, "typed info is kept");
    assert_ne!(after.file_key, a.file_key);
    assert_ne!(after.thumb_key, a.thumb_key);
    assert!(analysed(after.analysis.as_deref()));
    assert_eq!(summary.changed, 1);
    assert_eq!(summary.rekeyed, [a.id]);
    let left: i64 = fx
        .album()
        .db()
        .read(|c| {
            c.query_row(
                "SELECT (SELECT count(*) FROM tags) + (SELECT count(*) FROM vectors)
                      + (SELECT count(*) FROM photo_labels)",
                [],
                |r| r.get(0),
            )
            .map_err(sqlite_error(text::album::READING_DATABASE))
        })
        .unwrap();
    assert_eq!(left, 0, "tags of the old content are gone");
    let rekeyed: Vec<i64> = fx
        .events
        .changes()
        .into_iter()
        .flat_map(|(c, _)| c.rekeyed)
        .collect();
    assert_eq!(rekeyed, [a.id], "album_changed lists it");
}

#[test]
fn a_new_modified_time_alone_reads_the_file_again_but_keeps_its_tags() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    fx.scan();
    fx.sql("INSERT INTO photo_labels VALUES ('keyword', 'sky', 1);");
    fx.reads.take();
    touch_later(&fx.path("a.png"));
    let summary = fx.scan();
    assert_eq!(fx.read("metadata"), ["a.png"]);
    assert!(summary.rekeyed.is_empty(), "same content, same key");
    let labels: i64 = fx
        .album()
        .db()
        .read(|c| {
            c.query_row("SELECT count(*) FROM photo_labels", [], |r| r.get(0))
                .map_err(sqlite_error(text::album::READING_DATABASE))
        })
        .unwrap();
    assert_eq!(labels, 1);
}

#[test]
fn a_raised_scan_version_reads_every_row_again() {
    let mut fx = Fx::trip();
    fx.scan();
    let before = fx.photos();
    fx.reads.take();
    fx.env.scan_version = SCAN_VERSION + 1;
    let summary = fx.scan();
    let read = fx.read("sample");
    assert_eq!(
        read.len(),
        before.len(),
        "every row's file is read: {read:?}"
    );
    assert_eq!(summary.changed as usize, before.len());
    for p in fx.photos() {
        assert_eq!(p.scan_version, SCAN_VERSION + 1);
        let old = before.iter().find(|b| b.id == p.id).unwrap();
        assert_eq!(p.rel_path, old.rel_path, "same rows");
        assert_eq!(p.thumb_key, old.thumb_key, "same thumbnails");
    }
    assert!(summary.rekeyed.is_empty());
}

#[test]
fn a_waiting_row_goes_to_the_pixels_pass_only() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    fx.scan();
    fx.sql("UPDATE photos SET analysis = NULL");
    fx.reads.take();
    let summary = fx.scan();
    assert_eq!(fx.read("metadata"), Vec::<String>::new(), "no facts pass");
    assert_eq!(fx.read("preview"), ["a.png"]);
    assert!(analysed(fx.photo("a.png").unwrap().analysis.as_deref()));
    assert_eq!(summary.done, 1);
}

// ---- Identity: renames, moves, missing ----

#[test]
fn a_rename_and_a_move_keep_the_row_and_its_thumbnails() {
    let fx = Fx::trip();
    fx.scan();
    let lake = fx.photo("day1/IMG_0002.jpg").unwrap();
    let plain = fx.photo("day3/plain.png").unwrap();
    fx.star(lake.id);
    fx.reads.take();

    rename(&fx, "day1/IMG_0002.jpg", "day1/portrait.jpg");
    rename(&fx, "day3/plain.png", "elsewhere/deeper/plain.png");
    let summary = fx.scan();

    let renamed = fx.photo("day1/portrait.jpg").unwrap();
    assert_eq!(renamed.id, lake.id);
    assert_eq!(renamed.star, 1);
    assert_eq!(renamed.thumb_key, lake.thumb_key);
    assert_eq!(renamed.file_name, "portrait.jpg");
    let moved = fx.photo("elsewhere/deeper/plain.png").unwrap();
    assert_eq!(moved.id, plain.id);
    assert_eq!(moved.rel_dir, "elsewhere/deeper");
    assert_eq!(moved.thumb_key, plain.thumb_key);
    assert!(fx.photo("day1/IMG_0002.jpg").is_none());
    assert_eq!(summary.moved, 2);
    assert_eq!(summary.missing, 0);
    assert_eq!(summary.added, 0);
    assert_eq!(
        fx.read("metadata"),
        Vec::<String>::new(),
        "nothing else is read"
    );
    assert_eq!(fx.photos().len(), expected_rows(&fx.manifest).len());
}

#[test]
fn a_case_only_rename_keeps_the_row() {
    let fx = Fx::empty();
    picture(&fx.path("Lake.png"), 1);
    fx.scan();
    let before = fx.photo("Lake.png").unwrap();
    // Through a temporary name, which works whether or not the file system
    // ignores case.
    rename(&fx, "Lake.png", "tmp.png");
    rename(&fx, "tmp.png", "lake.png");
    fx.scan();
    let after = fx.photo("lake.png").unwrap();
    assert_eq!(after.id, before.id);
    assert!(fx.photo("Lake.png").is_none());
}

#[test]
fn a_candidate_with_the_same_name_is_preferred_then_path_order() {
    let fx = Fx::empty();
    picture(&fx.path("a/one.png"), 7);
    std::fs::create_dir_all(fx.path("b")).unwrap();
    std::fs::copy(fx.path("a/one.png"), fx.path("b/two.png")).unwrap();
    fx.scan();
    let one = fx.photo("a/one.png").unwrap();
    let two = fx.photo("b/two.png").unwrap();
    assert_eq!(one.file_key, two.file_key);

    rename(&fx, "b/two.png", "c/two.png");
    std::fs::remove_file(fx.path("a/one.png")).unwrap();
    fx.scan();
    assert_eq!(fx.photo("c/two.png").unwrap().id, two.id, "same name wins");
    assert!(fx.by_id(one.id).missing);
}

#[test]
fn two_new_files_with_one_candidates_key_the_first_in_walker_order_takes_it() {
    let fx = Fx::empty();
    picture(&fx.path("x/photo.png"), 3);
    fx.scan();
    let old = fx.photo("x/photo.png").unwrap();
    std::fs::create_dir_all(fx.path("b")).unwrap();
    std::fs::create_dir_all(fx.path("a")).unwrap();
    std::fs::copy(fx.path("x/photo.png"), fx.path("b/photo.png")).unwrap();
    std::fs::copy(fx.path("x/photo.png"), fx.path("a/photo.png")).unwrap();
    std::fs::remove_file(fx.path("x/photo.png")).unwrap();
    let summary = fx.scan();
    assert_eq!(fx.photo("a/photo.png").unwrap().id, old.id);
    assert!(fx.photo("b/photo.png").unwrap().id > old.id, "a new row");
    assert_eq!((summary.moved, summary.added), (1, 1));
}

#[test]
fn a_deleted_file_becomes_missing_and_putting_it_back_clears_it() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    let a = fx.photo("a.png").unwrap();
    fx.star(a.id);
    let saved = std::fs::read(fx.path("a.png")).unwrap();
    std::fs::remove_file(fx.path("a.png")).unwrap();
    let summary = fx.scan();
    assert_eq!(summary.missing, 1);
    let gone = fx.by_id(a.id);
    assert!(gone.missing);
    assert_eq!(gone.star, 1, "a missing row keeps its typed info");

    std::fs::write(fx.path("a.png"), &saved).unwrap();
    set_mtime(
        &fx.path("a.png"),
        SystemTime::UNIX_EPOCH + Duration::from_millis(gone.mtime_ms.unsigned_abs()),
    );
    fx.reads.take();
    fx.scan();
    let back = fx.by_id(a.id);
    assert!(!back.missing);
    assert_eq!(back.star, 1);
    assert_eq!(
        fx.read("metadata"),
        Vec::<String>::new(),
        "unchanged: nothing read"
    );
}

#[test]
fn a_delete_seen_by_one_scan_and_the_create_by_the_next_keep_the_row() {
    let fx = Fx::empty();
    picture(&fx.path("a/photo.png"), 4);
    fx.scan();
    let old = fx.photo("a/photo.png").unwrap();
    let saved = std::fs::read(fx.path("a/photo.png")).unwrap();
    std::fs::remove_file(fx.path("a/photo.png")).unwrap();
    fx.scan();
    assert!(fx.by_id(old.id).missing);

    std::fs::create_dir_all(fx.path("b")).unwrap();
    std::fs::write(fx.path("b/photo.png"), saved).unwrap();
    fx.scan();
    let back = fx.photo("b/photo.png").unwrap();
    assert_eq!(back.id, old.id);
    assert!(!back.missing);
    assert_eq!(fx.photos().len(), 1);
}

#[test]
fn nothing_is_marked_missing_after_a_cancelled_scan() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    fx.scan();
    std::fs::remove_file(fx.path("a.png")).unwrap();
    fx.events.clear();
    fx.cancel.store(true, Ordering::SeqCst);
    let summary = fx.run(Request::default());
    assert!(summary.cancelled);
    assert!(!fx.photo("a.png").unwrap().missing);
    let (last, _) = *fx.events.progress().last().unwrap();
    assert!(last.finished && last.cancelled, "{last:?}");
    assert_eq!(last.phase, ScanPhase::Idle);
}

#[test]
fn nothing_is_marked_missing_when_the_scope_cannot_be_read() {
    let mut fx = Fx::empty();
    picture(&fx.path("sub/a.png"), 1);
    let sub = fx.path("sub");
    fx.reopen(&sub, &OpenOptions::default());
    fx.scan();
    assert_eq!(fx.photos().len(), 1);
    // The scoped folder disappears (an unplugged drive looks the same).
    std::fs::remove_dir_all(&sub).unwrap();
    let summary = fx.scan();
    assert_eq!(summary.error.unwrap().code, ErrorCode::NotAFolder);
    assert!(!fx.photo("sub/a.png").unwrap().missing);
    let (last, _) = *fx.events.progress().last().unwrap();
    assert!(last.finished && !last.cancelled);
}

#[cfg(unix)]
#[test]
fn nothing_is_marked_missing_under_an_unreadable_subfolder() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fx::empty();
    picture(&fx.path("locked/a.png"), 1);
    picture(&fx.path("open/b.png"), 2);
    fx.scan();
    let locked = fx.path("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_ok() {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: permissions do not apply to this user");
        return;
    }
    let summary = fx.scan();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(summary.walk.unreadable.len(), 1);
    assert!(!fx.photo("locked/a.png").unwrap().missing);
    assert_eq!(summary.missing, 0);
}

// ---- Errors per file ----

#[test]
fn a_corrupt_file_is_retried_on_rescan_but_not_on_an_automatic_scan() {
    let fx = Fx::empty();
    std::fs::write(fx.path("bad.png"), b"not a picture at all").unwrap();
    picture(&fx.path("good.png"), 1);
    let first = fx.scan();
    let bad = fx.photo("bad.png").unwrap();
    assert_eq!(code(&bad), Some("decode"));
    assert_eq!(
        bad.scan_error.as_deref(),
        Some("decode: This file is damaged or is not a PNG picture.")
    );
    assert_eq!((bad.kind.as_str(), bad.width, bad.height), ("image", 0, 0));
    assert_eq!(first.failed, 1);
    fx.reads.take();

    let auto = fx.scan();
    assert_eq!(fx.reads.take(), [], "an automatic scan leaves it");
    assert_eq!(auto.failed, 0);

    let manual = fx.rescan();
    assert_eq!(fx.read("sample"), ["bad.png"], "a Rescan tries it again");
    assert_eq!(manual.failed, 1);
    assert_eq!(fx.photo("bad.png").unwrap().id, bad.id);
}

#[test]
fn an_io_error_is_retried_by_every_scan() {
    let fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    fx.scan();
    fx.sql(
        "UPDATE photos SET scan_error = 'io: This file could not be read: busy', analysis = NULL",
    );
    fx.reads.take();
    fx.scan();
    assert_eq!(fx.read("sample"), ["a.png"]);
    let p = fx.photo("a.png").unwrap();
    assert_eq!(p.scan_error, None);
    assert!(analysed(p.analysis.as_deref()));
}

#[test]
fn a_file_still_being_written_is_retried_not_recorded() {
    let mut fx = Fx::empty();
    fx.env.busy_recent = Duration::from_secs(60);
    fx.env.busy_wait = Duration::from_millis(150);
    // Half a file: a decoder refuses it.
    picture(&fx.path("good.png"), 5);
    let whole = std::fs::read(fx.path("good.png")).unwrap();
    std::fs::remove_file(fx.path("good.png")).unwrap();
    std::fs::write(fx.path("a.png"), &whole[..whole.len() / 3]).unwrap();
    let path = fx.path("a.png");
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(path, whole).unwrap();
    });
    let summary = fx.scan();
    writer.join().unwrap();
    let p = fx.photo("a.png").unwrap();
    assert_eq!(p.scan_error, None, "{p:?}");
    assert_eq!((p.width, p.height), (40, 30));
    assert_eq!(summary.failed, 0);
}

#[test]
fn a_file_that_stays_broken_is_recorded_after_the_retries() {
    let mut fx = Fx::empty();
    fx.env.busy_recent = Duration::from_secs(60);
    fx.env.busy_wait = Duration::from_millis(10);
    std::fs::write(fx.path("bad.png"), b"never a picture").unwrap();
    let summary = fx.scan();
    assert_eq!(code(&fx.photo("bad.png").unwrap()), Some("decode"));
    assert_eq!(summary.failed, 1);
    let tries = fx
        .reads
        .take()
        .iter()
        .filter(|(_, w)| *w == "sample")
        .count();
    assert_eq!(tries, 1 + fx.env.busy_tries as usize);
}

// ---- Running ----

#[test]
fn writes_arrive_in_transactions_of_at_most_200_rows() {
    let fx = Fx::large(450);
    let summary = fx.scan();
    assert_eq!(fx.photos().len(), 450);
    assert!(summary.largest_batch <= 200, "{}", summary.largest_batch);
    assert!(summary.batches >= 3, "{}", summary.batches);
}

#[test]
fn a_typed_change_during_a_scan_commits_within_one_batch() {
    let mut fx = Fx::large(600);
    let album = fx.take_album();
    let scanner = Scanner::new(Arc::clone(&album), fx.env.clone());
    scanner.request(Request::default());
    // Wait for the first rows.
    let start = Instant::now();
    let id = loop {
        let id: Option<i64> = album
            .db()
            .read(|c| {
                c.query_row("SELECT max(id) FROM photos", [], |r| r.get(0))
                    .map_err(sqlite_error(text::album::READING_DATABASE))
            })
            .unwrap();
        if let Some(id) = id {
            break id;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "no rows arrived");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(scanner.is_running(), "the scan is still running");
    let t = Instant::now();
    album
        .db()
        .write(move |tx| {
            tx.execute("UPDATE photos SET star = 1 WHERE id = ?1", [id])
                .map_err(sqlite_error(text::album::WRITING_DATABASE))
        })
        .unwrap();
    let took = t.elapsed();
    scanner.wait();
    assert!(took < Duration::from_secs(1), "the star waited {took:?}");
}

#[test]
fn a_cancelled_scan_leaves_waiting_rows_the_next_scan_finishes_without_the_facts_pass() {
    let fx = Fx::empty();
    for i in 0..6 {
        picture(&fx.path(&format!("{i}.png")), i);
    }
    *fx.reads.cancel_at_preview.lock().unwrap() = Some(Arc::clone(&fx.cancel));
    let first = fx.run(Request::default());
    assert!(first.cancelled);
    let waiting = fx.photos().iter().filter(|p| p.analysis.is_none()).count();
    assert!(waiting > 0, "rows were left waiting");
    assert_eq!(fx.photos().len(), 6, "the facts pass had written every row");
    assert!(fx.photos().iter().all(|p| !p.missing));

    *fx.reads.cancel_at_preview.lock().unwrap() = None;
    fx.reads.take();
    let second = fx.scan();
    assert_eq!(
        fx.read("metadata"),
        Vec::<String>::new(),
        "no facts pass again"
    );
    assert_eq!(fx.read("preview").len(), waiting);
    assert!(fx.photos().iter().all(|p| p.analysis.is_some()));
    assert_eq!(second.done, 6);
}

#[test]
fn a_scoped_scan_touches_no_row_outside_the_scope() {
    let mut fx = Fx::empty();
    picture(&fx.path("in/a.png"), 1);
    picture(&fx.path("out/b.png"), 2);
    picture(&fx.path("in-other/c.png"), 3);
    fx.scan();
    let before = fx.photos();
    std::fs::remove_file(fx.path("out/b.png")).unwrap();
    std::fs::remove_file(fx.path("in-other/c.png")).unwrap();
    picture(&fx.path("in/new.png"), 4);
    let scope = fx.path("in");
    fx.reopen(&scope, &OpenOptions::default());
    fx.reads.take();
    let summary = fx.scan();
    assert_eq!(summary.found, 2);
    assert_eq!(fx.read("sample"), ["in/new.png"]);
    for p in before.iter().filter(|p| !p.rel_path.starts_with("in/")) {
        assert_eq!(&fx.by_id(p.id), p, "untouched");
    }
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                out.push((e.path(), std::fs::read(e.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_view_only_album_scans_into_memory_and_writes_nothing_in_the_album() {
    let mut fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    let db_rows = fx.photos();
    picture(&fx.path("c.png"), 3);
    std::fs::remove_file(fx.path("b.png")).unwrap();
    rename(&fx, "a.png", "moved/a.png");
    let root = fx.root.clone();
    fx.reopen(
        &root,
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    );
    assert!(fx.album().is_view_only());
    let before = snapshot(&root);

    let summary = fx.scan();
    assert!(summary.error.is_none(), "{:?}", summary.error);
    assert_eq!(
        snapshot(&root),
        before,
        "nothing written in the album folder"
    );
    assert_eq!(fx.photos(), db_rows, "the database is unchanged");

    let overlay = fx.overlay.lock().unwrap();
    let max_db = db_rows.iter().map(|p| p.id).max().unwrap();
    let c = overlay.rows().find(|r| r.rel_path == "c.png").unwrap();
    assert!(c.id > max_db, "a new file gets an id above the database's");
    assert!(analysed(c.facts.analysis.as_deref()));
    let a = db_rows.iter().find(|p| p.rel_path == "a.png").unwrap();
    assert_eq!(overlay.get(a.id).unwrap().rel_path, "moved/a.png");
    let b = db_rows.iter().find(|p| p.rel_path == "b.png").unwrap();
    assert!(overlay.get(b.id).unwrap().missing, "hidden in memory");
    drop(overlay);

    // A second scan in the session finds what memory holds unchanged.
    fx.reads.take();
    let again = fx.scan();
    assert_eq!(fx.reads.take(), []);
    assert_eq!(again.done, 2);
}

#[test]
fn an_album_with_no_database_scans_into_memory() {
    let mut fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    let root = fx.root.clone();
    // A folder that cannot be written and has no album yet.
    fx.album = None;
    std::fs::remove_dir_all(root.join(".owlbum")).unwrap();
    let env = Env {
        pretend_read_only: true,
        ..Env::real()
    };
    fx.reopen_with(&root, &OpenOptions::default(), &env);
    assert!(fx.album().safety().in_memory);
    let summary = fx.scan();
    assert!(summary.error.is_none(), "{:?}", summary.error);
    let overlay = fx.overlay.lock().unwrap();
    assert_eq!(overlay.rows().count(), 1);
    assert_eq!(overlay.rows().next().unwrap().id, 1);
}

#[test]
fn scan_progress_is_at_most_ten_a_second_and_ends_finished() {
    let fx = Fx::large(300);
    let summary = fx.scan();
    let events = fx.events.progress();
    let (last, _) = *events.last().unwrap();
    assert!(last.finished && !last.cancelled);
    assert_eq!(last.phase, ScanPhase::Idle);
    assert_eq!((last.found, last.done, last.failed), (300, 300, 0));
    assert_eq!(summary.done, 300);
    // Every event but the last is at least ~100 ms after the one before.
    let times: Vec<Instant> = events[..events.len() - 1].iter().map(|(_, t)| *t).collect();
    for w in times.windows(2) {
        assert!(
            w[1] - w[0] >= Duration::from_millis(99),
            "{:?}",
            w[1] - w[0]
        );
    }
    // Walking comes before reading, and no event but the last is finished.
    let phases: Vec<ScanPhase> = events.iter().map(|(p, _)| p.phase).collect();
    let first_reading = phases.iter().position(|p| *p == ScanPhase::Reading);
    if let Some(i) = first_reading {
        assert!(phases[..i].iter().all(|p| *p == ScanPhase::Walking));
    }
    assert!(events[..events.len() - 1].iter().all(|(p, _)| !p.finished));
}

#[test]
fn album_changed_is_sent_at_most_once_a_second_with_the_revision() {
    let fx = Fx::large(300);
    fx.scan();
    let changes = fx.events.changes();
    assert!(!changes.is_empty());
    let revisions: Vec<u64> = changes.iter().map(|(c, _)| c.revision).collect();
    let expected: Vec<u64> = (1..=revisions.len() as u64).collect();
    assert_eq!(revisions, expected);
    assert_eq!(fx.album().revision(), *revisions.last().unwrap());
    for (c, _) in &changes {
        assert_eq!(c.reason, crate::album::ChangeReason::Scan);
    }
    let times: Vec<Instant> = changes.iter().map(|(_, t)| *t).collect();
    for w in times[..times.len().saturating_sub(1)].windows(2) {
        assert!(w[1] - w[0] >= Duration::from_millis(990));
    }
    // An unchanged second scan writes nothing and says nothing.
    fx.events.clear();
    fx.scan();
    assert!(fx.events.changes().is_empty());
}

// ---- The scanner ----

/// Waits until the scanner's scan has started (or already ended).
fn wait_until_scanning(scanner: &Scanner) {
    let start = Instant::now();
    while scanner.status().phase == ScanPhase::Idle && scanner.last().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the scan never started"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_scanner_runs_one_scan_at_a_time_and_queues_one_more() {
    let mut fx = Fx::large(200);
    let album = fx.take_album();
    let scanner = Scanner::new(Arc::clone(&album), fx.env.clone());
    scanner.request(Request::default());
    wait_until_scanning(&scanner);
    scanner.request(Request::default());
    scanner.request(Request::manual());
    scanner.wait();
    let finished = fx
        .events
        .progress()
        .iter()
        .filter(|(p, _)| p.finished)
        .count();
    assert_eq!(finished, 2, "the running scan and one more");
    assert!(!scanner.is_running());
    assert_eq!(scanner.status().phase, ScanPhase::Idle);
    assert_eq!(scanner.status().found, 200);
}

#[test]
fn cancelling_stops_the_scan_and_drops_the_queued_one() {
    let mut fx = Fx::large(400);
    let album = fx.take_album();
    let scanner = Scanner::new(Arc::clone(&album), fx.env.clone());
    scanner.request(Request::default());
    wait_until_scanning(&scanner);
    scanner.request(Request::default());
    scanner.cancel();
    scanner.wait();
    let ends: Vec<ScanProgress> = fx
        .events
        .progress()
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| p.finished)
        .collect();
    assert_eq!(ends.len(), 1, "{ends:?}");
    // A scan cancelled this early may still have finished; when it did not,
    // it says so.
    let last = scanner.last().unwrap();
    assert_eq!(ends[0].cancelled, last.cancelled);
    // Cancelling with nothing running is fine.
    scanner.cancel();
}

#[test]
fn dropping_the_scanner_cancels_and_waits_for_its_scan() {
    let mut fx = Fx::large(300);
    let album = fx.take_album();
    let scanner = Scanner::new(Arc::clone(&album), fx.env.clone());
    scanner.request(Request::default());
    wait_until_scanning(&scanner);
    drop(scanner);
    let (last, _) = *fx.events.progress().last().unwrap();
    assert!(last.finished, "the scan ended before drop returned");
    assert_eq!(
        Arc::strong_count(&album),
        1,
        "no scan thread holds the album"
    );
}

#[test]
fn walker_order_is_natural_and_used_for_rows() {
    let fx = Fx::empty();
    for name in ["IMG_10.png", "IMG_2.png", "IMG_1.png"] {
        picture(&fx.path(name), 1);
    }
    let summary = fx.scan();
    let order: Vec<&str> = summary
        .walk
        .files
        .iter()
        .map(|f| f.rel_path.as_str())
        .collect();
    assert_eq!(order, ["IMG_1.png", "IMG_2.png", "IMG_10.png"]);
}

/// Timings for the issue, not a gate: `cargo test -p owlbum-core --release
/// scan_timings -- --ignored --nocapture`.
#[test]
#[ignore]
fn scan_timings() {
    for n in [2_000, 10_000] {
        for thumbnails in [false, true] {
            let mut fx = Fx::large(n);
            let data = tempfile::tempdir().unwrap();
            if thumbnails {
                let app = crate::appdata::AppData::open(data.path()).unwrap();
                fx.env.thumbs = Some(crate::thumbs::Thumbs::open(app).unwrap());
            }
            let t = Instant::now();
            let first = fx.scan();
            let first_took = t.elapsed();
            let t = Instant::now();
            let again = fx.scan();
            let again_took = t.elapsed();
            println!(
                "{n} files, thumbnails {thumbnails}: first scan {first_took:?} ({} done, {} batches), second scan {again_took:?} ({} done)",
                first.done, first.batches, again.done
            );
        }
    }
}

// ---- The album index follows the scan (#12) ----

/// What the album index holds, in a form that does not depend on where
/// names sit in memory.
fn index_rows(index: &crate::index::AlbumIndex) -> Vec<String> {
    index.read(|v| {
        v.photos()
            .iter()
            .map(|p| {
                format!(
                    "{} {}/{} rank={} date={:?} star={} {}x{} analysed={}",
                    p.id,
                    v.folder(p.folder),
                    v.file_name(p),
                    p.name_rank,
                    p.date(),
                    p.star(),
                    p.width,
                    p.height,
                    p.analysis().is_some()
                )
            })
            .collect()
    })
}

#[test]
fn the_album_index_follows_a_scan() {
    let fx = Fx::empty();
    picture(&fx.path("b.png"), 1);
    picture(&fx.path("sub/a.png"), 2);
    std::fs::write(fx.path("broken.jpg"), b"not a picture").unwrap();
    fx.scan();
    let fresh = || {
        let scope = fx.album().location().scope.clone();
        fx.album()
            .db()
            .read(|c| crate::index::AlbumIndex::load(c, &scope))
            .unwrap()
    };
    let live = index_rows(fx.album().index());
    assert_eq!(
        live.len(),
        2,
        "the scan error row is not indexed: {live:#?}"
    );
    assert_eq!(live, index_rows(&fresh()));
    // The analysis is `{}` until #10 measures it, so nothing is analysed
    // yet; the rows still follow.

    std::fs::remove_file(fx.path("b.png")).unwrap();
    picture(&fx.path("c.png"), 3);
    fx.scan();
    let live = index_rows(fx.album().index());
    assert_eq!(live, index_rows(&fresh()));
    assert!(live.iter().all(|r| !r.contains("/b.png")), "{live:#?}");
    assert!(live.iter().any(|r| r.contains(" /c.png")), "{live:#?}");
}

#[test]
fn the_album_index_follows_a_view_only_scan_in_memory() {
    let mut fx = Fx::empty();
    picture(&fx.path("a.png"), 1);
    picture(&fx.path("b.png"), 2);
    fx.scan();
    let a = fx.photo("a.png").unwrap();
    let b = fx.photo("b.png").unwrap();
    fx.star(a.id);
    picture(&fx.path("c.png"), 3);
    std::fs::remove_file(fx.path("b.png")).unwrap();
    rename(&fx, "a.png", "moved/a.png");
    let root = fx.root.clone();
    fx.reopen(
        &root,
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    );
    fx.scan();
    let c = fx
        .overlay
        .lock()
        .unwrap()
        .rows()
        .find(|r| r.rel_path == "c.png")
        .unwrap()
        .id;
    let live = index_rows(fx.album().index());
    assert_eq!(live.len(), 2, "{live:#?}");
    let a_row = live
        .iter()
        .find(|r| r.starts_with(&format!("{} ", a.id)))
        .unwrap();
    assert!(a_row.contains(" moved/a.png "), "{a_row}");
    assert!(
        a_row.contains("star=true"),
        "typed info comes from the database: {a_row}"
    );
    assert!(
        live.iter().all(|r| !r.starts_with(&format!("{} ", b.id))),
        "b is missing"
    );
    assert!(
        live.iter().any(|r| r.starts_with(&format!("{c} /c.png"))),
        "{live:#?}"
    );
}

// ---- Metadata and analysis (#10) ----

/// `captured_at`, `gps_lat`, `gps_lon`, `camera` and `exif` of one row.
type MetaRow = (
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<String>,
    Option<String>,
);

/// The metadata facts of one row.
fn meta_facts(fx: &Fx, rel: &str) -> MetaRow {
    let rel = rel.to_owned();
    fx.album()
        .db()
        .read(move |c| {
            Ok(c.query_row(
                "SELECT captured_at, gps_lat, gps_lon, camera, exif FROM photos WHERE rel_path = ?1",
                [rel],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap())
        })
        .unwrap()
}

fn analysis_of(fx: &Fx, rel: &str) -> Value {
    serde_json::from_str(fx.photo(rel).unwrap().analysis.as_deref().unwrap()).unwrap()
}

#[test]
fn the_facts_pass_reads_date_place_camera_and_exif() {
    let fx = Fx::trip();
    fx.scan();
    for f in expected_rows(&fx.manifest) {
        let p = fx.photo(&f.rel_path).unwrap();
        if p.scan_error.is_some() {
            // Error rows keep every metadata fact NULL.
            assert_eq!(meta_facts(&fx, &f.rel_path), (None, None, None, None, None));
            continue;
        }
        let (taken, lat, lon, camera, exif) = meta_facts(&fx, &f.rel_path);
        assert_eq!(taken, f.taken, "{}", f.rel_path);
        match f.gps {
            Some((want_lat, want_lon)) => {
                let (lat, lon) = (lat.unwrap(), lon.unwrap());
                assert!((lat - want_lat).abs() < 1e-5, "{}: {lat}", f.rel_path);
                assert!((lon - want_lon).abs() < 1e-5, "{}: {lon}", f.rel_path);
            }
            None => assert_eq!((lat, lon), (None, None), "{}", f.rel_path),
        }
        // Every trip photo with a date was written by one of its two
        // cameras; the others have no metadata at all.
        if f.taken.is_some() {
            let camera = camera.unwrap();
            assert!(
                camera == "Owlbum Fixture One" || camera == "Owlbum Pocket 2",
                "{}: {camera}",
                f.rel_path
            );
            let exif: Value = serde_json::from_str(&exif.unwrap()).unwrap();
            assert_eq!(exif["iso"], 100, "{}", f.rel_path);
            assert_eq!(exif["exposure_time"], 0.004, "{}", f.rel_path);
            assert_eq!(exif["flash"], false, "{}", f.rel_path);
        } else {
            assert_eq!((camera, exif), (None, None), "{}", f.rel_path);
        }
    }
    let (_, _, _, camera, exif) = meta_facts(&fx, "day1/lake.jpg");
    assert_eq!(camera.as_deref(), Some("Owlbum Fixture One"));
    assert_eq!(
        exif.as_deref(),
        Some(
            r#"{"lens":"Fixture 24-70mm F2.8","focal_length":35.0,"aperture":5.6,"exposure_time":0.004,"iso":100,"flash":false}"#
        )
    );
}

#[test]
fn animated_pictures_store_their_frame_count() {
    let fx = Fx::extras();
    fx.scan();
    let animated: Vec<_> = fx.manifest.of_kind(Kind::Animated).collect();
    assert_eq!(animated.len(), 3);
    for f in animated {
        let (.., exif) = meta_facts(&fx, &f.rel_path);
        let exif: Value = serde_json::from_str(exif.as_deref().unwrap()).unwrap();
        assert_eq!(exif["frames"], 3, "{}", f.rel_path);
    }
}

#[test]
fn the_pixels_pass_stores_the_analysis() {
    let fx = Fx::trip();
    fx.scan();
    let blurred = analysis_of(&fx, "day2/blurred.jpg")["sharpness"]
        .as_f64()
        .unwrap();
    for f in fx.manifest.files.iter().filter(|f| {
        // Photos painted by the same scene generator as the blurred one.
        matches!(f.kind, Kind::Burst | Kind::Portrait | Kind::Square)
    }) {
        let s = analysis_of(&fx, &f.rel_path)["sharpness"].as_f64().unwrap();
        assert!(blurred < s, "{} {s} vs blurred {blurred}", f.rel_path);
    }
    // An exact copy analyses the same.
    assert_eq!(
        analysis_of(&fx, "copies/lake.jpg"),
        analysis_of(&fx, "day1/lake.jpg")
    );
    // The burst's shots are near-duplicates by the 9 x 8 dHash (docs/05,
    // "Bursts": at most 12 bits apart).
    let hash = |rel: &str| {
        u64::from_str_radix(analysis_of(&fx, rel)["dhash"].as_str().unwrap(), 16).unwrap()
    };
    let burst: Vec<u64> = fx
        .manifest
        .of_kind(Kind::Burst)
        .map(|f| hash(&f.rel_path))
        .collect();
    assert_eq!(burst.len(), 5);
    for a in &burst {
        for b in &burst {
            assert!((a ^ b).count_ones() <= 12, "{a:016x} {b:016x}");
        }
    }
    // The transparent PNG of the extras is transparent; an opaque one is not.
    let fx = Fx::extras();
    fx.scan();
    let t = fx.manifest.of_kind(Kind::Transparent).next().unwrap();
    assert_eq!(analysis_of(&fx, &t.rel_path)["transparent"], true);
    let o = fx.manifest.of_kind(Kind::Orientation).next().unwrap();
    assert_eq!(analysis_of(&fx, &o.rel_path)["transparent"], false);
}

#[test]
fn the_same_file_gives_byte_identical_analysis_twice() {
    let fx = Fx::trip();
    fx.scan();
    let first = fx.photo("day1/lake.jpg").unwrap().analysis;
    fx.sql("UPDATE photos SET analysis = NULL WHERE rel_path = 'day1/lake.jpg'");
    fx.scan();
    assert_eq!(fx.photo("day1/lake.jpg").unwrap().analysis, first);
}

/// True when `analysis` holds the measures (not NULL and not `{}`).
fn analysed(analysis: Option<&str>) -> bool {
    analysis
        .and_then(|a| serde_json::from_str::<Value>(a).ok())
        .is_some_and(|v| v.get("dhash").is_some())
}
