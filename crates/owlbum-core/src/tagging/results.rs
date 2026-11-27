//! How results are derived from a model's raw output (docs/07-tagging.md,
//! "How results are derived"): caption cleaning, object counting, keyword
//! scoring and vector checks. Pure functions, shared by every backend.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The most keywords a photo keeps.
pub const MAX_KEYWORDS: usize = 12;
/// CLIP's logit scale, for the keyword softmax.
pub const LOGIT_SCALE: f32 = 100.0;

/// An object label and how many times it was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ObjectCount {
    pub label: String,
    pub count: u32,
}

/// A keyword and its share of the softmax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordScore {
    pub label: String,
    pub score: f32,
}

/// What a describe run gives for one picture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Described {
    /// The cleaned caption; None when it came out empty.
    pub description: Option<String>,
    pub objects: Vec<ObjectCount>,
}

/// What a keywords run gives for one picture.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Keyworded {
    pub keywords: Vec<KeywordScore>,
    /// Unit length, the model's `dim` numbers.
    pub vector: Vec<f32>,
}

/// Removes a `<…>` tag wherever one appears.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        match rest[start..].find('>') {
            Some(end) => {
                out.push_str(&rest[..start]);
                rest = &rest[start + end + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// A Florence-2 caption made fit to store: special tokens and every other
/// tag removed, white space trimmed. None when nothing is left.
pub fn clean_caption(raw: &str) -> Option<String> {
    let text = strip_tags(raw);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// The labels of a Florence-2 `<OD>` answer, counted: a label before each
/// run of four `<loc_N>` tokens (a run with no text repeats the previous
/// label), lower case and trimmed, sorted by count descending then label.
pub fn count_objects(raw: &str) -> Vec<ObjectCount> {
    let text = raw
        .replace("<s>", "")
        .replace("</s>", "")
        .replace("<pad>", "");
    let mut labels: Vec<String> = Vec::new();
    let mut pending = String::new();
    let mut locs = 0;
    let mut rest = text.as_str();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<loc_")
            && let Some(end) = after.find('>')
            && !after[..end].is_empty()
            && after[..end].bytes().all(|b| b.is_ascii_digit())
        {
            locs += 1;
            rest = &after[end + 1..];
            if locs == 4 {
                let label = if pending.trim().is_empty() {
                    labels.last().cloned().unwrap_or_default()
                } else {
                    pending.clone()
                };
                labels.push(label);
                pending.clear();
                locs = 0;
            }
            continue;
        }
        if locs > 0 {
            // An unfinished run of locations does not make a match.
            locs = 0;
            pending.clear();
        }
        let ch = rest.chars().next().unwrap_or(' ');
        if ch == '<' {
            pending.clear();
        } else {
            pending.push(ch);
        }
        rest = &rest[ch.len_utf8()..];
    }
    let mut counts: std::collections::BTreeMap<String, u32> = Default::default();
    for l in labels {
        let l = l.trim().to_lowercase();
        if !l.is_empty() {
            *counts.entry(l).or_default() += 1;
        }
    }
    let mut out: Vec<ObjectCount> = counts
        .into_iter()
        .map(|(label, count)| ObjectCount { label, count })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    out
}

/// `x` scaled to unit length; a zero length is treated as 1.
pub fn unit(mut x: Vec<f32>) -> Vec<f32> {
    let len = x.iter().map(|v| v * v).sum::<f32>().sqrt();
    let len = if len == 0.0 { 1.0 } else { len };
    for v in &mut x {
        *v /= len;
    }
    x
}

/// The dot product of two vectors of the same length.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The keywords of a picture whose unit vector is `v`, against the unit
/// vectors of the vocabulary's prompts (`vocab[i]` for `words[i]`).
pub fn keywords(
    v: &[f32],
    vocab: &[Vec<f32>],
    words: &[&str],
    threshold: f32,
) -> Vec<KeywordScore> {
    if vocab.is_empty() {
        return Vec::new();
    }
    let sims: Vec<f32> = vocab.iter().map(|w| dot(v, w)).collect();
    let top = sims.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f64> = sims
        .iter()
        .map(|s| f64::from(LOGIT_SCALE * (s - top)).exp())
        .collect();
    let sum: f64 = exps.iter().sum();
    let mut out: Vec<(usize, f32)> = exps
        .iter()
        .enumerate()
        .map(|(i, e)| (i, (e / sum) as f32))
        .filter(|(_, p)| *p >= threshold)
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(MAX_KEYWORDS);
    out.into_iter()
        .map(|(i, score)| KeywordScore {
            label: words[i].to_owned(),
            score,
        })
        .collect()
}

/// True when a vector may be stored for a model of `dim`: the right length,
/// every number finite, and not of zero length.
pub fn usable_vector(v: &[f32], dim: u32) -> bool {
    v.len() == dim as usize && v.iter().all(|x| x.is_finite()) && v.iter().any(|x| *x != 0.0)
}

/// A vector as stored: little-endian float32.
pub fn vector_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A stored vector read back; None when its length is not `dim × 4`.
pub fn vector_from_bytes(bytes: &[u8], dim: usize) -> Option<Vec<f32>> {
    (bytes.len() == dim * 4).then(|| {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_lose_every_tag() {
        assert_eq!(
            clean_caption("</s><s>A dog <pad> on a <loc_12>beach. </s>").as_deref(),
            Some("A dog on a beach.")
        );
        assert_eq!(clean_caption("<s></s>  "), None);
        assert_eq!(clean_caption("a < b").as_deref(), Some("a < b"));
    }

    #[test]
    fn objects_are_counted_and_ordered() {
        let raw = "</s><s>dog<loc_1><loc_2><loc_3><loc_4>Person<loc_5><loc_6><loc_7><loc_8>\
                   <loc_9><loc_10><loc_11><loc_12>car<loc_1><loc_2><loc_3><loc_4></s>";
        assert_eq!(
            count_objects(raw),
            vec![
                ObjectCount {
                    label: "person".into(),
                    count: 2
                },
                ObjectCount {
                    label: "car".into(),
                    count: 1
                },
                ObjectCount {
                    label: "dog".into(),
                    count: 1
                },
            ]
        );
        assert!(count_objects("<s>nothing here</s>").is_empty());
        assert!(count_objects("cat<loc_1><loc_2>").is_empty());
    }

    #[test]
    fn keywords_follow_the_softmax_threshold_order_and_cap() {
        let words: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        // Twenty prompts the picture matches equally well, so each has 1/20.
        let vocab: Vec<Vec<f32>> = (0..20).map(|_| vec![1.0, 0.0]).collect();
        let v = [1.0, 0.0];
        let k = keywords(&v, &vocab, &words, 0.04);
        assert_eq!(k.len(), MAX_KEYWORDS);
        assert_eq!(k[0].label, "w0");
        assert_eq!(k[11].label, "w11");
        assert!((k[0].score - 0.05).abs() < 1e-6);
        // Above the threshold nothing passes.
        assert!(keywords(&v, &vocab, &words, 0.06).is_empty());
        // One clear winner takes nearly all of the softmax.
        let vocab = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.6, 0.8]];
        let k = keywords(&v, &vocab, &words[..3], 0.05);
        assert_eq!(k.len(), 1);
        assert_eq!(k[0].label, "w0");
        assert!(k[0].score > 0.99);
    }

    #[test]
    fn vectors_are_checked_and_round_trip() {
        assert!(usable_vector(&[0.6, 0.8], 2));
        assert!(!usable_vector(&[0.6, 0.8], 3));
        assert!(!usable_vector(&[0.0, 0.0], 2));
        assert!(!usable_vector(&[f32::NAN, 1.0], 2));
        let v = unit(vec![3.0, 4.0]);
        assert_eq!(v, vec![0.6, 0.8]);
        assert_eq!(vector_from_bytes(&vector_bytes(&v), 2), Some(v));
        assert_eq!(vector_from_bytes(&[0; 7], 2), None);
        assert_eq!(unit(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }
}
