//! Media: the format registry and the handlers that read each format
//! (docs/03-media-and-scanning.md, "The media format registry" and "Media
//! handlers").
//!
//! `registry` is the only module that knows which files Owlbum handles.
//! Every row names a handler; the still handler (`still`) reads JPEG, PNG,
//! WebP, GIF, TIFF and BMP, and `jxl` reads JPEG XL. RAW (#49) and video
//! (#50) rows use the placeholder handlers in `pending` until their
//! decoders are built. `colour` converts embedded colour profiles to sRGB
//! (#7).

pub mod colour;
pub mod exif;
pub mod handler;
mod headers;
pub mod jxl;
pub mod pending;
pub mod registry;
mod sniff;
pub mod still;

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
