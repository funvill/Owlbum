//! Where a scan's thumbnail writes spend their time, step by step (#95).
//! Timings for the issue, not a gate:
//! `cargo test -p owlbum-core --release --lib write_profile -- --ignored --nocapture`.
//! `OWLBUM_PROFILE_DIR` sets the folder written in (the system's temporary
//! folder by default). Each variant's files are deleted before the next.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use super::{Kind, store};

const KEYS: usize = 10_000;

#[derive(Default)]
struct Steps {
    probe: AtomicU64,
    mkdir: AtomicU64,
    create: AtomicU64,
    write: AtomicU64,
    close: AtomicU64,
    rename: AtomicU64,
    unlink: AtomicU64,
}

fn add(slot: &AtomicU64, t: Instant) {
    slot.fetch_add(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
}

fn key(i: usize) -> String {
    format!(
        "{:040x}",
        (i as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835)
    )
}

fn body(n: usize) -> Vec<u8> {
    let mut b = vec![7u8; n];
    b[..2].copy_from_slice(&[0xFF, 0xD8]);
    b[n - 2..].copy_from_slice(&[0xFF, 0xD9]);
    b
}

/// Runs `one(key_index)` over every key on `threads` threads; answers the
/// wall time in seconds.
fn run(threads: usize, one: impl Fn(usize) + Sync) -> f64 {
    let next = AtomicUsize::new(0);
    let t = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= KEYS {
                        return;
                    }
                    one(i);
                }
            });
        }
    });
    t.elapsed().as_secs_f64()
}

/// The steps of `store::write` as the scan ran them before #95, for both
/// sizes, timed one by one.
fn stepwise(threads: usize, dir: &Path, bytes: &[u8], s: &Steps) -> f64 {
    run(threads, |i| {
        let k = key(i);
        for kind in [Kind::Small, Kind::Medium] {
            let path = store::path(dir, &k, kind);
            let t = Instant::now();
            let _ = store::is_whole_file(&path);
            add(&s.probe, t);
            let t = Instant::now();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            add(&s.mkdir, t);
            let mut tmp = path.as_os_str().to_owned();
            tmp.push(".0123456789ab.tmp");
            let tmp = PathBuf::from(tmp);
            let t = Instant::now();
            let mut f = std::fs::File::create(&tmp).unwrap();
            add(&s.create, t);
            let t = Instant::now();
            f.write_all(bytes).unwrap();
            add(&s.write, t);
            let t = Instant::now();
            drop(f);
            add(&s.close, t);
            let t = Instant::now();
            std::fs::rename(&tmp, &path).unwrap();
            add(&s.rename, t);
            let t = Instant::now();
            let _ = std::fs::remove_file(&tmp);
            add(&s.unlink, t);
        }
    })
}

fn steps_line(threads: usize, s: &Steps) -> String {
    let ms = |a: &AtomicU64| a.load(Ordering::Relaxed) as f64 / 1e6 / threads as f64;
    format!(
        "per thread ms: probe {:.0}, mkdir {:.0}, create {:.0}, write {:.0}, close {:.0}, rename {:.0}, unlink-missing {:.0}",
        ms(&s.probe),
        ms(&s.mkdir),
        ms(&s.create),
        ms(&s.write),
        ms(&s.close),
        ms(&s.rename),
        ms(&s.unlink)
    )
}

/// Every `thumbs/<aa>` folder, made ahead.
fn folders(dir: &Path) {
    for aa in 0..=255u8 {
        std::fs::create_dir_all(store::thumbs_dir(dir).join(format!("{aa:02x}"))).unwrap();
    }
}

#[test]
#[ignore]
fn write_profile() {
    let base = std::env::var_os("OWLBUM_PROFILE_DIR").map(PathBuf::from);
    let tmp = match &base {
        Some(b) => tempfile::tempdir_in(b).unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    println!("{} keys, 2 files each, in {}", KEYS, tmp.path().display());
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let pool = cpus.saturating_sub(1).max(1);
    let tiny = body(2_000);
    let mut n = 0;
    let mut fresh = || {
        n += 1;
        let d = tmp.path().join(format!("v{n}"));
        let _ = std::fs::remove_dir_all(tmp.path().join(format!("v{}", n - 1)));
        d
    };

    for threads in [1, 3, pool] {
        let s = Steps::default();
        let secs = stepwise(threads, &fresh(), &tiny, &s);
        println!(
            "2 KB, {threads} threads, old store::write steps: {secs:.2} s; {}",
            steps_line(threads, &s)
        );
    }

    let d = fresh();
    let secs = run(pool, |i| {
        let k = key(i);
        for kind in [Kind::Small, Kind::Medium] {
            let p = store::path(&d, &k, kind);
            let _ = store::is_whole_file(&p);
            store::write(&p, &tiny).unwrap();
        }
    });
    println!("2 KB, {pool} threads, store::write now: {secs:.2} s");

    for (label, ext) in [
        ("straight to <key>_s.jpg", "jpg"),
        ("straight to <key>_s.bin", "bin"),
    ] {
        let d = fresh();
        folders(&d);
        let secs = run(pool, |i| {
            let k = key(i);
            for kind in [Kind::Small, Kind::Medium] {
                let p = store::path(&d, &k, kind).with_extension(ext);
                std::fs::write(p, &tiny).unwrap();
            }
        });
        println!("2 KB, {pool} threads, {label}, folders made ahead: {secs:.2} s");
    }

    let d = fresh();
    folders(&d);
    let secs = run(pool, |i| {
        let k = key(i);
        for kind in [Kind::Small, Kind::Medium] {
            let p = store::path(&d, &k, kind);
            let mut tmp = p.as_os_str().to_owned();
            tmp.push(".0123456789ab.tmp");
            std::fs::write(&tmp, &tiny).unwrap();
            std::fs::rename(&tmp, &p).unwrap();
        }
    });
    println!("2 KB, {pool} threads, temporary file then rename, folders made ahead: {secs:.2} s");

    let d = fresh();
    folders(&d);
    let secs = run(pool, |i| {
        let k = key(i);
        let _ = std::fs::read(store::path(&d, &k, Kind::Small));
    });
    println!("{pool} threads, reading a missing file per key: {secs:.2} s");
    let _ = std::fs::remove_dir_all(&d);
}

/// Where one photo's first scan spends its time, stage by stage, on one
/// thread (#109): the key sample, the header read, the decode, the resize,
/// and the two JPEG encodes. Timings for the issue, not a gate:
///
/// ```text
/// OWLBUM_TIMING_ALBUM=<album> cargo test -p owlbum-core --release --lib \
///     stage_profile -- --ignored --nocapture
/// ```
///
/// Run it on a freshly made album to see the virus scanner's first-open
/// cost in the sample stage, and again to see it warm.
#[test]
#[ignore]
fn stage_profile() {
    use crate::media::registry::format_of;
    use crate::scan::reader::{MediaReader, Reader};
    use crate::thumbs::make;

    let Some(root) = std::env::var_os("OWLBUM_TIMING_ALBUM").map(PathBuf::from) else {
        println!("OWLBUM_TIMING_ALBUM is not set");
        return;
    };
    let walked = crate::scan::walk::walk(
        &root,
        "",
        &std::sync::atomic::AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let files: Vec<PathBuf> = walked
        .files
        .iter()
        .map(|f| crate::scan::walk::absolute(&root, &f.rel_path))
        .collect();
    let reader = MediaReader;
    let mut stages = [0f64; 6];
    let names = [
        "sample", "metadata", "decode", "resize", "encode m", "encode s",
    ];
    let mut bytes = [0usize; 2];
    for path in &files {
        let format = format_of(path.file_name().unwrap().to_str().unwrap()).unwrap();
        let mut lap = |i: usize, t: Instant| stages[i] += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let _ = reader.sample(path).unwrap();
        lap(0, t);
        let t = Instant::now();
        let _ = reader.metadata(format, path);
        lap(1, t);
        let t = Instant::now();
        let px = reader.preview(format, path).unwrap();
        lap(2, t);
        let t = Instant::now();
        let b = make::buffers(&px);
        lap(3, t);
        let t = Instant::now();
        bytes[0] += make::encode(&b.medium, make::MEDIUM_QUALITY).len();
        lap(4, t);
        let t = Instant::now();
        bytes[1] += make::encode(&b.small, make::SMALL_QUALITY).len();
        lap(5, t);
    }
    let n = files.len().max(1) as f64;
    let total: f64 = stages.iter().sum();
    println!("{} photos, one thread, {total:.2} s in all", files.len());
    for (name, s) in names.iter().zip(stages) {
        println!(
            "  {name:>9}: {s:7.2} s, {:6.1} ms a photo, {:4.1}%",
            s * 1000.0 / n,
            s * 100.0 / total
        );
    }
    println!(
        "  bytes: medium {} KB, small {} KB a photo",
        bytes[0] / files.len().max(1) / 1000,
        bytes[1] / files.len().max(1) / 1000
    );
}
