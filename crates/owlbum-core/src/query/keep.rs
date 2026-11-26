//! Where the current photo and the selection land when a result changes.

use super::shape::{Keep, Kept};

/// Settles `keep` against a new result.
pub fn settle(_keep: &Keep, _old: Option<&[u32]>, _new: &[u32]) -> Kept {
    unimplemented!()
}
