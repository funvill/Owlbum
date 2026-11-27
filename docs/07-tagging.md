# Tagging ("What's in it")

Owlbum can look at each photo of the open album with machine-learning models that run on the computer, and record what it finds:

- a **description**: a sentence or a short paragraph about the photo;
- **objects**: labels with counts (`dog ×2`);
- **keywords**: up to 12 words from a fixed list of 564;
- a **vector**: a list of numbers that places the picture near pictures and phrases that mean similar things. It powers **search by meaning**.

Nothing about a photo leaves the computer. The only network use is downloading a model's files, and only after the user asks for it in Settings.

This part builds on [Architecture](01-architecture.md) and [Data](02-data.md). Results live in the album database tables `tags` and `vectors`, keyed by photo id; the `photo_labels` table and three columns of the `search` table are derived from `tags` for filters and text search. There are no other files.

## The two parts

Tagging is two independent jobs, called **parts**.

| Part id | Shown in Settings as | Note under the title | Produces | Model family today |
|---|---|---|---|---|
| `describe` | "Descriptions and objects" | "A sentence about each photo, and the things in it." | description, objects | Florence-2 |
| `keywords` | "Keywords and search by meaning" | "Keywords for each photo, and search that finds photos by what you mean." | keywords, vector | CLIP |

For each part the user chooses **Off** or one installed model. The choice is the setting `tagging.describe` or `tagging.keywords` in `app.sqlite`: a model id, or null for Off. With both parts Off, tagging is off.

The choice belongs to the computer, not the album. The results belong to the album. An album tagged on one computer shows its tags on another computer where tagging is off.

Terms used below:

| Term | Meaning |
|---|---|
| Chosen model | The model id stored in the setting for a part. |
| Active model | A chosen model that is installed, verified, and whose part is not in the not-working set. Only active models run. |
| Not working | The set of parts whose chosen model is damaged, would not load, or kept failing. Kept in memory. |
| Eligible photo | A row of `photos` inside the open scope with `missing = 0` and `scan_error` NULL. Stills, RAW files and videos are all eligible. Rows with `scan_error`, rows with `missing = 1` and rows outside the scope are not tagged; the photo score in [Organising](05-organising.md#inputs) uses the same rule, so the same rows are not scored. A video is tagged from its poster frame, the picture [Media and scanning](03-media-and-scanning.md#the-video-handler) analyses and makes the medium thumbnail from. |

## The model catalogue

The catalogue is a table compiled into `owlbum-core`. It is never fetched. Adding or replacing a model is a code change: a new entry, with a new id. An id is never reused for different files, because rows in `tags` and `vectors` are keyed by it.

```rust
pub enum Part { Describe, Keywords }
pub enum Family { Florence2, Clip }           // which pipeline in this document runs the files
pub enum Size { Smaller, Larger }
pub enum Licence { Mit, Apache2, Bsd3Clause }

pub struct ModelFile {
    pub path: &'static str,     // inside the repo, and inside <app data>/models/<id>/
    pub bytes: u64,
    pub sha256: &'static str,   // 64 hex characters
}

pub struct ModelChoice {
    pub id: &'static str,            // [a-z0-9-]+
    pub part: Part,
    pub family: Family,
    pub size: Size,
    pub label: &'static str,
    pub repo: &'static str,          // Hugging Face repository
    pub revision: &'static str,      // 40 hex characters: a pinned commit
    pub files: &'static [ModelFile],
    pub licence: Licence,
    pub seconds_per_photo: f32,      // typical, six-core desktop processor; shown in Settings
    pub dim: Option<u32>,            // keywords models: length of the vector
    pub keyword_threshold: Option<f32>,  // keywords models: smallest softmax share for a keyword
    pub meaning_threshold: Option<f32>,  // keywords models: smallest cosine for search by meaning
}
```

`model_bytes(m)` is the sum of `files[].bytes`. `choices_for(part)` returns that part's models, smaller first.

Rules checked by a unit test:

| Rule |
|---|
| Ids are unique and match `[a-z0-9-]+`. |
| Every `revision` is 40 hex characters. Every `sha256` is 64 hex characters. Every `bytes` is above zero. No `path` contains `..`, a backslash, a drive letter or a leading `/`. |
| Each part has exactly one `Smaller` and one `Larger` model in version 1. |
| A keywords model has `dim` and both thresholds, each threshold strictly between 0 and 1. A describe model has none of the three. |
| The licence is one of the three listed. |
| No repository lives under `deepinsight/` or has `insightface` in its name. Their models do not allow commercial use. |

### The four models

| id | part | size | label | repo @ revision | total bytes | s per photo | licence | dim | keyword threshold | meaning threshold |
|---|---|---|---|---|---|---|---|---|---|---|
| `florence2-base-q8` | describe | smaller | Florence-2 base | `onnx-community/Florence-2-base-ft` @ `e88a44eaf3791a35eae0c5a47b3dbcd36e67eb6f` | 277,470,179 (277 MB) | 6 | MIT | | | |
| `florence2-large-q8` | describe | larger | Florence-2 large | `onnx-community/Florence-2-large-ft` @ `04ace74913c28d7ec94af32cee5c111a10126b6f` | 832,001,774 (832 MB) | 20 | MIT | | | |
| `clip-base-q8` | keywords | smaller | CLIP base | `Xenova/clip-vit-base-patch32` @ `6b7c523703ae26e39be4b03774966f8571c7f523` | 155,851,446 (156 MB) | 0.1 | MIT | 512 | 0.05 | 0.27 |
| `clip-large-q8` | keywords | larger | CLIP large | `Xenova/clip-vit-large-patch14` @ `c307790166907339eed5a9a53a249af534102536` | 434,215,233 (434 MB) | 0.5 | MIT | 768 | 0.05 | 0.24 |

All four use the 8-bit quantised ONNX files (`q8` in the id). The seconds-per-photo figures and the thresholds were measured with the old program's runtime on 2026-11-24. The prototype (#52) measured the base models with the Rust runtime on a four-core Intel i5-3550 without AVX2, two threads: Florence-2 base 12 to 25 s a photo (caption and objects; load 3 to 7 s), CLIP base 0.4 to 0.6 s a photo (load 6 to 11 s, most of it the 564 vocabulary prompts). That processor is far slower than the six-core reference, so the catalogue keeps the old figures until they are measured on one; the large models were not measured. The `dim` values are what these two CLIP shapes normally produce; the prototype confirms them.

### Files of each model

**florence2-base-q8**

| path | bytes | sha256 |
|---|---|---|
| `config.json` | 5432 | `d90c22ed72eb55291f183fcd9b98ebd3bd3d92bfcffb6c7f6e1606085e793525` |
| `generation_config.json` | 292 | `7b8eb17bbd6cf8a07f619ad83ae03881eff05b6b9237bab89005b40e77783c29` |
| `onnx/decoder_model_merged_quantized.onnx` | 98177697 | `f22f52f980c33df0efa15932c2f3db6d9d3595ce6387eca938b8cfe23dc4c641` |
| `onnx/embed_tokens_quantized.onnx` | 39390433 | `6b2258db1c8ee9b160576ccde3cd3814d83a2edaed0dd1c6ca9ff3c38fa62214` |
| `onnx/encoder_model_quantized.onnx` | 43651493 | `f4ad7a68f1fb875d3bcf735ea14a7021b7ba7e83baf7cf10289881b4ed6d9b85` |
| `onnx/vision_encoder_quantized.onnx` | 93746540 | `3b79d54f23f666f731549db23cb070c35a979ce19cbd9720e90e67a78dc9768c` |
| `preprocessor_config.json` | 2673 | `c892857e34a7082284983a7717717d39c9bf7e574f1f41d80d4c918c97502efa` |
| `tokenizer.json` | 2297961 | `d69dcdb2323e124ac4f800cb9863ddccea0d7bb11e16125e8df3bd60f2f8aeac` |
| `tokenizer_config.json` | 197658 | `d8e64607233cb53b619fb46664f6cad08176c26e0e8735b2d30d888364f19600` |

**florence2-large-q8**

| path | bytes | sha256 |
|---|---|---|
| `config.json` | 5460 | `226291bd5b65c932b01704838489cdc810f0f4949f182abc3de67d7a6ab3632a` |
| `generation_config.json` | 62 | `6257874d1d2fd60a5c0786253005d9dfa46f5287903fdb079ca9829ec13cf8b4` |
| `onnx/decoder_model_merged_quantized.onnx` | 257619824 | `167e7807fb1e5b8d693138f84bc81e1368ea11833264131c1ea96eafc9d13222` |
| `onnx/embed_tokens_quantized.onnx` | 52520480 | `dab1d09ab449f1302df6e439b35ec9e93fb037134bf6bed9ffeaa93a5b35d43c` |
| `onnx/encoder_model_quantized.onnx` | 152985855 | `9fe30b02cc8c2247631f8f0b9bc4d6613df1c33da0afe4087abff2f940bc2fe0` |
| `onnx/vision_encoder_quantized.onnx` | 366371802 | `0dbf2172ef1dc6fbb4da0f02498ef9e3a009bf84e8f2e0d971f39cc5b7e053ce` |
| `preprocessor_config.json` | 2672 | `3adbbeec44c2b8f41a1e97b3ab2197227fb24be1fc1779c9d0d925f4e77d6559` |
| `tokenizer.json` | 2297961 | `d69dcdb2323e124ac4f800cb9863ddccea0d7bb11e16125e8df3bd60f2f8aeac` |
| `tokenizer_config.json` | 197658 | `d8e64607233cb53b619fb46664f6cad08176c26e0e8735b2d30d888364f19600` |

**clip-base-q8**

| path | bytes | sha256 |
|---|---|---|
| `config.json` | 4524 | `493ef57ff783e42d1530c91b53469b7fdf8db8a9c1408e86998fcb7899a4f495` |
| `onnx/text_model_quantized.onnx` | 64504507 | `73baab855d406190da9faa498cfedf65f15cf309f4cc7385b7b032e6d08e5c3a` |
| `onnx/vision_model_quantized.onnx` | 89117001 | `583fd1110a514667812fee7d684952aaf82a99b959760c8d7dca7e0ab9839299` |
| `preprocessor_config.json` | 520 | `6f638fb9401a6d6296feff533ee7efe657b787c49f954f82f5906b36ef2a1b1f` |
| `tokenizer.json` | 2224119 | `f7f3b7af117d467b58374797691a6438d3e6b9e9cef800dfd5dced7f697a90cd` |
| `tokenizer_config.json` | 775 | `60ba2912bc6344c94bc16bbdec27fa1209409167b6f2fdf3cfe9e65462ea3967` |

**clip-large-q8**

| path | bytes | sha256 |
|---|---|---|
| `config.json` | 694 | `99d2f15ccaf4f72c1e4c656dc77ae0fa487696860617ea50626d3a14a018185d` |
| `onnx/text_model_quantized.onnx` | 124987663 | `6d31c7a212681c3918d78193b6b4f98c70fe540577f9a0a57fe7ba8c7439244c` |
| `onnx/vision_model_quantized.onnx` | 307001500 | `9a658de8944a2001719beabf6c5030cfe7b3dc80e85983f69fc648d6b4d7ec0f` |
| `preprocessor_config.json` | 520 | `6f638fb9401a6d6296feff533ee7efe657b787c49f954f82f5906b36ef2a1b1f` |
| `tokenizer.json` | 2224081 | `72ed5c96db5729294468543e4bc75fce14ca63f58e37300290189ba1c1e52b85` |
| `tokenizer_config.json` | 775 | `60ba2912bc6344c94bc16bbdec27fa1209409167b6f2fdf3cfe9e65462ea3967` |

The two CLIP models share one `preprocessor_config.json` and one `tokenizer_config.json`.

The Rust runtime may need ONNX files with other names, or a different export of the same weights (see Open questions). If so, the file lists above change, and with them the byte counts, hashes and perhaps the ids. Nothing else in this document changes.

## Model files on disk

| Item | Location |
|---|---|
| A model's files | `<app data>/models/<id>/<file.path>`. The layout mirrors the repository, so ONNX files sit in `onnx/`. |
| A partial download | `<app data>/models/<id>/<file.path>.part` |
| The verified marker | `<app data>/models/<id>/verified.json` |

`verified.json` records what Owlbum has hashed: `{ "revision": "<40 hex>", "files": { "<path>": { "bytes": n, "mtime_ms": n, "sha256": "<64 hex>" } } }`. It is written with a temporary file and a rename.

### Install state

A model is in exactly one state. Working it out reads file sizes and times only, so it is quick.

| State | Rule | Can be chosen |
|---|---|---|
| `absent` | The folder does not exist, or holds none of the catalogue's files and no `.part` file. | No |
| `partial` | Some files are missing, or a `.part` file exists, and no file is known to be damaged. | No |
| `unverified` | Every file is present with the catalogue's size, but a file has no marker entry, or its size or modified time differs from its entry, or the marker's revision differs. | No |
| `checking` | An `unverified` model is being hashed. | No |
| `ok` | Every file is present, has the catalogue's size, and matches its marker entry (size, modified time, and the catalogue's SHA-256). | Yes |
| `damaged` | A file has the wrong size, or failed its SHA-256 check. | No |

A model becomes `ok` only after every file's SHA-256 has been computed and matched. Hashing 832 MB takes a few seconds, so it is not repeated on every start: the marker carries the result, and the size and modified time say whether the file has been touched since.

An `unverified` model is hashed in the background the first time `tagging_models` sees it. It then becomes `ok` or `damaged`. Files that fail are deleted, so "Download again" fetches only those.

### Verification after a failed load

When a model fails to load, every present file is hashed again whatever the marker says. A file whose size or SHA-256 differs is deleted and its marker entry removed. The model then reports `damaged` or `partial`, its part joins the not-working set, and Settings offers "Download again". If every file passes, the files are sound and the fault is elsewhere: the part still joins the not-working set, and the load error is written to the log.

### Removal

`tagging_remove(model)` cancels that model's download if one is running, deletes `<app data>/models/<id>` with everything in it, and sets the part to Off if this model was chosen. If the model is loaded it is unloaded first. Results already in album databases are kept.

## Model download

One download runs at a time, on its own thread. It never runs unless the user chose Download and confirmed.

### Allowed hosts

A URL may be fetched only when all of these hold:

| Rule |
|---|
| It parses, and its scheme is `https`. |
| It has no explicit port. |
| It has no user name and no password. |
| Its host, lower-cased, is `huggingface.co`, or ends in `.huggingface.co`, or ends in `.hf.co`. |

| URL | Allowed |
|---|---|
| `https://huggingface.co/x/resolve/abc/model.onnx` | yes |
| `https://cdn-lfs.huggingface.co/...` | yes |
| `https://cas-bridge.xethub.hf.co/...` | yes |
| `https://cdn-lfs.hf.co/...` | yes |
| `http://huggingface.co/x` | no |
| `https://huggingface.co.evil.com/x` | no |
| `https://evilhuggingface.co/x` | no |
| `https://huggingface.co:8443/x` | no |
| `https://user:pw@huggingface.co/x` | no |

### Redirects

The HTTP client follows no redirect by itself. Owlbum follows them by hand, at most 5 hops. Every hop's URL must pass the allowed-hosts rule. The `Range` header is sent on every hop. No cookie and no credential is ever sent.

### Algorithm

```text
download(model):
  total = sum(files.bytes); received = 0
  for each file f, in catalogue order:
    if cancelled: stop with "Download cancelled."
    target = <dir>/<f.path>
    if target exists with size f.bytes:
      if its marker entry matches, or sha256(target) == f.sha256:
        write its marker entry; received += f.bytes; report; continue
    delete target
    download_file(f)
  state is now ok

download_file(f):
  part = target + ".part"; create the parent folders
  have = size(part), or 0; if have > f.bytes: delete part; have = 0
  received += have; report
  if have < f.bytes:
    url = https://huggingface.co/<repo>/resolve/<revision>/<f.path>
    res = get(url, Range: "bytes=<have>-" when have > 0)
    if have > 0 and res.status is 200 or 416:        // the server ignored or refused the range
      delete part; received -= have; have = 0
      if res.status is 416: res = get(url)
    if res.status is not 200 or 206: fail "Hugging Face answered <status> for <path>."
    if res.status is 206 and its Content-Range does not start at <have>: delete part; fail the same way
    copy the body to part (append when have > 0), in chunks:
      received += chunk length; report
      if cancelled: stop with "Download cancelled."          // part is kept
      if more than f.bytes have arrived: stop reading         // the check below fails it
      if no byte arrives for 60 s: fail "The download stalled. Try again."   // part is kept
      on a write error: fail (see wording)                    // part is kept
  if size(part) != f.bytes or sha256(part) != f.sha256:
    delete part; fail "<path> did not pass its check (wrong size or SHA-256); it was deleted. Try again."
  flush part to disk; rename part to target; write its marker entry
```

A file found already complete is hashed unless its marker entry already vouches for it. Nothing is trusted on size alone.

### Resume and cancel

A cancelled or failed download leaves its `.part` files. The next download of the same model continues them with a `Range` request. `tagging_download_cancel()` stops the download and returns only after it has stopped. Cancel is not an error and leaves no error text. Closing Owlbum cancels a running download the same way.

### Progress

`download_progress { model, received, total, error, finished }` is emitted at most 10 times a second while bytes arrive, and once when the download ends, with `finished: true`. `received` can go down when a server ignores a range. On the last event, `received == total` with `error` null means installed, and a non-null `error` means failed. After a cancel the page learns the state from the reply to `tagging_download_cancel` and re-reads `tagging_models`.

### Error wording

A failed download is reported as "Couldn't download {label}: {reason}" in `download_progress.error` and in `tagging_models().error`, with code `download_failed`. Starting another download clears it.

| Cause | Reason text |
|---|---|
| A status other than 200 or 206 | "Hugging Face answered {status} for {path}." |
| A redirect to a host that is not allowed | "Download host not allowed: {host}" |
| A redirect with no `Location` | "Hugging Face sent a redirect without a location." |
| More than 5 redirects | "Too many redirects from Hugging Face." |
| No connection, DNS failure, TLS failure | "Couldn't reach Hugging Face. Check the internet connection and try again." |
| No bytes for 60 s | "The download stalled. Try again." |
| The disk is full | "Not enough disk space for {path}. Free some space and download again." |
| Any other write error | "Couldn't write {path}: {system message}" |
| Wrong size or hash | "{path} did not pass its check (wrong size or SHA-256); it was deleted. Try again." |

Errors returned by the commands themselves:

| Command | Code | Message |
|---|---|---|
| `tagging_download`, `tagging_remove` | `no_such_model` | "No such model." |
| `tagging_download` | `download_busy` | "Another model is downloading. Wait for it, or cancel it." |
| `tagging_set` | `model_not_ready` | "Choose a downloaded model of the right kind, or Off." |

`tagging_set` accepts, for each part, null or the id of a catalogue model of that part whose state is `ok`. Every other state is refused, including `partial`, `unverified` and `damaged`.

## The runtime

### What runs the models

Models run in ONNX Runtime through the `ort` crate, on the processor only. No graphics card is used in version 1. Text is turned into tokens by the `tokenizers` crate, from each model's own `tokenizer.json`.

Owlbum drives `ort` directly (`owlbum_core::tagging::onnx`): CLIP is two sessions and a tokenizer, and Florence-2 is four sessions (vision encoder, token embedder, encoder, merged decoder) with a token loop written in Owlbum, which gives sampled captions, the deadline check between steps and the raw `<OD>` labels this document needs. The prototype (#52) chose this over `usls`: `usls` pins its own `ort` and fetches model files through its own hub client, and Owlbum needs neither. The code sits behind one trait, `TagBackend` (see Tests), and the rest of Owlbum does not know what runs the models.

The merged decoder is driven as `transformers.js` drives it: the first step has `use_cache_branch` false and empty caches; the encoder's cached keys and values from that step are kept for every later step. The model's own `generation_config.json` sets `no_repeat_ngram_size` 3, and Owlbum applies it to greedy and sampled captions alike (without it greedy captions repeat themselves); its `num_beams` 3 is not used. The prompts read from the pinned files are the ones this document expects, and CLIP base gives vectors of 512 numbers.

The runtime never touches the network. It is given a folder of local files and nothing else.

### Threads

| Work | Thread |
|---|---|
| The queue, model loading and unloading, every picture run, Regenerate | The one tagging thread, created the first time it is needed. |
| Turning search text into a vector | The blocking-pool thread of the `search_meaning` command, using the loaded text session. |
| Downloading and hashing model files | One download thread. |
| Writing results | The album's writer thread, by message. |

Search text is the one model run that does not happen on the tagging thread. A describe run takes many seconds, and a search must not wait behind it. ONNX Runtime allows several threads to run one session at once, so the search runs the CLIP text session directly. A lock around each model slot stops the tagging thread unloading a model while a search is using it.

### Session options

| Option | Value |
|---|---|
| Intra-op threads | `max(1, floor(logical processors / 2))` |
| Inter-op threads | 1 |
| Execution mode | Sequential |
| Execution provider | CPU |
| Graph optimisation | The runtime's default level |

Every session uses these. A search that arrives during a describe run briefly uses the other half of the processors.

### Loading and unloading

There are two slots, one for each part. A slot holds at most one loaded model.

```text
load(part, model):
  if the model's state is not ok: fail "{label} is incomplete ({paths}). Download it again."
  drop whatever the slot holds
  describe: open the vision encoder, the token embedder, the encoder and the merged decoder;
            read the tokenizer; read the image settings and task prompts from preprocessor_config.json
  keywords: open the vision and text sessions; read the tokenizer and preprocessor_config.json;
            embed the 564 vocabulary prompts (see Keywords); check every vector has length model.dim
  the whole load has 180 s; after that it fails
```

A failed load leaves the slot empty, runs the verification above, and puts the part in the not-working set. A loaded model is unloaded when:

| Event |
|---|
| Its part is set to Off or to another model. |
| The model is removed, or found damaged. |
| A run on it timed out or panicked (the next run loads it again). |
| The tagging thread has had nothing to do for 5 minutes. |
| No album has been open for 5 minutes. |
| Owlbum closes. |

When an album opens and a keywords model is active, that model is loaded before the first photo is tagged, so the first search does not wait.

If the runtime itself cannot start (for example its library cannot be loaded), both parts join the not-working set and `problem` reads "The model runtime would not start: {reason}".

### Memory

A loaded model holds roughly its file size in memory, plus working memory during a run. The planning figures are: Florence-2 base about 0.5 GB, Florence-2 large about 1.5 GB, CLIP base about 0.3 GB, CLIP large about 0.7 GB. These are estimates until the prototype measures them. The rules that keep memory in check are: one model per part, one photo at a time, unload after 5 idle minutes, and pictures passed to a model at the model's own small input size.

### Timeouts

Every model run has a time limit.

| Run | Limit |
|---|---|
| Model load, including the vocabulary vectors | 180 s |
| Caption (tagging or Regenerate) | 120 s |
| Object detection | 120 s |
| Keywords and vector for one picture | 30 s |
| Search text to vector | 15 s |

A running thread cannot be killed, so a limit is enforced in three steps:

1. **Between steps.** Florence-2 writes one token per decoder run. The deadline is checked before each one, so a slow caption stops cleanly within one step.
2. **Inside a run.** A watchdog asks ONNX Runtime to end the run in progress when the deadline passes. `ort` exposes this through its run options; the prototype confirms it for the pinned version.
3. **Stuck.** If the run has not returned 30 s after it was asked to end, the tagging thread is written off. A new tagging thread is started with empty slots. The stuck thread is left to finish or not, and its result is ignored. (Not built yet: steps 1 and 2 are. The prototype saw a run end within about half a second of its deadline through the run options.)

What a timeout does:

| Where | Result |
|---|---|
| Tagging a photo | A `failed` row for that photo and model with the error "Tagging this photo took too long." The model is unloaded. It counts as one **loss** for the part (see The broken-model rule). |
| Regenerate | The command fails with code `tagging_timeout`: "{label} took too long. Try again." The old sentence stays. The model is unloaded. |
| Search text | `search_meaning` returns no extra matches, and the timeout is logged. |
| Model load | The load fails, with everything a failed load does. |

## How results are derived

Where this section fixes a detail (how a caption is cleaned, how keywords are scored), the aim is that Owlbum produces what the old program produced from the same model files. The prototype compares the two on the fixture album.

### The picture given to a model

Every model is given the photo's **medium thumbnail**: the upright 1600 px JPEG. The tagging thread asks the thumbnail cache for it, and the cache makes it if it is missing, so pruning the cache never stops tagging. A video is tagged from the thumbnail of its poster frame; a video with no poster frame (no codec installed) has no thumbnail and gets the failed row below. A stored edit is ignored: the model sees the photo as it is in the file.

If the thumbnail cannot be made or decoded, each active model gets a `failed` row with the error "Couldn't read the photo."

Each model then resizes and normalises the picture as its own `preprocessor_config.json` says (size, resampling, mean, standard deviation). Florence-2 squashes the picture to its square input. CLIP resizes the shorter side and crops the centre.

### Description (Florence-2 caption)

```text
florence_task(image, task, max_new_tokens, sampling):
  prompt   = task_prompts_without_inputs[task]        // from preprocessor_config.json
  features = vision_encoder(preprocess(image))
  inputs   = concat(features, embed_tokens(tokenise(prompt)))
  memory   = encoder(inputs)
  ids      = generate(decoder, memory, max_new_tokens, sampling)   // deadline checked each step
  return decode(ids, keep special tokens)

caption(image, sampling):
  text = florence_task(image, "<MORE_DETAILED_CAPTION>", 100, sampling)
  remove "<s>", "</s>" and "<pad>"
  remove every remaining tag: each match of the pattern <[^>]*>
  trim white space
```

| Setting | Value |
|---|---|
| Task token | `<MORE_DETAILED_CAPTION>` |
| Token limit | 100 new tokens |
| Prompt text | Read from the model's files, never typed into Owlbum. For the pinned models it is expected to be "Describe with a paragraph what is shown in the image."; the prototype confirms it. A model whose file has no prompt for the task fails to load. |
| Tagging | **Greedy**: at each step the likeliest token is taken. The same picture always gives the same sentence. |
| Regenerate | **Sampled**: temperature 0.7, then one token drawn at random from the 50 likeliest (`top_k = 50`). Each run can give different words. |
| Stop | At the end-of-sentence token, or at the token limit. |

An empty caption during tagging is stored as a `done` row with a NULL description. An empty caption from Regenerate is an error (see Regenerate).

### Objects (Florence-2 object detection)

A second generation runs on the same picture, reusing the vision features: task `<OD>`, 256 new tokens, always greedy. The expected prompt is "Locate the objects with category name in the image."

```text
text = florence_task(image, "<OD>", 256, greedy), with "<s>", "</s>" and "<pad>" removed
labels = []
for each match of  ([^<]+)?<loc_N><loc_N><loc_N><loc_N>  in text:      // N is a number
  label = the captured text if there is one, else the previous match's label
  labels.push(label)

counts = empty map
for l in labels:
  label = lower_case(trim(l)); if label is not empty: counts[label] += 1
objects = the pairs {label, count}, sorted by count descending, then by label ascending (code-point order)
```

The boxes are not kept. There is no threshold and no limit. Labels are always lower case.

`describe` for one photo is the greedy caption followed by object detection. Both must succeed for the row to be `done`.

### Keywords and the vector (CLIP)

```text
LOGIT_SCALE  = 100
MAX_KEYWORDS = 12

at load:
  for each word w of the vocabulary, in order, in batches of 64 with padding and truncation:
    vocab[w] = unit(text_model(tokenise("a photo of " + w)))

for one picture:
  v       = unit(vision_model(preprocess(image)))          // length model.dim
  sims[i] = dot(v, vocab[i])                               // cosine: both are unit length
  top     = max(sims)
  p[i]    = exp(100 × (sims[i] − top)) / Σ_j exp(100 × (sims[j] − top))     // softmax over all 564
  keywords = the pairs {label: word[i], score: p[i]}
             with p[i] >= model.keyword_threshold
             sorted by score descending, then by vocabulary position
             cut to the first 12
  return { keywords, vector: v }

unit(x) = x / sqrt(Σ x²); a zero length is treated as 1
```

- The vocabulary is the fixed list of 564 words in the appendix. Every entry is lower case, so **keywords are always lower case**.
- `score` is the word's share of the softmax, between 0 and 1. With a threshold of 0.05 at most 20 words could pass; the cap is 12.
- A picture can have no keywords. It is still `done`, and it still has a vector.
- Before a vector is stored it is checked: its length equals `model.dim`, every number is finite, and its length is not zero. A vector that fails is a `failed` row with the error "The model returned an unusable vector."
- Changing the vocabulary affects only photos tagged afterwards. The vector does not depend on the vocabulary.

### Text for search by meaning

The search text is given to the CLIP text model as typed, with no "a photo of" in front, truncated to the model's token limit. The result is made unit length. The last 32 text vectors for each model are kept in memory, so repeating a search costs nothing.

### What is stored

For each photo and each active model, one row of `tags`:

| Column | `describe` row | `keywords` row |
|---|---|---|
| `status` | `done` or `failed` | `done` or `failed` |
| `description` | the sentence, or NULL | NULL |
| `objects` | JSON `[{label, count}]`, NULL when empty | NULL |
| `keywords` | NULL | JSON `[{label, score}]`, NULL when empty |
| `error` | the message, on `failed` only | the message, on `failed` only |
| `made_at` | milliseconds since the epoch | the same |

A `done` keywords row always has a row in `vectors` with the same photo id and model id, written in the same transaction: `dim` and `dim × 4` bytes of little-endian float32.

Rows for models that are no longer chosen are kept. Switching back to an earlier model costs no tagging.

**The search index and the label table.** The columns `tag_description`, `keywords` and `objects` of the `search` table, and the rows of `photo_labels` that the keyword and object filters read (one row per keyword or object, lower case), do not depend on this computer's settings, because the album can move. For each part they use the **newest `done` row** (largest `made_at`) of any model, as [Data](02-data.md#schema) says. The writer thread updates both in the transaction that writes a `tags` row.

**When the picture changes.** When the scanner finds that a photo's bytes changed (a new `file_key`, for example after an edit was saved into its file), the writer deletes that photo's `tags`, `vectors` and `photo_labels` rows in the transaction that stores the new scan facts. The ids arrive in `rekeyed` on `album_changed`. The photo is tagged again. A rename or a move keeps the `file_key`, so it keeps the rows.

### Copies of the same picture

Two rows are **copies** when they have the same `file_key` and the same perceptual hash (`dHash`) in `analysis`. The second test is there because `file_key` reads only the first and last 16 KiB of a file, and two different uncompressed scans of the same size can share both.

| Situation | Rule |
|---|---|
| A result is written for a photo | The same `tags` row, and the same vector, are written for every copy in the album that has no row for that model. `failed` rows are copied too: the same bytes fail the same way. |
| A new photo appears and a copy of it already has a row | The row is copied to the new photo at once. No model runs. |
| Regenerate | The new sentence is written to every copy's row for that model. |
| Try failed photos again | Clearing one copy's failed row clears the others'. |
| Search by meaning | Copies appear once (see Search by meaning). |

## The tagging queue

There is one queue. It serves the open album, and it runs on the tagging thread.

### What needs work

A photo needs work when it is eligible and some active model has no row for it in `tags`. A `failed` row is a row: a failed photo is not tried again until the user asks.

The queue does nothing when:

- no album is open, or the album is view-only;
- there is no active model;
- tagging is paused;
- a full scan of the album is running. Tagging starts when the scan finishes, so the two do not fight over the processor.

When work starts, one query per active model lists the ids that need work, in photo id order (the order the scanner found them). The list is kept in memory and maintained as rows are written and photos arrive. It is rebuilt when the active models change.

### Ordering

1. **The focus list first**, in order. The page sends `tagging_focus(ids)`: the selected photo, then the photos in the rows on screen, at most 200. It sends it 500 ms after the selection, the scroll position or the query last changed, and only when the list differs from the last one sent. The queue keeps the first 1,000 ids it is given. A focus list sent during a scan is kept for when tagging starts.
2. Then **scan order**: the next id in the list.

One photo is tagged at a time. For that photo the active models run one after the other: `describe`, then `keywords`.

### Tagging one photo

```text
tag(photo):
  if the file is no longer there: skip it (the scanner will mark it missing)
  image = the medium thumbnail, or none
  for m in active models, describe first:
    if the photo has a row for m: continue
    if a copy has a row for m: copy it; continue
    if no image:                         row = failed, "Couldn't read the photo."
    else run m with its time limit:
      success                            → row = done, with the results
      the model refused this picture     → row = failed, with the model's message
      timeout                            → row = failed, "Tagging this photo took too long."; loss(m.part)
      panic, or the model was not loaded → no row; loss(m.part)        // the photo is tried again
    if the album closed or changed meanwhile: stop; write nothing
  send the rows to the writer thread in one transaction, with the copies' rows and the search index
  a model with a done or failed row from a real run that was not a loss: clear its part's loss count
```

### Failure states

| State | Cause | What the user sees | Cleared by |
|---|---|---|---|
| A photo failed | The thumbnail could not be read, the model returned an error for it, or it ran out of time. A `failed` row holds the reason. | "Couldn't look at this photo" in the details panel, and a count in the tagging bar. | **Try failed photos again**; switching the part to another model (which has no row yet); the picture's bytes changing. |
| A part is not working | Its model is damaged, would not load, or lost three photos in a row. | The not-working note in the details panel and the bar; "Not working · Download again" in Settings. | Choosing a model in Settings, a download finishing, removing a model, or **Try again**. |
| Tagging stopped | Something outside the models failed, for example the database could not be written. The reason is in `problem`. | "Tagging stopped: {problem}" with **Try again**. | **Try again**. |

**Try failed photos again** (`tagging_retry_failed`) deletes the `failed` rows of the active models for the eligible photos, and their copies' rows. Those photos need work again and the queue picks them up. It returns how many photos were cleared.

**Try again** (`tagging_restart`) clears `problem`, the not-working set and the loss counts, then starts the queue.

### The broken-model rule

A **loss** is a run that ended for a reason that is not the photo's fault: a panic inside the run (caught on the tagging thread), a model that turned out not to be loaded, or a timeout.

```text
MAX_LOSSES = 3

loss(part):  unload the part's model; losses[part] += 1
             if losses[part] >= 3: add part to the not-working set
a photo that the part's model handled without a loss:  losses[part] = 0
```

After a loss the model is loaded again for the next photo. Three losses in a row mean the model, not the photos, is at fault, and the part stops. A failed load puts the part in the not-working set at once, without counting. Because a timeout is a loss, a model that hangs marks at most three photos failed before it is stopped.

### Pause and resume

`tagging_pause(true)` sets a flag in memory. The photo in hand finishes and its result is written; then the queue stops. `tagging_pause(false)` clears the flag and starts the queue.

The pause is never saved. It is cleared when an album is opened or closed, and when Owlbum starts. Regenerate and search by meaning work while tagging is paused.

### Status and the `tagging_progress` event

```text
tagging_progress {
  total:       number,       // eligible photos, when at least one model is active; else 0
  done:        number,       // of those, photos where every active model has a done row
  failed:      number,       // of those, photos where some active model has a failed row
  paused:      boolean,
  running:     boolean,      // the queue is tagging, or loading a model in order to tag
  not_working: string[],     // parts, in the order describe, keywords
  problem:     string | null // why tagging stopped by itself
}
```

A photo with a failed row counts as failed, never as done. Photos are counted, not pictures: two copies count as two.

The counts come from one query when an album opens, when a scan finishes, and when the active models change. After that they are kept up to date in memory as rows are written. Reading the status changes nothing: parts join or leave the not-working set only through the events listed under Failure states.

The event is emitted on every change of `paused`, `running`, `not_working` or `problem`, and otherwise at most 4 times a second while the counts move. `tagging_status()` returns the same payload, for when the page loads. The page never polls.

### Telling the page that tags changed

Results reach the page through `album_changed` with `reason: "tags"`. A keywords model can finish ten photos a second, and the page must not re-run a query over 100,000 photos that often. So:

| Rule |
|---|
| While tagging runs, `album_changed { reason: "tags" }` is emitted at most once every 5 s. |
| It is emitted at once when the photo just written is the first of the focus list (the selected photo), when the queue stops or finishes, and after Regenerate or Try failed photos again. |
| On this reason the page always re-reads the selected photo's detail. It re-runs its query only when the query has search text or a keyword or object filter. Nothing else in a result depends on tags. |

### Album open and close, new photos, view-only

| Event | Behaviour |
|---|---|
| An album opens | The pause is cleared. The counts are read. When the scan finishes, the queue starts. Photos tagged in an earlier session, or on another computer, already have rows and are not looked at again. |
| An album opens scoped to a subfolder | Only photos under the scope are tagged and counted. Copies outside the scope still receive copied rows. |
| The album closes, or another opens | The queue stops. The run in hand is told to end at its next step and its result is thrown away. The focus list, the pause and `problem` are cleared. Models stay loaded for 5 minutes in case an album opens. |
| The watcher adds photos | Each new row first takes a copy's results if there are any. Otherwise it joins the end of the list and `total` goes up. The queue starts by itself if it was idle. |
| The watcher removes a photo | The id leaves the list. A photo in hand whose file has gone is skipped and nothing is written. |
| A file's bytes change | Its rows are deleted (see What is stored) and it needs work again. |
| Settings change | `tagging_set` saves the choice, clears the not-working set and the loss counts, unloads a model that is no longer chosen, rebuilds the list and counts, and starts the queue. |
| A view-only album | Nothing is tagged, `total` is 0 and no bar shows. Existing tags are shown. Regenerate and Try failed photos again are disabled; the tooltip is the first sentence of the view-only banner in force (see [Browsing](04-browsing.md#album-banners)). Search by meaning still works over the vectors already stored, because it only reads. |
| Owlbum closes | The queue stops as for an album close, the download is cancelled, and the models are unloaded. |

## Regenerate

Regenerate writes a new description for one photo. It replaces only the sentence. Objects, keywords and the vector are untouched, and no other photo is looked at.

A plain second run would return the same sentence, because tagging is greedy. Regenerate therefore samples (temperature 0.7, `top_k` 50), so each press can give different words.

```text
tagging_regenerate(photo_id):
  if the album is view-only                          → view_only
  m = the active describe model; none                → no_describe_model
  row = the photo's tags row for m
  if there is none, or its status is not done        → not_described_yet
  on the tagging thread, after the photo in hand and before the next one:
    for attempt in 1..=2:
      load m if it is not loaded; if that fails      → model_load_failed
      text = caption(medium thumbnail, sampled), with the 120 s limit
        a timeout                                    → tagging_timeout
        a panic or a lost model, on attempt 1        → unload; try once more
        the same on attempt 2                        → tagging_lost
      if text is empty                               → empty_description     // nothing is saved
      update description and made_at of the row, and of every copy's row for m, where status is done
      update the search index; emit album_changed { reason: "tags" }
      return the photo's new tags (the PhotoTags shape under The page)
```

Regenerate works while tagging is paused and when the queue is idle. It loads the model if needed.

| Code | Message |
|---|---|
| `view_only` | The shared code. The message is the first sentence of the view-only banner in force. |
| `photo_not_found` | "That photo is not in the album any more." |
| `no_describe_model` | "There is no working descriptions model. Choose one in Settings." |
| `not_described_yet` | "{label} has not described this photo yet." |
| `model_load_failed` | "{label} would not load. See Settings." |
| `empty_description` | "{label} wrote an empty description. Try again." |
| `tagging_timeout` | "{label} took too long. Try again." |
| `tagging_lost` | "{label} stopped working. Try again." |
| `tagging_stopped` | "Tagging stopped." (Owlbum is closing, or the album closed.) |

On every error the old sentence stays.

## Search by meaning

Search by meaning finds photos whose picture is close to the typed words, even when no word matches. It adds to the text search; it never replaces it.

### Rules

| Rule | Value |
|---|---|
| Smallest query | 3 Unicode letters in the free text of the search box, after query words such as `star:yes` are taken out. `se` and `  a 1 2 b  ` do not search. |
| Model | The active keywords model. With none, there are no meaning matches. |
| Vectors compared | Rows of `vectors` with that `model_id` and with `dim` equal to the model's `dim`. Any other row is ignored. |
| Score | Cosine of the text vector and the picture vector. Both are unit length, so it is their dot product. |
| Threshold | The model's `meaning_threshold`: 0.27 for CLIP base, 0.24 for CLIP large. |
| Most results | 200 |
| Order | Score descending, then photo id ascending. |
| Copies | One result for each picture. |

Photos the chosen model has not yet looked at have no vector and cannot be found this way.

### Algorithm

```text
search_meaning(token):
  q = the free text of the query held under token, trimmed
  if letters(q) < 3: return { state: "too_short" }
  m = the active keywords model; none: return { state: "off" }
  t = text vector of q (15 s limit); on failure: log; return { state: "unavailable" }
  V = the vector cache for (album, m.id)
  hits = every photo in V with dot(t, V[photo]) >= m.meaning_threshold
  sort hits by score descending, then id ascending
  drop hits that are missing, outside the scope, or already among the text matches
  drop hits that fail the query's other filters (everything except the search text)
  drop hits that are a copy of a text match, or of an earlier hit
  keep the first 200
  append them to the list held under token, as its meaning segment
  return { state: "ok", count: the new length of the list, photos: the new X of "Showing X of Y",
           meaning_start: index of the first one, or null when none was added }
```

Among copies the best score wins. Copies have equal vectors, so in practice the smallest id is kept.

### How results appear

Search by meaning is part of the query model, so the page never holds a list of matches.

1. The page calls `query_run`. The text matches come back at once.
2. If the query has free text of 3 letters or more, the page waits 300 ms and calls `search_meaning(token)`. A newer `query_run` makes the older token's reply irrelevant.
3. Rust appends the meaning matches to the token's list and returns `meaning_start` with the new `count` and `photos`. The token model, and the rule that Rust keeps the last 4 tokens, are in [Organising](05-organising.md#the-query); a dropped token answers `query_expired`.
4. The grid shows the text matches, then a note row reading "Also matching by meaning", then the meaning matches starting on a fresh row. The count of photos shown includes them.

Meaning matches keep score order whatever the sort setting is. They are never stacked. If the call fails or returns nothing, the grid shows the text matches alone and no note row.

### Performance for 100,000 vectors

| Item | CLIP base (512) | CLIP large (768) |
|---|---|---|
| Size of the vectors in the database and in memory | 100,000 × 512 × 4 = 205 MB | 307 MB |
| Multiply-adds for one search | 51 million | 77 million |
| Expected time for one search, one thread | under 50 ms | under 80 ms |
| Text to vector | about 20 ms | about 50 ms |

A plain scan of every vector is fast enough. No index structure is needed at this size.

Reading 100,000 blobs from SQLite takes seconds, and longer on a network share, so it happens once. The **vector cache** holds, for the open album and one model, a single block of `count × dim` float32 values and a matching list of photo ids.

| Rule |
|---|
| The cache is filled by the first `search_meaning` for the album and model, on the blocking pool. |
| Rows whose `dim` is wrong or whose blob is not `dim × 4` bytes are skipped and counted in the log. |
| The writer thread tells the cache about every vector written or deleted, so it never needs re-reading. |
| It is dropped when the album closes or the keywords model changes. |

The times above are estimates. A benchmark test with 100,000 random vectors fixes the real figure, and the target is a whole search under 150 ms. If memory proves too tight, the cache can hold 16-bit floats at half the size with no visible change in results.

## The page

### "What's in it" in the details panel

A foldable section titled "What's in it", between the file facts and Camera. `photo_detail` carries the data:

```ts
interface PhotoTags {
  state: 'off' | 'waiting' | 'done' | 'failed';
  description: string | null;
  written_by: string | null;                    // label of the model that wrote the description
  keywords: string[];                           // labels only, at most 12, best first, lower case
  objects: { label: string; count: number }[];
  error: string | null;                         // the first failed row's message, when state is 'failed'
  can_regenerate: boolean;
}
```

How Rust fills it:

| Field | Rule |
|---|---|
| `state` | `off` when neither part has a chosen model **and** the photo has no `done` row. Else `waiting` when a chosen model has no row for the photo. Else `failed` when a chosen model's row is failed. Else `done`. With no chosen model but existing rows (an album tagged elsewhere), `done`. |
| The row shown for a part | The chosen model's own row if it exists, even a failed one. Otherwise the newest `done` row of another model of that part. So after switching from base to large, the base results stay visible, with state `waiting`, until the large model has looked. |
| `description`, `objects` | From the shown describe row, when it is done. |
| `keywords` | From the shown keywords row, when it is done: the labels, best score first. |
| `can_regenerate` | True when the shown describe row is done, belongs to the active describe model, and the album is not view-only. |

Vectors are never sent to the page.

```text
| ▾ What's in it                                          |
|   Not working: a chosen model is damaged or would not    |   error note; only when not_working is not empty
|   load. See Settings.                                    |
|   A dog running along a sandy beach at sunset, with      |   the description; tooltip "Written by Florence-2 base"
|   two people walking behind it.                          |
|   [Regenerate]                                           |   small button
|   Keywords                                               |   muted label
|   (beach) (dog) (sunset) (sand)                          |   chips
|   Objects                                                |   muted label
|   (dog) (person ×2)                                      |   chips; "label ×count" when count is above 1
```

| Condition | Shown |
|---|---|
| `state` is `off` | "Tagging is off." and a **Settings** link that opens Settings at Photo tagging. Nothing else. |
| `not_working` is not empty | The error note "Not working: a chosen model is damaged or would not load. See Settings." above whatever else shows. |
| `state` is `waiting` with no description, keywords or objects | "Not looked at yet" |
| `state` is `waiting` with results from another model | Those results, and under them the muted note "Waiting for {chosen model label}". |
| `state` is `failed` | The error note "Couldn't look at this photo", with the reason (`error`) as its tooltip, and a **Try again** link that calls `tagging_retry_failed`. Results from the part that worked still show. |
| A description | The sentence. Its tooltip is "Written by {written_by}". |
| `can_regenerate` | The **Regenerate** button under the sentence. |
| Keywords | The label "Keywords", then one chip for each keyword. |
| Objects | The label "Objects", then one chip for each object: "label", or "label ×count". |

**Chips.** Each chip is a toggle button with `aria-pressed`. Pressing it adds a keyword filter or an object filter to the current query; pressing it again removes it. The tooltip is "Show only photos with this" or "Stop filtering by this". A chip whose filter is on is drawn as the Active filter chip of [Visual design](09-visual-design.md#chips) (`--accent` fill). The filters themselves, and the `tag:word` query word, are specified in [Organising](05-organising.md). A keyword filter matches a whole keyword exactly; both sides are lower case.

**Regenerate.**

| Aspect | Behaviour |
|---|---|
| Label | "Regenerate". While it runs: "Writing a new description…", disabled, with `aria-busy`. |
| Tooltip | "Ask {label} to describe this photo again". `aria-label`: "Regenerate the description". |
| Press | Calls `tagging_regenerate(id)`. The reply replaces the photo's tags in the panel. |
| Busy state | Kept in the store as a set of photo ids. It lasts while the section is folded, another photo is shown or the panel is closed, and a second request for the same photo is not sent. |
| Failure | A toast shows the error's message. The old sentence stays. If another album was opened meanwhile, the reply is dropped and no toast shows. |
| View-only | The button is disabled; the tooltip is the first sentence of the view-only banner in force. |

### The tagging bar

One line under the top bar, shown only with an album open. It is driven by `tagging_progress` alone. Numbers use the locale's grouping (2,140).

```text
| [==========------------------]  Tagging 1,204 of 2,140 · 3 failed                  [Pause] |
```

The first row that matches wins:

| Condition | Shown |
|---|---|
| `total` is 0 | Nothing. |
| `problem` is set and not `running` | "Tagging stopped: {problem}" in `--error`, and **Try again** (`tagging_restart`). |
| `not_working` is not empty and not `running` | "Tagging is not working: a chosen model is damaged or would not load." in `--error`, with **Settings** and **Try again** (`tagging_restart`). |
| `running` | A bar filled to `(done + failed) / total`, "Tagging {done + failed} of {total}", then " · {failed} failed" when `failed` is above 0, and **Pause**. |
| `paused` and `done + failed < total` | The same bar and text with " · paused" at the end, and **Resume**. |
| `done + failed == total` and `failed` is above 0 | "{failed} photos could not be tagged." ("1 photo could not be tagged."), with **Try failed photos again** (`tagging_retry_failed`) and a close button that hides the line until the counts next change. |
| Otherwise (finished, nothing failed) | Nothing. |

The bar is a `role=progressbar` with the label "Tagging photos". **Pause** and **Resume** call `tagging_pause`. The page does not guess the new state; it waits for the next event, which arrives at once. An error from any of these buttons raises a toast.

### Settings → Photo tagging

A section of the Settings dialog (the dialog itself is in [Desktop, release and tests](08-desktop-and-release.md)).

```text
| Photo tagging                                                    |
| Runs on this computer. The models are downloaded from Hugging    |
| Face only when you ask, and nothing else leaves the computer.    |
|                                                                  |
| Descriptions and objects                                         |
| A sentence about each photo, and the things in it.               |
| (o) Off                                                          |
| ( ) Florence-2 base (smaller)                         [Download] |
|     277 MB · about 6 s a photo · MIT                             |
| ( ) Florence-2 large (larger)       [=====     ] 41%    [Cancel] |
|     832 MB · about 20 s a photo · MIT                            |
|                                                                  |
| Keywords and search by meaning                                   |
| Keywords for each photo, and search that finds photos by what    |
| you mean.                                                        |
| ( ) Off                                                          |
| (o) CLIP base (smaller)                                 [Remove] |
|     156 MB · less than a second a photo · MIT                    |
| ( ) CLIP large (larger)          [Not working · Download again]  |
|     434 MB · less than a second a photo · MIT                    |
| +--------------------------------------------------------------+ |
| | Download Florence-2 base, 277 MB, from Hugging Face?          | |
| |                                         [Cancel]  [Download]  | |
| +--------------------------------------------------------------+ |
| Couldn't download CLIP large: The download stalled. Try again.   |
```

- **Loading.** The section calls `tagging_models()` when it opens and shows "Loading photo tagging…" until the reply arrives. A failure shows the error's message in its place.
- **Parts.** One group for each part, with the title and note from the table at the top of this document.
- **Choices.** An **Off** radio, then the part's models, smaller first.
- **A model row.** The radio's text is the label in bold and "({size})". The line under it is "{bytes} · {speed} · {licence}".

| Piece | Rule |
|---|---|
| `{bytes}` | The model's total. At 1,000,000,000 bytes or more: "X.X GB". Otherwise megabytes of 1,000,000 bytes: a whole number at 10 MB or more ("277 MB"), one decimal below that. |
| `{speed}` | `seconds_per_photo` below 1: "less than a second a photo". Otherwise "about {N} s a photo", N rounded to a whole number. |
| `{licence}` | "MIT", "Apache-2.0" or "BSD-3-Clause". |
| The radio | Enabled only when the model's state is `ok`. Choosing it calls `tagging_set`. The page shows the choice at once and takes the reply as the truth, so a refusal puts the radio back. |

What sits at the right of a model row:

| State | Shown |
|---|---|
| `absent` | **Download**. Disabled while any download runs. |
| `partial`, not downloading | **Continue download**. Disabled while any download runs. |
| This model is downloading | A progress bar (`role=progressbar`, `received / total`), the percentage, and **Cancel**. |
| `unverified` or `checking` | "Checking…" |
| `ok`, and its part is not in the not-working set or it is not the chosen model | **Remove** |
| `ok`, chosen, and its part is in the not-working set | **Not working · Download again**, and **Remove** |
| `damaged` | **Not working · Download again** |

- **Confirming.** Download, Continue download and Download again open a confirm strip: "Download {label}, {size}, from Hugging Face?" with **Cancel** and **Download**. Download has the focus. `{size}` is the model's full size. Confirming calls `tagging_download(id)`.
- **Download again.** For a `damaged` model only the missing files are fetched. For an `ok` model that would not load, every file is hashed first, and whatever fails is fetched.
- **Progress.** The bar follows `download_progress`. When a download ends, the section calls `tagging_models()` again.
- **Errors.** An error from an action, or the last download error, shows in `--error` with `role=alert`.

`tagging_models()` returns:

```ts
interface ModelsState {
  models: {
    id: string; part: 'describe' | 'keywords'; size: 'smaller' | 'larger'; label: string;
    bytes: number; licence: string; seconds_per_photo: number;
    state: 'absent' | 'partial' | 'unverified' | 'checking' | 'ok' | 'damaged';
    bytes_present: number;
  }[];
  settings: { describe: string | null; keywords: string | null };
  not_working: ('describe' | 'keywords')[];
  downloading: { model: string; received: number; total: number } | null;
  error: string | null;       // the last download failure
}
```

File lists and hashes are not sent to the page.

## Network rules

- **Models never use the network.** A model is a folder of local files given to the runtime. Neither `ort` nor any wrapper around it is allowed to fetch anything at run time; if `usls` cannot be configured that way, it is not used.
- **Downloads happen only on request.** Files are fetched only after the user confirms Download in Settings. Nothing is fetched in the background. There is no check for newer models: the catalogue is fixed in each version of Owlbum.
- **Who makes the request.** Rust, never the webview. The page has no way to name a URL: `tagging_download` takes a model id, and the URL is built from the catalogue.
- **Where it may go.** Only to URLs that pass the allowed-hosts rule, on every redirect hop, over HTTPS.
- **What is trusted.** Not the host. Every file is pinned to a commit and checked against the SHA-256 compiled into Owlbum before it is used.
- **What is sent.** The request path (repository, commit, file name), a `Range` header when resuming, and a `User-Agent` of `Owlbum/{version}`. No account, no token, no cookie, and nothing about the user's photos.
- **Offline.** A download fails with "Couldn't reach Hugging Face…" and keeps its `.part` files. Installed models, tagging, Regenerate and search by meaning all work with no network.

## Licences

| Component | Licence | Notes |
|---|---|---|
| Florence-2 base and large (Microsoft; ONNX export by `onnx-community`) | MIT | Downloaded by the user, not shipped with Owlbum. |
| CLIP ViT-B/32 and ViT-L/14 (OpenAI; ONNX export by `Xenova`) | MIT | Downloaded by the user, not shipped. |
| ONNX Runtime (Microsoft) | MIT | Shipped inside Owlbum. |
| `ort` | MIT or Apache-2.0 | |
| `tokenizers` | Apache-2.0 | |
| `usls`, if used or ported | To be confirmed before any code is copied | A port carries its copyright notice. |

Everything shipped is listed in the third-party notices. The model licences are shown in Settings beside each model, and in the notices with a line saying the models are downloaded on request. A model may be added to the catalogue only if its licence is MIT, Apache-2.0 or BSD-3-Clause.

## Packaging

ONNX Runtime is a native library. There are two ways to ship it with `ort`.

| | A. `download-binaries` | B. `load-dynamic` |
|---|---|---|
| How | At build time `ort` fetches a prebuilt ONNX Runtime and links it into the Owlbum program. | Owlbum ships `onnxruntime.dll` (Windows) or `libonnxruntime.so` (Linux) as a separate file and loads it at run time from a path it is given. |
| Shipped files | None extra. | One library per platform, as a Tauri resource, from Microsoft's official release. |
| Build | The build machine needs the network. The version is fixed by the `ort` version in `Cargo.lock`. | The release workflow downloads the pinned library and checks its SHA-256. |
| Risks | The prebuilt library may need a newer C library than the oldest Linux Owlbum supports, or may not link cleanly with Tauri's Windows build. | Windows has its own older `onnxruntime.dll` in the system folder; loading by name could pick it up. A missing or blocked file breaks tagging at run time. |
| Installer size | About 15 to 20 MB more. | About the same. |

**Chosen: B** (#52). The prebuilt library of choice A stopped the program with an illegal instruction on a processor without AVX2 (an Intel i5-3550), before any model was loaded, which would take the whole of Owlbum down on such a computer. Microsoft's own library has no such requirement.

Owlbum ships Microsoft's `onnxruntime.dll` (Windows) or `libonnxruntime.so` (Linux) from the NuGet package `Microsoft.ML.OnnxRuntime` 1.31.1 (`runtimes/win-x64/native/onnxruntime.dll`, 17,253,728 bytes, SHA-256 `cb2fa518b18356617f6f9f460c012501da2819ec7e5c1d80bd8f5de20492db0c`) and loads it with `ort`'s load-from-path call, always by a full path, never by name: the path set by the shell, else `OWLBUM_ORT_DYLIB` (development and the real-model tests), else the file next to the program. A missing library makes tagging report "The model runtime would not start: …"; the rest of Owlbum is unaffected. Putting the library into the installers (a Tauri resource fetched and checked by the release workflow) is a follow-up issue; until then a development build needs `OWLBUM_ORT_DYLIB`.

Other notes:

- No model file is ever put in an installer.
- The release build prints the ONNX Runtime version in the log at the first model load, and About lists it.
- The prebuilt ONNX Runtime uses the processor's vector instructions when they exist and works without them. No per-processor build is made.

## Commands and events

Photos are named by id. Every error is an `AppError` with a stable code.

| Command | Arguments | Reply | Errors |
|---|---|---|---|
| `tagging_models` | | `ModelsState` | |
| `tagging_download` | `model` | nothing; the download runs in the background | `no_such_model`, `download_busy` |
| `tagging_download_cancel` | | nothing; returns when the download has stopped | |
| `tagging_remove` | `model` | nothing | `no_such_model` |
| `tagging_set` | `describe`, `keywords` (each an id or null) | the saved settings | `model_not_ready` |
| `tagging_status` | | the `tagging_progress` payload | |
| `tagging_pause` | `paused` | nothing | `view_only` |
| `tagging_restart` | | nothing | `view_only` |
| `tagging_retry_failed` | | `{ cleared }` | `view_only` |
| `tagging_focus` | `ids` | nothing; unknown ids are ignored | |
| `tagging_regenerate` | `id` | `PhotoTags` | see Regenerate |
| `search_meaning` | `token` | `{ state, count?, photos?, meaning_start? }`; `state` is `ok`, `too_short`, `off` or `unavailable` | `query_expired` |

| Event | Payload | Defined in |
|---|---|---|
| `tagging_progress` | `{ total, done, failed, paused, running, not_working, problem }` | Status |
| `download_progress` | `{ model, received, total, error, finished }` | Model download |
| `album_changed` | `{ revision, reason: "tags" }` | Telling the page that tags changed |

New error codes: `no_such_model`, `download_busy`, `download_failed`, `model_not_ready`, `no_describe_model`, `not_described_yet`, `model_load_failed`, `empty_description`, `tagging_timeout`, `tagging_lost`, `tagging_stopped`. `view_only`, `photo_not_found` and `query_expired` are the shared codes in [Architecture](01-architecture.md#commands).

## Tests

All tests run locally on Windows and Linux. None needs the network, and none needs a model, except the one opt-in group below.

### The backend trait

Everything that runs a model sits behind a trait in `owlbum-core`, so the queue, the commands and search are tested without one.

```rust
pub struct TagImage<'a> { pub rgb: &'a RgbImage, pub name: &'a str }   // name is for logs and for the fake

pub enum BackendError {
    Load(String),      // the model would not load
    Photo(String),     // this picture could not be handled; becomes a failed row
    Lost(String),      // the model is not loaded, or the run panicked
    Timeout,
}

pub trait TagBackend: Send {
    fn load(&mut self, model: &ModelChoice, dir: &Path, threads: usize, vocabulary: &[&str], deadline: &Deadline) -> Result<(), BackendError>;
    fn unload(&mut self, part: Part);
    fn loaded(&self, part: Part) -> Option<&'static str>;               // the id in a part's slot
    fn describe(&mut self, image: &TagImage, deadline: &Deadline) -> Result<Described, BackendError>;   // greedy caption + objects
    fn caption(&mut self, image: &TagImage, deadline: &Deadline) -> Result<String, BackendError>;       // sampled caption only
    fn keywords(&mut self, image: &TagImage, deadline: &Deadline) -> Result<Keyworded, BackendError>;   // keywords + vector
    fn text_embedder(&self) -> Option<Arc<dyn TextEmbedder>>;   // safe to call from another thread
}
```

Downloads sit behind a second small trait, `Fetch`, with one method that performs a GET and returns the status, headers and a body stream.

### The fake backend

`FakeBackend` is deterministic and instant (with an optional delay).

| Call | Fake result |
|---|---|
| `describe` | "A picture of {words}." where the words come from the file name; objects: the first word, count 2. |
| `caption` | "Another look at {words} ({n})." with a counter, so each Regenerate differs. |
| `keywords` | The file name's words that are in the vocabulary, each with score 1 / count; an 8-number unit vector built from those words. |
| Text vector | Built the same way from the query's words, with a small table of synonyms (`seaside` is near `harbour`), so a matching word gives a high cosine. |
| Scripted faults | By file name: `fail-…` returns `Photo`, `lost-…` returns `Lost` a set number of times, `slow-…` returns `Timeout`; a switch makes the next load fail. |

A fake catalogue has four tiny models (dim 8, keyword threshold 0.2, meaning threshold 0.5). A fake `Fetch` serves their files from memory, supports ranges, makes one redirect hop to a second allowed host, and can be told to ignore a range, cut a body short, corrupt a byte, stall, or redirect to a host that is not allowed.

### Rust tests

| Area | What is checked |
|---|---|
| Catalogue | The rules table; each model's file sizes sum to the stated total; the vocabulary has 564 entries, all unique, all lower case. |
| Allowed hosts | The table of nine URLs. |
| Download | A clean download; resume from a `.part`; a server that ignores the range; a wrong hash deletes the part; a file already present is hashed, and a wrong one is fetched again; cancel leaves no error; one download at a time; every row of the wording table. |
| Install state | Each of the six states; a file changed in place at the same size is caught by verification after a failed load; `tagging_set` refuses every state but `ok`. |
| Results | Caption cleaning strips tags; object counting and order; the softmax, threshold, order and cap of 12 on hand-made vectors; a vector of the wrong length is refused. |
| Queue | Focus before scan order; `describe` before `keywords`; nothing runs during a scan, when paused, or in a view-only album; a photo added by the watcher is tagged; a copy takes its twin's rows with no run; a changed file is tagged again; an album closed mid-run writes nothing. |
| Failures | A `Photo` error stores a failed row and is not retried; `tagging_retry_failed` clears and re-runs it; two losses then a success reset the count; three losses stop the part; a timeout stores a failed row and counts as a loss; a failed load stops the part at once; `tagging_restart` clears it. |
| Status | Counts for done, failed and copies; reading the status changes nothing; counts are right while paused and in an album tagged in an earlier session. |
| Regenerate | New text replaces old; objects, keywords and the vector are kept; copies get the new text; it works while paused; one retry after a loss; every row of the error table. |
| Search | `seaside` finds the harbour photos, best first; fewer than 3 letters finds nothing; describe-only finds nothing; a vector with another `dim` is ignored; copies appear once; text matches are not repeated; the other filters apply; at most 200; a vector written after the cache was filled is found. |
| Search speed | A benchmark over 100,000 random vectors of 512 and of 768 numbers. |
| Events | `tagging_progress` on each state change; `album_changed` for tags is held to one in 5 s, with the listed exceptions. |

### Real-model tests

These run only when `OWLBUM_TEST_MODELS_DIR` names a folder holding downloaded models (`<folder>/<model id>/…`). Without it they are skipped. They never download.

| Check |
|---|
| Each installed model loads from the pinned files and passes its hashes. |
| A description of the lake fixture is longer than 20 characters and contains no `<` or `>`. |
| The same photo described twice greedily gives the same text; three sampled captions are not all the same. |
| The lake fixture has between 1 and 12 keywords and `lake` is one of them. |
| Vectors have the catalogue's `dim` and unit length to 4 decimal places. |
| The text "seaside" scores the harbour fixture above the others, and above the model's meaning threshold. |
| A search text is answered while a describe run is in progress. |
| The measured seconds per photo and peak memory are printed. |

### Page tests

Run with Vitest against the faked API module.

| Area | What is checked |
|---|---|
| What's in it | Each row of the states table; chips toggle a filter and show `aria-pressed`; "label ×count"; Regenerate's busy text, its lasting busy state, the toast on failure, and the dropped reply after another album opens. |
| Tagging bar | Each row of the bar table, including "1 photo could not be tagged."; Pause, Resume, Try again and Try failed photos again call the right command. |
| Settings | Each row of the model-row table; the size and speed text ("156 MB", "1.3 GB", "less than a second a photo", "about 6 s a photo"); the confirm strip; a refused choice puts the radio back; the bar follows `download_progress`; the error line. |
| Search by meaning | The note row and the fresh row; the 300 ms wait; fewer than 3 letters makes no call; an older reply is ignored. |
| No polling | No timer in the page asks for tagging status or model state. |

## Prototype plan

The first spike answers one question: can Rust run the pinned files and get the old program's results? It is done before any other tagging work, on Windows and on Linux.

1. Download the nine `florence2-base-q8` files and the six `clip-base-q8` files by hand, and check their hashes against this document.
2. Make a small command-line program in the workspace that depends on `ort` with `download-binaries`, and on `usls`.
3. With `usls`, load the pinned Florence-2 base files from the local folder with the network unplugged. Record what happened: loaded, wrong file names, wrong inputs, or tried to fetch.
4. Caption the lake fixture greedily with `<MORE_DETAILED_CAPTION>`, and run `<OD>`. Compare both with what the old program stored for the same fixture.
5. Produce three sampled captions (temperature 0.7, `top_k` 50) and check they vary.
6. Load CLIP base. Embed the fixture, the 564 vocabulary prompts and the text "seaside". Compute the keywords as this document says and compare with the old program's. Note the vector length.
7. Measure, for each model, on both platforms: load time, seconds per photo over the fixture album, and peak memory. Do the same for the two large models if time allows.
8. Check that a deadline can stop a run: between decoder steps, and by ending a run through `ort`.
9. Build a release Tauri program that includes the runtime. Start it on a clean Windows 10 machine and on the oldest supported Linux, and tag one photo.
10. Write the findings on the tagging issue, and update the catalogue, the memory figures and this document.

Fallback decisions:

| If | Then |
|---|---|
| `usls` loads the files and gives matching results, but offers no sampling or no deadline | Use `usls` for CLIP. Port its Florence-2 module and add sampling and the deadline check. |
| `usls` cannot load the pinned files, or cannot be kept off the network | Port Florence-2 and write CLIP directly on `ort` and `tokenizers`. CLIP is two session runs and a tokenizer, so this is small. |
| The merged, quantised decoder cannot be driven from Rust in reasonable time | Pin a different export of the same weights (for example separate decoder files), with new catalogue entries. |
| The quantised files give clearly different or worse results under the Rust runtime | Try the half-precision or full-precision files of the same commits, and weigh size against quality on the issue. |
| Captions differ from the old program's only in small ways | Accept, and record examples. An exact match is a goal, not a requirement. |
| Seconds per photo is more than twice the old figure | Try the runtime's other thread settings and graph optimisation levels before accepting. Update the Settings text to the measured figure. |
| `download-binaries` fails on either platform | Switch to `load-dynamic` with a shipped library (Packaging, choice B). |
| A run cannot be ended from outside | Keep the between-steps check and the "stuck" rule. Raise the single-run limits so only a true hang trips them. |

## Open questions

1. **Do the pinned ONNX files load in the Rust runtime as they are?** The exports were made for another runtime. The merged decoder takes a switch input and a set of cached values that `usls` may not expect. The answer decides the catalogue's file lists.
2. **Greedy or beam search for tagging?** The base model's `generation_config.json` may ask for several beams. This document specifies greedy, which is what the old program is believed to have done in effect. The prototype compares captions to settle it.
3. **The exact prompt strings and vector lengths.** They are stated here as expected values (the two prompts; 512 and 768). The prototype reads them from the pinned files.
4. **Can `ort` end a run in progress on both platforms?** If not, the time limit for a single run rests on the "stuck" rule alone.
5. **Memory.** The figures in this document are estimates. If Florence-2 large needs much more than 1.5 GB, Settings should say so beside the model.
6. **The licence of `usls`.** It must allow copying a module into an MIT program.
7. **Database size.** 100,000 vectors add 205 MB (CLIP base) or 307 MB (CLIP large) to the album database. The daily backup leaves the `vectors` table out (see [Data](02-data.md#backups-and-damage)), so a restored backup has its keywords model run again. Whether Tidy should offer to delete rows of models no longer chosen is undecided.
8. **Proxies and certificates.** Whether model downloads should follow the system's proxy settings and certificate store by default, as the updater does, has not been checked on Linux.

## Appendix: the keyword vocabulary

The vocabulary is one ordered list of 564 lower-case entries, compiled into `owlbum-core`. It is the ten groups below joined in this order, with any entry that already appeared earlier left out (`drawing` appears twice; the first place counts). The order matters only for ties.

**People and occasions:** person, people, man, woman, child, baby, toddler, boy, girl, teenager, family, couple, friends, crowd, group photo, portrait, selfie, grandparent, bride, groom, wedding, birthday, birthday cake, party, graduation, christmas, christmas tree, halloween, easter, new year, fireworks, concert, festival, parade, ceremony, picnic, barbecue, camping, hiking, holiday, vacation, road trip, reunion, dinner party, baby shower, funeral, school, classroom, sports day, dancing, singing, playing, laughing, smiling, hugging, sleeping, eating, cooking, reading, swimming, running, cycling, skiing, snowboarding, surfing, sailing, fishing, climbing, skating, gardening, painting, drawing, shopping, working, studying, gaming

**Animals:** dog, puppy, cat, kitten, bird, parrot, duck, goose, swan, chicken, horse, pony, cow, sheep, goat, pig, rabbit, hamster, fish, goldfish, turtle, frog, snake, lizard, butterfly, bee, spider, insect, squirrel, deer, fox, bear, wolf, lion, tiger, elephant, giraffe, zebra, monkey, gorilla, kangaroo, koala, panda, penguin, seal, dolphin, whale, shark, crab, jellyfish, owl, eagle, pigeon, seagull, flamingo, peacock, camel, donkey, zoo, aquarium, farm animals, pet

**Nature and landscape:** landscape, nature, mountain, hill, valley, cliff, canyon, volcano, glacier, cave, rock, desert, sand dune, forest, jungle, woods, tree, palm tree, pine tree, autumn leaves, leaf, flower, rose, tulip, sunflower, daisy, cherry blossom, lavender, garden, lawn, grass, meadow, field, wheat field, vineyard, orchard, farm, countryside, river, stream, waterfall, lake, pond, sea, ocean, beach, coast, bay, island, wave, harbour, pier, lighthouse, sand, shell, coral reef, underwater, sky, cloud, blue sky, sunset, sunrise, dusk, night sky, stars, moon, rainbow, storm, lightning, fog, mist, rain, snow, ice, frost, winter, spring, summer, autumn, park, national park, trail, path, mountain view, scenery

**Places and buildings:** city, town, village, street, alley, road, highway, bridge, tunnel, skyline, skyscraper, building, house, cottage, cabin, apartment, castle, palace, tower, church, cathedral, mosque, temple, shrine, monument, statue, fountain, ruins, museum, art gallery, library, market, shop, supermarket, mall, restaurant, cafe, bar, pub, bakery, hotel, hotel room, pool, swimming pool, gym, stadium, playground, amusement park, roller coaster, ferris wheel, theatre, cinema, airport, train station, bus stop, port, marina, parking lot, gas station, hospital, office, factory, warehouse, construction site, square, plaza, old town, harbour town, graffiti, mural, neon sign, street sign, shop window, balcony, rooftop, courtyard, staircase, doorway, window, fence, wall, gate, garden path, backyard, patio, porch, driveway

**Indoors and home:** room, living room, kitchen, bedroom, bathroom, dining room, hallway, garage, attic, basement, sofa, chair, table, bed, desk, shelf, bookshelf, fireplace, lamp, candle, mirror, clock, carpet, curtain, pillow, blanket, plant, houseplant, vase, painting on wall, photo frame, television, computer, laptop, phone, camera, headphones, keyboard, piano, guitar, violin, drum, book, magazine, newspaper, letter, map, toy, teddy bear, doll, lego, board game, puzzle, balloon, gift, present, wrapping paper, decoration, christmas lights, umbrella, bag, backpack, suitcase, luggage, hat, sunglasses, glasses, shoes, boots, dress, suit, jacket, coat, scarf, costume, uniform, jewellery, watch, ring, flowers in vase, bouquet

**Food and drink:** food, meal, breakfast, lunch, dinner, dessert, snack, cake, cupcake, cookie, bread, sandwich, burger, pizza, pasta, noodles, rice, sushi, salad, soup, steak, chicken dish, seafood, fish dish, fruit, apple, banana, orange, strawberry, grapes, watermelon, lemon, vegetables, tomato, carrot, potato, cheese, eggs, chocolate, ice cream, candy, pie, pancakes, coffee, tea, cup, mug, wine, beer, cocktail, juice, water bottle, glass, plate, bowl, cutlery, picnic blanket, street food, food market, buffet, kitchen table

**Vehicles and transport:** car, vintage car, truck, van, bus, train, tram, subway, motorcycle, scooter, bicycle, boat, sailboat, ship, cruise ship, ferry, canoe, kayak, yacht, airplane, helicopter, hot air balloon, tractor, taxi, police car, fire truck, ambulance, skateboard, stroller, wheelchair, horse and cart

**Sport and play:** football, soccer, basketball, tennis, golf, baseball, cricket, rugby, hockey, volleyball, boxing, yoga, martial arts, horse riding, race, marathon, swimming race, trophy, medal, ball, kite, swing, slide, sandcastle, snowman, sledding, water park, trampoline

**Photo kinds and look:** close-up, macro, aerial view, panorama, black and white photo, old photo, scanned photo, blurry photo, dark photo, bright photo, silhouette, reflection, shadow, long exposure, bokeh, night photo, indoor photo, outdoor photo, screenshot, document, receipt, text, sign, poster, drawing, illustration, cartoon, abstract pattern, texture, colourful, pastel colours, gradient, minimalist, symmetry, crowded, empty, quiet, cozy, romantic, happy, funny

**Colours:** red, orange colour, yellow, green, blue, purple, pink, brown, black, white, grey, gold, silver
