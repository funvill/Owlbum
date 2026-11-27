//! Undo (docs/05-organising.md, "Undo (Ctrl+Z)").
//!
//! Each changing command records one [`Step`] on the open album's
//! [`UndoList`]; `album_undo` ([`undo`]) takes the newest back. The list is
//! in memory only, so it is empty after the album closes or Owlbum
//! restarts.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::Album;
use crate::edit::store::{self as edits, EditStep};
use crate::error::AppResult;
use crate::info::typed::{self, UndoStep};
use crate::organise::nudge::{self, NudgeStep};

/// The most steps the list holds; the oldest is dropped past it.
pub const DEPTH: usize = 100;

/// One undo step: what one command changed, with the values before.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A `photo_set_info` call: star, hide or a typed field.
    Info(UndoStep),
    /// A `photo_nudge` tap.
    Score(NudgeStep),
    /// An `edit_set`, `edit_turn` or `edit_revert` call.
    Edit(EditStep),
}

impl Step {
    /// The step's label, as the toast names it.
    pub fn label(&self) -> &'static str {
        match self {
            Step::Info(s) => s.label,
            Step::Score(s) => s.label,
            Step::Edit(s) => s.label,
        }
    }

    /// The photos the step holds.
    pub fn ids(&self) -> Vec<i64> {
        match self {
            Step::Info(s) => s.before.iter().map(|p| p.id).collect(),
            Step::Score(s) => s.before.iter().map(|(id, _)| *id).collect(),
            Step::Edit(s) => s.before.iter().map(|(id, _)| *id).collect(),
        }
    }
}

/// The album's undo list, newest last.
#[derive(Debug, Default)]
pub struct UndoList {
    steps: Mutex<VecDeque<Step>>,
}

impl UndoList {
    fn steps(&self) -> std::sync::MutexGuard<'_, VecDeque<Step>> {
        self.steps.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `step` as the newest, dropping the oldest past [`DEPTH`].
    pub fn push(&self, step: Step) {
        let mut steps = self.steps();
        steps.push_back(step);
        while steps.len() > DEPTH {
            steps.pop_front();
        }
    }

    /// Takes the newest step off the list.
    pub fn pop(&self) -> Option<Step> {
        self.steps().pop_back()
    }

    /// Drops every step whose photos are all in `deleted` (Tidy removed
    /// their rows). A step that still holds another photo is kept as it
    /// is, so its undo answers `partial`.
    pub fn drop_deleted(&self, deleted: &std::collections::HashSet<i64>) {
        self.steps()
            .retain(|step| !step.ids().iter().all(|id| deleted.contains(id)));
    }

    /// Removes the edits of `ids` from every edit step, and drops an edit
    /// step left with no photo. Saving an edit into its file calls this, so
    /// undo never puts back an edit that is now in the file's pixels
    /// (docs/06-editing-and-file-actions.md, "Save into the file", step 8).
    /// Other steps are kept.
    pub fn drop_edits_of(&self, ids: &std::collections::HashSet<i64>) {
        let mut steps = self.steps();
        for step in steps.iter_mut() {
            if let Step::Edit(s) = step {
                s.before.retain(|(id, _)| !ids.contains(id));
            }
        }
        steps.retain(|step| !matches!(step, Step::Edit(s) if s.before.is_empty()));
    }

    /// The number of steps held.
    pub fn len(&self) -> usize {
        self.steps().len()
    }

    /// Whether no step is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How an undo went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UndoOutcome {
    /// There was no step to undo.
    Nothing,
    /// Every photo of the step was put back.
    Done,
    /// Some photos of the step are gone; the others were put back.
    Partial,
    /// Every photo of the step is gone; nothing was written.
    Gone,
}

/// `album_undo`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Undone {
    pub outcome: UndoOutcome,
    /// The step's label; null for `nothing`.
    pub label: Option<String>,
    /// The photos put back.
    pub ids: Vec<i64>,
    /// The album's revision after the undo; unchanged when nothing was
    /// written.
    pub revision: u64,
}

/// `album_undo()`: takes the newest step off the list and writes its
/// earlier values back in one transaction, leaving out photos whose rows
/// no longer exist or are missing. Records no new step. The album index
/// follows; when any photo was written the revision is raised and the
/// caller sends `album_changed` with reason `info`. When the write fails
/// the step goes back on the list.
pub fn undo(album: &Album) -> AppResult<Undone> {
    let Some(step) = album.undo().pop() else {
        return Ok(Undone {
            outcome: UndoOutcome::Nothing,
            label: None,
            ids: Vec::new(),
            revision: album.revision(),
        });
    };
    let written = match &step {
        Step::Info(s) => typed::restore_info(album, s),
        Step::Score(s) => nudge::restore(album, s),
        Step::Edit(s) => edits::restore(album, s),
    };
    let ids = match written {
        Ok(ids) => ids,
        Err(e) => {
            album.undo().push(step);
            return Err(e);
        }
    };
    let outcome = if ids.is_empty() {
        UndoOutcome::Gone
    } else if ids.len() < step.ids().len() {
        UndoOutcome::Partial
    } else {
        UndoOutcome::Done
    };
    let revision = if ids.is_empty() {
        album.revision()
    } else {
        album.next_revision()
    };
    Ok(Undone {
        outcome,
        label: Some(step.label().to_owned()),
        ids,
        revision,
    })
}

#[cfg(test)]
mod tests;
