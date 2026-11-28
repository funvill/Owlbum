//! The real backend (docs/07-tagging.md, "The runtime"): Florence-2 and
//! CLIP on ONNX Runtime through `ort`, with `tokenizers` for text. It is
//! given a folder of local files and never touches the network.
//!
//! ONNX Runtime is Microsoft's own library, loaded at run time by its full
//! path ([`set_library`]); see "Packaging". The prototype (#52) chose to
//! drive `ort` directly rather than through `usls`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use image::RgbImage;
use image::imageops::FilterType;
use ort::environment::Environment;
use ort::session::{RunOptions, Session};
use ort::value::Tensor;
use tokenizers::Tokenizer;

use super::backend::{BackendError, Deadline, TagBackend, TagImage, TextEmbedder};
use super::catalogue::{Family, ModelChoice, Part};
use super::results::{self, Described, Keyworded};

static LIBRARY: OnceLock<PathBuf> = OnceLock::new();
static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
static VERSION: OnceLock<String> = OnceLock::new();

/// The loaded ONNX Runtime's version, such as `1.31.1`; None until the
/// library has loaded (at the first model load). The About box lists it
/// (docs/07-tagging.md, "Packaging").
pub fn loaded_version() -> Option<String> {
    VERSION.get().cloned()
}

/// The version in ONNX Runtime's build info, which reads like `ORT Build
/// Info: git-branch=rel-1.31.1, git-commit-id=…`; the whole line when it
/// names no release branch.
fn version_of(info: &str) -> String {
    info.split([',', ' '])
        .find_map(|part| part.trim().strip_prefix("git-branch=rel-"))
        .map_or_else(|| info.trim().to_owned(), str::to_owned)
}

/// Names the ONNX Runtime library file to load. Only the first call counts.
pub fn set_library(path: PathBuf) {
    let _ = LIBRARY.set(path);
}

/// The platform's file name for the library.
pub const LIBRARY_FILE: &str = if cfg!(windows) {
    "onnxruntime.dll"
} else {
    "libonnxruntime.so"
};

/// The library in a folder (the program's resource folder, where the
/// installers put it; docs/08-desktop-and-release.md, "Bundle
/// configuration"), when it is there.
pub fn library_in(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(LIBRARY_FILE);
    path.is_file().then_some(path)
}

/// The library: the one set, else `OWLBUM_ORT_DYLIB`, else the platform's
/// file name next to the program.
fn library() -> Result<PathBuf, String> {
    if let Some(p) = LIBRARY.get() {
        return Ok(p.clone());
    }
    if let Some(p) = std::env::var_os("OWLBUM_ORT_DYLIB") {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.with_file_name(LIBRARY_FILE))
}

/// Starts ONNX Runtime once; the reason when it cannot start.
pub fn environment() -> Result<&'static Environment, String> {
    ENV.get_or_init(|| {
        let path = library()?;
        if !path.is_file() {
            return Err(format!("{} was not found", path.display()));
        }
        let env = ort::init_from(&path)
            .map_err(|e| e.to_string())?
            .with_name("owlbum")
            .with_telemetry(false)
            .build()
            .map_err(|e| e.to_string())?;
        let version = version_of(ort::info());
        tracing::info!(target: "tagging", "ONNX Runtime {version} loaded from {}", path.display());
        let _ = VERSION.set(version);
        Ok(env)
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// `max(1, floor(logical processors / 2))`.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).max(1))
}

fn load_err(what: impl std::fmt::Display) -> BackendError {
    BackendError::Load(what.to_string())
}

fn session(dir: &Path, file: &str, threads: usize) -> Result<Session, BackendError> {
    let env = environment().map_err(load_err)?;
    Session::builder(env)
        .map_err(load_err)?
        .with_intra_threads(threads)
        .map_err(load_err)?
        .with_inter_threads(1)
        .map_err(load_err)?
        .with_parallel_execution(false)
        .map_err(load_err)?
        .commit_from_file(dir.join(file))
        .map_err(|e| load_err(format!("{file}: {e}")))
}

fn tokenizer(dir: &Path) -> Result<Tokenizer, BackendError> {
    Tokenizer::from_file(dir.join("tokenizer.json"))
        .map_err(|e| load_err(format!("tokenizer.json: {e}")))
}

fn read_json(dir: &Path, file: &str) -> Result<serde_json::Value, BackendError> {
    let bytes = std::fs::read(dir.join(file)).map_err(|e| load_err(format!("{file}: {e}")))?;
    serde_json::from_slice(&bytes).map_err(|e| load_err(format!("{file}: {e}")))
}

/// Ends the runs of one task when its deadline passes, through ONNX
/// Runtime's run options (step 2 of "Timeouts").
struct Watchdog {
    options: Arc<RunOptions>,
    done: Arc<AtomicBool>,
}

impl Watchdog {
    fn start(deadline: &Deadline) -> Result<Self, BackendError> {
        let options = Arc::new(RunOptions::new().map_err(lost)?);
        let done = Arc::new(AtomicBool::new(false));
        let (o, d, dl) = (Arc::clone(&options), Arc::clone(&done), deadline.clone());
        std::thread::Builder::new()
            .name("owlbum-tag-watchdog".into())
            .spawn(move || {
                while !d.load(Ordering::Relaxed) {
                    if dl.expired() {
                        let _ = o.terminate();
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .map_err(lost)?;
        Ok(Self { options, done })
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
    }
}

/// A run's error: a timeout when the deadline passed, else lost.
fn run_err(deadline: &Deadline) -> impl Fn(ort::Error) -> BackendError + '_ {
    move |e| {
        if deadline.expired() {
            BackendError::Timeout
        } else {
            BackendError::Lost(e.to_string())
        }
    }
}

fn lost(e: impl std::fmt::Display) -> BackendError {
    BackendError::Lost(e.to_string())
}

/// An owned copy of an output tensor.
struct Owned {
    shape: Vec<i64>,
    data: Vec<f32>,
}

impl Owned {
    fn tensor(&self) -> Result<Tensor<f32>, BackendError> {
        Tensor::from_array((self.shape.clone(), self.data.clone())).map_err(lost)
    }
}

fn extract(outputs: &ort::session::SessionOutputs<'_>, name: &str) -> Result<Owned, BackendError> {
    let value = outputs
        .get(name)
        .ok_or_else(|| BackendError::Lost(format!("no output {name}")))?;
    let (shape, data) = value.try_extract_tensor::<f32>().map_err(lost)?;
    Ok(Owned {
        shape: shape.iter().copied().collect(),
        data: data.to_vec(),
    })
}

/// How a model wants its picture: size, crop, mean and spread.
#[derive(Debug, Clone)]
struct Preprocess {
    /// Florence-2 squashes to this; CLIP resizes the shorter side to it.
    width: u32,
    height: u32,
    /// CLIP: crop the centre to this square.
    crop: Option<u32>,
    mean: [f32; 3],
    std: [f32; 3],
}

impl Preprocess {
    fn read(dir: &Path, family: Family) -> Result<Self, BackendError> {
        let c = read_json(dir, "preprocessor_config.json")?;
        let triple = |key: &str| -> Result<[f32; 3], BackendError> {
            let v: Vec<f32> = c[key]
                .as_array()
                .ok_or_else(|| load_err(format!("preprocessor_config.json has no {key}")))?
                .iter()
                .filter_map(|x| x.as_f64().map(|f| f as f32))
                .collect();
            v.try_into()
                .map_err(|_| load_err(format!("preprocessor_config.json: bad {key}")))
        };
        let num = |v: &serde_json::Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
        let (width, height, crop) = match family {
            Family::Clip => {
                let edge = num(&c["size"]["shortest_edge"]).unwrap_or(224);
                let crop = num(&c["crop_size"]["height"]).unwrap_or(edge);
                (edge, edge, Some(crop))
            }
            _ => (
                num(&c["size"]["width"]).unwrap_or(768),
                num(&c["size"]["height"]).unwrap_or(768),
                None,
            ),
        };
        Ok(Self {
            width,
            height,
            crop,
            mean: triple("image_mean")?,
            std: triple("image_std")?,
        })
    }

    /// The picture as a `[1, 3, h, w]` tensor's numbers, and its shape.
    fn tensor(&self, rgb: &RgbImage) -> (Vec<i64>, Vec<f32>) {
        let img = match self.crop {
            Some(crop) => {
                let (w, h) = rgb.dimensions();
                let s = self.width as f32 / w.min(h).max(1) as f32;
                let nw = ((w as f32 * s).round() as u32).max(crop);
                let nh = ((h as f32 * s).round() as u32).max(crop);
                let resized = image::imageops::resize(rgb, nw, nh, FilterType::CatmullRom);
                image::imageops::crop_imm(&resized, (nw - crop) / 2, (nh - crop) / 2, crop, crop)
                    .to_image()
            }
            None => image::imageops::resize(rgb, self.width, self.height, FilterType::CatmullRom),
        };
        let (w, h) = img.dimensions();
        let plane = (w * h) as usize;
        let mut data = vec![0f32; plane * 3];
        for (i, px) in img.pixels().enumerate() {
            for c in 0..3 {
                data[c * plane + i] = (f32::from(px[c]) / 255.0 - self.mean[c]) / self.std[c];
            }
        }
        (vec![1, 3, i64::from(h), i64::from(w)], data)
    }
}

// ---------------------------------------------------------------- CLIP --

/// CLIP's text side, shared with search.
pub struct ClipText {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    max_len: usize,
    eos: i64,
    /// The last 32 search texts and their vectors.
    cache: Mutex<VecDeque<(String, Vec<f32>)>>,
}

impl ClipText {
    fn ids(&self, text: &str) -> Result<Vec<i64>, BackendError> {
        let enc = self.tokenizer.encode(text, true).map_err(lost)?;
        let mut ids: Vec<i64> = enc.get_ids().iter().map(|&i| i64::from(i)).collect();
        if ids.len() > self.max_len {
            ids.truncate(self.max_len - 1);
            ids.push(self.eos);
        }
        Ok(ids)
    }

    /// Unit vectors of several texts, run as one padded batch.
    fn embed_batch(
        &self,
        texts: &[String],
        deadline: &Deadline,
    ) -> Result<Vec<Vec<f32>>, BackendError> {
        let ids: Vec<Vec<i64>> = texts
            .iter()
            .map(|t| self.ids(t))
            .collect::<Result<_, _>>()?;
        let len = ids.iter().map(Vec::len).max().unwrap_or(1);
        let mut flat = Vec::with_capacity(ids.len() * len);
        for row in &ids {
            flat.extend_from_slice(row);
            // CLIP's text model reads the end token's place, the first
            // largest id; padding with the end token keeps that place.
            flat.extend(std::iter::repeat_n(self.eos, len - row.len()));
        }
        let input = Tensor::from_array((vec![ids.len() as i64, len as i64], flat)).map_err(lost)?;
        let dog = Watchdog::start(deadline)?;
        let mut session = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let out = session
            .run_with_options(ort::inputs!["input_ids" => input], &dog.options)
            .map_err(run_err(deadline))?;
        let o = extract(&out, "text_embeds")?;
        let dim = (*o.shape.last().unwrap_or(&1)).max(1) as usize;
        Ok(o.data
            .chunks(dim)
            .map(|c| results::unit(c.to_vec()))
            .collect())
    }
}

impl TextEmbedder for ClipText {
    fn embed(&self, text: &str, deadline: &Deadline) -> Result<Vec<f32>, BackendError> {
        if let Some((_, v)) = self
            .cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(t, _)| t == text)
        {
            return Ok(v.clone());
        }
        let v = self
            .embed_batch(&[text.to_owned()], deadline)?
            .pop()
            .ok_or_else(|| lost("no vector"))?;
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        cache.push_back((text.to_owned(), v.clone()));
        if cache.len() > 32 {
            cache.pop_front();
        }
        Ok(v)
    }
}

struct Clip {
    model: &'static ModelChoice,
    vision: Session,
    text: Arc<ClipText>,
    pre: Preprocess,
    words: Vec<String>,
    vocab: Vec<Vec<f32>>,
}

impl Clip {
    fn load(
        model: &'static ModelChoice,
        dir: &Path,
        threads: usize,
        vocabulary: &[&str],
        deadline: &Deadline,
    ) -> Result<Self, BackendError> {
        let vision = session(dir, "onnx/vision_model_quantized.onnx", threads)?;
        let tok = tokenizer(dir)?;
        let eos = tok
            .token_to_id("<|endoftext|>")
            .ok_or_else(|| load_err("the tokenizer has no end token"))?;
        let text = Arc::new(ClipText {
            session: Mutex::new(session(dir, "onnx/text_model_quantized.onnx", threads)?),
            tokenizer: tok,
            max_len: 77,
            eos: i64::from(eos),
            cache: Mutex::new(VecDeque::new()),
        });
        let pre = Preprocess::read(dir, Family::Clip)?;
        let dim = model.dim.unwrap_or(0) as usize;
        let mut vocab = Vec::with_capacity(vocabulary.len());
        for batch in vocabulary.chunks(64) {
            deadline
                .check()
                .map_err(|_| load_err("loading took too long"))?;
            let prompts: Vec<String> = batch.iter().map(|w| format!("a photo of {w}")).collect();
            let vs = text.embed_batch(&prompts, deadline).map_err(load_err)?;
            vocab.extend(vs);
        }
        if vocab.iter().any(|v| v.len() != dim) {
            return Err(load_err(format!("its vectors are not {dim} long")));
        }
        Ok(Self {
            model,
            vision,
            text,
            pre,
            words: vocabulary.iter().map(|w| (*w).to_owned()).collect(),
            vocab,
        })
    }

    fn keywords(
        &mut self,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<Keyworded, BackendError> {
        let (shape, data) = self.pre.tensor(image.rgb);
        let input = Tensor::from_array((shape, data)).map_err(lost)?;
        let dog = Watchdog::start(deadline)?;
        let out = self
            .vision
            .run_with_options(ort::inputs!["pixel_values" => input], &dog.options)
            .map_err(run_err(deadline))?;
        let v = results::unit(extract(&out, "image_embeds")?.data);
        let words: Vec<&str> = self.words.iter().map(String::as_str).collect();
        let threshold = self.model.keyword_threshold.unwrap_or(1.0);
        let keywords = results::keywords(&v, &self.vocab, &words, threshold);
        Ok(Keyworded {
            keywords,
            vector: v,
        })
    }
}

// ----------------------------------------------------------- Florence-2 --

struct Florence {
    vision: Session,
    embed: Session,
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    prompts: HashMap<String, String>,
    pre: Preprocess,
    layers: usize,
    heads: i64,
    head_dim: i64,
    no_repeat: usize,
    rng: u64,
}

const DECODER_START: u32 = 2;
const BOS: u32 = 0;
const EOS: u32 = 2;

/// How the next token is chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    Greedy,
    /// Temperature, then one of the `top_k` likeliest at random.
    Sampled {
        temperature: f32,
        top_k: usize,
    },
}

/// Regenerate's sampling (docs/07-tagging.md, "Description").
pub const SAMPLED: Pick = Pick::Sampled {
    temperature: 0.7,
    top_k: 50,
};

type Inputs<'a> = Vec<(
    std::borrow::Cow<'a, str>,
    ort::session::SessionInputValue<'a>,
)>;

impl Florence {
    fn load(dir: &Path, threads: usize) -> Result<Self, BackendError> {
        let pre = Preprocess::read(dir, Family::Florence2)?;
        let c = read_json(dir, "preprocessor_config.json")?;
        let prompts: HashMap<String, String> = c["task_prompts_without_inputs"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        for task in ["<MORE_DETAILED_CAPTION>", "<OD>"] {
            if !prompts.contains_key(task) {
                return Err(load_err(format!(
                    "preprocessor_config.json has no prompt for {task}"
                )));
            }
        }
        let no_repeat = read_json(dir, "generation_config.json")
            .ok()
            .and_then(|g| g["no_repeat_ngram_size"].as_u64())
            .unwrap_or(0) as usize;
        let decoder = session(dir, "onnx/decoder_model_merged_quantized.onnx", threads)?;
        let layers = decoder
            .inputs()
            .iter()
            .filter(|i| i.name().ends_with(".decoder.key"))
            .count();
        let (heads, head_dim) = decoder
            .inputs()
            .iter()
            .find(|i| i.name().ends_with(".decoder.key"))
            .and_then(|i| match i.dtype() {
                ort::value::ValueType::Tensor { shape, .. } if shape.len() == 4 => {
                    Some((shape[1], shape[3]))
                }
                _ => None,
            })
            .ok_or_else(|| load_err("the decoder has no cache inputs"))?;
        Ok(Self {
            vision: session(dir, "onnx/vision_encoder_quantized.onnx", threads)?,
            embed: session(dir, "onnx/embed_tokens_quantized.onnx", threads)?,
            encoder: session(dir, "onnx/encoder_model_quantized.onnx", threads)?,
            decoder,
            tokenizer: tokenizer(dir)?,
            prompts,
            pre,
            layers,
            heads,
            head_dim,
            no_repeat,
            rng: seed(),
        })
    }

    fn features(
        &mut self,
        rgb: &RgbImage,
        dog: &Watchdog,
        deadline: &Deadline,
    ) -> Result<Owned, BackendError> {
        let (shape, data) = self.pre.tensor(rgb);
        let input = Tensor::from_array((shape, data)).map_err(lost)?;
        let out = self
            .vision
            .run_with_options(ort::inputs!["pixel_values" => input], &dog.options)
            .map_err(run_err(deadline))?;
        extract(&out, "image_features")
    }

    fn embed(
        &mut self,
        ids: &[i64],
        dog: &Watchdog,
        deadline: &Deadline,
    ) -> Result<Owned, BackendError> {
        let input = Tensor::from_array((vec![1, ids.len() as i64], ids.to_vec())).map_err(lost)?;
        let out = self
            .embed
            .run_with_options(ort::inputs!["input_ids" => input], &dog.options)
            .map_err(run_err(deadline))?;
        extract(&out, "inputs_embeds")
    }

    /// `florence_task`: the raw text of one task, special tokens kept.
    #[allow(clippy::too_many_arguments)]
    fn task(
        &mut self,
        features: &Owned,
        task: &str,
        max_new: usize,
        pick: Pick,
        dog: &Watchdog,
        deadline: &Deadline,
    ) -> Result<String, BackendError> {
        let prompt = self.prompts[task].clone();
        let enc = self.tokenizer.encode(prompt, true).map_err(lost)?;
        let ids: Vec<i64> = enc.get_ids().iter().map(|&i| i64::from(i)).collect();
        let prompt_embeds = self.embed(&ids, dog, deadline)?;
        let d = *features.shape.last().unwrap_or(&768);
        let n = features.shape[1] + prompt_embeds.shape[1];
        let mut joined = features.data.clone();
        joined.extend_from_slice(&prompt_embeds.data);
        let mask = vec![1i64; usize::try_from(n).unwrap_or(0)];
        let memory = {
            let embeds = Tensor::from_array((vec![1, n, d], joined)).map_err(lost)?;
            let mask = Tensor::from_array((vec![1, n], mask.clone())).map_err(lost)?;
            let out = self
                .encoder
                .run_with_options(
                    ort::inputs!["inputs_embeds" => embeds, "attention_mask" => mask],
                    &dog.options,
                )
                .map_err(run_err(deadline))?;
            extract(&out, "last_hidden_state")?
        };
        let empty = Owned {
            shape: vec![1, self.heads, 0, self.head_dim],
            data: Vec::new(),
        };
        let mut past_dec: Vec<(Owned, Owned)> = Vec::new();
        let mut past_enc: Vec<(Owned, Owned)> = Vec::new();
        let mut tokens: Vec<u32> = Vec::new();
        let mut last = DECODER_START;
        for step in 0..max_new {
            deadline.check()?;
            let embeds = self.embed(&[i64::from(last)], dog, deadline)?;
            let first = step == 0;
            let mut inputs: Inputs<'_> = vec![
                ("inputs_embeds".into(), embeds.tensor()?.into()),
                ("encoder_hidden_states".into(), memory.tensor()?.into()),
                (
                    "encoder_attention_mask".into(),
                    Tensor::from_array((vec![1, n], mask.clone()))
                        .map_err(lost)?
                        .into(),
                ),
                (
                    "use_cache_branch".into(),
                    Tensor::from_array((vec![1i64], vec![!first]))
                        .map_err(lost)?
                        .into(),
                ),
            ];
            for l in 0..self.layers {
                let (dk, dv) = past_dec.get(l).map_or((&empty, &empty), |(k, v)| (k, v));
                let (ek, ev) = past_enc.get(l).map_or((&empty, &empty), |(k, v)| (k, v));
                for (name, t) in [
                    (format!("past_key_values.{l}.decoder.key"), dk),
                    (format!("past_key_values.{l}.decoder.value"), dv),
                    (format!("past_key_values.{l}.encoder.key"), ek),
                    (format!("past_key_values.{l}.encoder.value"), ev),
                ] {
                    inputs.push((name.into(), t.tensor()?.into()));
                }
            }
            let out = self
                .decoder
                .run_with_options(inputs, &dog.options)
                .map_err(run_err(deadline))?;
            let logits = extract(&out, "logits")?;
            let mut dec = Vec::with_capacity(self.layers);
            for l in 0..self.layers {
                dec.push((
                    extract(&out, &format!("present.{l}.decoder.key"))?,
                    extract(&out, &format!("present.{l}.decoder.value"))?,
                ));
                if first {
                    past_enc.push((
                        extract(&out, &format!("present.{l}.encoder.key"))?,
                        extract(&out, &format!("present.{l}.encoder.value"))?,
                    ));
                }
            }
            drop(out);
            past_dec = dec;
            let vocab = (*logits.shape.last().unwrap_or(&1)).max(1) as usize;
            let row = &logits.data[logits.data.len() - vocab..];
            let next = if first {
                BOS
            } else {
                let banned = banned_tokens(&tokens, self.no_repeat);
                choose(row, &banned, pick, &mut self.rng)
            };
            tokens.push(next);
            if next == EOS {
                break;
            }
            last = next;
        }
        self.tokenizer.decode(&tokens, false).map_err(lost)
    }
}

/// Tokens that would repeat an n-gram already written
/// (`no_repeat_ngram_size` of the model's generation settings).
fn banned_tokens(tokens: &[u32], n: usize) -> Vec<u32> {
    if n == 0 || tokens.len() + 1 < n {
        return Vec::new();
    }
    let prefix = &tokens[tokens.len() + 1 - n..];
    tokens
        .windows(n)
        .filter(|w| &w[..n - 1] == prefix)
        .map(|w| w[n - 1])
        .collect()
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    t | 1
}

fn next_random(state: &mut u64) -> f64 {
    // xorshift64*
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
}

/// The next token from one row of logits.
fn choose(row: &[f32], banned: &[u32], pick: Pick, rng: &mut u64) -> u32 {
    let allowed = |i: usize| !banned.contains(&(i as u32));
    match pick {
        Pick::Greedy => row
            .iter()
            .enumerate()
            .filter(|(i, _)| allowed(*i))
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or(EOS, |(i, _)| i as u32),
        Pick::Sampled { temperature, top_k } => {
            let mut top: Vec<(usize, f32)> = row
                .iter()
                .enumerate()
                .filter(|(i, _)| allowed(*i))
                .map(|(i, l)| (i, l / temperature))
                .collect();
            top.sort_by(|a, b| b.1.total_cmp(&a.1));
            top.truncate(top_k.max(1));
            let max = top.first().map_or(0.0, |t| t.1);
            let weights: Vec<f64> = top.iter().map(|(_, l)| f64::from(l - max).exp()).collect();
            let sum: f64 = weights.iter().sum();
            let mut r = next_random(rng) * sum;
            for ((i, _), w) in top.iter().zip(&weights) {
                if r < *w {
                    return *i as u32;
                }
                r -= w;
            }
            top.last().map_or(EOS, |t| t.0 as u32)
        }
    }
}

// --------------------------------------------------------------- backend --

/// Runs the real models.
pub struct OnnxBackend {
    describe: Option<(&'static ModelChoice, Florence)>,
    keywords: Option<Clip>,
}

impl OnnxBackend {
    /// Starts ONNX Runtime; the reason when it cannot.
    pub fn new() -> Result<Self, String> {
        environment()?;
        Ok(Self {
            describe: None,
            keywords: None,
        })
    }

    fn florence(&mut self) -> Result<&mut Florence, BackendError> {
        self.describe
            .as_mut()
            .map(|(_, f)| f)
            .ok_or_else(|| BackendError::Lost("no describe model is loaded".into()))
    }

    /// A caption with any picking, for the prototype and the real-model
    /// tests.
    pub fn caption_with(
        &mut self,
        image: &TagImage,
        pick: Pick,
        deadline: &Deadline,
    ) -> Result<String, BackendError> {
        let f = self.florence()?;
        let dog = Watchdog::start(deadline)?;
        let features = f.features(image.rgb, &dog, deadline)?;
        let raw = f.task(
            &features,
            "<MORE_DETAILED_CAPTION>",
            100,
            pick,
            &dog,
            deadline,
        )?;
        Ok(results::clean_caption(&raw).unwrap_or_default())
    }
}

impl TagBackend for OnnxBackend {
    fn load(
        &mut self,
        model: &ModelChoice,
        dir: &Path,
        threads: usize,
        vocabulary: &[&str],
        deadline: &Deadline,
    ) -> Result<(), BackendError> {
        let model = super::catalogue::Catalogue::real()
            .get(model.id)
            .ok_or_else(|| load_err(format!("{} is not in the catalogue", model.id)))?;
        match model.part {
            Part::Describe => {
                self.describe = None;
                self.describe = Some((model, Florence::load(dir, threads)?));
            }
            Part::Keywords => {
                self.keywords = None;
                self.keywords = Some(Clip::load(model, dir, threads, vocabulary, deadline)?);
            }
        }
        Ok(())
    }

    fn unload(&mut self, part: Part) {
        match part {
            Part::Describe => self.describe = None,
            Part::Keywords => self.keywords = None,
        }
    }

    fn loaded(&self, part: Part) -> Option<&'static str> {
        match part {
            Part::Describe => self.describe.as_ref().map(|(m, _)| m.id),
            Part::Keywords => self.keywords.as_ref().map(|c| c.model.id),
        }
    }

    fn describe(
        &mut self,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<Described, BackendError> {
        let f = self.florence()?;
        let dog = Watchdog::start(deadline)?;
        let features = f.features(image.rgb, &dog, deadline)?;
        let caption = f.task(
            &features,
            "<MORE_DETAILED_CAPTION>",
            100,
            Pick::Greedy,
            &dog,
            deadline,
        )?;
        let od = f.task(&features, "<OD>", 256, Pick::Greedy, &dog, deadline)?;
        Ok(Described {
            description: results::clean_caption(&caption),
            objects: results::count_objects(&od),
        })
    }

    fn caption(&mut self, image: &TagImage, deadline: &Deadline) -> Result<String, BackendError> {
        self.caption_with(image, SAMPLED, deadline)
    }

    fn keywords(
        &mut self,
        image: &TagImage,
        deadline: &Deadline,
    ) -> Result<Keyworded, BackendError> {
        self.keywords
            .as_mut()
            .ok_or_else(|| BackendError::Lost("no keywords model is loaded".into()))?
            .keywords(image, deadline)
    }

    fn text_embedder(&self) -> Option<Arc<dyn TextEmbedder>> {
        self.keywords
            .as_ref()
            .map(|c| Arc::clone(&c.text) as Arc<dyn TextEmbedder>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_read_from_the_build_info() {
        let info = "ORT Build Info: git-branch=rel-1.31.1, git-commit-id=26250ae, build type=Release";
        assert_eq!(version_of(info), "1.31.1");
        assert_eq!(version_of(" custom build "), "custom build");
    }

    #[test]
    fn repeated_ngrams_are_banned() {
        // "a b c a b" with n = 3: the next "c" would repeat "a b c".
        assert_eq!(banned_tokens(&[1, 2, 3, 1, 2], 3), vec![3]);
        assert!(banned_tokens(&[1, 2], 3).is_empty());
        assert!(banned_tokens(&[1, 2, 3], 0).is_empty());
    }

    #[test]
    fn greedy_takes_the_likeliest_allowed_token() {
        let mut rng = 1;
        assert_eq!(choose(&[0.1, 3.0, 2.0], &[], Pick::Greedy, &mut rng), 1);
        assert_eq!(choose(&[0.1, 3.0, 2.0], &[1], Pick::Greedy, &mut rng), 2);
    }

    #[test]
    fn the_library_is_found_in_a_folder_only_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(library_in(dir.path()), None);
        std::fs::create_dir(dir.path().join(LIBRARY_FILE)).unwrap();
        assert_eq!(library_in(dir.path()), None, "a folder is not the library");
        std::fs::remove_dir(dir.path().join(LIBRARY_FILE)).unwrap();
        std::fs::write(dir.path().join(LIBRARY_FILE), b"x").unwrap();
        assert_eq!(library_in(dir.path()), Some(dir.path().join(LIBRARY_FILE)));
    }

    #[test]
    fn sampling_stays_in_the_top_k_and_varies() {
        let mut rng = 7;
        let row: Vec<f32> = (0..100).map(|i| i as f32 / 10.0).collect();
        let pick = Pick::Sampled {
            temperature: 0.7,
            top_k: 5,
        };
        let seen: std::collections::HashSet<u32> = (0..200)
            .map(|_| choose(&row, &[], pick, &mut rng))
            .collect();
        assert!(seen.iter().all(|t| *t >= 95));
        assert!(seen.len() > 1);
    }

    /// The real-model checks (docs/07-tagging.md, "Real-model tests"). They
    /// run only when `OWLBUM_TEST_MODELS_DIR` names a folder of downloaded
    /// models (`<folder>/<model id>/…`) and ONNX Runtime can be found
    /// (`OWLBUM_ORT_DYLIB`); otherwise they pass without doing anything.
    /// They never download.
    #[test]
    fn real_models_when_a_folder_is_given() {
        let Some(dir) = std::env::var_os("OWLBUM_TEST_MODELS_DIR").map(PathBuf::from) else {
            return;
        };
        let mut b = OnnxBackend::new().expect("ONNX Runtime starts");
        let cat = super::super::catalogue::Catalogue::real();
        let long = || Deadline::after(Duration::from_secs(600));
        let lake = owlbum_fixtures::picture::lake(640, 480);
        let img = TagImage {
            rgb: &lake,
            name: "lake.jpg",
        };
        for m in cat.all() {
            let mdir = dir.join(m.id);
            if !mdir.is_dir() {
                continue;
            }
            assert_eq!(
                super::super::install::verify(&mdir, m, true),
                super::super::install::InstallState::Ok,
                "{} passes its hashes",
                m.id
            );
            let t = std::time::Instant::now();
            b.load(
                m,
                &mdir,
                default_threads(),
                super::super::vocabulary::VOCABULARY,
                &long(),
            )
            .expect("loads");
            println!("{}: loaded in {:.1} s", m.id, t.elapsed().as_secs_f32());
            match m.part {
                Part::Describe => {
                    let t = std::time::Instant::now();
                    let d = b.describe(&img, &long()).unwrap();
                    println!("{}: {:.1} s a photo", m.id, t.elapsed().as_secs_f32());
                    let text = d.description.unwrap();
                    assert!(
                        text.len() > 20 && !text.contains('<') && !text.contains('>'),
                        "{text}"
                    );
                    let again = b.describe(&img, &long()).unwrap();
                    assert_eq!(
                        again.description.as_deref(),
                        Some(text.as_str()),
                        "greedy is repeatable"
                    );
                    let sampled: std::collections::HashSet<String> =
                        (0..3).map(|_| b.caption(&img, &long()).unwrap()).collect();
                    assert!(sampled.len() > 1, "sampled captions vary");
                }
                Part::Keywords => {
                    let k = b.keywords(&img, &long()).unwrap();
                    assert_eq!(k.vector.len(), m.dim.unwrap() as usize);
                    let len: f32 = k.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                    assert!((len - 1.0).abs() < 1e-4);
                    assert!(k.keywords.len() <= results::MAX_KEYWORDS);
                    // The painted lake fixture is not recognised as a lake
                    // by CLIP (#52), so `lake` is not required here.
                }
            }
        }
    }
}
