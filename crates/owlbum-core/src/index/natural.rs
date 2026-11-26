//! The natural comparison of file names (docs/05-organising.md, "Sorting").
//!
//! Both strings are case-folded one character at a time, then walked
//! together. A run of ASCII digits is compared as a number, whatever its
//! length; equal numbers put the run with fewer leading zeros first. Any
//! other character is compared by its code point. A string that ends first
//! comes first. So `IMG_9.jpg` sorts before `img_10.jpg`.
//!
//! **Case folding.** The spec asks for Unicode simple case folding. The
//! standard library offers lowercasing, which differs from simple folding
//! for a few characters. [`fold`] is a reduced rule: lowercasing where it
//! maps a character to exactly one character, with the known differences
//! from simple folding listed by hand (the Greek symbol forms and final
//! sigma, long s, micro sign, capital sharp s, Cherokee). A character whose
//! lowercase is several characters (such as `İ`) is kept as it is, which is
//! also what simple folding does. Any difference left is in characters file
//! names almost never hold; the order stays total and the same on every
//! platform either way, because the tie-breaks end at the id.

use std::cmp::Ordering;

/// Compares two file names (or two folder paths) in the natural order.
pub fn compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    loop {
        match (i < a.len(), j < b.len()) {
            (false, false) => return Ordering::Equal,
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            (true, true) => {}
        }
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (ra, rb) = (digit_run(a, i), digit_run(b, j));
            let order = compare_runs(&a[i..ra], &b[j..rb]);
            if order != Ordering::Equal {
                return order;
            }
            (i, j) = (ra, rb);
            continue;
        }
        let (ca, la) = next_char(a, i);
        let (cb, lb) = next_char(b, j);
        let order = fold(ca).cmp(&fold(cb));
        if order != Ordering::Equal {
            return order;
        }
        (i, j) = (i + la, j + lb);
    }
}

/// The end of the run of ASCII digits starting at `at`.
fn digit_run(s: &[u8], at: usize) -> usize {
    at + s[at..].iter().take_while(|b| b.is_ascii_digit()).count()
}

/// Two digit runs as numbers of any length; equal numbers put the run with
/// fewer leading zeros (the shorter run) first.
fn compare_runs(a: &[u8], b: &[u8]) -> Ordering {
    let za = a.iter().take_while(|&&d| d == b'0').count();
    let zb = b.iter().take_while(|&&d| d == b'0').count();
    let (na, nb) = (&a[za..], &b[zb..]);
    na.len()
        .cmp(&nb.len())
        .then_with(|| na.cmp(nb))
        .then_with(|| za.cmp(&zb))
}

/// The character starting at byte `at` of valid UTF-8, and its length.
fn next_char(s: &[u8], at: usize) -> (char, usize) {
    let len = match s[at] {
        b if b < 0x80 => 1,
        b if b >= 0xf0 => 4,
        b if b >= 0xe0 => 3,
        _ => 2,
    };
    let c = std::str::from_utf8(&s[at..at + len])
        .ok()
        .and_then(|t| t.chars().next())
        .unwrap_or(char::REPLACEMENT_CHARACTER);
    (c, len)
}

/// One character's simple case folding (see the module notes).
pub fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    match c {
        // Simple folding that lowercasing does not give (CaseFolding.txt,
        // status C or S, where it differs from the simple lowercase).
        '\u{b5}' => '\u{3bc}',               // micro sign -> mu
        '\u{17f}' => 's',                    // long s
        '\u{345}' | '\u{1fbe}' => '\u{3b9}', // iota subscript, prosgegrammeni
        '\u{3c2}' => '\u{3c3}',              // final sigma
        '\u{3d0}' => '\u{3b2}',              // beta symbol
        '\u{3d1}' => '\u{3b8}',              // theta symbol
        '\u{3d5}' => '\u{3c6}',              // phi symbol
        '\u{3d6}' => '\u{3c0}',              // pi symbol
        '\u{3f0}' => '\u{3ba}',              // kappa symbol
        '\u{3f1}' => '\u{3c1}',              // rho symbol
        '\u{3f5}' => '\u{3b5}',              // lunate epsilon
        '\u{1e9b}' => '\u{1e61}',            // long s with dot above
        '\u{1e9e}' => '\u{df}',              // capital sharp s
        // Cherokee folds to its capitals, the other way from lowercasing.
        '\u{13a0}'..='\u{13f5}' => c,
        '\u{13f8}'..='\u{13fd}' => char::from_u32(c as u32 - 8).unwrap_or(c), // Cherokee small
        '\u{ab70}'..='\u{abbf}' => char::from_u32(c as u32 - 0x97d0).unwrap_or(c), // Cherokee small
        _ => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::{Equal, Greater, Less};

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();
        v.sort_by(|a, b| compare(a, b));
        v
    }

    #[test]
    fn digits_compare_as_numbers() {
        assert_eq!(compare("IMG_9.jpg", "img_10.jpg"), Less);
        assert_eq!(compare("img_10.jpg", "IMG_9.jpg"), Greater);
        assert_eq!(
            sorted(&["a10", "a2", "a1", "a100", "a20"]),
            ["a1", "a2", "a10", "a20", "a100"]
        );
    }

    #[test]
    fn equal_numbers_put_fewer_leading_zeros_first() {
        assert_eq!(compare("a1", "a01"), Less);
        assert_eq!(compare("a001", "a01"), Greater);
        // The leading zeros decide at the run, as the spec words it, before
        // the rest of the string is looked at.
        assert_eq!(compare("a01b", "a1c"), Greater);
        assert_eq!(compare("a0", "a00"), Less);
    }

    #[test]
    fn digit_runs_of_any_length_compare_as_numbers() {
        let big = "x99999999999999999999999999999999999999";
        let bigger = "x100000000000000000000000000000000000000";
        assert_eq!(compare(big, bigger), Less);
        assert_eq!(compare(bigger, big), Greater);
        assert_eq!(compare(bigger, bigger), Equal);
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(compare("Beach.JPG", "beach.jpg"), Equal);
        assert_eq!(compare("ÉTÉ", "été"), Equal);
        assert_eq!(compare("b", "A"), Greater);
    }

    #[test]
    fn simple_folding_where_lowercase_differs() {
        assert_eq!(fold('ſ'), 's');
        assert_eq!(fold('ς'), 'σ');
        assert_eq!(fold('µ'), 'μ');
        assert_eq!(fold('ẞ'), 'ß');
        assert_eq!(compare("ſun", "SUN"), Equal);
        assert_eq!(compare("ΟΔΟΣ", "οδος"), Equal);
        // A character whose lowercase is two characters is kept.
        assert_eq!(fold('İ'), 'İ');
    }

    #[test]
    fn other_characters_compare_by_code_point() {
        assert_eq!(compare("a-b", "a_b"), Less, "'-' is 0x2d, '_' is 0x5f");
        assert_eq!(compare("a b", "a0"), Less);
        assert_eq!(compare("z", "é"), Less);
    }

    #[test]
    fn a_string_that_ends_first_comes_first() {
        assert_eq!(compare("abc", "abcd"), Less);
        assert_eq!(compare("abc1", "abc"), Greater);
        assert_eq!(compare("", ""), Equal);
        assert_eq!(compare("", "a"), Less);
    }

    #[test]
    fn digits_against_letters_compare_by_code_point() {
        // '1' (0x31) is below 'a' (0x61).
        assert_eq!(compare("1", "a"), Less);
        assert_eq!(compare("img1", "imga"), Less);
    }

    #[test]
    fn the_order_is_total_and_consistent() {
        let names = [
            "IMG_0001.jpg",
            "img_1.jpg",
            "IMG_1.JPG",
            "img_01.jpg",
            "img_10.jpg",
            "Img_2.jpg",
            "a",
            "",
            "z9",
            "z09",
        ];
        for a in names {
            for b in names {
                assert_eq!(compare(a, b), compare(b, a).reverse(), "{a} {b}");
            }
        }
        assert_eq!(
            sorted(&names),
            [
                "",
                "a",
                "img_1.jpg",
                "IMG_1.JPG",
                "img_01.jpg",
                "IMG_0001.jpg",
                "Img_2.jpg",
                "img_10.jpg",
                "z9",
                "z09"
            ]
        );
    }
}
