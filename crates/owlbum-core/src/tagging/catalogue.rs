//! The model catalogue (docs/07-tagging.md, "The model catalogue"): a table
//! compiled into Owlbum, never fetched. An id is never reused for different
//! files, because rows in `tags` and `vectors` are keyed by it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A tagging job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// "Descriptions and objects".
    Describe,
    /// "Keywords and search by meaning".
    Keywords,
}

impl Part {
    /// Both parts, in the order they run and are listed.
    pub const ALL: [Part; 2] = [Part::Describe, Part::Keywords];

    /// The id stored in `tags.part`.
    pub fn as_str(self) -> &'static str {
        match self {
            Part::Describe => "describe",
            Part::Keywords => "keywords",
        }
    }

    /// The part named `text`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == text)
    }

    /// Its index in [`Part::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Which pipeline runs a model's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Florence2,
    Clip,
    /// The fake catalogue's models, run by the fake backend in tests.
    Fake,
}

/// Smaller or larger, as Settings shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Smaller,
    Larger,
}

/// A licence a catalogue model may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Licence {
    Mit,
    Apache2,
    Bsd3Clause,
}

impl Licence {
    /// The name Settings shows.
    pub fn as_str(self) -> &'static str {
        match self {
            Licence::Mit => "MIT",
            Licence::Apache2 => "Apache-2.0",
            Licence::Bsd3Clause => "BSD-3-Clause",
        }
    }
}

/// One file of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Inside the repository, and inside `<app data>/models/<id>/`.
    pub path: &'static str,
    pub bytes: u64,
    /// 64 lower-case hex characters.
    pub sha256: &'static str,
}

/// One model of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelChoice {
    /// `[a-z0-9-]+`.
    pub id: &'static str,
    pub part: Part,
    pub family: Family,
    pub size: Size,
    pub label: &'static str,
    /// The Hugging Face repository.
    pub repo: &'static str,
    /// A pinned commit: 40 hex characters.
    pub revision: &'static str,
    pub files: &'static [ModelFile],
    pub licence: Licence,
    /// Typical, on the processor; shown in Settings.
    pub seconds_per_photo: f32,
    /// Keywords models: the length of the vector.
    pub dim: Option<u32>,
    /// Keywords models: the smallest softmax share for a keyword.
    pub keyword_threshold: Option<f32>,
    /// Keywords models: the smallest cosine for search by meaning.
    pub meaning_threshold: Option<f32>,
}

impl ModelChoice {
    /// The sum of its files' sizes.
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

/// A list of models: the real catalogue, or the fake one in tests.
#[derive(Debug, Clone, Copy)]
pub struct Catalogue(pub &'static [ModelChoice]);

impl Catalogue {
    /// The catalogue compiled into Owlbum.
    pub fn real() -> Self {
        Catalogue(MODELS)
    }

    /// The model with this id.
    pub fn get(&self, id: &str) -> Option<&'static ModelChoice> {
        self.0.iter().find(|m| m.id == id)
    }

    /// A part's models, smaller first.
    pub fn choices_for(&self, part: Part) -> Vec<&'static ModelChoice> {
        let mut v: Vec<_> = self.0.iter().filter(|m| m.part == part).collect();
        v.sort_by_key(|m| m.size as u8);
        v
    }

    /// All models.
    pub fn all(&self) -> &'static [ModelChoice] {
        self.0
    }
}

/// The four models of version 1. The speeds were measured with this
/// runtime by the prototype (#52).
pub static MODELS: &[ModelChoice] = &[
    ModelChoice {
        id: "florence2-base-q8",
        part: Part::Describe,
        family: Family::Florence2,
        size: Size::Smaller,
        label: "Florence-2 base",
        repo: "onnx-community/Florence-2-base-ft",
        revision: "e88a44eaf3791a35eae0c5a47b3dbcd36e67eb6f",
        files: FLORENCE2_BASE_Q8_FILES,
        licence: Licence::Mit,
        seconds_per_photo: 6.0,
        dim: None,
        keyword_threshold: None,
        meaning_threshold: None,
    },
    ModelChoice {
        id: "florence2-large-q8",
        part: Part::Describe,
        family: Family::Florence2,
        size: Size::Larger,
        label: "Florence-2 large",
        repo: "onnx-community/Florence-2-large-ft",
        revision: "04ace74913c28d7ec94af32cee5c111a10126b6f",
        files: FLORENCE2_LARGE_Q8_FILES,
        licence: Licence::Mit,
        seconds_per_photo: 20.0,
        dim: None,
        keyword_threshold: None,
        meaning_threshold: None,
    },
    ModelChoice {
        id: "clip-base-q8",
        part: Part::Keywords,
        family: Family::Clip,
        size: Size::Smaller,
        label: "CLIP base",
        repo: "Xenova/clip-vit-base-patch32",
        revision: "6b7c523703ae26e39be4b03774966f8571c7f523",
        files: CLIP_BASE_Q8_FILES,
        licence: Licence::Mit,
        seconds_per_photo: 0.1,
        dim: Some(512),
        keyword_threshold: Some(0.05),
        meaning_threshold: Some(0.27),
    },
    ModelChoice {
        id: "clip-large-q8",
        part: Part::Keywords,
        family: Family::Clip,
        size: Size::Larger,
        label: "CLIP large",
        repo: "Xenova/clip-vit-large-patch14",
        revision: "c307790166907339eed5a9a53a249af534102536",
        files: CLIP_LARGE_Q8_FILES,
        licence: Licence::Mit,
        seconds_per_photo: 0.5,
        dim: Some(768),
        keyword_threshold: Some(0.05),
        meaning_threshold: Some(0.24),
    },
];

const FLORENCE2_BASE_Q8_FILES: &[ModelFile] = &[
    ModelFile {
        path: "config.json",
        bytes: 5432,
        sha256: "d90c22ed72eb55291f183fcd9b98ebd3bd3d92bfcffb6c7f6e1606085e793525",
    },
    ModelFile {
        path: "generation_config.json",
        bytes: 292,
        sha256: "7b8eb17bbd6cf8a07f619ad83ae03881eff05b6b9237bab89005b40e77783c29",
    },
    ModelFile {
        path: "onnx/decoder_model_merged_quantized.onnx",
        bytes: 98177697,
        sha256: "f22f52f980c33df0efa15932c2f3db6d9d3595ce6387eca938b8cfe23dc4c641",
    },
    ModelFile {
        path: "onnx/embed_tokens_quantized.onnx",
        bytes: 39390433,
        sha256: "6b2258db1c8ee9b160576ccde3cd3814d83a2edaed0dd1c6ca9ff3c38fa62214",
    },
    ModelFile {
        path: "onnx/encoder_model_quantized.onnx",
        bytes: 43651493,
        sha256: "f4ad7a68f1fb875d3bcf735ea14a7021b7ba7e83baf7cf10289881b4ed6d9b85",
    },
    ModelFile {
        path: "onnx/vision_encoder_quantized.onnx",
        bytes: 93746540,
        sha256: "3b79d54f23f666f731549db23cb070c35a979ce19cbd9720e90e67a78dc9768c",
    },
    ModelFile {
        path: "preprocessor_config.json",
        bytes: 2673,
        sha256: "c892857e34a7082284983a7717717d39c9bf7e574f1f41d80d4c918c97502efa",
    },
    ModelFile {
        path: "tokenizer.json",
        bytes: 2297961,
        sha256: "d69dcdb2323e124ac4f800cb9863ddccea0d7bb11e16125e8df3bd60f2f8aeac",
    },
    ModelFile {
        path: "tokenizer_config.json",
        bytes: 197658,
        sha256: "d8e64607233cb53b619fb46664f6cad08176c26e0e8735b2d30d888364f19600",
    },
];

const FLORENCE2_LARGE_Q8_FILES: &[ModelFile] = &[
    ModelFile {
        path: "config.json",
        bytes: 5460,
        sha256: "226291bd5b65c932b01704838489cdc810f0f4949f182abc3de67d7a6ab3632a",
    },
    ModelFile {
        path: "generation_config.json",
        bytes: 62,
        sha256: "6257874d1d2fd60a5c0786253005d9dfa46f5287903fdb079ca9829ec13cf8b4",
    },
    ModelFile {
        path: "onnx/decoder_model_merged_quantized.onnx",
        bytes: 257619824,
        sha256: "167e7807fb1e5b8d693138f84bc81e1368ea11833264131c1ea96eafc9d13222",
    },
    ModelFile {
        path: "onnx/embed_tokens_quantized.onnx",
        bytes: 52520480,
        sha256: "dab1d09ab449f1302df6e439b35ec9e93fb037134bf6bed9ffeaa93a5b35d43c",
    },
    ModelFile {
        path: "onnx/encoder_model_quantized.onnx",
        bytes: 152985855,
        sha256: "9fe30b02cc8c2247631f8f0b9bc4d6613df1c33da0afe4087abff2f940bc2fe0",
    },
    ModelFile {
        path: "onnx/vision_encoder_quantized.onnx",
        bytes: 366371802,
        sha256: "0dbf2172ef1dc6fbb4da0f02498ef9e3a009bf84e8f2e0d971f39cc5b7e053ce",
    },
    ModelFile {
        path: "preprocessor_config.json",
        bytes: 2672,
        sha256: "3adbbeec44c2b8f41a1e97b3ab2197227fb24be1fc1779c9d0d925f4e77d6559",
    },
    ModelFile {
        path: "tokenizer.json",
        bytes: 2297961,
        sha256: "d69dcdb2323e124ac4f800cb9863ddccea0d7bb11e16125e8df3bd60f2f8aeac",
    },
    ModelFile {
        path: "tokenizer_config.json",
        bytes: 197658,
        sha256: "d8e64607233cb53b619fb46664f6cad08176c26e0e8735b2d30d888364f19600",
    },
];

const CLIP_BASE_Q8_FILES: &[ModelFile] = &[
    ModelFile {
        path: "config.json",
        bytes: 4524,
        sha256: "493ef57ff783e42d1530c91b53469b7fdf8db8a9c1408e86998fcb7899a4f495",
    },
    ModelFile {
        path: "onnx/text_model_quantized.onnx",
        bytes: 64504507,
        sha256: "73baab855d406190da9faa498cfedf65f15cf309f4cc7385b7b032e6d08e5c3a",
    },
    ModelFile {
        path: "onnx/vision_model_quantized.onnx",
        bytes: 89117001,
        sha256: "583fd1110a514667812fee7d684952aaf82a99b959760c8d7dca7e0ab9839299",
    },
    ModelFile {
        path: "preprocessor_config.json",
        bytes: 520,
        sha256: "6f638fb9401a6d6296feff533ee7efe657b787c49f954f82f5906b36ef2a1b1f",
    },
    ModelFile {
        path: "tokenizer.json",
        bytes: 2224119,
        sha256: "f7f3b7af117d467b58374797691a6438d3e6b9e9cef800dfd5dced7f697a90cd",
    },
    ModelFile {
        path: "tokenizer_config.json",
        bytes: 775,
        sha256: "60ba2912bc6344c94bc16bbdec27fa1209409167b6f2fdf3cfe9e65462ea3967",
    },
];

const CLIP_LARGE_Q8_FILES: &[ModelFile] = &[
    ModelFile {
        path: "config.json",
        bytes: 694,
        sha256: "99d2f15ccaf4f72c1e4c656dc77ae0fa487696860617ea50626d3a14a018185d",
    },
    ModelFile {
        path: "onnx/text_model_quantized.onnx",
        bytes: 124987663,
        sha256: "6d31c7a212681c3918d78193b6b4f98c70fe540577f9a0a57fe7ba8c7439244c",
    },
    ModelFile {
        path: "onnx/vision_model_quantized.onnx",
        bytes: 307001500,
        sha256: "9a658de8944a2001719beabf6c5030cfe7b3dc80e85983f69fc648d6b4d7ec0f",
    },
    ModelFile {
        path: "preprocessor_config.json",
        bytes: 520,
        sha256: "6f638fb9401a6d6296feff533ee7efe657b787c49f954f82f5906b36ef2a1b1f",
    },
    ModelFile {
        path: "tokenizer.json",
        bytes: 2224081,
        sha256: "72ed5c96db5729294468543e4bc75fce14ca63f58e37300290189ba1c1e52b85",
    },
    ModelFile {
        path: "tokenizer_config.json",
        bytes: 775,
        sha256: "60ba2912bc6344c94bc16bbdec27fa1209409167b6f2fdf3cfe9e65462ea3967",
    },
];

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn hex(s: &str, n: usize) -> bool {
        s.len() == n && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// The rules table of "The model catalogue".
    pub(crate) fn check_rules(c: &Catalogue) {
        let mut ids = std::collections::HashSet::new();
        for m in c.all() {
            assert!(ids.insert(m.id), "{} repeats", m.id);
            assert!(!m.id.is_empty());
            assert!(
                m.id.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            );
            assert!(hex(m.revision, 40), "{} revision", m.id);
            assert!(!m.files.is_empty());
            for f in m.files {
                assert!(hex(f.sha256, 64), "{} {}", m.id, f.path);
                assert!(f.bytes > 0);
                assert!(!f.path.contains(".."));
                assert!(!f.path.contains('\\'));
                assert!(!f.path.starts_with('/'));
                assert!(!f.path.contains(':'));
            }
            match m.part {
                Part::Keywords => {
                    assert!(m.dim.is_some_and(|d| d > 0));
                    for t in [m.keyword_threshold, m.meaning_threshold] {
                        assert!(t.is_some_and(|t| t > 0.0 && t < 1.0), "{} threshold", m.id);
                    }
                }
                Part::Describe => {
                    assert!(m.dim.is_none() && m.keyword_threshold.is_none());
                    assert!(m.meaning_threshold.is_none());
                }
            }
            assert!(!m.repo.starts_with("deepinsight/"));
            assert!(!m.repo.to_lowercase().contains("insightface"));
        }
        for part in Part::ALL {
            let sizes: Vec<Size> = c.choices_for(part).iter().map(|m| m.size).collect();
            assert_eq!(sizes, [Size::Smaller, Size::Larger], "{part:?}");
        }
    }

    #[test]
    fn the_real_catalogue_keeps_the_rules() {
        check_rules(&Catalogue::real());
    }

    #[test]
    fn sizes_sum_to_the_stated_totals() {
        let c = Catalogue::real();
        let total = |id| c.get(id).unwrap().bytes();
        assert_eq!(total("florence2-base-q8"), 277_470_179);
        assert_eq!(total("florence2-large-q8"), 832_001_774);
        assert_eq!(total("clip-base-q8"), 155_851_446);
        assert_eq!(total("clip-large-q8"), 434_215_233);
    }

    #[test]
    fn choices_are_smaller_first() {
        let c = Catalogue::real();
        let ids: Vec<_> = c.choices_for(Part::Keywords).iter().map(|m| m.id).collect();
        assert_eq!(ids, ["clip-base-q8", "clip-large-q8"]);
    }
}
