//! The `large-N` album: `n` small photos from a seed, spread over ten
//! years in `YYYY/MM` folders, with bursts, undated photos and some GPS.

use std::fs;
use std::io;
use std::path::Path;

use crate::encode::{self, Meta};
use crate::picture::{nudge, scene};
use crate::rng::Rng;
use crate::{Album, Fixture, Kind, Writer, degrees};

/// The seed `large` uses when none is given on the command line.
pub const DEFAULT_SEED: u64 = 0x0b1b;

/// 2016-01-01 as days since 1970-01-01.
const FIRST_DAY: i64 = 16_801;
const SPAN_SECONDS: i64 = 3_653 * 86_400;

const CAMERAS: [(&str, &str); 4] = [
    ("Owlbum", "Fixture One"),
    ("Owlbum", "Pocket 2"),
    ("Fixture Optics", "FO-7"),
    ("Fixture Optics", "FO-9 Pro"),
];

struct Plan {
    fixture: Fixture,
    scene_seed: u64,
    shot: u8,
    camera: usize,
    exif_time: Option<String>,
    gps: Option<(i32, i32)>,
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn exif_time(seconds: i64) -> String {
    let (y, mo, d) = civil(FIRST_DAY + seconds.div_euclid(86_400));
    let s = seconds.rem_euclid(86_400);
    format!(
        "{y:04}:{mo:02}:{d:02} {:02}:{:02}:{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn plan(n: usize, seed: u64) -> Vec<Plan> {
    let mut r = Rng::new(seed);
    let step = SPAN_SECONDS / (n.max(1) as i64);
    let mut out: Vec<Plan> = Vec::with_capacity(n);
    let mut i = 0;
    let mut burst_id = 0;
    while i < n {
        let start = i as i64 * step + i64::from(r.range(0, (step / 2).clamp(1, 86_400) as u32));
        let dated = !r.chance(5);
        let camera = r.range(0, CAMERAS.len() as u32) as usize;
        let gps = r.chance(30).then(|| {
            (
                49_000_000 + r.range(0, 600_000) as i32,
                -123_500_000 + r.range(0, 1_000_000) as i32,
            )
        });
        let (w, h, kind) = match r.range(0, 10) {
            0..=5 => (64, 48, Kind::Landscape),
            6..=7 => (48, 64, Kind::Portrait),
            8 => (64, 64, Kind::Square),
            _ => (96, 24, Kind::Panorama),
        };
        let scene_seed = r.next_u64();
        let shots = if dated && r.chance(4) {
            burst_id += 1;
            r.range(3, 6) as usize
        } else {
            1
        };
        for shot in 0..shots.min(n - i) {
            let time = dated.then(|| exif_time(start + shot as i64));
            let folder = match &time {
                Some(t) => format!("{}/{}", &t[0..4], &t[5..7]),
                None => "undated".to_string(),
            };
            let mut f = Fixture::new(
                format!("{folder}/IMG_{:06}.jpg", i + 1),
                if shots > 1 { Kind::Burst } else { kind },
                Some("jpeg"),
            )
            .size(w, h);
            f.taken = time.as_deref().map(crate::iso);
            f.gps = degrees(gps);
            f.burst = (shots > 1).then_some(burst_id);
            out.push(Plan {
                fixture: f,
                scene_seed,
                shot: shot as u8,
                camera,
                exif_time: time,
                gps,
            });
            i += 1;
        }
    }
    out
}

fn render(p: &Plan) -> Vec<u8> {
    let f = &p.fixture;
    let mut img = scene(p.scene_seed, f.width, f.height);
    if p.shot > 0 {
        img = nudge(&img, i64::from(p.shot), 0, p.shot);
    }
    let (make, model) = CAMERAS[p.camera];
    let meta = Meta {
        make: Some(make),
        model: Some(model),
        orientation: 1,
        taken: p.exif_time.clone(),
        iso: Some(100 << (p.camera % 4)),
        gps: p.gps,
        pixels: Some((f.width, f.height)),
        ..Meta::default()
    };
    encode::jpeg(&img, 75, &meta)
}

/// Write `n` photos made from `seed` into `root` (missing or empty).
pub fn large(root: &Path, n: usize, seed: u64) -> io::Result<Album> {
    let w = Writer::new(root)?;
    let plans = plan(n, seed);
    let mut folders: Vec<&str> = plans
        .iter()
        .map(|p| p.fixture.rel_path.rsplit_once('/').map_or("", |(d, _)| d))
        .collect();
    folders.dedup();
    for d in folders {
        fs::create_dir_all(root.join(d))?;
    }

    // Each file depends only on its plan, so threads change nothing but
    // the time it takes.
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
    let chunk = plans.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = plans
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || -> io::Result<()> {
                    for p in part {
                        fs::write(root.join(&p.fixture.rel_path), render(p))?;
                    }
                    Ok(())
                })
            })
            .collect();
        handles
            .into_iter()
            .try_for_each(|h| h.join().expect("fixture thread panicked"))
    })?;

    let mut album = w.finish();
    album.files = plans.into_iter().map(|p| p.fixture).collect();
    Ok(album)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(FIRST_DAY), (2016, 1, 1));
        assert_eq!(civil(FIRST_DAY + 59), (2016, 2, 29));
        assert_eq!(exif_time(3_661), "2016:01:01 01:01:01");
    }

    #[test]
    fn plans_are_ordered_and_spread() {
        let p = plan(1000, 3);
        assert_eq!(p.len(), 1000);
        let dated: Vec<&str> = p.iter().filter_map(|p| p.exif_time.as_deref()).collect();
        assert!(dated.windows(2).all(|w| w[0] <= w[1]));
        assert!(dated.first().unwrap().starts_with("2016"));
        assert!(dated.last().unwrap().starts_with("2025"));
    }
}
