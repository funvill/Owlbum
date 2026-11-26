use super::*;
use crate::index::PhotoFacts;
use crate::index::record::build;

/// A photo with a dHash, a date (seconds from a base, None for undated)
/// and an average colour.
fn shot(id: u32, hash: u64, at: Option<i64>, colour: Option<[u8; 3]>) -> Photo {
    let colour = colour.map_or(String::new(), |[r, g, b]| {
        format!(r#","avg_colour":[{r},{g},{b}]"#)
    });
    let json = format!(
        r#"{{"sharpness":10,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5,"dhash":"{hash:016x}"{colour}}}"#
    );
    let captured_at = at.map(|s| {
        let base = crate::index::parse_date("2024-06-01T08:00:00").unwrap() + s;
        let (d, t) = (base.div_euclid(86_400), base.rem_euclid(86_400));
        // 2024-06-01 is day 19875 from 1970-01-01; tests stay within June.
        format!(
            "2024-06-{:02}T{:02}:{:02}:{:02}",
            d - 19_875 + 1,
            t / 3600,
            t / 60 % 60,
            t % 60
        )
    });
    let mut p = build(
        &PhotoFacts {
            id: i64::from(id),
            file_name: format!("{id}.jpg"),
            kind: "image".into(),
            width: 4000,
            height: 3000,
            captured_at,
            analysis: Some(json),
            ..PhotoFacts::default()
        },
        0,
    )
    .unwrap();
    p.name_rank = id;
    p
}

/// A hash `bits` bits away from `h`.
fn flip(h: u64, bits: u32) -> u64 {
    h ^ ((1_u128 << bits) - 1) as u64
}

const H: u64 = 0x8f0c_3c3e_1e0e_0603;
const GREY: Option<[u8; 3]> = Some([100, 100, 100]);

fn paired(a: Photo, b: Photo) -> bool {
    let found = find(&[a, b]);
    assert_eq!(pairs(&a, &b), pairs(&b, &a));
    assert_eq!(found[0].is_some(), pairs(&a, &b), "find agrees with pairs");
    found[0].is_some()
}

#[test]
fn the_shape_limit_is_twelve_bits_when_dated_and_six_otherwise() {
    let d = |id, bits| shot(id, flip(H, bits), Some(0), GREY);
    assert!(paired(shot(1, H, Some(0), GREY), d(2, 12)));
    assert!(!paired(shot(1, H, Some(0), GREY), d(2, 13)));
    let u = |id, bits| shot(id, flip(H, bits), None, GREY);
    assert!(paired(shot(1, H, None, GREY), u(2, 6)));
    assert!(!paired(shot(1, H, None, GREY), u(2, 7)));
    // One dated and one undated: the undated limit, no time test.
    assert!(paired(shot(1, H, Some(0), GREY), u(2, 6)));
    assert!(!paired(shot(1, H, Some(0), GREY), u(2, 7)));
}

#[test]
fn dated_shots_pair_within_two_hours() {
    let a = shot(1, H, Some(0), GREY);
    assert!(paired(a, shot(2, H, Some(120 * 60), GREY)));
    assert!(!paired(a, shot(2, H, Some(121 * 60), GREY)));
    assert!(!paired(a, shot(2, H, Some(-121 * 60), GREY)));
}

#[test]
fn colours_pair_within_seventy() {
    let a = shot(1, H, Some(0), Some([100, 100, 100]));
    // 70 exactly: (42, 56, 0) is a 3-4-5 triangle of 70.
    assert!(paired(a, shot(2, H, Some(0), Some([142, 156, 100]))));
    assert!(!paired(a, shot(2, H, Some(0), Some([171, 100, 100]))));
    // A photo with no colour skips the colour test.
    assert!(paired(a, shot(2, H, Some(0), None)));
}

#[test]
fn a_photo_without_a_dhash_is_in_no_burst() {
    let a = shot(1, H, Some(0), GREY);
    let mut b = build(
        &PhotoFacts {
            id: 2,
            file_name: "2.jpg".into(),
            captured_at: Some("2024-06-01T08:00:00".into()),
            analysis: Some(r#"{"sharpness":10,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5}"#.into()),
            ..PhotoFacts::default()
        },
        0,
    )
    .unwrap();
    b.name_rank = 2;
    assert_eq!(find(&[a, b]), [None, None]);
}

#[test]
fn pairs_chain_and_the_id_is_the_first_member_by_date() {
    // c pairs with b and b with a, but a and c are 24 bits apart.
    let a = shot(5, H, Some(600), GREY);
    let b = shot(3, flip(H, 12), Some(300), GREY);
    let c = shot(9, H ^ (flip(0, 24)), Some(0), GREY);
    assert!(!pairs(&a, &c));
    let found = find(&[a, b, c]);
    assert_eq!(found, vec![NonZeroU32::new(9); 3], "c is earliest");
    // Undated members come last; the name order breaks ties.
    let u = shot(1, H, None, GREY);
    let found = find(&[a, u]);
    assert_eq!(found, vec![NonZeroU32::new(5); 2]);
    let (x, y) = (shot(8, H, Some(0), GREY), shot(4, H, Some(0), GREY));
    assert_eq!(find(&[x, y]), vec![NonZeroU32::new(4); 2]);
}

#[test]
fn exact_duplicates_form_a_burst() {
    let found = find(&[shot(1, H, None, GREY), shot(2, H, None, GREY)]);
    assert_eq!(found, vec![NonZeroU32::new(1); 2]);
}

#[test]
fn the_lookahead_is_limited_in_date_order() {
    // 2,002 shots in one second: the first reaches only the next 2,000.
    let mut photos = vec![shot(1, H, Some(0), GREY)];
    for id in 2..=LOOKAHEAD as u32 + 1 {
        // Far from H and from each other.
        photos.push(shot(
            id,
            u64::from(id).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            Some(0),
            GREY,
        ));
    }
    photos.push(shot(LOOKAHEAD as u32 + 2, H, Some(0), GREY));
    let found = find(&photos);
    assert_eq!(found[0], None, "the twin is 2,001 places on");
}

#[test]
fn the_keeper_order() {
    let base = || shot(1, H, Some(0), GREY);
    let with = |f: &dyn Fn(&mut Photo)| {
        let mut p = base();
        f(&mut p);
        p
    };
    let star = |p: &mut Photo| p.set_flags(true, p.hidden());
    let hide = |p: &mut Photo| p.set_flags(p.star(), true);

    // 1. not hidden before hidden, even over a star and a better quality.
    let shown = base();
    let hidden = with(&|p| {
        star(p);
        hide(p);
        p.derived.quality = 0.9;
    });
    assert_eq!(keeper_order(&shown, &hidden), Ordering::Less);
    // 2. starred before not.
    let starred = with(&|p| star(p));
    let better = with(&|p| p.derived.quality = 0.9);
    assert_eq!(keeper_order(&starred, &better), Ordering::Less);
    // 3. higher quality.
    assert_eq!(keeper_order(&better, &base()), Ordering::Less);
    // 4. earlier date, undated last.
    let early = with(&|p| p.date -= 1);
    assert_eq!(keeper_order(&early, &base()), Ordering::Less);
    let undated = with(&|p| p.date = crate::index::NO_DATE);
    assert_eq!(keeper_order(&base(), &undated), Ordering::Less);
    // 5. lower name rank.
    let later_name = with(&|p| p.name_rank = 2);
    assert_eq!(keeper_order(&base(), &later_name), Ordering::Less);
}

#[test]
fn a_hidden_photo_is_not_the_keeper_while_another_member_is_shown() {
    let mut a = shot(1, H, Some(0), GREY);
    let mut b = shot(2, H, Some(0), GREY);
    a.derived.quality = 0.9;
    a.set_flags(true, true);
    let burst = NonZeroU32::new(1);
    a.derived.burst = burst;
    b.derived.burst = burst;
    let mut photos = [a, b];
    pick_keepers(&mut photos, None);
    assert!(!photos[0].derived.keeper);
    assert!(photos[1].derived.keeper);
    // Every member hidden: the order goes on to the star.
    photos[1].set_flags(false, true);
    pick_keepers(&mut photos, None);
    assert!(photos[0].derived.keeper);
}

#[test]
fn a_photo_in_no_burst_is_its_own_keeper() {
    let mut photos = [shot(1, H, Some(0), GREY)];
    pick_keepers(&mut photos, None);
    assert!(photos[0].derived.keeper);
}

/// A tiny seeded generator, so the test needs no crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Every pair compared: the reference [`find`] must agree with.
fn every_pair(photos: &[Photo]) -> Vec<Option<NonZeroU32>> {
    let mut sets = Sets::new(photos.len());
    for i in 0..photos.len() {
        for j in i + 1..photos.len() {
            if pairs(&photos[i], &photos[j]) {
                sets.union(i, j);
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..photos.len() {
        let root = sets.find(i);
        groups.entry(root).or_default().push(i);
    }
    let mut out = vec![None; photos.len()];
    for members in groups.values().filter(|m| m.len() > 1) {
        let lead = members
            .iter()
            .min_by_key(|&&k| (photos[k].date, photos[k].name_rank))
            .unwrap();
        for &k in members {
            out[k] = NonZeroU32::new(photos[*lead].id);
        }
    }
    out
}

#[test]
fn the_fast_method_finds_the_same_bursts_as_every_pair() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let mut photos = Vec::new();
    let mut id = 0;
    while photos.len() < 5000 {
        // A scene: one hash and colour, shot one to five times over a few
        // minutes; some scenes undated; each shot a few bits off.
        let hash = rng.next();
        let colour = [
            rng.below(256) as u8,
            rng.below(256) as u8,
            rng.below(256) as u8,
        ];
        let at = rng.below(25 * 86_400) as i64;
        let undated = rng.below(10) == 0;
        for _ in 0..=rng.below(5) {
            id += 1;
            let mut h = hash;
            for _ in 0..rng.below(9) {
                h ^= 1 << rng.below(64);
            }
            let jitter = |c: u8, rng: &mut Rng| c.saturating_add(rng.below(40) as u8);
            let c = [
                jitter(colour[0], &mut rng),
                jitter(colour[1], &mut rng),
                jitter(colour[2], &mut rng),
            ];
            let when = (!undated || rng.below(2) == 0).then(|| at + rng.below(9000) as i64);
            photos.push(shot(id, h, when, (rng.below(20) != 0).then_some(c)));
        }
    }
    let fast = find(&photos);
    assert_eq!(fast, every_pair(&photos));
    let in_bursts = fast.iter().filter(|b| b.is_some()).count();
    assert!(
        in_bursts > 1000,
        "the seeded album has many bursts: {in_bursts}"
    );
}
