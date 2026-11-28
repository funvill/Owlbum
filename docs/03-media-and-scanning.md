# Media and scanning

This part covers how Owlbum recognises files, reads them, and keeps the album database in step with the folder. It builds on [Architecture](01-architecture.md) and [Data](02-data.md). Terms such as album root, scoped open, `file_key`, `thumb_key`, missing row and view-only are defined there and are not redefined here.

Everything in this part lives in `owlbum-core`, except the `owlbum://` protocol handler and the commands, which are thin wrappers in the shell crate.

## The media format registry

One table in Rust (`owlbum_core::media::FORMATS`) is the only place that knows which files Owlbum handles. No other module holds an extension list, a MIME type or a "is this a RAW" test. A unit test searches the workspace sources (Rust and TypeScript) and fails if a media extension list or extension regex appears outside the registry module (`media/registry.rs`). It counts a file as holding a list when it has string literals naming two or more different known extensions (with or without the dot, any case), and as holding a regex when two known extensions are joined by `|`. Rust byte strings (`b"heic"`, a file signature) are not counted. The allowed exceptions are the handler tests, the fake backend's `app_formats` answer, the generated bindings in `ui/src/api/bindings/`, and the fixture generator (`crates/owlbum-fixtures/`), which writes and names one test file per format.

```rust
pub enum MediaKind { Image, Raw, Video }
pub enum SaveFormat { Jpeg, Png, Webp }

pub struct Capabilities {
    pub edit_in_place: bool,   // Save (with edit_target = file) may replace the original
    pub alpha: bool,           // the format can carry transparency
    pub save_as: SaveFormat,   // the format an edited copy is written in
    pub animated: bool,        // the format can hold more than one frame
}

pub struct MediaFormat {
    pub id: &'static str,                    // stored in photos.format
    pub extensions: &'static [&'static str], // lower case, with the dot; the first is the default
    pub mime: &'static str,                  // Content-Type for owlbum:// and the clipboard
    pub kind: MediaKind,
    pub label: &'static str,                 // shown to the user
    pub caps: Capabilities,
    pub handler: &'static dyn MediaHandler,
}
```

### Formats in version 1

| id | Extensions | MIME | Kind | Label | Edit in place | Alpha | Save as | Animated | Handler |
|---|---|---|---|---|---|---|---|---|---|
| `jpeg` | `.jpg` `.jpeg` `.jpe` `.jfif` | image/jpeg | image | JPEG | yes | no | jpeg | no | still |
| `png` | `.png` | image/png | image | PNG | yes | yes | png | no | still |
| `webp` | `.webp` | image/webp | image | WebP | no | yes | webp | yes | still |
| `gif` | `.gif` | image/gif | image | GIF | no | yes | png | yes | still |
| `tiff` | `.tif` `.tiff` | image/tiff | image | TIFF | no | yes | png | no | still |
| `bmp` | `.bmp` | image/bmp | image | BMP | no | no | png | no | still |
| `jxl` | `.jxl` | image/jxl | image | JPEG XL | no | yes | jpeg | no | jxl |
| `cr2` | `.cr2` | image/x-canon-cr2 | raw | Canon RAW | no | no | jpeg | no | raw |
| `cr3` | `.cr3` | image/x-canon-cr3 | raw | Canon RAW | no | no | jpeg | no | raw |
| `nef` | `.nef` | image/x-nikon-nef | raw | Nikon RAW | no | no | jpeg | no | raw |
| `arw` | `.arw` | image/x-sony-arw | raw | Sony RAW | no | no | jpeg | no | raw |
| `raf` | `.raf` | image/x-fuji-raf | raw | Fujifilm RAW | no | no | jpeg | no | raw |
| `orf` | `.orf` | image/x-olympus-orf | raw | Olympus RAW | no | no | jpeg | no | raw |
| `rw2` | `.rw2` | image/x-panasonic-rw2 | raw | Panasonic RAW | no | no | jpeg | no | raw |
| `dng` | `.dng` | image/x-adobe-dng | raw | DNG | no | no | jpeg | no | raw |
| `mp4` | `.mp4` `.m4v` | video/mp4 | video | MP4 video | no | no | jpeg | no | video |
| `mov` | `.mov` | video/quicktime | video | QuickTime video | no | no | jpeg | no | video |
| `webm` | `.webm` | video/webm | video | WebM video | no | no | jpeg | no | video |
| `mkv` | `.mkv` | video/x-matroska | video | Matroska video | no | no | jpeg | no | video |
| `avi` | `.avi` | video/x-msvideo | video | AVI video | no | no | jpeg | no | video |

Notes on the capabilities:

- **A file that is animated is never edited in place**, whatever its format says, because saving would throw the other frames away. Its edit is saved as a copy made from the first frame. This includes an animated PNG: `metadata` detects its `acTL` chunk and sets `exif.frames`, even though Owlbum shows it as a still.
- **WebP** is saved as a copy only, written as lossless WebP. The pure-Rust encoder in the `image` crate has no lossy mode (to be confirmed against the pinned version), and overwriting a small lossy original with a much larger lossless file would surprise people.
- **TIFF, BMP and GIF** are saved as a PNG copy, which loses nothing and keeps transparency. **JPEG XL** is saved as a JPEG copy because `jxl-oxide` only decodes.
- **RAW** edits are saved as a JPEG copy only. There are no RAW development controls. The user-facing refusal is "<label> photos can only be saved as a copy."
- **Video** has no editing at all. Its `save_as` value is never used.
- `dng` stays in the table: the RAW prototype (#49) read an uncompressed Bayer DNG laid out like a camera's or Adobe's converter's (the fixture generator's) with `rawler` 0.8, its embedded preview and its development both. Lossy and linear DNG from phones were not checked, for want of sample files in the repository; a file `rawler` cannot read is a `decode` error row like any other.
- **RAW and video handlers.** The RAW rows use `media::raw` (#49) and the video rows `media::video` (#50). Until they were built, these rows named placeholder handlers that answered `unsupported`; `SCAN_VERSION` 4 reads the error rows they left again.
- **WebP and JPEG XL encoders, confirmed.** `image` 0.25.10 writes only lossless WebP, and `jxl-oxide` 0.12.6 only decodes, so the save-as values above stand (open question 9).

### Files recognised but not supported yet

A second table in the same module lists extensions Owlbum knows are pictures but cannot open in version 1. The walker counts these files and the album reports them, so nobody thinks photos are missing. They get no row in the database.

| Label | Extensions | Reason shown |
|---|---|---|
| HEIC | `.heic` `.heif` `.hif` | "HEIC photos are not supported yet." |
| AVIF | `.avif` | "AVIF photos are not supported yet." |
| Camera RAW | `.crw` `.nrw` `.srf` `.sr2` `.pef` `.srw` `.rwl` `.3fr` `.iiq` `.erf` `.mef` `.mos` `.kdc` `.dcr` `.x3f` | "This camera's RAW format is not supported yet." |
| Video | `.mts` `.m2ts` `.3gp` `.wmv` `.mpg` `.mpeg` | "This video format is not supported yet." |

The notice that reports them, its wording ("N files not supported yet (HEIC, AVIF)") and the **Files not shown** dialog it opens are defined in [Browsing](04-browsing.md#notices). The labels in the brackets are the distinct labels found, in the order of this table. The "Reason shown" column is the text beside each file in that dialog.

Moving a format from this table to the first one is the whole job of supporting it: one registry row, one handler if needed, one fixture.

### Lookups

| Function | Rule |
|---|---|
| `extension_of(name)` | The text from the last dot of the file name, lower-cased, with the dot. Empty when there is no dot or the only dot is the first character (`.jpg` has no extension). |
| `format_of(name)` | The format whose extensions contain `extension_of(name)`, or none. |
| `format_by_id(id)` | The format with that id, or none. |
| `unsupported_of(name)` | The row of the second table for that extension, or none. |
| `is_supported(name)` | `format_of(name)` is some, and the name does not start with `._`. |

### When the extension lies

Each handler has a `sniff` test on the first bytes of the file. The scanner already reads the first 16 KiB for `file_key`, so sniffing costs no extra read.

| Case | Result |
|---|---|
| The extension's own handler recognises the bytes | Normal. |
| Another supported format's handler recognises them (a PNG named `.jpg`) | The file is read as what it is. `photos.format` holds the real format, and capabilities follow it. |
| The bytes are a recognised-but-unsupported format (a HEIC named `.jpg`) | `scan_error` with code `unsupported` and that format's reason. This is the one case where an unsupported picture has a row: the walker took it for a supported file by its name. It is listed with the unreadable files, not counted with the unsupported ones. |
| Nothing recognises them | The extension's handler tries anyway. Failure is a `decode` error. |

RAW and video containers are sniffed by container family only (TIFF-based, ISO base media, RIFF, EBML). The extension picks the format id within a family.

`identify(name, head)` in the registry applies this table. Each still format has its own handler instance, so "the extension's own handler" means that format's signature, not the whole still family. When bytes match several rows, the first row in table order wins, so a TIFF named `.cr2` stays RAW but a CR2 named `.jpg` reads as TIFF. An ISO base media file is told apart by its `ftyp` brands: HEIF (`heic`, `mif1` and kin) and AVIF (`avif`, `avis`) are unsupported stills, `crx ` is Canon RAW, anything else is video. A RIFF file is told apart by its form type (`WEBP` or `AVI `).

## Media handlers

A handler knows how to read one family of formats. Handlers are compiled in. Every registry row names one, and a test fails if a handler is not used by any row.

```rust
pub enum Cost { Cheap, Decode, Heavy }

pub struct Pixels {            // always upright, 8 bits per channel, sRGB
    pub width: u32,            // size of this buffer
    pub height: u32,
    pub full_width: u32,       // upright size of the full picture; equals width/height from `working`,
    pub full_height: u32,      // 0 when a reduced or embedded preview cannot tell
    pub rgba: Vec<u8>,         // 4 bytes per pixel; alpha is 255 when the source has none
    pub has_alpha: bool,
}

pub trait MediaHandler: Send + Sync {
    /// True when the first bytes of the file belong to this handler.
    fn sniff(&self, head: &[u8]) -> bool;
    /// Facts from headers only: no pixel decode.
    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError>;
    /// Fast picture, at least `min_side` on its longest side when the file allows. Good enough for thumbnails.
    fn preview(&self, file: &Path, min_side: u32) -> Result<Pixels, MediaError>;
    /// Full quality, full size, 8-bit sRGB. Used for zoom, copy and tagging.
    fn working(&self, file: &Path) -> Result<Pixels, MediaError>;
    /// Full size at the source's own bit depth with its colour profile untouched. Used only by the
    /// edit renderer (see Editing and file actions), so a crop keeps 16-bit samples and a wide gamut.
    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError>;
    /// Video only: duration, dimensions, rotation, codecs. None for stills.
    fn probe(&self, file: &Path) -> Result<Option<VideoInfo>, MediaError>;
    /// How expensive `preview` is, for scheduling.
    fn cost(&self, file_size: u64) -> Cost;
}
```

Rules that hold for every handler:

- **Upright.** Orientation from the file (EXIF Orientation 1 to 8, including the mirrored values, or the container's own rotation) is applied before pixels are returned. Nothing downstream looks at orientation again.
- **sRGB.** A file with an embedded colour profile that is not sRGB is converted to sRGB. A file with no profile is taken as sRGB. Deeper than 8 bits is reduced to 8. The one exception is `source`, which returns upright pixels at the file's own depth (8 or 16 bits) together with the embedded profile's bytes, unconverted.
- **Files are opened read-only and closed before the call returns.** No handler keeps a file open, so the user can always move, rename or delete a photo.
- **Size limit.** A picture of more than 300,000,000 pixels is refused with the `too_large` error "This picture is too large to open ({width} × {height})." The check uses the header, before any pixel memory is allocated.
- **Lenient decoding.** A truncated or slightly damaged file that still yields a picture is accepted. Only a file that yields none is an error.
- **No network and no child processes.**

`MediaError` carries a code (`io`, `decode`, `unsupported`, `too_large`) and a sentence for the user.

The other types the trait names:

```rust
pub struct Metadata {
    pub width: u32,                // upright; 0 when the headers cannot tell
    pub height: u32,
    pub has_alpha: bool,
    pub frames: Option<u32>,       // only when more than one: animated GIF, WebP and PNG
    pub duration_ms: Option<u64>,  // loop length, animated GIF and WebP only (an animated PNG is a still)
    pub exif: Option<Exif>,        // the EXIF tags (media::exif), for the metadata reader
}

pub enum Layout { Grey, GreyAlpha, Rgb, Rgba }
pub enum Samples { U8(Vec<u8>), U16(Vec<u16>) }   // deeper or floating-point sources become 16 bits

pub struct SourcePixels {      // upright, at the source's own depth
    pub width: u32,
    pub height: u32,
    pub layout: Layout,
    pub samples: Samples,      // width × height × channels, row-major
    pub icc: Option<Vec<u8>>,  // the profile the samples are in, unconverted; None means sRGB
}

pub struct VideoInfo {
    pub duration_ms: u64,
    pub width: u32,            // displayed size, after rotation and pixel aspect ratio
    pub height: u32,
    pub rotation: u16,         // 0, 90, 180 or 270
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}
```

Every `preview` and `working` result passes through one function, `media::colour::to_srgb(pixels, icc)`, which converts with `moxcms` (relative to the profile's own matrix and curves, perceptual intent for table-based profiles):

- An RGB profile converts the colour channels. A grey profile converts a grey picture. Alpha is never changed.
- A profile that gives sRGB back (every colour of a 9 × 9 × 9 test lattice, or every grey level, within one level) is skipped, so the many files that embed sRGB cost about half a millisecond. `media::colour::is_srgb(icc)` is true when `to_srgb` would leave the pixels as they are, for whatever reason (no profile, sRGB, or a profile it cannot use).
- A profile that cannot be read or used, a profile for another colour space, and a grey profile on a colour picture leave the pixels as decoded. A damaged profile never costs the user the picture. A CMYK JPEG is the common case: its decoder has already turned it into RGB, so its CMYK profile cannot apply (#82).
- The picture is converted in strips of 16,384 pixels through a small scratch buffer, so a large picture is never held twice. A 24-megapixel picture takes about 60 ms on one thread.

`source` never passes through `to_srgb`: it keeps the profile's bytes as the file has them.

Frame counts and loop lengths are read by walking the file's blocks (GIF blocks, WebP `ANMF` chunks, the PNG `acTL` chunk) without decompressing pixels.

### The still handler

It covers JPEG, PNG, WebP, GIF, TIFF and BMP, all decoded with the `image` crate, whose JPEG decoder is `zune-jpeg`. A JPEG's size and EXIF block for `metadata` (and the size check before a decode) come from a walk of its markers, because `image` reads the whole JPEG into memory before it reports a size.

| Format | Notes |
|---|---|
| JPEG | `zune-jpeg` 0.5 has no reduced-size decode, so `preview(min_side)` is decoded by `jpeg-decoder` 0.3 (no rayon), scaled in the DCT to 1/2, 1/4 or 1/8: the smallest of these whose longest side is still at least `min_side` (a 5504 × 4128 photo is decoded at 2752 × 2064 for `preview(1600)`, and at 688 × 516 for the page's `preview(512)`; a 2048 × 1536 photo at 512 × 384 for `preview(512)`), then turned upright from its EXIF orientation and converted with its embedded profile, with `full_width`/`full_height` the full upright size (#114). When no scale below full size fits (a 2048 × 1536 photo for 1600), or the file is CMYK, 16-bit or lossless, or `jpeg-decoder` cannot read it, `preview` is the full `zune-jpeg` decode. `working` and `source` are always the full `zune-jpeg` decode. CMYK and greyscale files are converted to RGB; a CMYK file's profile is not applied (#82). A truncated file yields the part that was read. Cost: cheap. |
| PNG | Cost: cheap under 8 MB, else decode. |
| WebP | Animated files: frame 1 is the picture. |
| GIF | Frame 1 is the picture, composed on a transparent canvas. |
| TIFF | The first page is the picture. Other pages are ignored. The ICC profile is read by a second decoder with no limits set, because once `image` sets limits a small TIFF's profile tag can no longer be read. Cost: decode; heavy over 100 MB. |
| BMP | A profile embedded in a version 5 header is not read (`image` does not expose it), so a BMP is taken as sRGB. Cost: decode. |

`preview` and `working` are the same decode for these formats. Cost for WebP and GIF: decode.

Lenient decoding is as lenient as each decoder: a truncated JPEG still yields a picture (`zune-jpeg` fills what is missing); the other decoders refuse most truncated files.

### Animated pictures

GIF and WebP files may hold more than one frame. `metadata` reports the frame count and the total length of one loop.

- Thumbnails, analysis, the full-size zoom copy, tagging and edit rendering all use **frame 1**.
- A file with more than one frame stores its loop length in `duration_ms`. A still stores NULL. So `kind = 'image'` with a `duration_ms` means "animated".
- The grid shows such a tile with a small "GIF" or "WebP" badge and does not animate it.
- The preview pane and the full-screen viewer show the animation by loading the original file bytes in an `<img>` from `owlbum://original/<photo_id>`. The webview plays it. Zooming an animated picture zooms that same element; it does not switch to the still zoom copy. The small thumbnail (frame 1) shows until the original is decoded; the medium thumbnail and the zoom copy are not asked for. An animated picture is told apart on the page by its row alone: `kind` is `image` and `duration_ms` is set.
- Animated PNG and animated JPEG XL are shown as stills in version 1.

### The JPEG XL handler

It decodes with `jxl-oxide`. Orientation and colour come from the codestream; `jxl-oxide` applies the orientation when it hands out pixels. `metadata` reads only as far as the image header, then walks the container's box headers (seeking past every other box, so the codestream is never read) to the `Exif` box, which may follow the codestream. A bare codestream has no EXIF, and a Brotli-compressed `brob` box is not read. `source` keeps 16 bits when the codestream's bit depth is above 8, and its `icc` is the profile `jxl-oxide` rendered into. A CMYK JPEG XL needs `jxl-oxide`'s colour management hook to render and is a `decode` error until #82. Cost: decode.

No JPEG XL encoder exists in Rust, so the handler's tests use five tiny files (under 300 bytes each) in `crates/owlbum-core/testdata/media/`, made once with ImageMagick and committed. `p3.jxl` is an 8 × 8 lossless picture of `rgb(200,100,50)` tagged with the fixture generator's Display P3 profile (`magick -size 8x8 "xc:rgb(200,100,50)" -depth 8 -profile p3.icc -quality 100 p3.jxl`). Every other test picture is generated by the test.

### The RAW handler

It uses `rawler` (LGPL-2.1, listed in the third-party notices).

| Call | Behaviour |
|---|---|
| `metadata` | Reads the maker's headers through `rawler`'s decode without the sensor data (`dummy`): dimensions after the default crop (else the active area), and the orientation from the EXIF tag (`rawler` leaves most decoders' own orientation at "normal"). The EXIF fields come from the file's own TIFF structure when it starts with a TIFF header (CR2, NEF, ARW, DNG), read by `media::exif`; otherwise (CR3, RAF, RW2, ORF) from the fields `rawler` read from the maker's headers. A DNG's uncompressed strips are still copied by `rawler` in this call. |
| `preview(min_side)` | The largest of the pictures `rawler` finds embedded (its `full_image`, `preview_image` and `thumbnail_image`, decoded by `rawler` with `image`), turned upright with the RAW's orientation, when its longest side is at least `min_side`; `full_width` and `full_height` are the developed size from the headers. Otherwise the sensor data is developed as in `working`, at full size. |
| `working` | The embedded picture when its longest side is at least 90% of the developed picture's (the zoom rule below); otherwise the sensor data developed with `rawler`'s default pipeline: demosaic, the camera's white balance, the camera's colour matrix, sRGB output. No controls are offered. `source` is the same picture, 16-bit when developed. `rawler` panics on some damaged files; every call into it is caught and becomes a `decode` error. |
| `cost` | Always heavy. Whether a file has a usable embedded picture is not known until it is opened. |

`photos.width` and `photos.height` are the dimensions of the **working** image, upright, never those of the embedded preview.

Which picture is used where:

| Use | Source |
|---|---|
| Small and medium thumbnails, analysis | `preview(1600)` |
| Zoom (`owlbum://full`), copy, edit rendering, Save as | The embedded JPEG when its longest side is at least 90% of the working image's longest side; otherwise `working`. |
| Tagging | The medium thumbnail. |

Many cameras embed only a small preview, so zoom then shows `rawler`'s rendering, which may differ in colour from the camera's own. Whether that jump is acceptable is an open question below.

### The video handler

Owlbum bundles no video decoder. Poster frames and playback use what the operating system has. The facts are read from the container by Owlbum itself, so they need no codec and are the same on every system (#50).

| Call | Windows | Linux |
|---|---|---|
| `metadata` | The probe's size and length; the date, place, make and model from `nom-exif`, given to the photo rules as EXIF tags. | Same. |
| `probe` | MP4 and MOV: a walk of the box headers to `moov` (at most 64 MB of it read), giving the length from `mvhd`, the displayed size and rotation from the video track's `tkhd` matrix, and the codec names from each track's first `stsd` entry. AVI: the `hdrl` list (`avih`, `strh`, `strf`). MKV and WebM: `nom-exif`'s length and size, no rotation; the codec names from a walk of the segment's `Tracks` element (the first video and audio track's `CodecID`, named as the MP4 walk names them: `V_MPEG4/ISO/AVC` is `h264`, `V_VP9` is `vp9`, `A_OPUS` is `opus`). | Same. |
| `preview` (the poster frame) | The shell's thumbnail for the file (`IShellItemImageFactory`, thumbnail only), asked at 1600 px but never more than the video's own size, so a small video is not enlarged. If the shell has none, one frame read with Media Foundation's source reader (video processing on, so the decoder's output is converted to RGB32) at the poster time below, cut to the stored size, turned by the rotation and made at most 1600 px; this needs a system decoder for the codec (H.264 in MP4, MOV and MKV out of the box; HEVC, VP9 and AV1, and so most WebM, only with the Microsoft Store video extensions). If neither gives a frame, `unsupported`. | None yet: `unsupported`, so videos are listed with placeholders. The GStreamer pipeline (`uridecodebin`, `videoconvert`, `appsink`) waits for Linux work to resume (#156). |
| `working` | Same as `preview`. | Same as `preview`. |
| `cost` | Heavy. | Heavy. |

```text
poster time = min(1000 ms, duration / 10)
```

Rules:

- `width` and `height` are the displayed frame size: rotation of 90° or 270° swaps them, and a non-square pixel aspect ratio is applied to the width.
- A video whose container can be read but whose frames cannot be decoded (no codec installed) **still gets a row**. It has no thumbnail; the page shows a film placeholder tile. Its `analysis` is `{}`. This is not a scan error.
- A video whose container cannot be read at all is a `decode` scan error.
- Video gets no editing. Tagging uses the poster frame.
- On Linux, GStreamer is found at run time. If it is missing, videos are listed with placeholders and the scan report says "Video previews need GStreamer, which was not found."

### Video playback

The viewer plays a video with a `<video controls>` element whose source is `owlbum://video/<photo_id>`.

| Platform | What plays |
|---|---|
| Windows (WebView2) | H.264, VP9 and AV1 in MP4, MOV, WebM and (where WebView2 accepts it) MKV. HEVC only when the user has the operating system's HEVC Video Extensions. AVI usually does not play. |
| Linux (WebKitGTK) | Whatever the installed GStreamer plug-ins decode. |

When the element reports an error, the preview and the viewer show the poster with the text "This video can't be played on this computer." and a **Show in folder** button (`file_reveal`), as [Browsing](04-browsing.md#video) words it. Owlbum never transcodes.

`owlbum://video` answers range requests:

- `Accept-Ranges: bytes` on every reply, and the registry's MIME type as `Content-Type`.
- A request with `Range: bytes=a-b` gets `206` with `Content-Range`. A request with no `Range` header is treated as `bytes=0-`; answering it with `206` is not standard, and whether both webviews accept it is checked under open question 4.
- No reply body is larger than 4 MiB. An open-ended range (`bytes=a-`) is answered with the next 4 MiB, and the player asks again. The file is never read into memory whole.
- An unsatisfiable range gets `416`.

**Content-Security-Policy.** The policy in [Architecture](01-architecture.md#network-rules) allows media from `owlbum:` only. Its exact string, in [Desktop, release and tests](08-desktop-and-release.md#content-security-policy), has `media-src owlbum: http://owlbum.localhost` (Tauri addresses a custom scheme as `http://<scheme>.localhost` on Windows; the image rule lists the same origin). `blob:` is added to `media-src` only if fallback 2 below is adopted.

**Linux fallback.** Tauri issue #3725 (open, labelled upstream) reports that video and audio loaded over a custom URI scheme fail on Linux, while images work. An early prototype tries these in order and the first that works on Ubuntu LTS and Fedora is adopted:

1. `owlbum://video` as specified above.
2. The page fetches the file through a command (`photo_video_chunk(id, offset, length)`, added only if this step is adopted) in 4 MiB pieces, builds a `Blob`, and plays its `blob:` URL. Only for files up to 256 MB; larger files go to step 3.
3. No playback inside Owlbum on Linux: the viewer shows the poster, the cannot-play text and **Show in folder**. Version 1 ships this way with an issue open, as the Linux-gaps rule in the README allows.

A local HTTP server is not an option. If poster frames also prove unreliable on Linux, video on Linux ships as placeholder tiles plus **Show in folder**.

## The walker

The walker lists the files of the scope: the album root, or the scoped folder. It always includes subfolders. It reads directory entries without following links.

| Entry | Rule |
|---|---|
| Name starts with `.` | Skipped, file or folder. This covers `.owlbum`, `.git`, `.Trash-1000` and dotfiles. |
| File name starts with `._` | Skipped (macOS AppleDouble files). Covered by the rule above; stated because it must hold even if the dot rule changes. |
| Folder named `$RECYCLE.BIN`, `System Volume Information`, `@eaDir`, `#recycle`, `#snapshot` or `__MACOSX` (compared without case) | Skipped. These hold deleted files or another program's thumbnails. |
| Symbolic link, to a file or a folder | Skipped and counted. On Windows this includes junctions; a test pins that the standard library's link test reports them. Cloud-drive folders are reparse points too but are not links, and are walked. |
| Subfolder that holds `.owlbum/owlbum.sqlite` (a nested album root) | Skipped whole and recorded as a nested album. See below. |
| Subfolder that cannot be read | Recorded with its path and the system's message, then skipped. The walk continues. |
| Supported file that is online-only (a cloud placeholder: Windows attribute `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN` or `OFFLINE`) | Counted as "stored online only" and not read, so a scan never downloads a whole cloud drive. The attributes come from the directory listing, which opens nothing. A row whose file is online-only is not gone: it is kept as it is, not read, not a move candidate and never marked missing. A recognised-but-unsupported file is counted as unsupported whether online or not (neither is read), and any other file is ignored as below. Folders are never tested: cloud folders are walked. Only Windows has these attributes; elsewhere no file is online-only. |
| File name that is not valid Unicode | Counted as skipped with the reason "This file's name can't be read." |
| Regular file where `is_supported(name)` | Listed, with its size and modified time from the directory entry where the system gives them free. |
| Regular file where `unsupported_of(name)` is some | Counted and listed as not supported yet. |
| Anything else | Ignored silently. |

**Symbolic links are skipped on purpose.** Following them can loop, can leave the album, and can list one file under two paths. The count of skipped links is in the scan report.

System folders and dot names are left out silently. Links, online-only files and unreadable names make up the `skipped` count of `album_status`; nested albums and unreadable folders are listed in the scan report instead.

If the walk's own root cannot be read, the scan fails with `folder_unreadable`: "Can't read the folder {root}: {system message}". If it is not a folder: `not_a_folder`, "Not a readable folder: {root}". In both cases nothing in the database is changed.

### Order

The walker's output, and the name order of the query engine, use one comparison function, `natural_cmp`. It does not depend on the computer's language settings. The walker applies it to whole relative paths, as below. The name order in [Organising](05-organising.md#sorting) applies the same string rules (steps 2 to 5) to the file name first, then to the folder, then compares ids; "walker order" in this part always means the path order here.

1. Compare the two relative paths one component at a time. A path that runs out of components first sorts first.
2. Within a component, split the text into runs of ASCII digits and runs of everything else.
3. Two digit runs compare by number (strip leading zeros, then the longer run is larger, then compare digit by digit). Equal numbers: the run with fewer leading zeros sorts first.
4. Two text runs compare by Unicode code point after simple lower-casing.
5. A digit run sorts before a text run.
6. If everything is equal, compare the original bytes.

So `IMG_2.jpg` sorts before `IMG_10.jpg`, and `a/z.jpg` before `a b/a.jpg`.

### Paths

`rel_path` is stored as the system reports it, with `/` separators, with no Unicode normalisation and no case change. On Windows a rename that changes only the case of a name is handled as a move by the scanner.

### Scoped opens

In a scoped open the walk starts at the scoped folder. Relative paths are still relative to the album root. Every scanner rule below that says "rows" means rows whose `rel_path` lies under the scope.

### Nested album roots

Following [Data](02-data.md): a subfolder with its own `.owlbum/owlbum.sqlite` belongs to that album, and the outer album skips the subtree. The walker checks every subfolder it enters for that file (one existence test per folder). The walk's own folder (the album root, or the scoped folder) is never treated as nested.

Rows the outer album already holds under a nested root are not found by the walk, so they become missing rows by the normal rule. The scan report lists nested albums: "{path} is its own album and is not shown here."

## The scanner

A scan brings the database in step with the scope. One scan runs per album at a time. It starts when an album opens, when the watcher asks, and when the user chooses Rescan.

The rows already in the database are queryable the moment the album opens. A second visit to an album shows its photos at once; the scan then confirms them in the background.

### Stages

```text
1. Walk          list files: rel_path, size, mtime_ms
2. Compare       sort every listed file and every row into: unchanged, changed, new, gone
3. Match moves   file_key for new files; pair with gone rows (rules in Data)
4. Facts pass    headers only for new and changed files; rows inserted or updated
5. Pixels pass   decode, thumbnails, analysis; rows completed
6. Finish        mark missing rows, final events, warm missing small thumbnails
```

Stages 1 to 4 are quick: they read directory entries, at most 32 KiB per new file for the key, and file headers. After stage 4 every photo is in the database with its name, size, dimensions, date, camera and place, so the grid, the timeline and the map are complete. Stage 5 is the slow one and fills in pictures and quality measures.

### Compare

`mtime_ms` is the file's modified time rounded to the nearest millisecond. For each listed file, look up the row with the same `rel_path`:

| Row found? | Test | Class |
|---|---|---|
| Yes | `size` and `mtime_ms` equal, `scan_version` current, no `scan_error`, `analysis` set | **Unchanged.** Nothing is read. If `missing = 1` it is set back to 0. |
| Yes | The same, but `analysis` is NULL | **Waiting.** The facts are kept; the file goes to the pixels pass and is not counted in `done` until that ends. |
| Yes | `size` and `mtime_ms` equal, `scan_version` current, `scan_error` with code `io` | **Retry.** Treated as changed. |
| Yes | `size` and `mtime_ms` equal, `scan_version` current, any other `scan_error` | **Unchanged** on an automatic scan; **retry** on a manual Rescan. |
| Yes | `size` or `mtime_ms` differ, or `scan_version` is older | **Changed.** |
| No | | **New.** |

A row in scope with no listed file is **gone**.

A changed file keeps its row, id, typed info and tags. Its `file_key` is recomputed and every scan fact is read again. A changed modified time always forces the full re-read, because `file_key` samples only the ends of the file and cannot prove the middle is the same. When the `file_key` changed, the writer deletes that photo's `tags`, `vectors` and `photo_labels` rows and clears its analysis in the same transaction, as [Data](02-data.md#schema) rules, and `album_changed` lists the ids in `rekeyed` so the page drops their cached pictures and tagging looks at them again (see [Tagging](07-tagging.md)).

### Match moves

`file_key` is computed for every new file and every changed file: `fk1:<size>:<sha1 of size, first 16 KiB, last 16 KiB>` as defined in [Data](02-data.md). The moves rule there is applied to new files against candidate rows. Details it leaves open are settled here:

- The candidates are the rows gone in this scan **and every row in scope already marked `missing = 1`**. A move whose two halves were seen by different scans (the watcher saw the delete, then later the create) still finds its row.
- New files are taken in walker order. Among several candidate rows for one file, a row with the same file name is preferred; then the first in path order, exactly as Data says. If two new files have the key of one candidate row, the first takes the row and the second gets a new row.
- A candidate row is offered only once per scan.
- A key with an empty hash (see Errors per file) never matches anything.

A matched row has its `rel_path`, `rel_dir` and `file_name` updated. If its `size` and `mtime_ms` also equal the file's, nothing else is read. Because `thumb_key` does not include the path, its thumbnails are still valid.

### Facts pass

For each new and changed file, on the scan pool:

1. Stat the file again. If it changed since the walk, use the new values.
2. Read the head and tail for `file_key` (already done for new files) and sniff the head.
3. Call the handler's `metadata` and, for video, `probe`.
4. Send the writer a row: identity, `kind`, `format`, `width`, `height`, `duration_ms`, `captured_at`, GPS, `camera`, `exif`, `scan_version` current, `analysis = NULL`, `scan_error = NULL`.

`analysis IS NULL AND scan_error IS NULL` is the mark of a row waiting for the pixels pass. It survives a cancelled scan and a crash, so the next scan picks the work up without redoing stage 4.

If `metadata` cannot give dimensions, the row is written with `width = 0` and `height = 0` and the pixels pass supplies them. The page draws a row with zero dimensions as a square tile.

### Pixels pass

For each waiting row, on the scan pool:

1. `preview(1600)` once. This single decode feeds everything below. For a large JPEG it is the DCT-scaled decode (see The still handler), so the buffers below are resized from about a quarter of the pixels.
2. Flatten on white and resize to the 1600 px buffer, then the 512 px buffer from it, then the analysis image from that (see Thumbnails and Image analysis). The 1600 px buffer is always built in memory, so the analysis numbers never depend on which thumbnails are kept.
3. Encode and write the small thumbnail, and the medium one when the cache has room.
4. Send the writer `analysis`, and `width` and `height` when the header gave none or when the decode reports a different full size (`full_width`, `full_height`, when not 0). The size of a reduced or embedded preview is never stored.

**Order of work (#106).** Decodes, the scan's and the page's together, run in **decode slots**: as many at once as the scan pool has threads (held by `Thumbs`, in `thumbs::slots`). A free slot goes:

| Priority | Jobs |
|---|---|
| 1 | Pictures the page asks for through `owlbum://` (thumbnails and zoom copies), **newest request first**: what is on screen now comes before what was on screen a moment ago, so scrolling to a part of the album the scan has not reached makes its thumbnails come next. |
| 2 | The scan's pixels pass, in its own order: cheap handlers (JPEG, small PNG), then decode-cost handlers, then heavy handlers (RAW, video, large TIFF), each in walker order. |
| 3 | The warm pass after a scan, which waits behind the page like the pixels pass. |

A job holds its slot from the decode until its thumbnails and analysis are made, so no more full-size pictures are in memory than there are slots, and the scan's resizing does not compete with the page's. A scan job takes a slot only while no request from the page waits, so a request waits at most for one of the running jobs to end. A folder of mixed JPEG and RAW shows its JPEGs first. At most `max(1, pool size / 2)` heavy jobs run at once, to bound memory. A busy file's retries (Errors per file) keep their slot; they are rare.

When the page asks for a photo still waiting for its pixels pass, the request's job **is** that photo's pixels pass: one decode makes the thumbnails and the analysis, and the analysis is kept in memory by `thumb_key` (at most 4,096 of them) until the scan reaches the photo and sends it to the writer like any other result, without decoding again. When the scan's job for that photo is already running, the request joins it instead (see "Concurrency and writes"). The scan's walker order is not changed: rows still arrive and complete in that order, while what is on screen jumps ahead through the page's requests.

### Errors per file

A file that fails in either pass gets a row with `scan_error` set. The scan carries on.

`scan_error` holds `<code>: <sentence>`:

| Code | Meaning | Example sentence | Tried again |
|---|---|---|---|
| `io` | The file could not be opened or read. | "This file could not be read: {system message}" | Every scan. |
| `decode` | The bytes are not a picture the handler can read. | "This file is damaged or is not a {label} picture." | When the file changes, on a manual Rescan, or after a `scan_version` bump. |
| `unsupported` | The bytes are a format Owlbum does not support yet. | "HEIC photos are not supported yet." | Same as `decode`. |
| `too_large` | Over the pixel limit. | "This picture is too large to open (40000 × 30000)." | Same as `decode`. |

Placeholders on an error row: `kind` and `format` from the extension's registry row, `width = 0`, `height = 0`, `scan_version` current, every other scan fact NULL. `size`, `mtime_ms` and `file_key` are real when the file could be read that far; when it could not be opened at all, `file_key` is `fk1:<size>:` with an empty hash, which move matching and tag copying ignore.

Error rows are **left out of every query result**, like missing rows, and are counted and listed in the scan report. Their typed info and tags are kept, so a photo that was briefly unreadable (a drive glitch) loses nothing when it reads again.

A file modified within the last 2 seconds that fails to decode is assumed to be still being written. It is not recorded as an error; it is tried again 2 seconds later, up to 5 times. If the fifth try still fails, the error is recorded as usual.

### Finish

Rows still gone after move matching are set `missing = 1`. This is done **only** when the walk completed and the scan was not cancelled, and never for:

- rows under a subfolder the walk could not read;
- rows whose file was listed as online-only;
- any row, when the walk's root could not be read (an unplugged drive must not mark an album missing).

After the last write the scanner emits the final `scan_progress` and starts the **warm pass**: at lowest priority it makes the small thumbnail (only the small one, through the same fast path as the page, see On demand) for any row in scope that has none in the cache (after the cache was pruned or app data deleted). It finds them from `cache_entries` in one query, not by testing files. The warm pass is not part of the progress count and stops on cancel and when another scan is asked for.

### `scan_version`

`SCAN_VERSION` is an integer constant in `owlbum-core`, starting at 1. Version 1 stored `analysis = '{}'` and no metadata facts; version 2 (#10) added the metadata reader and the [image analysis](#image-analysis); version 3 (#114) decodes a large JPEG's preview at a reduced DCT scale, which moves the analysis numbers a little; version 4 (#49, #50) reads RAW and video files, which version 3 recorded as `unsupported` error rows. It is raised in the same commit as any change to the metadata rules, the analysis formulas or the placeholders. On the next scan every row with an older value is treated as changed: facts and analysis are read again. Thumbnails that already exist under the same `thumb_key` are not encoded again.

### Concurrency and writes

- The scan pool has `max(1, logical processors − 1)` threads, as [Architecture](01-architecture.md) sets. The walk runs on one of them.
- Scan threads never touch the database for writing. They send results to the album's writer thread.
- The writer collects scan results and commits them in one transaction when **200 rows** are waiting or **250 ms** have passed since the first waiting row, whichever comes first. The FTS `search` table is updated in the same transaction.
- A typed change (a star, a note) is always its own transaction. It waits for at most one scan batch.
- The compare stage reads `id, rel_path, size, mtime_ms, scan_version, scan_error, missing, file_key, analysis IS NULL, format, width, height` for the scope in one query (the last four tell a waiting row and give its pixels job its cost) into a hash map. It never queries per file.
- Work in flight is keyed by `thumb_key`, for the page's requests and the scan's pixels pass alike (#91, #106): a request that finds the scan decoding that photo waits for the scan's job and serves its thumbnail; a scan job that finds a request's job running waits for it and writes the pass that job made. Either way the photo is decoded once. The scan pool itself is still made per stage; the decode slots in `Thumbs`, which outlive a scan, are what puts the page's work before the scan's.

### Progress and change events

`scan_progress` is emitted at most 10 times a second, plus once at the end.

| Field | Meaning |
|---|---|
| `found` | Supported files listed so far. It grows during the walk and is final when the walk ends. |
| `done` | Files that need nothing more: unchanged files count at the compare stage; the others count when their pixels pass ends or fails. |
| `failed` | Files given a `scan_error` by this scan. |
| `finished` | True on the last event of a scan, including a cancelled one. |
| `phase` | `walking` until the compare stage ends, then `reading`; `idle` on the last event. |
| `cancelled` | True on the last event when the scan was cancelled. |

The bar reads "Looking for photos…" while walking and "Reading photos {done} / {found}" while reading. The time-left estimate and its wording are in Browsing; it must not count unchanged files as work done.

`album_changed` with reason `scan` is emitted at most once a second while a scan is writing, and once at the end. Its payload carries the new revision; the page re-runs its query.

### Cancellation

A scan is cancelled by `album_scan_cancel`, by closing the album, by opening another album, and by quitting. The flag is checked per directory in the walk and between files in both passes. A decode already running finishes and its result is kept. Waiting results are committed. No row is marked missing. Waiting rows stay waiting and are picked up by the next scan.

A scan request that arrives while a scan runs does not start a second one. A request for the whole scope sets "scan again when this one ends". A request for some folders (from the watcher) is merged into the pending set.

### View-only albums

A view-only album is scanned the same way, but nothing is written to the album database. Results for files the database does not know are kept in memory for the session, as [Data](02-data.md) says, with ids above the database's largest id. Thumbnails are still written, because they live in app data. Missing rows are hidden in memory, not marked.

### Speed targets

These are targets to measure against with the `large-N` fixture, not promises. Each is recorded on the scanner's issue with the measured number.

| Case | Target |
|---|---|
| Reopen an album of 100,000 photos already scanned | First tiles within 1 s of opening; walk and compare done within 15 s on a local SSD. |
| First open of 10,000 JPEGs | First tiles within 2 s; every photo listed (facts pass done) within 20 s. |
| First open of 2,000 photos of 12 to 24 MP (#106) | First rows within 1 s; the first screen of thumbnails within about 2 s while the rest keeps scanning. |
| Any scan | A star or a note is saved within 300 ms while the scan runs. Memory stays bounded: no stage holds decoded pixels for more than the pool's worth of files. |

**First open (#106).** `crates/owlbum-core/tests/first_open.rs` plays the page against a new album (its header says how to run it). On the development machine (11 decode slots), release build, 2,000 photos of 12 to 24 MP: the first `album_changed`, with 400 rows, came 0.2 s after opening, well inside the target. The first screen of 60 thumbnails, asked for all at once, was served by 4.4 s after opening (half of them by 2.7 s), against 7.9 s before the decode slots, when a request's resizing and encoding ran outside any limit on all 60 request threads at once. A screen near the end of the walk, asked for while the scan decoded, was served by 3.9 s (half by 2.4 s), against 6.8 s (3.9 s) when the page's decodes ran beside the scan's. The whole scan took 112 s (116 s before). The 2 s target is not met: a 24 MP JPEG costs about 270 ms alone (90 ms decoding at full size, 160 ms resizing from it), and making that smaller is #114.

**Reopen of 100,000 (#26).** `crates/owlbum-core/tests/large_album.rs` scans the `large-100000` fixture of tiny photos, then opens it again with its thumbnails kept. Release build, local SSD, development machine: the first scan took 274 s (first `album_changed` at 2.6 s). On the reopen the walk and compare took 1.9 s, well inside 15 s. The first screen of 60 tiles (open, `query_run`, the first window, 60 small thumbnails from the cache) was served 3.0 s after opening on the reopen just after the scan and 1.0 s on a later one; almost all of it is `Album::open`, which runs `quick_check` (0.3 to 0.5 s) and loads the album index (0.76 to 1.2 s) before it answers. The 3.0 s case is the first reopen of the day, which also made the day's backup (about 1.7 s at 100,000 photos) before answering.

**Reopen after #124.** The day's backup is now made on a thread after the open answers (docs/02, "Backups and damage"), the album index is read beside `quick_check` instead of after it, and the name order's string comparison has an ASCII path (its sort went from about 300 ms to 86 ms at 100,000 photos). The same run: the first screen of 60 tiles 0.61 s after opening on the first reopen of the day (was 2.7 s), and 0.57 s on a later reopen (was 1.0 s). The open is now the longer of `quick_check` (about 0.28 s) and the index load (about 0.55 s).

**Reopen after #162.** By #162 the index load had grown to 1.4 to 1.8 s (the analysis JSON, the bursts and the name order came with #28 to #32). The open now reads the album's rows on several read-only connections at once and builds the records on several threads, and the burst search reads a compact copy of each photo's date and hash; see [Organising](05-organising.md#performance-targets). The first screen of 60 tiles: 0.87 to 0.92 s after opening (target 1 s), with `Album::open` 0.83 to 0.89 s.

**Windows and Defender (#95).** On Windows a first scan with thumbnails pays for Defender's real-time protection, which scans each thumbnail file as it is created: on the development machine `MsMpEng` used two to almost four cores while thumbnails were written, and 20,000 tiny files took about 11 s at 11 threads even written in place with their folders made ahead. Owlbum keeps that to one created file per thumbnail (no temporary file, no rename, no folder call per write) and does not change Defender's settings or ask for an exclusion. On real photos the cost is small next to decoding: on 2,000 photos of 12 to 24 megapixels a first scan took about 106 s without thumbnails and 118 to 128 s with them, within the noise between runs. An album of tens of thousands of tiny files shows it most. A user who adds the app data folder to Defender's exclusions removes it; Owlbum does not suggest that.

## Metadata reading

`metadata` reads only the metadata blocks of a file, never the whole file. Each handler finds the EXIF block (JPEG `APP1`, PNG `eXIf`, WebP `EXIF`, JPEG XL `Exif` box) and gives it to one small EXIF reader, `media::exif`, which reads IFD 0 and its Exif and GPS sub-IFDs with seeks and keeps every text and numeric value of at most 4 KiB. A TIFF file is itself the structure EXIF uses, so its tags are read from the file the same way, seeking to its IFDs. The rules below are applied to those tags by `scan::metadata`. A file with no metadata, or with metadata that fails to parse, is not an error: the fields are NULL.

`nom-exif` was the first choice, but it parses the three date tags with a strict `%Y:%m:%d %H:%M:%S` and drops a value that does not match, so the date rule's other forms could never be seen; it also wants a TIFF file in memory (#10). Limits that remain: a PNG `eXIf` chunk after the pixel data is not seen (`image` reads chunks only up to the first `IDAT`), and GIF and BMP carry no EXIF.

### Capture date

`captured_at` is the first of these that parses:

| Order | EXIF tag |
|---|---|
| 1 | DateTimeOriginal (0x9003) |
| 2 | DateTimeDigitized (0x9004), also called CreateDate |
| 3 | DateTime (0x0132), also called ModifyDate |

A value parses when it matches:

```text
^(\d{4})[:-](\d{2})[:-](\d{2})[ T](\d{2}):(\d{2}):(\d{2})
```

and the year is not `0000`, and (new in Owlbum) the six numbers form a real calendar date and time (a second of 60 is refused). Only ASCII digits count. It is stored as `YYYY-MM-DDTHH:MM:SS`. Sub-seconds and time-zone offsets are ignored: the value is the wall-clock time the camera showed, never converted through any time zone. The file's own modified time is never used as a capture date.

### Camera

Trim Make (0x010F) and Model (0x0110).

| Make | Model | `camera` |
|---|---|---|
| empty | empty | NULL |
| empty | present | Model |
| present | empty | Make |
| present | present, and starts with Make (ignoring case) | Model |
| present | present, otherwise | `Make Model` |

### The `exif` JSON

Missing or nonsense values are left out. An object with no keys is stored as NULL.

| Key | Source | Kept when |
|---|---|---|
| `lens` | LensModel, prefixed with `LensMake + ' '` unless the model already starts with the make (ignoring case) | Not empty after trimming. |
| `focal_length` | FocalLength, mm | A finite number above 0. |
| `focal_length_35` | FocalLengthIn35mmFilm | A finite number above 0. |
| `aperture` | FNumber | A finite number above 0. |
| `exposure_time` | ExposureTime, seconds | A finite number above 0. |
| `iso` | ISO (PhotographicSensitivity); the first value when there are several | Above 0. |
| `exposure_bias` | ExposureBiasValue, EV | Any finite number, including 0 and negatives. |
| `flash` | Flash | `true` when bit 0 of the value is set, `false` when the tag is present and bit 0 is clear; absent when the tag is absent. |
| `video_codec`, `audio_codec` | Video probe | Video only; short names such as `h264`, `hevc`, `aac`. |
| `frames` | Frame count | Animated pictures only. |

Rationals with a zero denominator are nonsense and left out.

### GPS

Latitude and longitude come from GPSLatitude and GPSLongitude with their reference tags (S and W are negative). Both are kept only when both are finite. New in Owlbum: both are dropped when the latitude is outside ±90, the longitude is outside ±180, or both are exactly 0 (cameras write 0, 0 when they have no fix). Altitude is not stored.

### Video

| Fact | Rule |
|---|---|
| `duration_ms` | From the probe, rounded to the millisecond. |
| `width`, `height` | Displayed frame size, as in the video handler. |
| `captured_at` | First, a creation date that is written with its own local time (QuickTime `com.apple.quicktime.creationdate`): its wall-clock part is taken and the offset dropped. Otherwise the container's creation time (`mvhd` in MP4 and MOV, `DateUTC` in Matroska), which is UTC by specification: it is converted to this computer's local time at scan time. A zero value or the 1904 epoch means none. |
| GPS | QuickTime location (`com.apple.quicktime.location.ISO6709` or `©xyz`), with the same checks as photos. |
| `camera` | QuickTime make and model, by the camera rule above. |

These rules are `scan::metadata::video_captured_at` and the photo rules above; the video handler (#50) supplies the container's values. `nom-exif` gives one creation date: a QuickTime creationdate with its offset when the file has one, else the `mvhd` time as UTC. A date with a non-zero offset is taken as the wall clock; one at UTC is converted to this computer's zone, so a creationdate written at +00:00 is treated as UTC.

Converting a UTC creation time with the local zone is wrong for a video shot in another time zone. The user can correct it with the date in Details; the limit is accepted for version 1.

## Image analysis

Analysis runs once per photo in the pixels pass. Its results are the `analysis` JSON. The numbers feed the photo score, stacks and colour sorting (see Organising), so the pipeline is pinned exactly.

**The analysis image.** Take the small thumbnail's pixels before JPEG encoding: upright, sRGB, flattened on white, at most 512 px on the longest side. Resize to fit inside 256 × 256, keeping the aspect ratio, never enlarging, each side at least 1 px, with the Lanczos3 filter. Call its size `w × h`. The greyscale version is:

```text
grey = round(0.299·R + 0.587·G + 0.114·B)      on 8-bit sRGB values, 0..255
```

Pinned details (#10), in `scan::pixels` and `scan::analysis`:

- **Fitting.** Every "fit inside N" (1600, 512, 256) scales the longest side to N and the other side to `round(side · N / longest)`, half up, at least 1. A picture that already fits is not resampled. Lanczos3 is `fast_image_resize`'s convolution, which works on 8-bit samples in fixed point. It brings a 24-megapixel decode to both thumbnail buffers in about 110 ms on one thread, against 450 ms for `image`'s own resize. One set of buffers (`thumbs::make::buffers`) is built per decode and feeds both the thumbnails and the analysis.
- **Flattening.** Each colour becomes `(c·a + 255·(255 − a) + 127) / 255` in integers. The alpha plane goes through the same resizes as the colours (1600, then 512, then 256).
- **Grey** is worked in integers: `(299·R + 587·G + 114·B + 500) / 1000`.
- **Box filter.** An area average: each output pixel is the mean of the source area it covers, with partly covered source pixels weighted by the part covered. It is worked in integers (positions in fractions of `1/N` of a source pixel), so a flat area gives equal means exactly. dHash compares these means exactly, unrounded; `colour_grid` and `avg_colour` round them to the nearest level, half up.
- **Sharpness** sums `v` and `v²` in integers before dividing, so it has no rounding drift.
- **Rounding.** Numbers are rounded half away from zero to their decimals, and a negative zero is written as 0.

Below, `p(x, y)` is a grey pixel, `l, r, u, d` are its left, right, upper and lower neighbours, and "interior" means `1 ≤ x ≤ w−2` and `1 ≤ y ≤ h−2`.

| Key | Formula | Rounding |
|---|---|---|
| `sharpness` | Variance of the 4-neighbour Laplacian `v = 4·p − l − r − u − d` over the interior pixels: `mean(v²) − mean(v)²`. 0 when there are no interior pixels. | 2 decimals |
| `mean_luma` | Mean of `p / 255` over all pixels. | 3 decimals |
| `clipped_shadows` | Fraction of pixels with `p ≤ 8`. | 3 decimals |
| `clipped_highlights` | Fraction of pixels with `p ≥ 247`. | 3 decimals |
| `saliency_x`, `saliency_y` | See below. | 3 decimals |
| `dhash` | See below. | 16 hex digits |
| `avg_colour` | `[R, G, B]`: the mean of each channel over the colour analysis image. | Integers 0..255 |
| `colour_grid` | 27 integers: the colour analysis image resized to exactly 3 × 3 with a box (area average) filter, row-major RGB. | Integers 0..255 |
| `transparent` | See below. | Boolean |

**Saliency.** For each interior pixel:

```text
g  = |r − l| + |d − u|
nx = x / w − 0.5
ny = y / h − 0.5
e  = g² · exp( −(nx² + ny²) / (2 · 0.3²) )

saliency_x = Σ(e · x / w) / Σe
saliency_y = Σ(e · y / h) / Σe
```

Both are 0.5 when `Σe` is 0.

**dHash.** Resize the grey analysis image to exactly 9 wide by 8 high, ignoring the aspect ratio, with a box filter. For each row top to bottom and `x = 0..7`, the bit is 1 when `p(x) < p(x+1)`. The 64 bits, in that order, are written as 16 hex digits, four bits per digit, most significant bit first.

**Transparency.** `transparent` is false when the source has no alpha channel. Otherwise take the source's alpha channel at the analysis image's size (before flattening); it is true when more than `w·h / 200` pixels (0.5%) have alpha below 250.

**What gets analysed.** Stills and RAW: the preview. Animated pictures: frame 1. Video: the poster frame, which also gives the video its small and medium thumbnails; a video with no poster stores `{}` and counts as not analysed. [Organising](05-organising.md#lines) scores a video from this analysis (without the Size line) and [Tagging](07-tagging.md) tags it from the medium thumbnail of the same frame. A row with a `scan_error`, a missing row and a row outside the scope are not analysed by a scan, not scored and not tagged.

## Thumbnails and the zoom copy

`thumb_key`, the folder layout under app data, the `cache_entries` table and the size limit are defined in [Data](02-data.md). This section says what is made and when.

### What a thumbnail is

| | Small (`s`) | Medium (`m`) |
|---|---|---|
| Longest side | 512 px | 1600 px |
| Starting JPEG quality | 78 | 84 |
| File | `thumbs/<aa>/<thumb_key>_s.jpg` | `thumbs/<aa>/<thumb_key>_m.jpg` |
| Made by the scan | Always | While the cache is under 70% of its limit |
| Made on demand | Yes | Yes |

The encoder is `jpeg-encoder` (pure Rust, 4:2:0 baseline). `image`'s own encoder writes only 4:4:4, and `mozjpeg` needs a C and assembler toolchain on every build machine, so neither was taken (#11). Both qualities are starting values, still to be confirmed by measurement: file size, encode time, and a visual check at 2× screen density.

The pipeline, in this order:

1. Decode with the handler's `preview(1600)`. The result is upright and sRGB.
2. Flatten transparency onto white (`#ffffff`).
3. Resize to fit inside N × N with Lanczos3, keeping the aspect ratio. A picture smaller than N is never enlarged.
4. Encode as JPEG, 4:2:0 chroma, no metadata, no embedded profile.

The medium thumbnail is made first. The small one is resized from the medium one's pixels, not decoded again. The one exception is the page's fast path (On demand, below): a small thumbnail asked for before anything made it is decoded with `preview(512)` and resized straight to 512 px. Its pixels can differ a little from the scan's; the scan never replaces it, and the analysis is always made from the 1600 → 512 chain. Thumbnails are always opaque JPEG.

Making the medium size for every photo of a very large album would fill the cache and push out the small ones, so the scan stops making it at 70% of `cache.limit_gb`. From then on medium thumbnails are made when asked for.

### Writing

- A new thumbnail is written straight to its final name, which is created only if nothing is there. No temporary file and no rename: on Windows, Defender's real-time protection scans every file a program creates and again when one is renamed, and a temporary file renamed into place cost twice as much as one file written in place (#95). A reader that finds the file before it is whole treats it like one cut short (below): it makes the picture again and writes over it.
- A file already at the final name (cut short, or another thread's) is replaced through `<final name>.<random>.tmp` in the same folder, renamed over it. If the rename fails and the final file exists, another thread got there first and that is success. The temporary file is removed whenever it is left behind.
- The `thumbs/<aa>` folder is made only when creating the file finds it missing, not before every write, so Clear cache needs nothing further.
- After the write, a `cache_entries` row is written with kind `s` or `m` and the file's size. Rows are written in batches (when 200 wait or a second after the first), because `app.sqlite` runs with `synchronous = FULL`; the cache's total size is kept in memory, so the size limit and the 70% rule cost no query.
- `last_used` is updated in memory when a thumbnail is served and flushed to `app.sqlite` every 60 seconds, not once per request.
- Thumbnail files are not flushed to disk with `fsync`; they are a cache. To catch a file cut short by a power loss, the protocol handler checks that a thumbnail is not empty and ends with the JPEG end marker (`FF D9`) before serving it. A file that fails is made again and written over; it is not deleted on sight, because it may be a new thumbnail still being written.

### On demand

`owlbum://thumb/<thumb_key>/<s|m>` is answered like this:

1. Find the photo. `thumb_key` is a stored column of `photos` with its own index (`photos_thumb_key`, see [Data](02-data.md#schema)), so this is one indexed read on a pooled read-only connection. A key that belongs to no row of the open album that is in scope and not missing answers 404, even if a file for it sits in the shared cache.
2. If the file exists and passes the end-marker check, serve it. Nothing is written to any database.
3. Otherwise make the thumbnail and serve it. Requests for the same key share one job, and share the scan's job when the scan is decoding that photo. Requests are answered on the blocking pool; their decodes take the decode slots ahead of the scan's, newest request first.
   - **Small (the fast path, #120).** The job decodes with `preview(512)` (for a large JPEG, the DCT-scaled decode at 1/4 or 1/8), resizes straight to 512 px, writes the small thumbnail and answers. Nothing else is made: for a photo still waiting, the scan's pixels pass later decodes it with `preview(1600)` as usual, writes the medium thumbnail and the analysis, and finds the small file whole and leaves it, so the tile never changes under its key. This costs a second decode of each photo the page asks for before the scan reaches it, in exchange for the first screen of a new album coming within about 2 s.
   - **Medium.** The job decodes once with `preview(1600)` and writes the medium thumbnail, and the small one when it is not on disk. For a photo still waiting for its pixels pass the job makes the analysis too, for the scan to write without decoding again (see "Order of work").
4. If the photo cannot be decoded, answer 404. A video with no poster answers 404 and the page shows its placeholder tile.

`query_window` reads each row's `thumb_key` from the same stored column, so no hashing happens per request. The scanner writes the column whenever it writes `file_key` or `mtime_ms`. In a view-only album the keys of rows held only in memory are kept with those rows.

### The zoom copy

`owlbum://full/<photo_id>?v=<thumb_key>` returns a full-size upright picture for zooming. Unlike the old program, which decoded and encoded the original again on every request, Owlbum makes it once and keeps it.

| Case | What is served |
|---|---|
| The original is a JPEG that is already upright (orientation 1 or absent) with no non-sRGB profile | The original file's bytes. Nothing is cached. |
| Any other still, or a RAW | `full/<thumb_key>.jpg`: the handler's `working` image (or the RAW's embedded JPEG by the 90% rule), flattened on white, JPEG quality 90, full size. Made on first request, on the blocking pool. |
| A video | 404. |

Rules: requests for the same photo share one job; the file is written by the same temporary-then-rename steps; a `cache_entries` row of kind `full` is written; the end-marker check applies. A picture with a side over 65,535 px (the JPEG limit) is scaled down to fit. The `v` parameter exists only so the webview's own cache drops the picture when the file changes.

## Folder watching

While an album is open, Owlbum watches the scope with `notify` so that photos added, changed or removed by other programs appear without the user doing anything.

### What is watched

One recursive watch on the scope's folder, set on a thread of its own (a recursive inotify watch walks the whole tree), not following links and not asking for access events, so the scanner's own reads never look like changes. Events are dropped at once when any path component below the scope starts with `.` (this covers `.owlbum`, whose database journal changes on every write and must never cause a rescan) or is one of the walker's system folders, when the path is a link, or when the path is a file that is neither supported nor recognised-but-unsupported and is not a folder.

### Debounce

Events are collected into a set of **folders to rescan**. The set is acted on when there has been no new event for **2 seconds**, or **10 seconds** after the first event, whichever comes first.

| Event | Added to the set |
|---|---|
| A file created, modified, removed or renamed | The file's parent folder (both parents for a rename across folders), not recursive. |
| A folder created, or renamed into place | That folder, recursive. |
| A folder removed, or renamed away | Its parent, recursive. |
| A path removed that can no longer be told apart (it is gone, so file or folder is unknown) | If any row has that path as its `rel_dir` or under it, treat it as a folder; otherwise as a file. |
| The watcher reports an overflow or "rescan needed", or more than 500 folders are in the set | The whole scope. |

A targeted rescan runs the normal scanner stages with the walk limited to the set. The compare stage looks only at rows in those folders, and move matching works across all folders in the set, so a file moved from one watched folder to another keeps its row. Rows already missing anywhere in the scope are candidates too, as in Match moves, so a move whose two halves land in two targeted rescans keeps its row. A file moved out of the scope becomes a missing row.

The walker's rules hold for the path down to each folder of the set, as in a whole walk: a folder reached through a dot name, a system folder or a link is not walked, and a folder inside a nested album root is not walked. The scope's own folder is checked before and after the walk; if it cannot be read, the scan fails as a whole walk does and nothing is marked missing. A folder of the set that no longer exists (while the scope does) is walked as empty, so its rows are gone. A targeted walk does not replace the last whole walk that the scan report and the `unsupported` and `skipped` counts come from. A whole-scope request merged with a targeted one scans the whole scope; two targeted ones scan both sets (the whole scope above 500 folders). The merged scan's reason is `watch` only when every request in it came from the watcher.

`album_changed` for a watch-driven scan has reason `watch`.

Owlbum's own file actions (Save into the file, Save as…, moving to the Recycle Bin or Trash) call the scanner directly for the paths they touched when they finish. The watcher's events for the same files then find nothing changed.

### When watching cannot be trusted

| Situation | How it is detected | Behaviour |
|---|---|---|
| Network share | Windows: a UNC path or a drive of type remote. Linux: the mount's file system is `nfs`, `nfs4`, `cifs`, `smb3`, `fuse.sshfs`, `davfs` or `9p`. | No watch is set. |
| Watch limit reached (Linux inotify), or the watch fails to start | `notify` returns an error. | No watch. |
| The watcher reports an error later (including the inotify limit reached while adding the watches of the tree, or for a folder made later) | An error event, except "path not found" for a folder that vanished while the watch was being added to it while the scope is still there. | The watch is dropped. Folders already collected are still scanned. |
| The watched folder goes away (an unplugged drive, a share that dropped, the folder renamed) | An event for the folder itself, or the check every 5 seconds that it is still a folder (not every system tells a watch that its own folder went away). | The watch is dropped. No scan is asked for, so nothing is marked missing. |

In all these cases Owlbum falls back to **rescan on window focus**: when the window gains focus and the last scan ended more than 5 minutes ago, a whole-scope scan starts. An information toast says once per session: "Owlbum can't watch this folder for changes. It checks when you come back to the window; use Rescan to check now." The page says it when `album_status` first answers `scan.watching: false`; entering any of these states sends `album_changed` with reason `status`, so the page reads it at once. While the watch is still being set up, `watching` is true.

A watch dropped because its folder went away (the last row) is started again on window focus once the folder is back, followed by a whole-scope scan, since changes made meanwhile were not seen. A network share is never watched, and a watch that would not start or reported an error while its folder was there is not tried again while the album is open: the inotify limit does not go away by itself, and trying would walk the whole tree on every focus.

### Manual Rescan

**File > Rescan** calls `album_rescan`. It has no key in version 1; the menu and the key table are owned by [Desktop, release and tests](08-desktop-and-release.md#menus). It scans the whole scope, retries every `scan_error` row, and works the same whether or not a watch is set. If a scan is already running, the rescan is queued to start when it ends (see Cancellation).

## Commands, events and URLs

### Commands

All use the `album_` and `app_` prefixes from [Architecture](01-architecture.md).

| Command | Reply | Notes |
|---|---|---|
| `album_open(path, options?)` | `{ status: AlbumStatus, session }` | Defined in [Browsing](04-browsing.md#commands-and-events). This area adds: it returns as soon as the database is open and starts a scan in the background. Fails with `not_a_folder` or `folder_unreadable`. |
| `album_status()` | `AlbumStatus` or null | Defined in Browsing. This area defines its `scan` object: `{ phase, found, done, failed, watching, unsupported, skipped }`, where `unsupported` and `skipped` are counts from the last walk and `watching` is false when the folder cannot be watched. |
| `album_rescan()` | `()` | Starts a whole-scope scan that retries errors, or queues one if a scan is running. |
| `album_scan_cancel()` | `()` | Cancels the running scan. No error when none is running. |
| `album_scan_report()` | `ScanReport` | Counts and short lists from the last walk that ended (a cancelled walk, or one whose folder could not be read, leaves the previous report; before any, everything is empty): unreadable folders (`{ rel_path, message }`) and nested albums (`{ rel_path, message }`), each the first 100 with a count of all; skipped links (count), online-only files (count), unreadable names (count), unsupported files by label (`{ label, count }`, in the order of the table of recognised-but-unsupported formats), and notices such as the missing-GStreamer one (none until video lands). |
| `album_scan_errors(offset, limit)` | `{ total, items: [{ id, rel_path, code, message }] }` | The `scan_error` rows in scope that are not missing, in walker order; in a view-only album, the rows held in memory replace the database's. `limit` at most 500: a larger one is taken as 500. |
| `album_unsupported(offset, limit)` | `{ total, items: [{ rel_path, label, reason }] }` | Files recognised but not supported yet, from the last walk that ended, in walker order. Held in memory, not in the database. `limit` as above. |
| `album_copy_not_shown()` | number of lines | Puts the whole Files not shown list on the clipboard as plain text: one line per file, its path relative to the album root, a tab and its reason; the unsupported files first, then the files that could not be read, each in walker order (see [Browsing](04-browsing.md#notices)). Fails with `no_album` or `clipboard_failed` ("Couldn't copy the list: {reason}"). |
| `app_formats()` | `[{ id, extensions, kind, label, edit_in_place, alpha, save_as, animated }]` | The registry, so the page never hard-codes a format rule. |

### Events

| Event | This area's use |
|---|---|
| `scan_progress` | `{ phase, found, done, failed, finished, cancelled }`, as in [Architecture](01-architecture.md#events); the fields are explained under [Progress and change events](#progress-and-change-events). |
| `album_changed` | Reasons `scan` and `watch`. The payload carries `rekeyed: [id…]`, every photo whose `file_key` changed since the last event. |

### Error codes

Both are shared codes listed in [Architecture](01-architecture.md#commands); this part fixes their messages.

| Code | Message |
|---|---|
| `not_a_folder` | "Not a readable folder: {path}" |
| `folder_unreadable` | "Can't read the folder {path}: {system message}" |

### `owlbum://` URLs served

| URL | Returns | Status codes |
|---|---|---|
| `owlbum://thumb/<thumb_key>/s` and `/m` | JPEG thumbnail, made on demand if absent. `Cache-Control: private, max-age=31536000, immutable`. | 200, 404 |
| `owlbum://full/<photo_id>?v=<thumb_key>` | The zoom copy. `Cache-Control: private, max-age=3600`. | 200, 404 |
| `owlbum://original/<photo_id>?v=<thumb_key>` | The original file's bytes, **only** for a still whose `duration_ms` is set (an animated GIF or WebP). `Cache-Control: private, max-age=3600`. | 200, 404 |
| `owlbum://video/<photo_id>` | The video file in ranges of at most 4 MiB, `Accept-Ranges: bytes`, `Content-Range` on every part. `Cache-Control: no-store`. | 206, 404, 416 |

Every URL resolves only to a photo of the open album that is in scope, whose row is not missing and has no scan error. A photo id of the wrong kind for the URL answers 404. Every reply carries `Access-Control-Allow-Origin: *` (the page draws edits from a thumbnail on a canvas, which needs the picture readable from the page's origin) and `X-Content-Type-Options: nosniff`. The handler accepts `owlbum://thumb/…`, `owlbum://localhost/thumb/…` and `http://owlbum.localhost/thumb/…` alike. On Windows the webview addresses the scheme as `http://owlbum.localhost/…`; `ui/src/api/` builds the URL for the platform and nothing else does.

## Constants

| Name | Value |
|---|---|
| Thumbnail sizes | small 512 px, medium 1600 px, longest side |
| Thumbnail JPEG quality (starting values) | small 78, medium 84 |
| Zoom copy JPEG quality | 90 |
| Medium thumbnails made by the scan | while the cache is under 70% of its limit |
| Largest picture | 300,000,000 pixels |
| `file_key` sample | first 16 KiB and last 16 KiB |
| Scan pool | `max(1, logical processors − 1)` threads; heavy jobs at most half of it |
| Scan write batch | 200 rows or 250 ms |
| `scan_progress` rate | at most 10 a second |
| `album_changed` rate during a scan | at most 1 a second |
| Still-being-written retry | file modified under 2 s ago; retry after 2 s, up to 5 times |
| Watch debounce | 2 s quiet, 10 s at most, whole-scope rescan above 500 folders |
| Rescan on focus (no watch) | when the last scan ended over 5 minutes ago |
| Watched folder check | every 5 s |
| Video range chunk | 4 MiB |
| Linux blob playback limit (fallback 2) | 256 MB |
| Poster time | `min(1000 ms, duration / 10)` |
| Analysis | 256 px image; dHash 9 × 8; shadows ≤ 8; highlights ≥ 247; saliency σ 0.3; transparent above 0.5% of pixels with alpha < 250 |
| `SCAN_VERSION` | 2 |

## Tests

Rust tests in `owlbum-core`, using albums made by `owlbum-fixtures` in a temporary folder with `OWLBUM_DATA_DIR` set. None needs a window. "Counting handler" means a test handler that records every file it is asked to read.

| Area | Must be covered |
|---|---|
| Registry | Every extension maps to one format; every format has a handler and every handler is used; no extension is in both tables. The guard test for extension lists outside the registry. `extension_of` and `is_supported` on `photo.JPG`, `.jpg`, `._IMG_1.jpg`, `a.tar.jpg` and a name with no dot. |
| Handlers | One fixture per format decodes upright at the expected size. All eight EXIF orientations. Alpha is reported and flattens to white. Animated GIF, WebP and PNG report their frames; frame 1 is the picture. A PNG named `.jpg` reads as PNG; a HEIC-signature file named `.jpg` gives `unsupported`. Random bytes give `decode`; a header over 300,000,000 pixels gives `too_large` without allocating. Video probe gives duration and displayed size, swapped for a rotated video. Poster tests are skipped, with a printed reason, where the system cannot decode the fixture. |
| Colour profiles | Expected colours are worked out from each test profile's own primaries and curves (`owlbum_fixtures::icc`), not taken from the library. Display P3 (two curves), Adobe RGB and a grey gamma profile convert a lattice within 2 levels, and move some colour by at least 10. JPEG, PNG, WebP, TIFF and JPEG XL files with an embedded profile give converted `working` pixels and an unconverted `source` with the same profile bytes; a 16-bit PNG keeps 16 bits in `source`. Alpha is unchanged. An sRGB profile changes nothing. Every truncation and every single-byte change of a profile neither panics nor changes alpha; an unreadable profile, a profile for another colour space and a grey profile on a colour picture leave the pixels as they are. |
| Walker | Dotfiles, dot-folders, `._` files, `.owlbum` and the named system folders are skipped. `natural_cmp` order, independent of locale. Links to files and folders (and a junction on Windows) are skipped and counted. An unreadable subfolder is reported and the rest listed; an unreadable root fails. `.heic` and `.avif` are counted and get no row. A nested album root is skipped and reported. Online-only is decided from the attributes alone (a table of attribute sets); on Windows a real file with `OFFLINE` set (`attrib +O`) is counted, never read, and its row is kept and not marked missing. `RECALL_ON_DATA_ACCESS` can only be set by a cloud-files driver, so no test sets it on a real file. |
| Scanner: change detection | A second scan reads no file (counting handler). Only a file whose size or modified time changed is read again, keeping its id and typed info. A raised `SCAN_VERSION` re-reads every row and encodes no thumbnail that exists. |
| Scanner: identity | Rename, move, and case-only rename keep the row and its thumbnails. Two new files with one candidate's key: the first in walker order takes it. A delete seen by one scan and the matching create seen by the next still keep the row. A deleted file becomes missing; putting it back clears the flag. Nothing is marked missing after a cancelled scan, under an unreadable subfolder, or with an unreadable root. |
| Scanner: errors | A corrupt file gets a `decode` row, is left out of results, is listed, and is retried on Rescan but not on an automatic scan. An `io` error is retried every scan. A file still being written is retried, not recorded, until its retries run out. |
| Scanner: running | Writes arrive in transactions of at most 200 rows; a typed change during a scan commits within one batch. A cancelled scan leaves waiting rows that the next scan finishes without repeating the facts pass. A scoped scan touches no row outside the scope. A view-only album scans and writes nothing in the album folder. `scan_progress` never exceeds 10 a second and ends with `finished: true`. |
| Metadata | Table tests: date strings (both separators, the `T` form, year `0000`, month 13, the order of the three tags); the camera rule's five cases; the lens rule; numeric fields (zero, negative, zero denominator, several ISO values, flash bit); GPS (signs, out of range, 0, 0); a video date with its own offset. |
| Analysis | Hand-made images with known answers: flat grey (sharpness 0, saliency 0.5), black and white (clipping 1.0), a ramp (a known dHash), an off-centre bright spot. 1% transparent pixels is `transparent`, 0.1% is not. The same file gives byte-identical JSON twice, and the same JSON with the cache above and below 70% of its limit (the 1600 px buffer is always built; the cache test comes with the thumbnail cache, #11). |
| Thumbnails and URLs | Sizes 512 and 1600; no enlarging; opaque JPEG with no metadata. Concurrent requests for a missing thumbnail decode once. A file cut short is remade. The zoom copy is made once; an upright sRGB JPEG is served as its own bytes. `owlbum://video` range replies (206, `Content-Range`, the 4 MiB cap, 416). `owlbum://original` refuses a still that is not animated. Every URL answers 404 for an unknown id, a missing row, and a key from another album. |
| Watcher | Pure parts with a fake clock: event to folder set, debounce, the 500-folder limit, paths under `.owlbum` ignored. With a real temporary folder: add, change, delete and rename each change the right row; a scan the watch asked for starts no other; a watch that will not start falls back to rescan on focus; a watched folder that goes away loses no rows and is watched again when back. Targeted walks keep the walker's rules on the way down to each folder. |

## Open questions

Only things a prototype or a measurement must settle. Each has an issue.

1. **`rawler` and DNG.** Partly settled in #49: `rawler` 0.8 reads an uncompressed Bayer DNG with its embedded preview and develops it. Checked in #157 with public samples from raw.pixls.us (release build): Adobe DNG Converter's lossy DNG (3960 × 2640) and a Google Pixel 4a DNG (4016 × 3008) have only small embedded pictures (1024 and 672 px), so their previews are developed (0.77 and 0.84 s); an iPhone 12 Pro ProRAW (linear) DNG carries a full-size embedded picture (4032 × 3024) and is shown from it (0.6 s, the JPEG decode). All three read and develop.
2. **RAW embedded previews.** Partly settled in #49: `rawler` hands out embedded pictures without developing (`full_image`, `preview_image`, `thumbnail_image`, per decoder), and the handler takes the largest. Measured in #157 on one public sample per maker: CR2 (EOS 100D), CR3 (EOS R6) and NEF (D3200) embed a full-size picture, so preview and working both use it (0.08 to 0.26 s); ARW (A6000) 1616 px, RAF (X-T20) 1920 px and RW2 (GX7) 1920 px, so `preview(1600)` uses it (0.05 to 0.06 s) and `working` develops (0.7 to 2.1 s); `rawler` finds no embedded picture in the ORF (E-M10), which is always developed (1.3 s). `metadata` takes 0.01 to 0.25 s per file (the header-only decode still copies the raw strips). Cost stays "heavy".
3. **RAW zoom look.** When zoom has to use `rawler`'s development, is the colour difference from the camera's embedded preview acceptable, or should zoom stay on the embedded preview and say "Preview only"?
4. **Tauri issue #3725.** Which of the three Linux playback steps works on current WebKitGTK, and does `<video>` over `owlbum://` with ranges behave on WebView2 for multi-gigabyte files?
5. **Linux poster frames.** Is the GStreamer pipeline reliable across distributions with only the base plug-ins, and how is its absence detected cleanly at run time?
6. **Windows poster frames.** In #50 the shell thumbnail is the only poster source; whether it gives 1600 px for large videos and whether it blocks on cloud files were not measured. The Media Foundation fallback and a check on real files are #155.
7. **Thumbnail encoding.** JPEG qualities for 512 px and 1600 px. The encoder is `jpeg-encoder` (#11); `mozjpeg` could still be measured against it if file size matters. (`zune-jpeg` 0.5.15 has no reduced-size decode, checked in #6; a JPEG's preview uses `jpeg-decoder`'s scaled decode instead, #114.)
8. **Colour conversion.** Settled in #7: `moxcms` 0.8.1, already a dependency of `image`. Measured against `lcms2` 6.3 (Little CMS 2.20) and `qcms` 0.3 on hand-built Display P3, Adobe RGB, sRGB and grey profiles, against values worked out from each profile's own numbers: all three stay within 1.5 levels (`moxcms` at most 1.2, mean 0.2). On five real monitor profiles `moxcms` and `lcms2` differ by at most 2 levels. All three refuse truncated, random and out-of-range profiles without panicking; given a profile whose colorants are all zero, `lcms2` and `qcms` turn every colour black while `moxcms` refuses it. The Windows "sRGB Color Space Profile" is recognised as sRGB and skipped. Converting a 6000 × 4000 picture on one thread: `moxcms` 57 to 77 ms, `qcms` 67 to 71 ms, `lcms2` 520 to 550 ms; building a transform takes 0.6 ms (`lcms2` 4 to 5 ms). The edit renderer in [Editing and file actions](06-editing-and-file-actions.md#rendering) uses `moxcms` too, with its 16-bit transform.
9. **Encoders and decoders assumed here.** Settled in #6: `jxl-oxide` 0.12.6 has no encoder and `image` 0.25.10 writes only lossless WebP, so the capability table stands.
10. **Analysis numbers.** The score thresholds in Organising were tuned on the old program's resampler. They must be checked against the pipeline pinned here on the `trip` fixture before they are trusted.
11. **`nom-exif` coverage.** Settled in #10: Owlbum does not use `nom-exif` for photos. Its date parsing drops the date rule's other forms, so `media::exif` reads EXIF blocks and TIFF files itself, and each handler finds the block in its container. RAW (#49) and video (#50) handlers give their tags to the same reader or rules.
