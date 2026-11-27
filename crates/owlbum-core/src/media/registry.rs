//! The media format registry: the only place that knows which files Owlbum
//! handles (docs/03-media-and-scanning.md, "The media format registry").
//!
//! No other module holds an extension list, a MIME type or a "is this a
//! RAW" test. A test below searches the workspace sources and fails if a
//! media extension list or extension regex appears anywhere else.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::handler::MediaHandler;
use super::{jxl, raw, sniff, still, video};
use crate::text;

/// What kind of photo a format holds (`photos.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Raw,
    Video,
}

/// The format an edited copy is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SaveFormat {
    Jpeg,
    Png,
    Webp,
}

/// What Owlbum may do with a format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Save (with `edit_target = file`) may replace the original.
    pub edit_in_place: bool,
    /// The format can carry transparency.
    pub alpha: bool,
    /// The format an edited copy is written in.
    pub save_as: SaveFormat,
    /// The format can hold more than one frame.
    pub animated: bool,
}

/// One row of the registry.
pub struct MediaFormat {
    /// Stored in `photos.format`.
    pub id: &'static str,
    /// Lower case, with the dot; the first is the default.
    pub extensions: &'static [&'static str],
    /// Content-Type for `owlbum://` and the clipboard.
    pub mime: &'static str,
    pub kind: MediaKind,
    /// Shown to the user.
    pub label: &'static str,
    pub caps: Capabilities,
    pub handler: &'static dyn MediaHandler,
}

impl std::fmt::Debug for MediaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaFormat")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// A picture or video format Owlbum recognises but cannot open yet.
#[derive(Debug)]
pub struct UnsupportedFormat {
    /// Shown in the notice: "N files not supported yet (HEIC, AVIF)".
    pub label: &'static str,
    /// Lower case, with the dot.
    pub extensions: &'static [&'static str],
    /// Shown beside each file in the Files not shown dialog.
    pub reason: &'static str,
    /// Recognises the format's bytes, for one named as a supported format.
    /// None for families that cannot be told from a supported one.
    pub sniff: Option<fn(&[u8]) -> bool>,
}

const fn caps(
    edit_in_place: bool,
    alpha: bool,
    save_as: SaveFormat,
    animated: bool,
) -> Capabilities {
    Capabilities {
        edit_in_place,
        alpha,
        save_as,
        animated,
    }
}

const RAW_CAPS: Capabilities = caps(false, false, SaveFormat::Jpeg, false);
const VIDEO_CAPS: Capabilities = caps(false, false, SaveFormat::Jpeg, false);

const fn raw(
    id: &'static str,
    extensions: &'static [&'static str],
    mime: &'static str,
    label: &'static str,
) -> MediaFormat {
    MediaFormat {
        id,
        extensions,
        mime,
        kind: MediaKind::Raw,
        label,
        caps: RAW_CAPS,
        handler: &raw::RAW,
    }
}

const fn video(
    id: &'static str,
    extensions: &'static [&'static str],
    mime: &'static str,
    label: &'static str,
) -> MediaFormat {
    MediaFormat {
        id,
        extensions,
        mime,
        kind: MediaKind::Video,
        label,
        caps: VIDEO_CAPS,
        handler: &video::VIDEO,
    }
}

/// The formats Owlbum handles in version 1, in the order of the spec's
/// table. When bytes match several rows' handlers, the first row wins.
pub static FORMATS: &[MediaFormat] = &[
    MediaFormat {
        id: "jpeg",
        extensions: &[".jpg", ".jpeg", ".jpe", ".jfif"],
        mime: "image/jpeg",
        kind: MediaKind::Image,
        label: "JPEG",
        caps: caps(true, false, SaveFormat::Jpeg, false),
        handler: &still::JPEG,
    },
    MediaFormat {
        id: "png",
        extensions: &[".png"],
        mime: "image/png",
        kind: MediaKind::Image,
        label: "PNG",
        caps: caps(true, true, SaveFormat::Png, false),
        handler: &still::PNG,
    },
    MediaFormat {
        id: "webp",
        extensions: &[".webp"],
        mime: "image/webp",
        kind: MediaKind::Image,
        label: "WebP",
        caps: caps(false, true, SaveFormat::Webp, true),
        handler: &still::WEBP,
    },
    MediaFormat {
        id: "gif",
        extensions: &[".gif"],
        mime: "image/gif",
        kind: MediaKind::Image,
        label: "GIF",
        caps: caps(false, true, SaveFormat::Png, true),
        handler: &still::GIF,
    },
    MediaFormat {
        id: "tiff",
        extensions: &[".tif", ".tiff"],
        mime: "image/tiff",
        kind: MediaKind::Image,
        label: "TIFF",
        caps: caps(false, true, SaveFormat::Png, false),
        handler: &still::TIFF,
    },
    MediaFormat {
        id: "bmp",
        extensions: &[".bmp"],
        mime: "image/bmp",
        kind: MediaKind::Image,
        label: "BMP",
        caps: caps(false, false, SaveFormat::Png, false),
        handler: &still::BMP,
    },
    MediaFormat {
        id: "jxl",
        extensions: &[".jxl"],
        mime: "image/jxl",
        kind: MediaKind::Image,
        label: "JPEG XL",
        caps: caps(false, true, SaveFormat::Jpeg, false),
        handler: &jxl::JXL,
    },
    raw("cr2", &[".cr2"], "image/x-canon-cr2", "Canon RAW"),
    raw("cr3", &[".cr3"], "image/x-canon-cr3", "Canon RAW"),
    raw("nef", &[".nef"], "image/x-nikon-nef", "Nikon RAW"),
    raw("arw", &[".arw"], "image/x-sony-arw", "Sony RAW"),
    raw("raf", &[".raf"], "image/x-fuji-raf", "Fujifilm RAW"),
    raw("orf", &[".orf"], "image/x-olympus-orf", "Olympus RAW"),
    raw("rw2", &[".rw2"], "image/x-panasonic-rw2", "Panasonic RAW"),
    raw("dng", &[".dng"], "image/x-adobe-dng", "DNG"),
    video("mp4", &[".mp4", ".m4v"], "video/mp4", "MP4 video"),
    video("mov", &[".mov"], "video/quicktime", "QuickTime video"),
    video("webm", &[".webm"], "video/webm", "WebM video"),
    video("mkv", &[".mkv"], "video/x-matroska", "Matroska video"),
    video("avi", &[".avi"], "video/x-msvideo", "AVI video"),
];

/// Files Owlbum knows are pictures or videos but cannot open in version 1.
/// They get no row; the walker counts and lists them.
pub static UNSUPPORTED: &[UnsupportedFormat] = &[
    UnsupportedFormat {
        label: "HEIC",
        extensions: &[".heic", ".heif", ".hif"],
        reason: text::media::HEIC_UNSUPPORTED,
        sniff: Some(sniff::heic),
    },
    UnsupportedFormat {
        label: "AVIF",
        extensions: &[".avif"],
        reason: text::media::AVIF_UNSUPPORTED,
        sniff: Some(sniff::avif),
    },
    UnsupportedFormat {
        label: "Camera RAW",
        extensions: &[
            ".crw", ".nrw", ".srf", ".sr2", ".pef", ".srw", ".rwl", ".3fr", ".iiq", ".erf", ".mef",
            ".mos", ".kdc", ".dcr", ".x3f",
        ],
        reason: text::media::RAW_UNSUPPORTED,
        sniff: None,
    },
    UnsupportedFormat {
        label: "Video",
        extensions: &[".mts", ".m2ts", ".3gp", ".wmv", ".mpg", ".mpeg"],
        reason: text::media::VIDEO_UNSUPPORTED,
        sniff: None,
    },
];

/// The text from the last dot of the file name, lower-cased, with the dot.
/// Empty when there is no dot or the only dot is the first character.
pub fn extension_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

/// The format whose extensions contain `extension_of(name)`.
pub fn format_of(name: &str) -> Option<&'static MediaFormat> {
    let ext = extension_of(name);
    FORMATS
        .iter()
        .find(|f| f.extensions.contains(&ext.as_str()))
}

/// The format with that id.
pub fn format_by_id(id: &str) -> Option<&'static MediaFormat> {
    FORMATS.iter().find(|f| f.id == id)
}

/// The row of the unsupported table for the name's extension.
pub fn unsupported_of(name: &str) -> Option<&'static UnsupportedFormat> {
    let ext = extension_of(name);
    UNSUPPORTED
        .iter()
        .find(|u| u.extensions.contains(&ext.as_str()))
}

/// True when Owlbum handles the file: a known extension, and not a macOS
/// AppleDouble (`._`) file.
pub fn is_supported(name: &str) -> bool {
    format_of(name).is_some() && !name.starts_with("._")
}

/// The label of the first row that uses `handler`, for its `decode` error.
pub fn label_of(handler: &dyn MediaHandler) -> &'static str {
    let addr = std::ptr::from_ref(handler).cast::<()>();
    FORMATS
        .iter()
        .find(|f| std::ptr::from_ref(f.handler).cast::<()>() == addr)
        .map_or("", |f| f.label)
}

/// What a file is, by its name and its first bytes.
#[derive(Debug, Clone, Copy)]
pub enum Identified {
    /// Read the file as this format; `photos.format` holds its id.
    Format(&'static MediaFormat),
    /// The bytes are a format Owlbum does not support yet: a `scan_error`
    /// with code `unsupported` and this row's reason.
    Unsupported(&'static UnsupportedFormat),
}

/// Decides how to read a supported file (docs/03-media-and-scanning.md,
/// "When the extension lies"). `head` is the start of the file; the scanner
/// already holds its first 16 KiB. None when the name is not a supported
/// one.
///
/// 1. The extension's own handler recognises the bytes: that format.
/// 2. Another format's handler recognises them: the first such format.
/// 3. The bytes are a recognised-but-unsupported format: that row.
/// 4. Nothing recognises them: the extension's format, whose handler tries
///    anyway and fails with `decode`.
pub fn identify(name: &str, head: &[u8]) -> Option<Identified> {
    let own = format_of(name)?;
    if own.handler.sniff(head) {
        return Some(Identified::Format(own));
    }
    if let Some(other) = FORMATS.iter().find(|f| f.handler.sniff(head)) {
        return Some(Identified::Format(other));
    }
    if let Some(u) = UNSUPPORTED
        .iter()
        .find(|u| u.sniff.is_some_and(|sniff| sniff(head)))
    {
        return Some(Identified::Unsupported(u));
    }
    Some(Identified::Format(own))
}

/// One format as the page sees it, for `app_formats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FormatInfo {
    pub id: String,
    /// Lower case, with the dot; the first is the default.
    pub extensions: Vec<String>,
    pub kind: MediaKind,
    pub label: String,
    /// Save (with `edit_target = file`) may replace the original.
    pub edit_in_place: bool,
    /// The format can carry transparency.
    pub alpha: bool,
    /// The format an edited copy is written in.
    pub save_as: SaveFormat,
    /// The format can hold more than one frame.
    pub animated: bool,
}

/// The registry, so the page never hard-codes a format rule.
pub fn formats() -> Vec<FormatInfo> {
    FORMATS
        .iter()
        .map(|f| FormatInfo {
            id: f.id.to_owned(),
            extensions: f.extensions.iter().map(|e| (*e).to_owned()).collect(),
            kind: f.kind,
            label: f.label.to_owned(),
            edit_in_place: f.caps.edit_in_place,
            alpha: f.caps.alpha,
            save_as: f.caps.save_as,
            animated: f.caps.animated,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    #[test]
    fn every_extension_maps_to_one_format() {
        let mut seen = HashSet::new();
        for f in FORMATS {
            assert!(!f.extensions.is_empty(), "{} has no extension", f.id);
            for e in f.extensions {
                assert!(seen.insert(*e), "{e} is listed twice");
                assert_eq!(format_of(&format!("x{e}")).unwrap().id, f.id);
            }
        }
    }

    #[test]
    fn extensions_are_lower_case_with_the_dot() {
        let all = FORMATS
            .iter()
            .flat_map(|f| f.extensions)
            .chain(UNSUPPORTED.iter().flat_map(|u| u.extensions));
        for e in all {
            assert!(e.starts_with('.') && e.len() > 1, "{e}");
            assert_eq!(*e, e.to_lowercase(), "{e}");
        }
    }

    #[test]
    fn no_extension_is_in_both_tables() {
        let supported: HashSet<_> = FORMATS.iter().flat_map(|f| f.extensions).collect();
        for u in UNSUPPORTED {
            for e in u.extensions {
                assert!(!supported.contains(e), "{e} is in both tables");
            }
        }
    }

    #[test]
    fn ids_are_unique_and_found_by_id() {
        let mut ids = HashSet::new();
        for f in FORMATS {
            assert!(ids.insert(f.id), "{} is listed twice", f.id);
            assert_eq!(format_by_id(f.id).unwrap().id, f.id);
        }
        assert!(format_by_id("heic").is_none());
    }

    fn handler_addr(h: &'static dyn MediaHandler) -> *const () {
        std::ptr::from_ref(h).cast::<()>()
    }

    #[test]
    fn every_handler_is_used_by_a_row() {
        let used: HashSet<_> = FORMATS.iter().map(|f| handler_addr(f.handler)).collect();
        let all: [&'static dyn MediaHandler; 9] = [
            &still::JPEG,
            &still::PNG,
            &still::WEBP,
            &still::GIF,
            &still::TIFF,
            &still::BMP,
            &jxl::JXL,
            &raw::RAW,
            &video::VIDEO,
        ];
        for h in all {
            assert!(used.contains(&handler_addr(h)), "a handler is not used");
        }
    }

    #[test]
    fn the_version_one_table() {
        let rows: Vec<_> = FORMATS
            .iter()
            .map(|f| {
                (
                    f.id,
                    f.kind,
                    f.caps.edit_in_place,
                    f.caps.alpha,
                    f.caps.save_as,
                    f.caps.animated,
                )
            })
            .collect();
        use MediaKind::*;
        use SaveFormat as S;
        assert_eq!(
            &rows[..7],
            &[
                ("jpeg", Image, true, false, S::Jpeg, false),
                ("png", Image, true, true, S::Png, false),
                ("webp", Image, false, true, S::Webp, true),
                ("gif", Image, false, true, S::Png, true),
                ("tiff", Image, false, true, S::Png, false),
                ("bmp", Image, false, false, S::Png, false),
                ("jxl", Image, false, true, S::Jpeg, false),
            ]
        );
        let raw: Vec<_> = rows.iter().filter(|r| r.1 == Raw).map(|r| r.0).collect();
        assert_eq!(
            raw,
            ["cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "dng"]
        );
        let video: Vec<_> = rows.iter().filter(|r| r.1 == Video).map(|r| r.0).collect();
        assert_eq!(video, ["mp4", "mov", "webm", "mkv", "avi"]);
        for r in &rows[7..] {
            assert_eq!(
                (r.2, r.3, r.4, r.5),
                (false, false, S::Jpeg, false),
                "{}",
                r.0
            );
        }
    }

    #[test]
    fn extension_of_follows_the_rule() {
        assert_eq!(extension_of("photo.JPG"), ".jpg");
        assert_eq!(extension_of(".jpg"), "");
        assert_eq!(extension_of("._IMG_1.jpg"), ".jpg");
        assert_eq!(extension_of("a.tar.jpg"), ".jpg");
        assert_eq!(extension_of("README"), "");
    }

    #[test]
    fn is_supported_follows_the_rule() {
        assert!(is_supported("photo.JPG"));
        assert!(!is_supported(".jpg"));
        assert!(!is_supported("._IMG_1.jpg"));
        assert!(is_supported("a.tar.jpg"));
        assert!(!is_supported("README"));
        assert!(!is_supported("clip.heic"));
        assert!(is_supported("IMG_1.CR3"));
    }

    #[test]
    fn unsupported_of_finds_the_row() {
        assert_eq!(unsupported_of("IMG_1.HEIC").unwrap().label, "HEIC");
        assert_eq!(unsupported_of("a.avif").unwrap().label, "AVIF");
        assert_eq!(unsupported_of("a.pef").unwrap().label, "Camera RAW");
        assert_eq!(unsupported_of("a.wmv").unwrap().label, "Video");
        assert!(unsupported_of("a.jpg").is_none());
        assert_eq!(
            unsupported_of("a.heic").unwrap().reason,
            "HEIC photos are not supported yet."
        );
    }

    #[test]
    fn formats_for_the_page_mirror_the_table() {
        let page = formats();
        assert_eq!(page.len(), FORMATS.len());
        let json = serde_json::to_value(&page[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "jpeg",
                "extensions": [".jpg", ".jpeg", ".jpe", ".jfif"],
                "kind": "image",
                "label": "JPEG",
                "edit_in_place": true,
                "alpha": false,
                "save_as": "jpeg",
                "animated": false,
            })
        );
    }

    // ---- The guard: no extension list outside this module ----

    /// Files allowed to name several media extensions.
    const GUARD_ALLOW: &[&str] = &[
        // This module.
        "crates/owlbum-core/src/media/registry.rs",
        // The handler tests name formats by id and make one file of each.
        "crates/owlbum-core/src/media/tests.rs",
        // The fake backend's `app_formats` answer stands in for this table in
        // page tests.
        "ui/src/api/fake/app.ts",
        // Generated from this module's types (`SaveFormat`), never edited.
        "ui/src/api/bindings/",
        // The fixture generator writes one test file per format and names
        // them by format; it is test data, not a rule Owlbum applies.
        "crates/owlbum-fixtures/",
        // Details panel tests: a photo's format id is test data there, and
        // the page writes it in capitals ("JPEG", "MP4").
        "crates/owlbum-core/src/info/detail/tests.rs",
        "ui/src/features/details/DetailsPanel.test.tsx",
        "ui/src/features/details/facts.test.ts",
        // Fake rows of files not shown (`2024/0.heic`), and the notice's
        // labels, which are what the backend sends.
        "ui/src/features/album/Notices.test.tsx",
        // Save tests name the extension Save as forces as test data.
        "crates/owlbum-core/src/edit/save/tests.rs",
    ];

    fn known_extensions() -> HashSet<String> {
        FORMATS
            .iter()
            .flat_map(|f| f.extensions)
            .chain(UNSUPPORTED.iter().flat_map(|u| u.extensions))
            .map(|e| e.trim_start_matches('.').to_owned())
            .collect()
    }

    /// String literals in a source file whose whole text is a known
    /// extension, with or without the dot, in any case. Rust byte strings
    /// (`b"heic"`, a file signature) are not extension literals.
    fn extension_literals(src: &str, known: &HashSet<String>) -> HashSet<String> {
        let mut found = HashSet::new();
        for quote in ['"', '\'', '`'] {
            let mut rest = src;
            while let Some(open) = rest.find(quote) {
                let byte_string = rest[..open].ends_with('b');
                let after = &rest[open + 1..];
                let Some(close) = after.find(quote) else {
                    break;
                };
                let piece = &after[..close];
                let word = piece.trim_start_matches('.').to_lowercase();
                if !byte_string && piece.len() <= 6 && known.contains(&word) {
                    found.insert(word);
                }
                rest = &after[close + 1..];
            }
        }
        found
    }

    /// An alternation of two extensions, as in a regex `jpe?g|png`.
    fn has_extension_alternation(src: &str, known: &HashSet<String>) -> bool {
        // `jpe?g` reads as `jpeg`.
        let is_ext = |w: &str| known.contains(&w.replace('?', "").to_lowercase());
        let bytes: Vec<char> = src.chars().collect();
        for (i, c) in bytes.iter().enumerate() {
            if *c != '|' {
                continue;
            }
            let before: String = bytes[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '?')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let after: String = bytes[i + 1..]
                .iter()
                .skip_while(|c| **c == '.' || **c == '\\')
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '?')
                .collect();
            if !before.is_empty() && !after.is_empty() && is_ext(&before) && is_ext(&after) {
                return true;
            }
        }
        false
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !matches!(name.as_str(), "target" | "node_modules" | "dist" | ".git") {
                    walk(&path, out);
                }
            } else if [".rs", ".ts", ".tsx"].iter().any(|e| name.ends_with(e)) {
                out.push(path);
            }
        }
    }

    #[test]
    fn the_guard_catches_lists_and_regexes() {
        let known = known_extensions();
        assert_eq!(
            extension_literals(r#"const EXT = ["jpg", ".PNG"];"#, &known).len(),
            2
        );
        assert_eq!(
            extension_literals(r#"let name = "photo.jpg"; "jpg""#, &known).len(),
            1
        );
        assert!(extension_literals(r#"[b"heic", b"avif"]"#, &known).is_empty());
        assert!(has_extension_alternation(r"/\.(jpe?g|png)$/i", &known));
        assert!(has_extension_alternation(r"\.jpg|\.gif", &known));
        assert!(!has_extension_alternation("a || b", &known));
        assert!(!has_extension_alternation("x | y", &known));
    }

    #[test]
    fn no_extension_list_outside_the_registry() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let known = known_extensions();
        let mut files = Vec::new();
        for dir in ["crates", "src-tauri/src", "src-tauri/tests", "ui/src"] {
            walk(&root.join(dir), &mut files);
        }
        assert!(files.len() > 10, "the guard found too few sources to check");
        let mut offenders = Vec::new();
        for path in files {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if GUARD_ALLOW
                .iter()
                .any(|a| rel == *a || (a.ends_with('/') && rel.starts_with(a)))
            {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let literals = extension_literals(&src, &known);
            if literals.len() >= 2 {
                offenders.push(format!("{rel}: extension list {literals:?}"));
            } else if has_extension_alternation(&src, &known) {
                offenders.push(format!("{rel}: extension regex"));
            }
        }
        assert!(
            offenders.is_empty(),
            "media extensions belong in media::registry only. Ask the registry \
             (format_of, format_by_id, identify) in Rust or `app_formats` on the \
             page; or, for a file that only looks like a list (generated code, \
             test data), add it to GUARD_ALLOW with a reason:\n{}",
            offenders.join("\n")
        );
    }
}
