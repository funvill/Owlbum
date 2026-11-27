//! The `owlbum://` protocol handler as a plain function, and the cache
//! commands, through Tauri's mock runtime.

mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::scan::ScanProgress;
use owlbum_core::thumbs::{CacheCleared, CacheUsage};
use owlbum_lib::protocol::respond;
use owlbum_lib::session::Session;
use serde_json::json;
use tauri::http::{Request, header};
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

struct Fixture {
    fixture: owlbum_fixtures::Album,
    app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    /// Last, so it is dropped last: fields drop in order, and on Windows
    /// the folder cannot be deleted while a database in it is still open.
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = owlbum_fixtures::trip(&tmp.path().join("trip")).unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data,
    ));
    Fixture {
        _tmp: tmp,
        fixture,
        app,
        window,
    }
}

impl Fixture {
    /// Opens the trip album and waits for its first scan.
    fn open(&self) {
        let (send, receive) = mpsc::channel();
        self.app.listen_any("scan_progress", move |event| {
            let p: ScanProgress = serde_json::from_str(event.payload()).unwrap();
            let _ = send.send(p);
        });
        let album = Album::open(&self.fixture.root, &OpenOptions::default()).unwrap();
        owlbum_lib::session::open_album(self.app.handle(), album);
        loop {
            let p = receive
                .recv_timeout(Duration::from_secs(60))
                .expect("the scan finishes");
            if p.finished {
                break;
            }
        }
    }

    /// The first picture's id and thumbnail key.
    fn first_jpeg(&self) -> (i64, String) {
        let scanner = self.app.state::<Session>().scanner().unwrap();
        scanner
            .album()
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id, thumb_key FROM photos
                     WHERE format = 'jpeg' AND scan_error IS NULL ORDER BY id LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap())
            })
            .unwrap()
    }

    fn get(&self, url: &str) -> tauri::http::Response<Vec<u8>> {
        let request = Request::builder().uri(url).body(Vec::<u8>::new()).unwrap();
        respond(self.app.handle(), &request)
    }

    fn get_range(&self, url: &str, range: &str) -> tauri::http::Response<Vec<u8>> {
        let request = Request::builder()
            .uri(url)
            .header(header::RANGE, range)
            .body(Vec::<u8>::new())
            .unwrap();
        respond(self.app.handle(), &request)
    }
}

impl Drop for Fixture {
    /// Closes the album before the folder goes, so no temporary files are
    /// left behind.
    fn drop(&mut self) {
        let _ = invoke(&self.window, "album_close", json!({}));
    }
}

#[test]
fn a_video_is_served_in_byte_ranges() {
    let f = fixture();
    f.open();
    let scanner = f.app.state::<Session>().scanner().unwrap();
    let id: i64 = scanner
        .album()
        .db()
        .read(|c| {
            Ok(
                c.query_row("SELECT id FROM photos WHERE kind = 'video'", [], |r| {
                    r.get(0)
                })
                .unwrap(),
            )
        })
        .unwrap();
    drop(scanner);
    let bytes = std::fs::read(f.fixture.root.join("video/clip.mp4")).unwrap();
    let len = bytes.len();

    let whole = f.get(&format!("http://owlbum.localhost/video/{id}"));
    assert_eq!(whole.status(), 206);
    assert_eq!(whole.headers()[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(whole.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(whole.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        whole.headers()[header::CONTENT_RANGE],
        format!("bytes 0-{}/{len}", len - 1).as_str()
    );
    assert_eq!(whole.body(), &bytes);

    let part = f.get_range(&format!("owlbum://video/{id}"), "bytes=10-19");
    assert_eq!(part.status(), 206);
    assert_eq!(part.body(), &bytes[10..20]);
    assert_eq!(
        part.headers()[header::CONTENT_RANGE],
        format!("bytes 10-19/{len}").as_str()
    );

    let past = f.get_range(&format!("owlbum://video/{id}"), &format!("bytes={len}-"));
    assert_eq!(past.status(), 416);
    assert_eq!(
        past.headers()[header::CONTENT_RANGE],
        format!("bytes */{len}").as_str()
    );
}

#[test]
fn without_an_open_album_every_url_is_not_found() {
    let f = fixture();
    let key = "0".repeat(40);
    let r = f.get(&format!("owlbum://thumb/{key}/s"));
    assert_eq!(r.status(), 404);
    assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
}

#[test]
fn a_thumbnail_is_served_in_both_url_forms() {
    let f = fixture();
    f.open();
    let (id, key) = f.first_jpeg();
    for url in [
        format!("owlbum://thumb/{key}/s"),
        format!("owlbum://localhost/thumb/{key}/s"),
        format!("http://owlbum.localhost/thumb/{key}/s"),
    ] {
        let r = f.get(&url);
        assert_eq!(r.status(), 200, "{url}");
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            r.headers()[header::CACHE_CONTROL],
            "private, max-age=31536000, immutable"
        );
        assert_eq!(&r.body()[..2], [0xFF, 0xD8]);
    }
    let full = f.get(&format!("http://owlbum.localhost/full/{id}?v={key}"));
    assert_eq!(full.status(), 200);
    assert_eq!(
        full.headers()[header::CACHE_CONTROL],
        "private, max-age=3600"
    );
    assert_eq!(f.get(&format!("owlbum://original/{id}")).status(), 404);
    assert_eq!(f.get(&format!("owlbum://video/{id}")).status(), 404);

    // Closed: nothing is served.
    f.app.state::<Session>().close();
    assert_eq!(f.get(&format!("owlbum://thumb/{key}/s")).status(), 404);
}

#[test]
fn cache_usage_and_clear() {
    let f = fixture();
    f.open();
    let usage: CacheUsage = invoke(&f.window, "app_cache_usage", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert!(usage.bytes > 0, "the scan made thumbnails");
    assert_eq!(usage.limit_bytes, 20_000_000_000);

    let cleared: CacheCleared = invoke(&f.window, "app_cache_clear", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(cleared.bytes_freed, usage.bytes);
    let after: CacheUsage = invoke(&f.window, "app_cache_usage", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(after.bytes, 0);

    // A thumbnail asked for after the clear is made again.
    let (_, key) = f.first_jpeg();
    assert_eq!(f.get(&format!("owlbum://thumb/{key}/m")).status(), 200);

    invoke(
        &f.window,
        "app_settings_set",
        json!({ "patch": { "cache.limit_gb": 3 } }),
    )
    .unwrap();
    let limited: CacheUsage = invoke(&f.window, "app_cache_usage", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(limited.limit_bytes, 3_000_000_000);
    f.app.state::<Session>().close();
}
