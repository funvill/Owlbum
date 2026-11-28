//! Country outlines: which country a place is in (the details panel's
//! Country row), from Natural Earth's admin-0 countries bundled in `data/countries.bin`
//! (docs/04-browsing.md, "How typed fields save"; docs/01-architecture.md,
//! "Dependencies chosen"). Nothing is fetched.
//!
//! The data file is written by `scripts/countries.mjs`, which describes its
//! format. It is read once, on first use, into an index of latitude bands:
//! each band holds, per country, the outline edges that reach into it, so a
//! lookup only tests the few edges a horizontal ray from the point can cross.
//!
//! Rules:
//! - A point inside an outline is in that country. Where outlines overlap
//!   (an enclave drawn without a hole in the country around it), the
//!   country with the smaller bounding box wins.
//! - A point at sea within 12 nautical miles of an outline (territorial
//!   waters, and the margin the simplified coastline needs) is in the
//!   country with the nearest outline. Farther out there is no country.
//! - Disputed areas follow Natural Earth's default view: the side that
//!   administers an area on the ground, and a feature of its own where
//!   Natural Earth draws one (Western Sahara, Kosovo, Northern Cyprus,
//!   Somaliland, Siachen Glacier). The name is the one in the data.

use std::sync::OnceLock;

/// How far out to sea a place still takes the nearest country: 12 nautical
/// miles, the width of territorial waters.
pub const SEA_MARGIN_KM: f64 = 22.224;

/// The bundled outlines (`scripts/countries.mjs` writes them).
static DATA: &[u8] = include_bytes!("../../data/countries.bin");

/// Coordinates in the data are whole units of this many per degree.
const UNIT: f64 = 10_000.0;
/// Latitude bands are this many units high (one degree).
const BAND: i32 = 10_000;
const BANDS: usize = 180;
const KM_PER_DEGREE: f64 = 111.32;

pub(crate) struct Country {
    pub name: String,
    /// The bounding box's area in square units, for overlaps.
    box_area: f64,
}

/// One outline edge, in units.
#[derive(Clone, Copy)]
struct Edge {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

/// One country's edges inside one band.
struct Entry {
    country: u16,
    x_min: i32,
    x_max: i32,
    edges: std::ops::Range<u32>,
}

pub(crate) struct Outlines {
    pub countries: Vec<Country>,
    /// Per band, the countries that reach into it.
    bands: Vec<Vec<Entry>>,
    edges: Vec<Edge>,
}

/// The outlines, read on first use.
pub(crate) fn outlines() -> &'static Outlines {
    static OUTLINES: OnceLock<Outlines> = OnceLock::new();
    OUTLINES.get_or_init(|| parse(DATA).expect("the bundled country outlines are well formed"))
}

/// Reads the data file. `None` when it is malformed.
fn parse(data: &[u8]) -> Option<Outlines> {
    let mut r = Reader { data, at: 0 };
    if r.take(8)? != b"OWLCTRY1" {
        return None;
    }
    let count = usize::from(u16::from_le_bytes(r.take(2)?.try_into().ok()?));
    let mut countries = Vec::with_capacity(count);
    // (band, country, edge) for every edge in every band it reaches.
    let mut placed: Vec<(u8, u16, Edge)> = Vec::new();
    for index in 0..count {
        let country = u16::try_from(index).ok()?;
        let len = usize::from(r.take(1)?[0]);
        let name = std::str::from_utf8(r.take(len)?).ok()?.to_owned();
        // Natural Earth's label point, which nothing reads since the
        // country search went (issue 208).
        r.take(8)?;
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for _ in 0..r.varint()? {
            let n = r.varint()? as usize;
            let mut ring = Vec::with_capacity(n);
            let (mut x, mut y) = (0i32, 0i32);
            for _ in 0..n {
                x = x.checked_add(unzigzag(r.varint()?))?;
                y = y.checked_add(unzigzag(r.varint()?))?;
                ring.push((x, y));
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                let edge = Edge {
                    x1: a.0,
                    y1: a.1,
                    x2: b.0,
                    y2: b.1,
                };
                for band in band_of(a.1.min(b.1))..=band_of(a.1.max(b.1)) {
                    placed.push((u8::try_from(band).ok()?, country, edge));
                }
            }
        }
        let box_area = if x1 >= x0 {
            f64::from(x1 - x0) * f64::from(y1 - y0)
        } else {
            0.0
        };
        countries.push(Country { name, box_area });
    }
    if r.at != data.len() {
        return None;
    }

    // Group by band, then by country; the sort is stable so each ring's
    // edges keep their order.
    placed.sort_by_key(|&(band, country, _)| (band, country));
    let mut bands: Vec<Vec<Entry>> = (0..BANDS).map(|_| Vec::new()).collect();
    let mut edges = Vec::with_capacity(placed.len());
    let mut i = 0;
    while i < placed.len() {
        let (band, country, _) = placed[i];
        let start = u32::try_from(edges.len()).ok()?;
        let (mut x_min, mut x_max) = (i32::MAX, i32::MIN);
        while i < placed.len() && placed[i].0 == band && placed[i].1 == country {
            let e = placed[i].2;
            x_min = x_min.min(e.x1.min(e.x2));
            x_max = x_max.max(e.x1.max(e.x2));
            edges.push(e);
            i += 1;
        }
        bands[usize::from(band)].push(Entry {
            country,
            x_min,
            x_max,
            edges: start..u32::try_from(edges.len()).ok()?,
        });
    }
    Some(Outlines {
        countries,
        bands,
        edges,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let bytes = self.data.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(bytes)
    }

    fn varint(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.take(1)?[0];
            value |= u32::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

fn unzigzag(v: u32) -> i32 {
    // The cast reinterprets the bits, which is what zigzag decoding wants.
    #[allow(clippy::cast_possible_wrap)]
    let half = (v >> 1) as i32;
    half ^ -((v & 1) as i32)
}

/// The band holding a latitude in units, clamped to the 180 bands.
fn band_of(y: i32) -> usize {
    let band = (y + 90 * BAND).div_euclid(BAND);
    band.clamp(0, BANDS as i32 - 1) as usize
}

/// The country that contains a place, or the country whose outline is
/// within [`SEA_MARGIN_KM`] of it; `None` in open sea and for a latitude or
/// longitude that is out of range or not a number.
pub fn country_at(lat: f64, lon: f64) -> Option<&'static str> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let o = outlines();
    let (x, y) = (lon * UNIT, lat * UNIT);
    o.containing(x, y)
        .or_else(|| o.nearest(x, y))
        .map(|c| o.countries[usize::from(c)].name.as_str())
}

impl Outlines {
    /// The country whose outlines hold the point (even-odd over all of a
    /// country's rings, so holes work); the smallest box among several.
    fn containing(&self, x: f64, y: f64) -> Option<u16> {
        // A point on a band's lower edge belongs to it; the band index of a
        // fractional unit uses the unit below.
        let band = band_of(y.floor() as i32);
        let mut best: Option<u16> = None;
        for entry in &self.bands[band] {
            if x < f64::from(entry.x_min) || x > f64::from(entry.x_max) {
                continue;
            }
            let mut inside = false;
            for e in &self.edges[entry.edges.start as usize..entry.edges.end as usize] {
                let (x1, y1, x2, y2) = (
                    f64::from(e.x1),
                    f64::from(e.y1),
                    f64::from(e.x2),
                    f64::from(e.y2),
                );
                if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                    inside = !inside;
                }
            }
            if inside
                && best.is_none_or(|b| {
                    self.countries[usize::from(entry.country)].box_area
                        < self.countries[usize::from(b)].box_area
                })
            {
                best = Some(entry.country);
            }
        }
        best
    }

    /// The country with the nearest outline edge within [`SEA_MARGIN_KM`],
    /// measured on a local flat map. Looks across the antimeridian too.
    fn nearest(&self, x: f64, y: f64) -> Option<u16> {
        let lat = y / UNIT;
        let km_per_unit_y = KM_PER_DEGREE / UNIT;
        let km_per_unit_x = km_per_unit_y * lat.to_radians().cos().max(1e-6);
        let reach_y = SEA_MARGIN_KM / km_per_unit_y;
        let reach_x = (SEA_MARGIN_KM / km_per_unit_x).min(360.0 * UNIT);
        let first = band_of((y - reach_y).floor() as i32);
        let last = band_of((y + reach_y).ceil() as i32);
        let mut best: Option<(f64, u16)> = None;
        for shift in [0.0, 360.0 * UNIT, -360.0 * UNIT] {
            let px = x + shift;
            if shift != 0.0 && (px - reach_x > 180.0 * UNIT || px + reach_x < -180.0 * UNIT) {
                continue;
            }
            for band in &self.bands[first..=last] {
                for entry in band {
                    if px < f64::from(entry.x_min) - reach_x
                        || px > f64::from(entry.x_max) + reach_x
                    {
                        continue;
                    }
                    for e in &self.edges[entry.edges.start as usize..entry.edges.end as usize] {
                        let d = segment_distance(
                            ((px) * km_per_unit_x, y * km_per_unit_y),
                            (
                                f64::from(e.x1) * km_per_unit_x,
                                f64::from(e.y1) * km_per_unit_y,
                            ),
                            (
                                f64::from(e.x2) * km_per_unit_x,
                                f64::from(e.y2) * km_per_unit_y,
                            ),
                        );
                        if d <= SEA_MARGIN_KM && best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, entry.country));
                        }
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }
}

/// The distance from `p` to the segment `a`–`b`.
fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// Lower case, with the accents of Latin letters removed, so "aland" finds
/// "Åland".
pub(crate) fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'ď' => 'd',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
            'ğ' => 'g',
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
            'ł' | 'ľ' => 'l',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
            'ř' => 'r',
            'ś' | 'š' | 'ş' | 'ș' => 's',
            'ť' | 'ţ' | 'ț' => 't',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests;
