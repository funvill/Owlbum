use super::*;

fn at(lat: f64, lon: f64) -> Option<&'static str> {
    country_at(lat, lon)
}

#[test]
fn the_bundled_outlines_load() {
    let o = outlines();
    assert_eq!(o.countries.len(), 258);
    let mut names: Vec<&str> = o.countries.iter().map(|c| c.name.as_str()).collect();
    assert!(names.iter().all(|n| !n.is_empty()));
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 258, "names are unique");
}

#[test]
fn cities_are_in_their_countries() {
    let cases = [
        ((48.8566, 2.3522), "France"),
        ((49.2827, -123.1207), "Canada"),
        ((40.7128, -74.0060), "United States of America"),
        ((35.6762, 139.6503), "Japan"),
        ((-33.8688, 151.2093), "Australia"),
        ((-1.2921, 36.8219), "Kenya"),
        ((-22.9068, -43.1729), "Brazil"),
        ((39.9042, 116.4074), "China"),
        ((55.7558, 37.6173), "Russia"),
        ((28.6139, 77.2090), "India"),
        ((64.1466, -21.9426), "Iceland"),
        ((1.3521, 103.8198), "Singapore"),
        ((-41.2865, 174.7762), "New Zealand"),
        ((5.6037, -0.1870), "Ghana"),
    ];
    for ((lat, lon), want) in cases {
        assert_eq!(at(lat, lon), Some(want), "{lat}, {lon}");
    }
}

#[test]
fn points_a_few_kilometres_either_side_of_a_border() {
    let cases = [
        // Strasbourg and Kehl, across the Rhine.
        ((48.5734, 7.7521), "France"),
        ((48.5600, 7.8300), "Germany"),
        // Tijuana and San Diego.
        ((32.5149, -117.0382), "Mexico"),
        ((32.5800, -117.0400), "United States of America"),
        // Ciudad Juarez and El Paso.
        ((31.6904, -106.4245), "Mexico"),
        ((31.7900, -106.4200), "United States of America"),
        // Geneva and Annemasse.
        ((46.2044, 6.1432), "Switzerland"),
        ((46.1934, 6.2342), "France"),
        // Detroit and Windsor.
        ((42.3600, -83.0800), "United States of America"),
        ((42.2900, -83.0000), "Canada"),
        // Narva and Ivangorod.
        ((59.3700, 28.1200), "Estonia"),
        ((59.3700, 28.3000), "Russia"),
    ];
    for ((lat, lon), want) in cases {
        assert_eq!(at(lat, lon), Some(want), "{lat}, {lon}");
    }
}

#[test]
fn enclaves_win_over_the_country_around_them() {
    assert_eq!(at(-29.52, 28.61), Some("Lesotho"));
    assert_eq!(at(43.9356, 12.4473), Some("San Marino"));
    assert_eq!(at(41.9029, 12.4534), Some("Vatican City"));
    assert_eq!(at(43.7384, 7.4246), Some("Monaco"));
    // Just outside them.
    assert_eq!(at(-29.0, 26.0), Some("South Africa"));
    assert_eq!(at(41.8902, 12.4922), Some("Italy"));
}

#[test]
fn mid_ocean_has_no_country() {
    for (lat, lon) in [
        (30.0, -40.0),
        (0.0, -140.0),
        (-45.0, 90.0),
        (20.0, 65.0),
        (90.0, 0.0),
    ] {
        assert_eq!(at(lat, lon), None, "{lat}, {lon}");
    }
}

#[test]
fn the_sea_near_a_coast_belongs_to_that_country() {
    // About 5 km off Nice, and 10 km off the Sydney Heads.
    assert_eq!(at(43.65, 7.30), Some("France"));
    assert_eq!(at(-33.84, 151.40), Some("Australia"));
    // About 40 km off Nice: past territorial waters.
    assert_eq!(at(43.35, 7.50), None);
}

#[test]
fn the_antimeridian() {
    // Chukotka spans it; 180 and -180 are the same meridian.
    assert_eq!(at(66.0, 179.5), Some("Russia"));
    assert_eq!(at(66.0, -179.5), Some("Russia"));
    assert_eq!(at(67.0, 180.0), Some("Russia"));
    assert_eq!(at(67.0, -180.0), Some("Russia"));
    // Fiji's islands lie on both sides of it.
    assert_eq!(at(-17.7134, 178.0650), Some("Fiji"));
    assert_eq!(at(-16.79, -179.98), Some("Fiji"));
    assert_eq!(at(-16.79, 179.98), Some("Fiji"));
    // Open sea on both sides.
    assert_eq!(at(10.0, 180.0), None);
    assert_eq!(at(10.0, -180.0), None);
}

#[test]
fn the_poles_and_bad_input() {
    assert_eq!(at(-90.0, 0.0), Some("Antarctica"));
    assert_eq!(at(-85.0, 120.0), Some("Antarctica"));
    assert_eq!(at(f64::NAN, 0.0), None);
    assert_eq!(at(0.0, f64::INFINITY), None);
    assert_eq!(at(91.0, 0.0), None);
    assert_eq!(at(0.0, 181.0), None);
}

/// Disputed areas follow Natural Earth's default view: the side that
/// administers an area on the ground, and a feature of its own where Natural
/// Earth draws one (docs/04-browsing.md, "How typed fields save").
#[test]
fn disputed_areas_follow_natural_earths_default_view() {
    let cases = [
        // Kashmir: each side of the Line of Control, and Aksai Chin.
        ((34.0837, 74.7973), "India"),
        ((35.9208, 74.3089), "Pakistan"),
        ((35.0, 79.3), "China"),
        ((35.4210, 77.1094), "Siachen Glacier"),
        // Laayoune, west of the berm, is administered by Morocco; Tifariti,
        // east of it, is in the area Natural Earth draws as Western Sahara.
        ((27.1536, -13.2033), "Morocco"),
        ((26.1580, -10.5670), "Western Sahara"),
        ((42.6629, 21.1655), "Kosovo"),
        ((35.3364, 33.3182), "Northern Cyprus"),
        ((31.9038, 35.2034), "Palestine"),
        ((25.0330, 121.5654), "Taiwan"),
        ((9.5624, 44.0770), "Somaliland"),
        ((-51.6977, -57.8517), "Falkland Islands"),
        ((44.9521, 34.1024), CRIMEA),
    ];
    for ((lat, lon), want) in cases {
        assert_eq!(at(lat, lon), Some(want), "{lat}, {lon}");
    }
}

/// Natural Earth 5.1.2's default view draws Crimea inside Russia, the side
/// that administers it; its Ukrainian point-of-view files are not bundled.
const CRIMEA: &str = "Russia";

#[test]
fn every_central_point_is_in_its_own_country() {
    let o = outlines();
    let mut wrong = Vec::new();
    for c in &o.countries {
        let (lat, lon) = c.centre();
        if country_at(lat, lon) != Some(c.name.as_str()) {
            wrong.push(format!("{} -> {:?}", c.name, country_at(lat, lon)));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn a_hundred_thousand_lookups_are_quick() {
    let start = std::time::Instant::now();
    let o = parse(DATA).unwrap();
    eprintln!(
        "read {} countries, {} band edges in {:?}",
        o.countries.len(),
        o.edges.len(),
        start.elapsed()
    );
    outlines();
    // A fixed spread of points: half on land near cities, half anywhere.
    let mut seed: u64 = 7;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let points: Vec<(f64, f64)> = (0..100_000)
        .map(|i| {
            if i % 2 == 0 {
                (48.0 + next() * 4.0, 2.0 + next() * 8.0)
            } else {
                (next() * 180.0 - 90.0, next() * 360.0 - 180.0)
            }
        })
        .collect();
    let start = std::time::Instant::now();
    let found = points
        .iter()
        .filter(|(lat, lon)| country_at(*lat, *lon).is_some())
        .count();
    let took = start.elapsed();
    eprintln!("100,000 lookups in {took:?}");
    assert!(found > 50_000);
    // Unoptimised test builds are several times slower than the release
    // build; a release build does this in a small fraction of the bound.
    assert!(took.as_millis() < 5_000, "{took:?}");
}

#[test]
fn search_lists_names_containing_the_text() {
    let names =
        |text: &str| -> Vec<String> { place_search(text).into_iter().map(|m| m.name).collect() };
    assert_eq!(names("france"), ["France"]);
    assert_eq!(names("FRANCE"), ["France"]);
    assert_eq!(names("  fran "), ["France"]);
    assert!(names("").is_empty());
    assert!(names("   ").is_empty());
    assert!(names("zzz").is_empty());
    // Accents are ignored too.
    assert_eq!(names("aland"), ["Åland", "New Zealand"]);
    assert_eq!(names("sao tome"), ["São Tomé and Príncipe"]);
    assert_eq!(names("curaçao"), ["Curaçao"]);
    // Names that start with the text first, then names with a word that
    // starts with it, then the rest; each group in name order. At most 6.
    assert_eq!(names("ger"), ["Germany", "Algeria", "Niger", "Nigeria"]);
    assert_eq!(
        names("in"),
        [
            "India",
            "Indonesia",
            "Australian Indian Ocean Territories",
            "British Indian Ocean Territory",
            "United Nations Buffer Zone in Cyprus",
            "Argentina",
        ]
    );
    assert_eq!(names("land").len(), 6);
}

#[test]
fn search_answers_each_countrys_central_point() {
    let found = place_search("japan");
    assert_eq!(found.len(), 1);
    let m = &found[0];
    assert_eq!(m.name, "Japan");
    assert_eq!(country_at(m.lat, m.lon), Some("Japan"));
    // Natural Earth's label point, to 4 decimals.
    assert!(
        (m.lat - 36.9).abs() < 2.0 && (m.lon - 138.4).abs() < 2.0,
        "{m:?}"
    );
}
