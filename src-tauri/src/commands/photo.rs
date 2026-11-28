//! Read or change one or more photos (`photo_`).

use crate::commands::off_main;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::{AlbumChanged, ChangeReason};
use owlbum_core::info::detail::{self, PhotoDetail};
use owlbum_core::info::exif_list::{self, ExifField};
use owlbum_core::info::histogram::{self, Histogram};
use owlbum_core::info::typed::{self, InfoChanged, InfoPatch};
use owlbum_core::organise::nudge::{self, Nudged};
use owlbum_core::organise::score::{self, ScoreBreakdown};
use owlbum_core::thumbs::{Thumbs, lookup};
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "photo",
    commands: [photo_detail, photo_exif, photo_histogram, photo_set_info, photo_nudge, photo_score],
    events: [],
}

command! {
    /// One photo's details for the details panel (docs/04-browsing.md,
    /// "The details panel"). `photo_not_found` when the id is not a photo
    /// the album shows; `no_album` when no album is open.
    pub async fn photo_detail<R: Runtime>(id: i64; app: AppHandle<R>) -> PhotoDetail {
        let (scanner, _) = app.state::<Session>().queries()?;
        let tagging = crate::commands::tagging::tagging(&app);
        off_main(text::photo::READING_DETAIL, move || {
            let open = scanner.open();
            let mut d = detail::photo_detail(open.album, open.overlay, id)?;
            if let Some(t) = tagging {
                d.tags = Some(t.photo_tags(open.album, id)?);
            }
            Ok(d)
        })
        .await
    }
}

command! {
    /// Every EXIF tag of one photo's file, in file order, for the details
    /// panel's EXIF section (docs/04-browsing.md, "EXIF section"). Read
    /// from the file each time; empty when it has none. `photo_not_found`
    /// as for `photo_detail`.
    pub async fn photo_exif(id: i64; session: State<'_, Session>) -> Vec<ExifField> {
        let (scanner, _) = session.queries()?;
        off_main(text::photo::READING_DETAIL, move || {
            let open = scanner.open();
            exif_list::photo_exif(open.album, open.overlay, id)
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

command! {
    /// Changes typed info (description, notes, people, date, place) of the
    /// photos `ids` (docs/04-browsing.md, "How typed fields save"). A
    /// field left out of `patch` is left alone; a null clears it. Answers
    /// the photos whose values changed and the album's revision, and sends
    /// `album_changed` with reason `info` when any changed. Errors:
    /// `view_only`, `photo_not_found`, `invalid_date`, `invalid_location`.
    pub async fn photo_set_info<R: Runtime>(ids: Vec<i64>, patch: InfoPatch; app: AppHandle<R>) -> InfoChanged {
        let scanner = app.state::<Session>().queries()?.0;
        let changed = off_main(text::album::WRITING_DATABASE, move || {
            typed::set_info(scanner.album(), ids, patch)
        })
        .await?;
        if !changed.changed.is_empty() {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged {
                    revision: changed.revision,
                    reason: ChangeReason::Info,
                    rekeyed: Vec::new(),
                },
            );
        }
        Ok(changed)
    }
}

command! {
    /// Adds `delta` (1 or -1) to the score nudge of the photos `ids`
    /// (docs/05-organising.md, "Nudges (Up and Down)"). The current photo
    /// goes first: `total` is its new score. A photo at the limit is left
    /// unchanged. Sends `album_changed` with reason `info` when any changed.
    /// Errors: `view_only`, `photo_not_found`.
    pub async fn photo_nudge<R: Runtime>(ids: Vec<i64>, delta: i64; app: AppHandle<R>) -> Nudged {
        let scanner = app.state::<Session>().queries()?.0;
        let nudged = off_main(text::album::WRITING_DATABASE, move || {
            nudge::nudge(scanner.album(), ids, delta)
        })
        .await?;
        if !nudged.changed.is_empty() {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged {
                    revision: nudged.revision,
                    reason: ChangeReason::Info,
                    rekeyed: Vec::new(),
                },
            );
        }
        Ok(nudged)
    }
}

command! {
    /// The breakdown of one photo's score for the score card
    /// (docs/05-organising.md, "The score card"). `photo_not_found` when
    /// the id is not a photo the album shows.
    pub async fn photo_score(id: i64; session: State<'_, Session>) -> ScoreBreakdown {
        let (scanner, _) = session.queries()?;
        off_main(text::photo::READING_DETAIL, move || {
            score::photo_score(scanner.album(), id)
        })
        .await
    }
}
