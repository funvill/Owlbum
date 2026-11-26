//! Sentences of the media area (docs/03-media-and-scanning.md).

/// `io`: the file could not be opened or read.
pub fn io_error(err: &std::io::Error) -> String {
    format!("This file could not be read: {err}")
}

/// `decode`: the bytes are not a picture of the format's label.
pub fn decode_error(label: &str) -> String {
    format!("This file is damaged or is not a {label} picture.")
}

/// `too_large`: over the pixel limit.
pub fn too_large(width: u32, height: u32) -> String {
    format!("This picture is too large to open ({width} × {height}).")
}

/// Reason shown for a HEIC file.
pub const HEIC_UNSUPPORTED: &str = "HEIC photos are not supported yet.";
/// Reason shown for an AVIF file.
pub const AVIF_UNSUPPORTED: &str = "AVIF photos are not supported yet.";
/// Reason shown for a RAW format with no decoder.
pub const RAW_UNSUPPORTED: &str = "This camera's RAW format is not supported yet.";
/// Reason shown for a video format with no reader.
pub const VIDEO_UNSUPPORTED: &str = "This video format is not supported yet.";
