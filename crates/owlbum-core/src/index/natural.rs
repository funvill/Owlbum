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
//! for a few characters. [`fold`] is lowercasing where it maps a character
//! to exactly one character, with the characters whose simple folding
//! differs from that listed by hand (from Unicode's `CaseFolding.txt`, status
//! `C` and `S`, for the scripts file names are likely to use). A character
//! whose lowercase is several characters (such as `İ`) is kept as it is,
//! which is also what simple folding does.

use std::cmp::Ordering;

/// Compares two file names (or two folder paths) in the natural order.
pub fn compare(a: &str, b: &str) -> Ordering {
    unimplemented!("{a}{b}")
}

/// One character's simple case folding (see the module notes).
pub fn fold(c: char) -> char {
    unimplemented!("{c}")
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
