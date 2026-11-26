//! Read or change one or more photos (`photo_`).

use crate::commands::off_main;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::info::detail::{self, PhotoDetail};
use owlbum_core::info::histogram::{self, Histogram};
use owlbum_core::thumbs::{Thumbs, lookup};
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "photo",
    commands: [photo_detail, photo_histogram],
    events: [],
}

command! {
    /// One photo's details for the details panel (docs/04-browsing.md,
    /// "The details panel"). `photo_not_found` when the id is not a photo
    /// the album shows; `no_album` when no album is open.
    pub async fn photo_detail(id: i64; session: State<'_, Session>) -> PhotoDetail {
        let (scanner, _) = session.queries()?;
        off_main(text::photo::READING_DETAIL, move || {
            let open = scanner.open();
            detail::photo_detail(open.album, open.overlay, id)
        })
        .await
    }
}

command! {
    /// The histogram of one photo's medium thumbnail: 64 counts each of
    /// luminance, red, green and blue (docs/04-browsing.md, "Histogram").
    /// Counted when asked, never stored. `photo_not_found` for an id that
    /// is not a photo the album shows; `internal` when no thumbnail can be
    /// made.
    pub async fn photo_histogram<R: Runtime>(id: i64; app: AppHandle<R>) -> Histogram {
        let session = app.state::<Session>();
        let scanner = session.queries()?.0;
        let thumbs = app
            .try_state::<Thumbs>()
            .map(|t| t.inner().clone())
            .ok_or_else(|| AppError::internal(text::photo::HISTOGRAM))?;
        off_main(text::photo::HISTOGRAM, move || {
            let open = scanner.open();
            let photo = lookup::by_id(open.album, open.overlay, id)?
                .ok_or_else(|| AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND))?;
            histogram::of_photo(&thumbs, &photo)
        })
        .await
    }
}
