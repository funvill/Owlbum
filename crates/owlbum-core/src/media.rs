//! Media: the format registry and the handlers that read each format
//! (docs/03-media-and-scanning.md, "The media format registry" and "Media
//! handlers").
//!
//! `registry` is the only module that knows which files Owlbum handles.
//! Every row names a handler; the still handler (`still`) reads JPEG, PNG,
//! WebP, GIF, TIFF and BMP, `jxl` reads JPEG XL, `raw` reads camera RAW
//! with `rawler` (#49) and `video` reads video containers and asks the
//! operating system for poster frames (#50). `colour` converts embedded
//! colour profiles to sRGB (#7).

pub mod colour;
pub mod exif;
pub mod handler;
mod headers;
pub mod jxl;
pub mod raw;
pub mod registry;
mod sniff;
pub mod still;
pub mod video;

#[cfg(test)]
mod tests;

pub use exif::Exif;
pub use handler::{
    Cost, Layout, MAX_PIXELS, MediaError, MediaErrorCode, MediaHandler, Metadata, Pixels, Samples,
    SourcePixels, VideoInfo,
};
pub use registry::{
    Capabilities, FORMATS, FormatInfo, Identified, MediaFormat, MediaKind, SaveFormat, UNSUPPORTED,
    UnsupportedFormat, extension_of, format_by_id, format_of, formats, identify, is_supported,
    unsupported_of,
};
