//! Small ICC v2 display profiles for the colour tests: Display P3, Adobe
//! RGB, sRGB and a grey gamma profile. Built by hand from published
//! primaries so no profile file is committed, and so tests can work out
//! the expected colours from the same numbers without trusting the colour
//! library they test.

/// The D50 white point every profile here uses (s15Fixed16).
pub const D50: [i32; 3] = [63190, 65536, 54061];

/// A tone curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trc {
    /// `y = x^g`, the gamma as u8Fixed8 (563 is 2.2, 461 is 1.8).
    Gamma(u16),
    /// The sRGB curve, written as an ICC parametric curve of type 3.
    Srgb,
}

impl Trc {
    /// The curve at `x` in 0..=1, from the numbers written in the profile.
    pub fn eval(self, x: f64) -> f64 {
        match self {
            Self::Gamma(g) => x.powf(f64::from(g) / 256.0),
            Self::Srgb => {
                let [g, a, b, c, d] = SRGB_PARA.map(|v| f64::from(v) / 65536.0);
                if x >= d { (a * x + b).powf(g) } else { c * x }
            }
        }
    }
}

/// The sRGB curve's parameters g, a, b, c, d as s15Fixed16.
const SRGB_PARA: [i32; 5] = [157286, 62119, 3417, 5072, 2651];

/// An RGB matrix-shaper profile: three colorants adapted to D50 (columns
/// of the RGB to XYZ matrix, as s15Fixed16) and one curve for all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbProfile {
    /// The `desc` text.
    pub name: &'static str,
    /// The red, green and blue colorants (`rXYZ`, `gXYZ`, `bXYZ`).
    pub colorants: [[i32; 3]; 3],
    /// The curve of every channel.
    pub trc: Trc,
}

/// Display P3 primaries with a 2.2 gamma, as the wide-gamut fixture has
/// always used.
pub const DISPLAY_P3_GAMMA_22: RgbProfile = RgbProfile {
    name: "Owlbum test Display P3",
    colorants: [
        [33758, 15807, -72],
        [19137, 45364, 2746],
        [10296, 4365, 51387],
    ],
    trc: Trc::Gamma(563),
};

/// Display P3 as Apple defines it: P3 primaries, D65 white, the sRGB curve.
pub const DISPLAY_P3: RgbProfile = RgbProfile {
    name: "Owlbum test Display P3 (sRGB curve)",
    trc: Trc::Srgb,
    ..DISPLAY_P3_GAMMA_22
};

/// Adobe RGB (1998): its primaries with a gamma of 563/256.
pub const ADOBE_RGB: RgbProfile = RgbProfile {
    name: "Owlbum test Adobe RGB",
    colorants: [
        [39960, 20389, 1276],
        [13453, 41004, 3989],
        [9777, 4143, 48795],
    ],
    trc: Trc::Gamma(563),
};

/// sRGB IEC 61966-2.1.
pub const SRGB: RgbProfile = RgbProfile {
    name: "Owlbum test sRGB",
    colorants: [
        [28578, 14581, 912],
        [25241, 46981, 6362],
        [9376, 3972, 46799],
    ],
    trc: Trc::Srgb,
};

impl RgbProfile {
    /// The profile's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        let trc = match self.trc {
            Trc::Gamma(g) => curve(g),
            Trc::Srgb => para(SRGB_PARA),
        };
        let [r, g, b] = self.colorants;
        let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"desc", desc(self.name)),
            (b"cprt", text("No copyright, use freely")),
            (b"wtpt", xyz(D50)),
            (b"rXYZ", xyz(r)),
            (b"gXYZ", xyz(g)),
            (b"bXYZ", xyz(b)),
            (b"rTRC", trc.clone()),
            (b"gTRC", trc.clone()),
            (b"bTRC", trc),
        ];
        profile(b"RGB ", &tags)
    }

    /// The RGB to XYZ (D50) matrix, rows X, Y, Z.
    pub fn matrix(&self) -> [[f64; 3]; 3] {
        let c = self.colorants.map(|v| v.map(|n| f64::from(n) / 65536.0));
        [0, 1, 2].map(|row| [c[0][row], c[1][row], c[2][row]])
    }

    /// The sRGB value, 0.0 to 255.0 per channel, of the 8-bit colour `c`
    /// in this profile: relative colorimetric through both profiles'
    /// matrices and curves, clipped to the sRGB gamut. Worked out here
    /// from the profile's own numbers, so a colour library can be checked
    /// against it.
    pub fn expected_srgb(&self, c: [u8; 3]) -> [f64; 3] {
        let linear = c.map(|v| self.trc.eval(f64::from(v) / 255.0));
        let xyz = mul(self.matrix(), linear);
        mul(inverse(SRGB.matrix()), xyz).map(|l| srgb_encode(l) * 255.0)
    }
}

/// The sRGB value, 0.0 to 255.0, of the grey level `v` in [`grey`]`(g)`.
pub fn expected_grey_srgb(gamma_x256: u16, v: u8) -> f64 {
    srgb_encode(Trc::Gamma(gamma_x256).eval(f64::from(v) / 255.0)) * 255.0
}

/// Linear light to the sRGB curve, clipped to 0..=1.
fn srgb_encode(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

fn mul(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}

fn inverse(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [e * i - f * h, c * h - b * i, b * f - c * e],
        [f * g - d * i, a * i - c * g, c * d - a * f],
        [d * h - e * g, b * g - a * h, a * e - b * d],
    ]
    .map(|row| row.map(|v| v / det))
}

/// Display P3 primaries adapted to D50 (as s15Fixed16), with a 2.2 gamma.
pub fn display_p3() -> Vec<u8> {
    DISPLAY_P3_GAMMA_22.bytes()
}

/// A grey profile whose curve is `y = x^g`, the gamma as u8Fixed8.
pub fn grey(gamma_x256: u16) -> Vec<u8> {
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc("Owlbum test grey")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz(D50)),
        (b"kTRC", curve(gamma_x256)),
    ];
    profile(b"GRAY", &tags)
}

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn xyz(v: [i32; 3]) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for c in v {
        t.extend_from_slice(&c.to_be_bytes());
    }
    t
}

fn desc(text: &str) -> Vec<u8> {
    let mut t = b"desc\0\0\0\0".to_vec();
    be32(&mut t, u32::try_from(text.len() + 1).expect("text"));
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    t.extend_from_slice(&[0; 4 + 4 + 2 + 1 + 67]); // no Unicode, no ScriptCode
    t
}

fn text(text: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend_from_slice(text.as_bytes());
    t.push(0);
    t
}

/// A gamma curve; `gamma_x256` is the gamma as u8Fixed8 (563 is 2.2).
fn curve(gamma_x256: u16) -> Vec<u8> {
    let mut t = b"curv\0\0\0\0".to_vec();
    be32(&mut t, 1);
    t.extend_from_slice(&gamma_x256.to_be_bytes());
    t
}

/// A parametric curve of type 3: `(a·x + b)^g` from `d` on, `c·x` below.
fn para(params: [i32; 5]) -> Vec<u8> {
    let mut t = b"para\0\0\0\0".to_vec();
    t.extend_from_slice(&3u16.to_be_bytes());
    t.extend_from_slice(&[0, 0]);
    for p in params {
        t.extend_from_slice(&p.to_be_bytes());
    }
    t
}

/// A display profile of colour space `space` holding `tags`.
fn profile(space: &[u8; 4], tags: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let table_len = 4 + 12 * tags.len();
    let mut data = Vec::new();
    let mut table = Vec::new();
    be32(&mut table, u32::try_from(tags.len()).expect("tags"));
    for (sig, bytes) in tags {
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let offset = 128 + table_len + data.len();
        table.extend_from_slice(*sig);
        be32(&mut table, u32::try_from(offset).expect("offset"));
        be32(&mut table, u32::try_from(bytes.len()).expect("size"));
        data.extend_from_slice(bytes);
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }

    let size = 128 + table_len + data.len();
    let mut out = Vec::with_capacity(size);
    be32(&mut out, u32::try_from(size).expect("size"));
    be32(&mut out, 0); // preferred CMM
    be32(&mut out, 0x0210_0000); // version 2.1
    out.extend_from_slice(b"mntr");
    out.extend_from_slice(space);
    out.extend_from_slice(b"XYZ ");
    for v in [2025u16, 1, 1, 0, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(b"acsp");
    out.extend_from_slice(&[0; 4 + 4 + 4 + 4 + 8]); // platform, flags, maker, model, attributes
    be32(&mut out, 0); // perceptual intent
    for c in D50 {
        out.extend_from_slice(&c.to_be_bytes());
    }
    out.extend_from_slice(&[0; 4 + 44]); // creator, ID and reserved
    assert_eq!(out.len(), 128);
    out.extend_from_slice(&table);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_sound(p: &[u8], class: &[u8; 8]) {
        assert_eq!(p.len() % 4, 0);
        assert_eq!(
            u32::from_be_bytes(p[..4].try_into().unwrap()) as usize,
            p.len()
        );
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(&p[12..20], class);
    }

    #[test]
    fn headers_are_sound() {
        assert_sound(&display_p3(), b"mntrRGB ");
        for p in [DISPLAY_P3, ADOBE_RGB, SRGB] {
            assert_sound(&p.bytes(), b"mntrRGB ");
        }
        assert_sound(&grey(461), b"mntrGRAY");
    }

    #[test]
    fn colorants_add_up_to_the_white_point() {
        for p in [DISPLAY_P3, ADOBE_RGB, SRGB] {
            let m = p.matrix();
            for (row, white) in m.iter().zip(D50) {
                let sum: f64 = row.iter().sum();
                assert!(
                    (sum - f64::from(white) / 65536.0).abs() < 2e-4,
                    "{}",
                    p.name
                );
            }
        }
    }

    #[test]
    fn srgb_to_srgb_is_the_identity() {
        for c in [[0, 0, 0], [255, 255, 255], [200, 100, 50], [3, 128, 250]] {
            let got = SRGB.expected_srgb(c);
            for (g, w) in got.iter().zip(c) {
                assert!((g - f64::from(w)).abs() < 0.05, "{c:?} gave {got:?}");
            }
        }
        assert!((expected_grey_srgb(563, 255) - 255.0).abs() < 1e-9);
    }

    #[test]
    fn srgb_curve_matches_the_standard() {
        for x in [0.0, 0.02, 0.04045, 0.2, 0.5, 1.0] {
            let want: f64 = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
            assert!((Trc::Srgb.eval(x) - want).abs() < 1e-4, "{x}");
        }
    }
}
