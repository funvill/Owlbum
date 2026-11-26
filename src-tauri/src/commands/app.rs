//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use crate::{area, command};
use owlbum_core::app::{self, AppVersion};
use owlbum_core::media::{self, FormatInfo};

area! {
    prefix: "app",
    commands: [app_formats, app_version],
    events: [],
}

command! {
    /// The media format registry, so the page never hard-codes a format rule
    /// (docs/03-media-and-scanning.md, "Commands").
    pub fn app_formats() -> Vec<FormatInfo> {
        Ok(media::formats())
    }
}

command! {
    /// The program's version.
    pub fn app_version() -> AppVersion {
        Ok(app::version())
    }
}
