//! Words of the photo score's breakdown (docs/05-organising.md, "Lines").

pub const FOCUS: &str = "Focus";
pub const EXPOSURE: &str = "Exposure";
pub const CLIPPING: &str = "Clipping";
pub const VARIETY: &str = "Colour variety";
pub const COMPOSITION: &str = "Composition";
pub const PEOPLE: &str = "People";
pub const SIZE: &str = "Size";
pub const BURST: &str = "Burst";
pub const STAR: &str = "Star";
pub const HIDDEN: &str = "Hidden";
pub const NUDGES: &str = "Your nudges";

pub const DARK_SIDE: &str = "on the dark side";
pub const BRIGHT_SIDE: &str = "on the bright side";
pub const WELL_EXPOSED: &str = "well exposed";
pub const CLIPPED: &str = "clipped highlights or shadows";
pub const MANY_COLOURS: &str = "many colours across the frame";
pub const SOME_COLOUR: &str = "some colour across the frame";
pub const NEAR_A_THIRD: &str = "subject near a third, not dead centre";
pub const BETTER_IN_BURST: &str = "a similar shot in this burst is better";
pub const STARRED: &str = "starred";
pub const HIDDEN_WHY: &str = "hidden";

/// What was being done, for an `internal` error while nudging.
pub const NUDGING: &str = "nudging the score";

/// "sharper than 80 % of this album".
pub fn sharper_than(percent: i64) -> String {
    format!("sharper than {percent} % of this album")
}

/// "4 % blown to white".
pub fn blown(percent: i64) -> String {
    format!("{percent} % blown to white")
}

/// "3 % crushed to black".
pub fn crushed(percent: i64) -> String {
    format!("{percent} % crushed to black")
}

/// "1 person tagged", "2 people tagged".
pub fn people(n: u16) -> String {
    if n == 1 {
        "1 person tagged".to_owned()
    } else {
        format!("{n} people tagged")
    }
}

/// "8.3 megapixels" below 10; "12 megapixels" at 10 and above.
pub fn megapixels(mp: f64) -> String {
    if mp < 10.0 {
        format!("{mp:.1} megapixels")
    } else {
        format!("{} megapixels", mp.round())
    }
}

/// "Up pressed 2 times more", "Down pressed 1 time more".
pub fn nudged(adjust: i32) -> String {
    let way = if adjust > 0 { "Up" } else { "Down" };
    let n = adjust.unsigned_abs();
    let times = if n == 1 { "time" } else { "times" };
    format!("{way} pressed {n} {times} more")
}
