//! The fake backend and the fake catalogue (docs/07-tagging.md, "The fake
//! backend"). Deterministic and instant: the words of a photo's file name
//! stand in for what a model would see. Used by the tests of this crate and
//! of the shell; never by the program.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::backend::{BackendError, Deadline, TagBackend, TagImage, TextEmbedder};
use super::catalogue::{Catalogue, Family, Licence, ModelChoice, ModelFile, Part, Size};
use super::results::{self, Described, KeywordScore, Keyworded, ObjectCount};

/// The length of a fake vector.
pub const FAKE_DIM: u32 = 8;

macro_rules! fake_model {
    ($id:literal, $part:expr, $size:expr, $label:literal, $files:expr, $dim:expr, $kt:expr, $mt:expr) => {
        ModelChoice {
            id: $id,
            part: $part,
            family: Family::Fake,
            size: $size,
            label: $label,
            repo: concat!("owlbum-test/", $id),
            revision: "0123456789abcdef0123456789abcdef01234567",
            files: $files,
            licence: Licence::Mit,
            seconds_per_photo: 0.1,
            dim: $dim,
            keyword_threshold: $kt,
            meaning_threshold: $mt,
        }
    };
}

macro_rules! fake_files {
    ($a:literal, $b:literal) => {
        &[
            ModelFile {
                path: "model.bin",
                bytes: 46,
                sha256: $a,
            },
            ModelFile {
                path: "onnx/weights.bin",
                bytes: 53,
                sha256: $b,
            },
        ]
    };
}

/// Four tiny models: dim 8, keyword threshold 0.2, meaning threshold 0.5.
pub static FAKE_MODELS: &[ModelChoice] = &[
    fake_model!(
        "fake-describe-small",
        Part::Describe,
        Size::Smaller,
        "Fake describer small",
        fake_files!(
            "018cbc797ce2a420f32f90bca8e979c6dee2b1c32862f4c7085bac80a23dcace",
            "e9644abfc892e690c461cd2a45c1d56b509fa01e4cf43b6265f8874d7516ef7a"
        ),
        None,
        None,
        None
    ),
    fake_model!(
        "fake-describe-large",
        Part::Describe,
        Size::Larger,
        "Fake describer large",
        fake_files!(
            "4f5cb73e03fca9e1f455560c25df690c0b736aeb468ea98b0c97fa1c6b8a5fac",
            "3c3b5bec036bf6c1a18c9ba01d387bd846dd96c7e913f279d3e04f5c74a1b29b"
        ),
        None,
        None,
        None
    ),
    fake_model!(
        "fake-keywords-small",
        Part::Keywords,
        Size::Smaller,
        "Fake keywords small",
        fake_files!(
            "087e55da918b7095615a23e4f1ae1864449d86b404b5069d7f72f6bd47d8136b",
            "02b85d8bbfcd898cd557a74d7b6dcd78fec2368b9a2318b9c69aea9e8e5ae393"
        ),
        Some(FAKE_DIM),
        Some(0.2),
        Some(0.5)
    ),
    fake_model!(
        "fake-keywords-large",
        Part::Keywords,
        Size::Larger,
        "Fake keywords large",
        fake_files!(
            "332335f006f5162a49051066f5b1d74aff2eb4e6c61a074f3e6b36df812e0ffb",
            "9298980fb71b0dc7a70688c2d054a832a5d27d5664995cd422358ab20529eb0b"
        ),
        Some(FAKE_DIM),
        Some(0.2),
        Some(0.5)
    ),
];

/// The fake catalogue.
pub fn catalogue() -> Catalogue {
    Catalogue(FAKE_MODELS)
}

/// The bytes of a fake model's file: what the fake fetch serves and what
/// a test writes to install a model by hand.
pub fn file_content(model: &str, path: &str) -> Vec<u8> {
    format!("fake model file {model}/{path}\n").into_bytes()
}

/// Writes every file of a fake model into `<models>/<id>/`, as a finished
/// download would (without the marker: the model is `unverified` until
/// hashed).
pub fn install(models: &Path, model: &ModelChoice) {
    for f in model.files {
        let p = models.join(model.id).join(f.path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, file_content(model.id, f.path)).unwrap();
    }
}

/// What a test can tell the fake to do, shared with the backend.
#[derive(Debug, Default)]
pub struct FakeControl {
    inner: Mutex<ControlState>,
}

#[derive(Debug, Default)]
struct ControlState {
    fail_next_load: bool,
    lost_left: usize,
    delay: Duration,
    hang: Duration,
    loads: Vec<&'static str>,
    runs: Vec<(Part, String)>,
    captions: u32,
}

impl FakeControl {
    fn state(&self) -> std::sync::MutexGuard<'_, ControlState> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The next load fails.
    pub fn fail_next_load(&self) {
        self.state().fail_next_load = true;
    }

    /// Photos named `lost-…` answer `Lost` this many more times, then work.
    pub fn set_lost(&self, times: usize) {
        self.state().lost_left = times;
    }

    /// Every run waits this long first.
    pub fn set_delay(&self, delay: Duration) {
        self.state().delay = delay;
    }

    /// Photos named `hang-…` block this long whatever the deadline says
    /// (a run stuck inside the runtime), then answer `Timeout`.
    pub fn set_hang(&self, hang: Duration) {
        self.state().hang = hang;
    }

    /// The models loaded so far, in order.
    pub fn loads(&self) -> Vec<&'static str> {
        self.state().loads.clone()
    }

    /// The runs so far: part and file name, in order.
    pub fn runs(&self) -> Vec<(Part, String)> {
        self.state().runs.clone()
    }
}

/// The fake backend.
pub struct FakeBackend {
    control: Arc<FakeControl>,
    slots: [Option<Loaded>; 2],
}

struct Loaded {
    model: &'static ModelChoice,
    vocabulary: Vec<String>,
}

impl FakeBackend {
    pub fn new(control: Arc<FakeControl>) -> Self {
        Self {
            control,
            slots: [None, None],
        }
    }

    fn run(
        &mut self,
        part: Part,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<&Loaded, BackendError> {
        let delay = {
            let mut s = self.control.state();
            s.runs.push((part, image.name.to_owned()));
            s.delay
        };
        let start = std::time::Instant::now();
        while start.elapsed() < delay {
            if deadline.stopped() {
                return Err(BackendError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let name = image.name.to_lowercase();
        if name.starts_with("hang-") {
            let hang = self.control.state().hang;
            std::thread::sleep(hang);
            return Err(BackendError::Timeout);
        }
        if name.starts_with("slow-") {
            return Err(BackendError::Timeout);
        }
        if name.starts_with("lost-") {
            let mut s = self.control.state();
            if s.lost_left > 0 {
                s.lost_left -= 1;
                return Err(BackendError::Lost("the fake lost its model".into()));
            }
        }
        if name.starts_with("fail-") {
            return Err(BackendError::Photo(
                "The fake model refused this picture.".into(),
            ));
        }
        self.slots[part.index()]
            .as_ref()
            .ok_or_else(|| BackendError::Lost("not loaded".into()))
    }
}

/// The words of a file name: lower case, letters only.
pub fn words(name: &str) -> Vec<String> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    stem.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// A unit vector of [`FAKE_DIM`] numbers built from words: each word adds
/// one to a place chosen by its letters. `seaside` counts as `harbour`.
pub fn vector_of(words: &[String]) -> Vec<f32> {
    let mut v = vec![0.0; FAKE_DIM as usize];
    for w in words {
        let w = match w.as_str() {
            "seaside" | "port" => "harbour",
            other => other,
        };
        let h = w
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        v[(h % FAKE_DIM) as usize] += 1.0;
    }
    if v.iter().all(|x| *x == 0.0) {
        v[0] = 1.0;
    }
    results::unit(v)
}

impl TagBackend for FakeBackend {
    fn load(
        &mut self,
        model: &ModelChoice,
        dir: &Path,
        _threads: usize,
        vocabulary: &[&str],
        _deadline: &Deadline,
    ) -> Result<(), BackendError> {
        let model = super::catalogue::Catalogue(FAKE_MODELS)
            .get(model.id)
            .ok_or_else(|| BackendError::Load(format!("{} is not a fake model", model.id)))?;
        self.slots[model.part.index()] = None;
        {
            let mut s = self.control.state();
            if std::mem::take(&mut s.fail_next_load) {
                return Err(BackendError::Load("the fake was told to fail".into()));
            }
            s.loads.push(model.id);
        }
        if !dir.join(model.files[0].path).is_file() {
            return Err(BackendError::Load("its files are missing".into()));
        }
        self.slots[model.part.index()] = Some(Loaded {
            model,
            vocabulary: vocabulary.iter().map(|w| (*w).to_owned()).collect(),
        });
        Ok(())
    }

    fn unload(&mut self, part: Part) {
        self.slots[part.index()] = None;
    }

    fn loaded(&self, part: Part) -> Option<&'static str> {
        self.slots[part.index()].as_ref().map(|l| l.model.id)
    }

    fn describe(
        &mut self,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<Described, BackendError> {
        self.run(Part::Describe, image, deadline)?;
        let w = words(image.name);
        Ok(Described {
            description: Some(format!("A picture of {}.", w.join(" "))),
            objects: w
                .first()
                .map(|f| {
                    vec![ObjectCount {
                        label: f.clone(),
                        count: 2,
                    }]
                })
                .unwrap_or_default(),
        })
    }

    fn caption(&mut self, image: &TagImage, deadline: &Deadline) -> Result<String, BackendError> {
        self.run(Part::Describe, image, deadline)?;
        let n = {
            let mut s = self.control.state();
            s.captions += 1;
            s.captions
        };
        Ok(format!(
            "Another look at {} ({n}).",
            words(image.name).join(" ")
        ))
    }

    fn keywords(
        &mut self,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<Keyworded, BackendError> {
        let loaded = self.run(Part::Keywords, image, deadline)?;
        let w = words(image.name);
        let found: Vec<&String> = w.iter().filter(|x| loaded.vocabulary.contains(x)).collect();
        let score = 1.0 / found.len().max(1) as f32;
        let mut keywords: Vec<KeywordScore> = Vec::new();
        for f in found {
            if !keywords.iter().any(|k| &k.label == f) {
                keywords.push(KeywordScore {
                    label: f.clone(),
                    score,
                });
            }
        }
        keywords.truncate(results::MAX_KEYWORDS);
        Ok(Keyworded {
            keywords,
            vector: vector_of(&w),
        })
    }

    fn text_embedder(&self) -> Option<Arc<dyn TextEmbedder>> {
        self.slots[Part::Keywords.index()]
            .as_ref()
            .map(|_| Arc::new(FakeText) as Arc<dyn TextEmbedder>)
    }
}

struct FakeText;

impl TextEmbedder for FakeText {
    fn embed(&self, text: &str, _deadline: &Deadline) -> Result<Vec<f32>, BackendError> {
        Ok(vector_of(&words(&format!("{text}.x"))))
    }
}

/// Counts of loads by model id, for tests that check unloading.
pub fn load_counts(control: &FakeControl) -> HashMap<&'static str, usize> {
    let mut m = HashMap::new();
    for id in control.loads() {
        *m.entry(id).or_default() += 1;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_catalogue_keeps_the_rules() {
        super::super::catalogue::tests::check_rules(&catalogue());
    }

    #[test]
    fn fake_files_match_their_hashes() {
        use sha2::Digest;
        for m in FAKE_MODELS {
            for f in m.files {
                let c = file_content(m.id, f.path);
                assert_eq!(c.len() as u64, f.bytes);
                let h = sha2::Sha256::digest(&c);
                let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
                assert_eq!(hex, f.sha256);
            }
        }
    }

    #[test]
    fn seaside_is_near_harbour() {
        let t = FakeText
            .embed("seaside", &Deadline::after(Duration::from_secs(1)))
            .unwrap();
        let h = vector_of(&words("harbour-boats.jpg"));
        let l = vector_of(&words("lake.jpg"));
        assert!(results::dot(&t, &h) > 0.5);
        assert!(results::dot(&t, &l) < results::dot(&t, &h));
    }
}
