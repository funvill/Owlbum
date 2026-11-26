//! The scanner (docs/03-media-and-scanning.md, "The walker" and "The
//! scanner"; docs/02-data.md, "Identity: renames, moves and missing files").
//!
//! A scan brings the album database in step with the scope:
//!
//! 1. Walk ([`walk`]): list files with their size and modified time.
//! 2. Compare: sort every listed file and every row into unchanged,
//!    waiting, changed, new and gone, from one query.
//! 3. Match moves: `file_key` ([`keys`]) for new files, paired with gone
//!    and missing rows.
//! 4. Facts pass: headers only, for new and changed files, with the
//!    metadata reader ([`metadata`]).
//! 5. Pixels pass: one decode per waiting row, the thumbnail buffers and
//!    the image analysis ([`pixels`], [`analysis`]).
//! 6. Finish: mark missing rows, the last events.
//!
//! [`run`] does one scan on the calling thread. [`Scanner`] owns an open
//! album's scans: one at a time, in the background, with requests that
//! arrive meanwhile merged into one more and cancellation.
//!
//! Scan threads never write: they hand [`rows::Op`]s to the album's writer
//! thread, which commits them in batches of 200 rows or 250 ms, or, in a
//! view-only album, to the in-memory [`memory::Overlay`].
//!
//! What the walk left out, and the files not shown, are answered from the
//! last whole walk and the error rows by [`report`].
//!
//! Left for other issues, with their seams here: thumbnails and on-demand
//! priority (#11, [`pixels`]).
//!
//! The folder watcher ([`watch`]) asks for targeted rescans of the folders
//! it saw change ([`Request::folders`]).

pub mod analysis;
pub mod folders;
pub mod keys;
pub mod memory;
pub mod metadata;
pub mod order;
pub mod pixels;
pub mod progress;
pub mod reader;
pub mod report;
pub mod rows;
mod run;
mod scanner;
pub mod status;
pub mod walk;
pub mod watch;

#[cfg(test)]
mod tests;

pub use folders::Folders;
pub use order::{natural_cmp, natural_cmp_name, natural_cmp_text};
pub use progress::{ScanPhase, ScanProgress};
pub use reader::{MediaReader, Reader};
pub use run::{Request, Scan, ScanEnv, ScanSummary, run};
pub use scanner::{ScanStatus, Scanner};
pub use status::{AlbumOpened, AlbumStatus, UnsupportedSummary};

/// Raised in the same commit as any change to the metadata rules, the
/// analysis formulas or the error placeholders: every row with an older
/// value is read again by the next scan.
pub const SCAN_VERSION: i64 = 3;
