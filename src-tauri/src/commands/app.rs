//! Start-up, settings, recent albums, session, updates, cache (`app_`).

use crate::{area, command};
use owlbum_core::app::{self, AppVersion};

area! {
    prefix: "app",
    commands: [app_version],
    events: [],
}

command! {
    /// The program's version.
    pub fn app_version() -> AppVersion {
        Ok(app::version())
    }
}
