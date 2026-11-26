//! The five sorts (docs/05-organising.md, "Sorting").
//!
//! Every order is total: the last key is always the name order
//! (`name_rank`, unique per photo), so no two photos compare equal and the
//! same query gives the same order everywhere. Keys are worked out once per
//! photo into a compact vector and sorted there, so no sort compares
//! strings or calls back into the index while it sorts.

use std::cmp::Reverse;

use super::shape::QuerySort;
use crate::index::{IndexView, Photo};
use crate::organise::facts;

/// The ids of `shown` in the order of `sort`. `view` is the whole album
/// index: "interesting" stretches quality over every analysed photo in
/// it, not over the photos shown.
pub fn order(view: &IndexView<'_>, shown: &[&Photo], sort: QuerySort) -> Vec<u32> {
    match sort {
        // Effective date, oldest first; undated last (`NO_DATE` is the
        // largest date).
        QuerySort::Date => by(shown, |p| p.date),
        QuerySort::Name => by(shown, |_| ()),
        // Highest first; a photo with no score yet ("–") after every
        // scored one.
        QuerySort::Score => by(shown, |p| match p.derived.score {
            Some(s) => (false, Reverse(s)),
            None => (true, Reverse(0)),
        }),
        // Analysed first, then most interesting first.
        QuerySort::Interesting => {
            // Quality is worked out once per photo: for the album's range,
            // and kept for the photos shown, which are records of the same
            // slice (a binary search of the sharpness list each time is
            // most of the cost).
            let photos = view.photos();
            let sharpness = view.sharpness();
            let quality: Vec<Option<f64>> = photos
                .iter()
                .map(|p| facts::quality(p, sharpness))
                .collect();
            let range = facts::quality_range(quality.iter().flatten().copied());
            let base = photos.as_ptr() as usize;
            by(shown, |p| {
                let at = (std::ptr::from_ref(p) as usize).wrapping_sub(base) / size_of::<Photo>();
                let q = match quality.get(at) {
                    Some(&q) if std::ptr::eq(&photos[at], p) => q,
                    _ => facts::quality(p, sharpness),
                };
                (
                    p.analysis().is_none(),
                    Reverse(ordered(facts::interesting_of(p, q, range) as f32)),
                )
            })
        }
        // With an average colour first, then bucket, then darkest first.
        QuerySort::Colour => by(shown, |p| {
            let avg = p.analysis().and_then(|a| a.avg_colour());
            let (bucket, lightness) = facts::colour(avg);
            (avg.is_none(), bucket, ordered(lightness))
        }),
    }
}

/// Sorts `shown` by `key`, then by the name order.
fn by<K: Ord>(shown: &[&Photo], key: impl Fn(&Photo) -> K) -> Vec<u32> {
    let mut keyed: Vec<(K, u32, u32)> = shown.iter().map(|p| (key(p), p.name_rank, p.id)).collect();
    keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    keyed.into_iter().map(|(_, _, id)| id).collect()
}

/// A float as an integer with the same order as [`f32::total_cmp`].
fn ordered(x: f32) -> i32 {
    let bits = x.to_bits() as i32;
    bits ^ ((((bits >> 31) as u32) >> 1) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_keeps_the_float_order() {
        let xs = [-1.5_f32, -0.0, 0.0, 1e-9, 0.5, 1.0, 2.0];
        for w in xs.windows(2) {
            assert!(ordered(w[0]) < ordered(w[1]), "{w:?}");
        }
    }

    /// Times each sort over 100,000 analysed photos (docs/05-organising.md,
    /// "Performance targets": any sort within 30 ms). Run once on a release
    /// build: `cargo test --release -p owlbum-core --lib sort::tests::
    /// -- --ignored --nocapture`.
    #[test]
    #[ignore = "a timing, run by hand on a release build"]
    fn sorts_of_100000_photos() {
        use crate::index::{AlbumIndex, PhotoFacts};
        let facts: Vec<PhotoFacts> = (1..=100_000_i64)
            .map(|i| PhotoFacts {
                id: i,
                rel_dir: format!("d{}", i % 50),
                file_name: format!("IMG_{}.jpg", (i * 7919) % 100_000),
                kind: "image".into(),
                captured_at: Some(format!("2024-06-{:02}T10:00:00", 1 + i % 28)),
                width: 4000,
                height: 3000,
                analysis: (i % 10 != 0).then(|| {
                    format!(
                        r#"{{"sharpness":{},"mean_luma":0.{},"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.4,"saliency_y":0.6,"avg_colour":[{},{},{}],"colour_grid":[{}]}}"#,
                        i % 997,
                        10 + i % 80,
                        i % 256,
                        (i * 3) % 256,
                        (i * 7) % 256,
                        (0..27).map(|k| ((i + k) % 256).to_string()).collect::<Vec<_>>().join(",")
                    )
                }),
                ..PhotoFacts::default()
            })
            .collect();
        let index = AlbumIndex::empty("");
        index.upsert(&facts);
        index.rescore();
        for sort in [
            QuerySort::Date,
            QuerySort::Name,
            QuerySort::Score,
            QuerySort::Interesting,
            QuerySort::Colour,
        ] {
            let t = std::time::Instant::now();
            let n = index.read(|v| {
                let shown: Vec<&Photo> = v.photos().iter().filter(|p| !p.hidden()).collect();
                order(v, &shown, sort).len()
            });
            println!("{sort:?}: {n} photos in {:?}", t.elapsed());
        }
    }
}
