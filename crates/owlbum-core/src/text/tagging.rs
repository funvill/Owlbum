//! Tagging's sentences (docs/07-tagging.md).

/// "Couldn't download {label}: {reason}".
pub fn download_failed(label: &str, reason: &str) -> String {
    format!("Couldn't download {label}: {reason}")
}

/// A status other than 200 or 206.
pub fn bad_status(status: u16, path: &str) -> String {
    format!("Hugging Face answered {status} for {path}.")
}

/// A redirect to a host that is not allowed.
pub fn host_not_allowed(host: &str) -> String {
    format!("Download host not allowed: {host}")
}

pub const REDIRECT_WITHOUT_LOCATION: &str = "Hugging Face sent a redirect without a location.";
pub const TOO_MANY_REDIRECTS: &str = "Too many redirects from Hugging Face.";
pub const UNREACHABLE: &str =
    "Couldn't reach Hugging Face. Check the internet connection and try again.";
pub const STALLED: &str = "The download stalled. Try again.";

/// The disk is full.
pub fn disk_full(path: &str) -> String {
    format!("Not enough disk space for {path}. Free some space and download again.")
}

/// Any other write error.
pub fn write_failed(path: &str, message: &str) -> String {
    format!("Couldn't write {path}: {message}")
}

/// Wrong size or hash.
pub fn failed_check(path: &str) -> String {
    format!("{path} did not pass its check (wrong size or SHA-256); it was deleted. Try again.")
}

pub const NO_SUCH_MODEL: &str = "No such model.";
pub const DOWNLOAD_BUSY: &str = "Another model is downloading. Wait for it, or cancel it.";
pub const MODEL_NOT_READY: &str = "Choose a downloaded model of the right kind, or Off.";

/// A load of a model that is not `ok`.
pub fn incomplete(label: &str, paths: &str) -> String {
    format!("{label} is incomplete ({paths}). Download it again.")
}

/// The runtime library would not start.
pub fn runtime_failed(reason: &str) -> String {
    format!("The model runtime would not start: {reason}")
}

pub const COULD_NOT_READ_PHOTO: &str = "Couldn't read the photo.";
pub const TOOK_TOO_LONG: &str = "Tagging this photo took too long.";
pub const UNUSABLE_VECTOR: &str = "The model returned an unusable vector.";

pub const NO_DESCRIBE_MODEL: &str = "There is no working descriptions model. Choose one in Settings.";

pub fn not_described_yet(label: &str) -> String {
    format!("{label} has not described this photo yet.")
}

pub fn model_load_failed(label: &str) -> String {
    format!("{label} would not load. See Settings.")
}

pub fn empty_description(label: &str) -> String {
    format!("{label} wrote an empty description. Try again.")
}

pub fn tagging_timeout(label: &str) -> String {
    format!("{label} took too long. Try again.")
}

pub fn tagging_lost(label: &str) -> String {
    format!("{label} stopped working. Try again.")
}

pub const TAGGING_STOPPED: &str = "Tagging stopped.";
