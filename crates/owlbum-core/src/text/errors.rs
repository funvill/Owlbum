//! Messages for errors shared by several areas.

/// The message of an `internal` error. `what` names what failed.
pub fn internal(what: impl std::fmt::Display) -> String {
    format!(
        "Something went wrong while {what}. Try again, and if it keeps happening, send feedback from the Help menu."
    )
}
