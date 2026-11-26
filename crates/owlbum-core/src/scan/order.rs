//! Walker order: `natural_cmp` (docs/03-media-and-scanning.md, "Order").
//!
//! One comparison for the walker's output and the name order of the query
//! engine. It never looks at the computer's language settings.

use std::cmp::Ordering;

/// Compares two relative paths (`/` separators) in walker order.
///
/// 1. One component at a time; a path that runs out of components first
///    sorts first.
/// 2. Within a component, runs of ASCII digits and runs of everything else.
/// 3. Two digit runs compare by number, then fewer leading zeros first.
/// 4. Two text runs compare by code point after simple lower-casing.
/// 5. A digit run sorts before a text run.
/// 6. Everything equal: the original bytes.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.split('/');
    let mut right = b.split('/');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return a.as_bytes().cmp(b.as_bytes()),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match runs_cmp(x, y) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
}

/// Steps 2 to 5 for one name (a path component or a file name), with step 6
/// (the bytes) as the last word.
pub fn natural_cmp_name(a: &str, b: &str) -> Ordering {
    match runs_cmp(a, b) {
        Ordering::Equal => a.as_bytes().cmp(b.as_bytes()),
        other => other,
    }
}

/// Steps 2 to 5 only, with no byte tie-break: the string rules the album
/// index's name order applies to a file name and then to a folder
/// (docs/05-organising.md, "Sorting"), before it compares ids. Names that
/// differ only in case answer `Equal`.
pub fn natural_cmp_text(a: &str, b: &str) -> Ordering {
    runs_cmp(a, b)
}

/// Steps 2 to 5 only: names that differ only in case answer `Equal` here.
fn runs_cmp(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        ascii_runs_cmp(a.as_bytes(), b.as_bytes())
    } else {
        unicode_runs_cmp(a, b)
    }
}

/// [`runs_cmp`] for two ASCII names, on bytes. For ASCII, simple
/// lower-casing is `to_ascii_lowercase`, so this answers what
/// [`unicode_runs_cmp`] answers, without its per-character iterators: it is
/// the comparison the album index's name sort makes about 1.7 million times
/// for 100,000 photos (#124).
fn ascii_runs_cmp(a: &[u8], b: &[u8]) -> Ordering {
    /// The end of the run that starts at `from`.
    fn run_end(s: &[u8], from: usize) -> usize {
        let digits = s[from].is_ascii_digit();
        s[from..]
            .iter()
            .position(|c| c.is_ascii_digit() != digits)
            .map_or(s.len(), |n| from + n)
    }
    let (mut i, mut j) = (0, 0);
    loop {
        match (i < a.len(), j < b.len()) {
            (false, false) => return Ordering::Equal,
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            (true, true) => {}
        }
        let (p_end, q_end) = (run_end(a, i), run_end(b, j));
        let (p, q) = (&a[i..p_end], &b[j..q_end]);
        let order = match (p[0].is_ascii_digit(), q[0].is_ascii_digit()) {
            (true, true) => {
                let zeros = |s: &[u8]| s.iter().take_while(|&&c| c == b'0').count();
                let (pv, qv) = (&p[zeros(p)..], &q[zeros(q)..]);
                // By number, then fewer leading zeros first.
                pv.len()
                    .cmp(&qv.len())
                    .then_with(|| pv.cmp(qv))
                    .then_with(|| p.len().cmp(&q.len()))
            }
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => p
                .iter()
                .map(u8::to_ascii_lowercase)
                .cmp(q.iter().map(u8::to_ascii_lowercase)),
        };
        if order != Ordering::Equal {
            return order;
        }
        (i, j) = (p_end, q_end);
    }
}

/// [`runs_cmp`] for any names.
fn unicode_runs_cmp(a: &str, b: &str) -> Ordering {
    let mut x = Runs::new(a);
    let mut y = Runs::new(b);
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(Run::Digits(p)), Some(Run::Digits(q))) => {
                let (pv, qv) = (p.trim_start_matches('0'), q.trim_start_matches('0'));
                // By number, then fewer leading zeros first.
                let order = pv
                    .len()
                    .cmp(&qv.len())
                    .then_with(|| pv.cmp(qv))
                    .then_with(|| p.len().cmp(&q.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(Run::Digits(_)), Some(Run::Text(_))) => return Ordering::Less,
            (Some(Run::Text(_)), Some(Run::Digits(_))) => return Ordering::Greater,
            (Some(Run::Text(p)), Some(Run::Text(q))) => {
                let by_text = p
                    .chars()
                    .flat_map(char::to_lowercase)
                    .cmp(q.chars().flat_map(char::to_lowercase));
                if by_text != Ordering::Equal {
                    return by_text;
                }
            }
        }
    }
}

enum Run<'a> {
    Digits(&'a str),
    Text(&'a str),
}

struct Runs<'a> {
    rest: &'a str,
}

impl<'a> Runs<'a> {
    fn new(s: &'a str) -> Self {
        Self { rest: s }
    }
}

impl<'a> Iterator for Runs<'a> {
    type Item = Run<'a>;

    fn next(&mut self) -> Option<Run<'a>> {
        let first = self.rest.chars().next()?;
        let digits = first.is_ascii_digit();
        let end = self
            .rest
            .char_indices()
            .find(|(_, c)| c.is_ascii_digit() != digits)
            .map_or(self.rest.len(), |(i, _)| i);
        let (run, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(if digits {
            Run::Digits(run)
        } else {
            Run::Text(run)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(vec!["IMG_10.jpg", "IMG_2.jpg", "IMG_1.jpg"]),
            ["IMG_1.jpg", "IMG_2.jpg", "IMG_10.jpg"]
        );
    }

    #[test]
    fn components_compare_one_at_a_time() {
        // `a/` is compared with `a b/` first, and `a` is shorter.
        assert_eq!(natural_cmp("a/z.jpg", "a b/a.jpg"), Ordering::Less);
        // A path that runs out of components first sorts first.
        assert_eq!(natural_cmp("a/b", "a/b/c"), Ordering::Less);
        assert_eq!(sorted(vec!["b.jpg", "a/c.jpg"]), ["a/c.jpg", "b.jpg"]);
    }

    #[test]
    fn equal_numbers_put_fewer_leading_zeros_first() {
        assert_eq!(natural_cmp("x01", "x1"), Ordering::Greater);
        assert_eq!(natural_cmp("x1", "x001"), Ordering::Less);
        // The zeros decide at once, before a later difference.
        assert_eq!(natural_cmp("x01b", "x1c"), Ordering::Greater);
        assert_eq!(natural_cmp("0", "00"), Ordering::Less);
    }

    #[test]
    fn digits_sort_before_text() {
        assert_eq!(natural_cmp("1", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "aa"), Ordering::Less);
    }

    #[test]
    fn text_ignores_case_then_the_bytes_decide() {
        assert_eq!(natural_cmp("B.jpg", "a.jpg"), Ordering::Greater);
        assert_eq!(natural_cmp("A.jpg", "a.jpg"), Ordering::Less, "bytes");
        assert_eq!(natural_cmp("a.jpg", "a.jpg"), Ordering::Equal);
        // The bytes decide only when the whole paths are otherwise equal.
        assert_eq!(natural_cmp("A/3.jpg", "a/2.jpg"), Ordering::Greater);
        assert_eq!(natural_cmp_name("A", "a"), Ordering::Less);
        // Code points after lower-casing, not any locale's alphabet.
        assert_eq!(natural_cmp("é", "z"), Ordering::Greater);
        assert_eq!(natural_cmp("Ébc", "éa"), Ordering::Greater);
    }

    #[test]
    fn long_numbers_do_not_overflow() {
        let big = "9".repeat(40);
        let bigger = format!("1{}", "0".repeat(40));
        assert_eq!(natural_cmp(&big, &bigger), Ordering::Less);
    }

    #[test]
    fn the_order_is_total_on_a_mixed_list() {
        let list = vec![
            "z.jpg",
            "a/2.jpg",
            "a/10.jpg",
            "a b/1.jpg",
            "A/3.jpg",
            "a/02.jpg",
            "a/2.JPG",
            "10",
            "9",
            "_x",
            "é",
        ];
        let s = sorted(list.clone());
        for w in s.windows(2) {
            assert_ne!(natural_cmp(w[0], w[1]), Ordering::Greater, "{w:?}");
        }
        assert_eq!(s.len(), list.len());
    }

    #[test]
    fn the_ascii_path_answers_what_the_general_path_answers() {
        // Short names from an alphabet chosen for the edges: case, leading
        // zeros, digit and text runs side by side, punctuation that sorts
        // between the cases.
        const ALPHABET: &[u8] = b"0019aAzZ_.- [`";
        let mut seed: u64 = 0x5eed;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as usize
        };
        let mut names: Vec<String> = vec![String::new(), "0".into(), "00".into()];
        for _ in 0..400 {
            let len = next() % 7;
            names.push(
                (0..len)
                    .map(|_| char::from(ALPHABET[next() % ALPHABET.len()]))
                    .collect(),
            );
        }
        for a in &names {
            for b in &names {
                assert_eq!(
                    ascii_runs_cmp(a.as_bytes(), b.as_bytes()),
                    unicode_runs_cmp(a, b),
                    "{a:?} against {b:?}"
                );
            }
        }
        // A name that is not ASCII takes the general path.
        assert_eq!(runs_cmp("IMG_1", "ÉMG_1"), Ordering::Less);
        assert_eq!(runs_cmp("émg_2", "ÉMG_10"), Ordering::Less);
    }
}
