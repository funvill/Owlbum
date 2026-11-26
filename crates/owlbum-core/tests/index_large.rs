//! The album index at size (docs/05-organising.md, "Performance targets":
//! loading the album index on open, under 2 s for 100,000 photos; about 150
//! bytes a photo).
//!
//! Rows are inserted straight into a database file shaped like the
//! `large-N` fixture (ten years of `YYYY/MM` folders, full analysis, some
//! GPS, some people) because the scanner that would fill it from the
//! fixture's files is a separate issue. Timings are printed, not asserted:
//! they depend on the machine. Run the full size on a release build:
//!
//! ```text
//! cargo test --release -p owlbum-core --test index_large -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use owlbum_core::album::schema;
use owlbum_core::index::{AlbumIndex, NO_DATE};
use rusqlite::Connection;

/// A small linear congruential generator, so the rows are the same on every
/// run without a dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn unit(&mut self) -> f64 {
        self.below(1000) as f64 / 1000.0
    }
}

/// Makes an album database at `path` with `n` photos.
fn make_album(path: &std::path::Path, n: usize) -> Connection {
    let mut conn = Connection::open(path).unwrap();
    schema::migrate(&mut conn).unwrap();
    let mut rng = Lcg(0x0b1b);
    let tx = conn.transaction().unwrap();
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, captured_at, gps_lat, gps_lon, analysis, scan_version, star, hidden)
                 VALUES (?1, ?2, ?3, ?4, 1000, 1, 'fk1:1000:x', 'tk', ?5, 'jpeg', ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12, ?13)",
            )
            .unwrap();
        for i in 0..n {
            let id = i as i64 + 1;
            let year = 2016 + (i * 10 / n.max(1));
            let month = 1 + rng.below(12);
            let rel_dir = format!("{year}/{month:02}");
            let file_name = format!("IMG_{:05}.jpg", rng.below(100_000));
            let rel_path = format!("{rel_dir}/{id}-{file_name}");
            let dated = rng.below(20) != 0;
            let captured_at = dated.then(|| {
                format!(
                    "{year}-{month:02}-{:02}T{:02}:{:02}:{:02}",
                    1 + rng.below(28),
                    rng.below(24),
                    rng.below(60),
                    rng.below(60)
                )
            });
            let gps = rng.below(3) == 0;
            let (lat, lon) = if gps {
                (Some(rng.unit() * 90.0), Some(rng.unit() * 180.0))
            } else {
                (None, None)
            };
            let grid: Vec<String> = (0..27).map(|_| rng.below(256).to_string()).collect();
            let analysis = format!(
                r#"{{"sharpness":{:.2},"mean_luma":{:.3},"clipped_shadows":{:.3},"clipped_highlights":{:.3},"saliency_x":{:.3},"saliency_y":{:.3},"dhash":"{:016x}","avg_colour":[{},{},{}],"colour_grid":[{}],"transparent":false}}"#,
                rng.unit() * 900.0,
                rng.unit(),
                rng.unit() / 10.0,
                rng.unit() / 10.0,
                rng.unit(),
                rng.unit(),
                rng.next() << 31 ^ rng.next(),
                rng.below(256),
                rng.below(256),
                rng.below(256),
                grid.join(",")
            );
            let kind = if rng.below(50) == 0 { "video" } else { "image" };
            insert
                .execute(rusqlite::params![
                    id,
                    rel_path,
                    rel_dir,
                    file_name,
                    kind,
                    4000 + rng.below(2000) as i64,
                    3000,
                    captured_at,
                    lat,
                    lon,
                    analysis,
                    rng.below(10) == 0,
                    rng.below(50) == 0,
                ])
                .unwrap();
        }
        tx.execute_batch("INSERT INTO people (id, name) VALUES (1, 'Ann'), (2, 'Bo')")
            .unwrap();
        let mut tag = tx
            .prepare("INSERT INTO photo_people VALUES (?1, ?2)")
            .unwrap();
        for id in (1..=n as i64).step_by(20) {
            tag.execute((id, 1)).unwrap();
            if id % 40 == 1 {
                tag.execute((id, 2)).unwrap();
            }
        }
    }
    tx.commit().unwrap();
    conn
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn measure(n: usize) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("owlbum.sqlite");
    let made = Instant::now();
    let conn = make_album(&path, n);
    let made = made.elapsed();
    drop(conn);

    // A fresh read-only connection, as the album's readers open it.
    let conn =
        Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let started = Instant::now();
    let index = AlbumIndex::load(&conn, "").unwrap();
    let load = started.elapsed();
    assert_eq!(index.len(), n);

    let bytes = index.heap_bytes();
    let per_photo = bytes as f64 / n as f64;

    // The smallest query the index exists for: leave hidden photos out and
    // sort by date with the name order as tie-break. Filters and sorts
    // proper are #29; this only shows the layout supports the 30 ms target.
    let started = Instant::now();
    let order = index.read(|v| {
        let mut keys: Vec<(i64, u32, u32)> = v
            .photos()
            .iter()
            .filter(|p| !p.hidden())
            .map(|p| (p.date, p.name_rank, p.id))
            .collect();
        keys.sort_unstable();
        keys.into_iter().map(|(_, _, id)| id).collect::<Vec<u32>>()
    });
    let query = started.elapsed();
    assert!(!order.is_empty());
    index.read(|v| {
        let first = v.get(order[0]).unwrap();
        let last = v.get(*order.last().unwrap()).unwrap();
        assert!(first.date <= last.date);
        assert!(order.len() < n, "some are hidden");
        assert_eq!(last.date, NO_DATE, "undated last");
    });

    // A scan batch: 200 new rows reported after one commit.
    let conn = Connection::open(&path).unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    let mut ids = Vec::new();
    for i in 0..200 {
        let id = (n + i + 1) as i64;
        tx.execute(
            "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
             VALUES (?1, ?2, '2030/01', ?3, 1, 1, 'fk', 'tk', 'image', 'jpeg', 1, 1, 1)",
            (id, format!("2030/01/new_{i}.jpg"), format!("new_{i}.jpg")),
        )
        .unwrap();
        ids.push(id);
    }
    tx.commit().unwrap();
    let started = Instant::now();
    index.refresh(&conn, &ids).unwrap();
    let batch = started.elapsed();
    assert_eq!(index.len(), n + 200);

    // The same 200 rows again, changed but not renamed: what a scan's
    // pixels pass sends. No row moves in the name order.
    conn.execute("UPDATE photos SET width = 2 WHERE id > ?1", [n as i64])
        .unwrap();
    let started = Instant::now();
    index.refresh(&conn, &ids).unwrap();
    let changed = started.elapsed();

    // One star, as `photo_set_info` will do it.
    conn.execute("UPDATE photos SET star = 1 WHERE id = 7", [])
        .unwrap();
    let started = Instant::now();
    index.refresh(&conn, &[7]).unwrap();
    let one = started.elapsed();

    println!("album index, {n} photos");
    println!("  make the database:        {}", ms(made));
    println!("  load (target < 2 s):      {}", ms(load));
    println!(
        "  memory:                   {:.1} MB, {per_photo:.0} bytes a photo (target about 150)",
        bytes as f64 / 1_000_000.0
    );
    println!("  filter + date sort pass:  {}", ms(query));
    println!("  refresh 200 new rows:     {}", ms(batch));
    println!("  refresh 200 changed rows: {}", ms(changed));
    println!("  refresh 1 changed row:    {}", ms(one));
    assert!(per_photo <= 170.0, "{per_photo} bytes a photo");
}

#[test]
fn a_few_thousand_photos() {
    measure(3_000);
}

#[test]
#[ignore = "benchmark: run on a release build with --ignored --nocapture"]
fn one_hundred_thousand_photos() {
    measure(100_000);
}
