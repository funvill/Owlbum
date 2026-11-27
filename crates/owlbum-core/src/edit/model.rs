//! The edit model: `PhotoEdit`, its stored form, parsing, identity, keys
//! and the quarter-turn operations (docs/06-editing-and-file-actions.md,
//! "The edit model").
//!
//! `parse_edit` is the only way an edit enters the program, from the
//! database or from the page. Light and colour (`colour`) are part of the
//! model already; the sliders that set them come with edit mode (#46).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// Two numbers closer than this count as the same.
pub const TOLERANCE: f64 = 1e-4;

/// The largest straighten angle either way, in degrees.
pub const STRAIGHTEN_MAX: f64 = 45.0;

/// The smallest crop `parse_edit` accepts, as a fraction of the frame.
pub const CROP_MIN: f64 = 0.01;

/// The largest colour value either way.
pub const COLOUR_MAX: f64 = 100.0;

/// A crop, in fractions of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl CropRect {
    /// The whole frame.
    pub const FULL: CropRect = CropRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
}

/// Light and colour, each from -100 to 100; 0 changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct Colour {
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub warmth: f64,
}

impl Colour {
    /// No change.
    pub const NONE: Colour = Colour {
        brightness: 0.0,
        contrast: 0.0,
        saturation: 0.0,
        warmth: 0.0,
    };

    /// The four values, in the model's order.
    pub fn values(&self) -> [f64; 4] {
        [self.brightness, self.contrast, self.saturation, self.warmth]
    }

    /// True when all four are 0.
    pub fn is_none(&self) -> bool {
        self.values().iter().all(|v| *v == 0.0)
    }
}

/// One photo's edit, applied in a fixed order to the upright photo: flip,
/// quarter turn, straighten, crop, colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct PhotoEdit {
    /// 0, 90, 180 or 270, clockwise.
    pub rotate: u16,
    /// Mirror left to right, before the turn.
    pub flip: bool,
    /// Degrees clockwise, -45 to 45.
    pub straighten: f64,
    /// Fractions of the frame.
    pub crop: CropRect,
    pub colour: Colour,
}

/// The edit that changes nothing.
pub const IDENTITY: PhotoEdit = PhotoEdit {
    rotate: 0,
    flip: false,
    straighten: 0.0,
    crop: CropRect::FULL,
    colour: Colour::NONE,
};

impl Default for PhotoEdit {
    fn default() -> Self {
        IDENTITY
    }
}

/// A quarter turn or a flip, as `edit_turn` and `edit_transform` take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TurnOp {
    RotateLeft,
    RotateRight,
    Flip,
}

impl TurnOp {
    /// The undo step's label.
    pub fn label(self) -> &'static str {
        match self {
            TurnOp::RotateLeft => "rotate left",
            TurnOp::RotateRight => "rotate right",
            TurnOp::Flip => "flip",
        }
    }

    /// Applies the operation to `e`.
    pub fn apply(self, e: &PhotoEdit) -> PhotoEdit {
        match self {
            TurnOp::RotateLeft => rotate_left(e),
            TurnOp::RotateRight => rotate_right(e),
            TurnOp::Flip => flip(e),
        }
    }
}

fn finite(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// Parses, validates and clamps an edit (docs/06, "Parsing, validation and
/// clamping"). None when it is invalid.
pub fn parse_edit(json: &Value) -> Option<PhotoEdit> {
    let obj = json.as_object()?;
    let rotate = match obj.get("rotate").and_then(Value::as_u64) {
        Some(r @ (0 | 90 | 180 | 270)) => u16::try_from(r).ok()?,
        _ => return None,
    };
    // A float such as 90.0 is not one of the four values either.
    if obj.get("rotate").is_some_and(|v| !v.is_u64()) {
        return None;
    }
    let flip = match obj.get("flip") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    let straighten = match obj.get("straighten") {
        None => 0.0,
        Some(v) => finite(Some(v))?.clamp(-STRAIGHTEN_MAX, STRAIGHTEN_MAX),
    };
    let crop = match obj.get("crop") {
        None => CropRect::FULL,
        Some(Value::Object(c)) => {
            let (x, y, w, h) = (
                finite(c.get("x"))?,
                finite(c.get("y"))?,
                finite(c.get("w"))?,
                finite(c.get("h"))?,
            );
            let w = w.clamp(CROP_MIN, 1.0);
            let h = h.clamp(CROP_MIN, 1.0);
            CropRect {
                x: x.clamp(0.0, 1.0 - w),
                y: y.clamp(0.0, 1.0 - h),
                w,
                h,
            }
        }
        Some(_) => return None,
    };
    let colour = match obj.get("colour") {
        Some(Value::Object(c)) => {
            let get = |k: &str| finite(c.get(k)).map_or(0.0, |v| v.clamp(-COLOUR_MAX, COLOUR_MAX));
            Colour {
                brightness: get("brightness"),
                contrast: get("contrast"),
                saturation: get("saturation"),
                warmth: get("warmth"),
            }
        }
        _ => Colour::NONE,
    };
    Some(PhotoEdit {
        rotate,
        flip,
        straighten,
        crop,
        colour,
    })
}

/// [`parse_edit`] of a stored text; None when it is not JSON or is invalid.
pub fn parse_stored(text: &str) -> Option<PhotoEdit> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| parse_edit(&v))
}

/// Rounds to four decimal places, with no `-0`.
pub fn round4(v: f64) -> f64 {
    let r = (v * 10_000.0).round() / 10_000.0;
    if r == 0.0 { 0.0 } else { r }
}

fn number(v: f64) -> Value {
    let r = round4(v);
    // Whole numbers are written without a fraction, as the spec's example.
    if r.fract() == 0.0 && r.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        Value::from(r as i64)
    } else {
        Value::from(r)
    }
}

/// The stored form: every field, numbers rounded to four decimal places.
pub fn to_stored(e: &PhotoEdit) -> String {
    let mut crop = Map::new();
    crop.insert("x".into(), number(e.crop.x));
    crop.insert("y".into(), number(e.crop.y));
    crop.insert("w".into(), number(e.crop.w));
    crop.insert("h".into(), number(e.crop.h));
    let mut colour = Map::new();
    colour.insert("brightness".into(), number(e.colour.brightness));
    colour.insert("contrast".into(), number(e.colour.contrast));
    colour.insert("saturation".into(), number(e.colour.saturation));
    colour.insert("warmth".into(), number(e.colour.warmth));
    let mut out = Map::new();
    out.insert("rotate".into(), Value::from(e.rotate));
    out.insert("flip".into(), Value::from(e.flip));
    out.insert("straighten".into(), number(e.straighten));
    out.insert("crop".into(), Value::Object(crop));
    out.insert("colour".into(), Value::Object(colour));
    Value::Object(out).to_string()
}

/// The stored value for `e`: None (NULL) for an identity edit.
pub fn stored_value(e: &PhotoEdit) -> Option<String> {
    (!is_identity(e)).then(|| to_stored(e))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE
}

/// True when the edit changes nothing.
pub fn is_identity(e: &PhotoEdit) -> bool {
    e.rotate == 0
        && !e.flip
        && e.straighten == 0.0
        && e.colour.is_none()
        && near(e.crop.x, 0.0)
        && near(e.crop.y, 0.0)
        && near(e.crop.w, 1.0)
        && near(e.crop.h, 1.0)
}

/// True when `a` and `b` are the same edit: equal turn and flip, every
/// number within [`TOLERANCE`].
pub fn same_edit(a: &PhotoEdit, b: &PhotoEdit) -> bool {
    a.rotate == b.rotate
        && a.flip == b.flip
        && numbers(a)
            .iter()
            .zip(numbers(b).iter())
            .all(|(x, y)| near(*x, *y))
}

/// The nine numbers of an edit in key order.
fn numbers(e: &PhotoEdit) -> [f64; 9] {
    [
        e.straighten,
        e.crop.x,
        e.crop.y,
        e.crop.w,
        e.crop.h,
        e.colour.brightness,
        e.colour.contrast,
        e.colour.saturation,
        e.colour.warmth,
    ]
}

/// The 32-bit FNV-1a hash of `rotate,flip,straighten,x,y,w,h,brightness,
/// contrast,saturation,warmth` in base 36, with `flip` as 0 or 1 and each
/// other number as `round(v × 10000)`.
pub fn edit_key(e: &PhotoEdit) -> String {
    let mut text = format!("{},{}", e.rotate, u8::from(e.flip));
    for v in numbers(e) {
        #[allow(clippy::cast_possible_truncation)]
        let n = (v * 10_000.0).round() as i64;
        text.push(',');
        text.push_str(&n.to_string());
    }
    let mut hash: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    base36(hash)
}

fn base36(mut n: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Turns the edit a quarter turn anticlockwise, keeping the same part of
/// the picture cropped.
pub fn rotate_left(e: &PhotoEdit) -> PhotoEdit {
    let c = e.crop;
    PhotoEdit {
        rotate: (e.rotate + 270) % 360,
        crop: CropRect {
            x: c.y,
            y: 1.0 - c.x - c.w,
            w: c.h,
            h: c.w,
        },
        ..*e
    }
}

/// Turns the edit a quarter turn clockwise, keeping the same part of the
/// picture cropped.
pub fn rotate_right(e: &PhotoEdit) -> PhotoEdit {
    let c = e.crop;
    PhotoEdit {
        rotate: (e.rotate + 90) % 360,
        crop: CropRect {
            x: 1.0 - c.y - c.h,
            y: c.x,
            w: c.h,
            h: c.w,
        },
        ..*e
    }
}

/// Mirrors what is on screen left to right. The stored flip comes before
/// the turn, so the turn and the straighten angle reverse too.
pub fn flip(e: &PhotoEdit) -> PhotoEdit {
    let c = e.crop;
    PhotoEdit {
        flip: !e.flip,
        rotate: (360 - e.rotate) % 360,
        straighten: if e.straighten == 0.0 {
            0.0
        } else {
            -e.straighten
        },
        crop: CropRect {
            x: 1.0 - c.x - c.w,
            ..c
        },
        colour: e.colour,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn edit(v: Value) -> PhotoEdit {
        parse_edit(&v).unwrap()
    }

    #[test]
    fn the_parsing_table() {
        assert_eq!(parse_edit(&json!([1])), None, "not an object");
        assert_eq!(parse_edit(&json!({})), None, "rotate is required");
        assert_eq!(parse_edit(&json!({ "rotate": 45 })), None);
        assert_eq!(parse_edit(&json!({ "rotate": 90.0 })), None);
        assert_eq!(parse_edit(&json!({ "rotate": "90" })), None);
        assert_eq!(edit(json!({ "rotate": 0 })), IDENTITY);
        assert_eq!(parse_edit(&json!({ "rotate": 0, "flip": "yes" })), None);
        assert_eq!(parse_edit(&json!({ "rotate": 0, "straighten": "1" })), None);
        assert_eq!(
            edit(json!({ "rotate": 0, "straighten": 80 })).straighten,
            45.0
        );
        assert_eq!(
            edit(json!({ "rotate": 0, "straighten": -80 })).straighten,
            -45.0
        );
        assert_eq!(parse_edit(&json!({ "rotate": 0, "crop": 1 })), None);
        assert_eq!(
            parse_edit(&json!({ "rotate": 0, "crop": { "x": 0, "y": 0, "w": 1 } })),
            None
        );
        let c = edit(json!({ "rotate": 0, "crop": { "x": 0.5, "y": 2, "w": 0, "h": 0.6 } })).crop;
        assert_eq!(
            c,
            CropRect {
                x: 0.5,
                y: 0.4,
                w: 0.01,
                h: 0.6
            },
            "w to 0.01, then y to 1 - h"
        );
        let c = edit(json!({ "rotate": 0, "crop": { "x": 0.9, "y": 0, "w": 0.5, "h": 1 } })).crop;
        assert_eq!(c.x, 0.5);
        assert_eq!(
            edit(json!({ "rotate": 0, "colour": 3 })).colour,
            Colour::NONE
        );
        let c = edit(json!({ "rotate": 0, "colour": { "brightness": 500, "contrast": "x", "saturation": -300, "warmth": 5 } })).colour;
        assert_eq!(
            c,
            Colour {
                brightness: 100.0,
                contrast: 0.0,
                saturation: -100.0,
                warmth: 5.0
            }
        );
        assert_eq!(
            edit(json!({ "rotate": 0, "other": 1 })),
            IDENTITY,
            "dropped"
        );
    }

    #[test]
    fn the_stored_form_round_trips() {
        let e = edit(
            json!({"rotate":90,"flip":false,"straighten":-2.5,"crop":{"x":0.1,"y":0.05,"w":0.8,"h":0.9},"colour":{"brightness":10,"contrast":0,"saturation":-20,"warmth":5}}),
        );
        let text = to_stored(&e);
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({"rotate":90,"flip":false,"straighten":-2.5,"crop":{"x":0.1,"y":0.05,"w":0.8,"h":0.9},"colour":{"brightness":10,"contrast":0,"saturation":-20,"warmth":5}})
        );
        assert!(
            text.contains(r#""brightness":10,"#),
            "whole numbers as integers"
        );
        assert_eq!(parse_stored(&text), Some(e));
        let odd = PhotoEdit {
            straighten: 1.234_567,
            ..IDENTITY
        };
        assert_eq!(parse_stored(&to_stored(&odd)).unwrap().straighten, 1.2346);
        assert_eq!(stored_value(&IDENTITY), None, "identity is NULL");
        assert_eq!(parse_stored("not json"), None);
    }

    #[test]
    fn identity_and_keys() {
        let changes = [
            PhotoEdit {
                rotate: 90,
                ..IDENTITY
            },
            PhotoEdit {
                flip: true,
                ..IDENTITY
            },
            PhotoEdit {
                straighten: 0.5,
                ..IDENTITY
            },
            PhotoEdit {
                crop: CropRect {
                    x: 0.1,
                    y: 0.0,
                    w: 0.9,
                    h: 1.0,
                },
                ..IDENTITY
            },
            PhotoEdit {
                colour: Colour {
                    brightness: 1.0,
                    ..Colour::NONE
                },
                ..IDENTITY
            },
            PhotoEdit {
                colour: Colour {
                    contrast: 1.0,
                    ..Colour::NONE
                },
                ..IDENTITY
            },
            PhotoEdit {
                colour: Colour {
                    saturation: 1.0,
                    ..Colour::NONE
                },
                ..IDENTITY
            },
            PhotoEdit {
                colour: Colour {
                    warmth: 1.0,
                    ..Colour::NONE
                },
                ..IDENTITY
            },
        ];
        assert!(is_identity(&IDENTITY));
        let tiny = PhotoEdit {
            crop: CropRect {
                x: 0.000_05,
                y: 0.0,
                w: 0.999_95,
                h: 1.0,
            },
            ..IDENTITY
        };
        assert!(is_identity(&tiny), "within 1e-4");
        let mut keys = std::collections::HashSet::new();
        keys.insert(edit_key(&IDENTITY));
        for e in changes {
            assert!(!is_identity(&e), "{e:?}");
            assert!(!same_edit(&e, &IDENTITY));
            assert!(keys.insert(edit_key(&e)), "a distinct key for {e:?}");
            assert_eq!(edit_key(&e), edit_key(&e.clone()));
        }
        assert!(same_edit(&tiny, &IDENTITY));
        // FNV-1a of "0,0,0,0,0,10000,10000,0,0,0,0".
        assert_eq!(
            edit_key(&IDENTITY),
            base36(fnv("0,0,0,0,0,10000,10000,0,0,0,0"))
        );
    }

    fn fnv(s: &str) -> u32 {
        s.bytes().fold(0x811c_9dc5, |h: u32, b| {
            (h ^ u32::from(b)).wrapping_mul(16_777_619)
        })
    }

    fn sample() -> PhotoEdit {
        PhotoEdit {
            rotate: 90,
            flip: false,
            straighten: 3.5,
            crop: CropRect {
                x: 0.1,
                y: 0.2,
                w: 0.3,
                h: 0.4,
            },
            colour: Colour {
                brightness: 10.0,
                ..Colour::NONE
            },
        }
    }

    #[test]
    fn turning_four_times_or_flipping_twice_gives_the_edit_back() {
        let e = sample();
        let four = |op: fn(&PhotoEdit) -> PhotoEdit| op(&op(&op(&op(&e))));
        assert!(same_edit(&four(rotate_left), &e));
        assert!(same_edit(&four(rotate_right), &e));
        assert!(same_edit(&flip(&flip(&e)), &e));
        assert!(same_edit(&rotate_right(&rotate_left(&e)), &e));
        assert_eq!(rotate_right(&IDENTITY).rotate, 90);
        assert_eq!(rotate_left(&IDENTITY).rotate, 270);
        let f = flip(&e);
        assert_eq!((f.rotate, f.flip, f.straighten), (270, true, -3.5));
        assert_eq!(f.colour, e.colour);
        assert!(is_identity(
            &TurnOp::Flip.apply(&TurnOp::Flip.apply(&IDENTITY))
        ));
    }
}
