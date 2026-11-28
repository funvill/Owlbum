//! The update rules (docs/08-desktop-and-release.md, "Updates"): when the
//! quiet check runs, the states the shell's updater moves through, the
//! feed address and the words of a failure. The shell drives
//! `tauri-plugin-updater`; everything here is plain data so it is tested
//! without a network.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;
use crate::appdata::Settings;

/// The update feed, written by the release workflow.
pub const FEED_URL: &str = "https://github.com/funvill/Owlbum/releases/latest/download/latest.json";

/// The newest release's page.
pub const RELEASE_PAGE_URL: &str = "https://github.com/funvill/Owlbum/releases/latest";

/// Replaces the feed address for one run.
pub const FEED_VAR: &str = "OWLBUM_UPDATE_FEED";

/// Any non-empty value stops the quiet check for one run.
pub const NO_CHECK_VAR: &str = "OWLBUM_NO_UPDATE_CHECK";

/// The quiet check runs this long after `app_startup`.
pub const QUIET_DELAY_MS: u64 = 10_000;

/// Then the rule is tested again this often while Owlbum stays open.
pub const QUIET_EVERY_MS: u64 = 6 * 60 * 60 * 1000;

/// A quiet check runs at most once in this long.
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The release notes of one version.
pub fn release_notes_url(version: &str) -> String {
    format!("https://github.com/funvill/Owlbum/releases/tag/v{version}")
}

/// The updater's state (docs/08, "States").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStateKind {
    /// A development build, or the updater is not set up (no public key).
    Unavailable,
    /// Nothing in progress.
    Idle,
    /// A check is running.
    Checking,
    /// A newer version was found and offered.
    Available,
    /// The installer is downloading.
    Downloading,
    /// The installer is downloaded and verified.
    Ready,
    /// Owlbum is closing to install.
    Installing,
}

/// The `update_state` event, also the reply of `app_update_state`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateState {
    pub state: UpdateStateKind,
    /// The offered version, from `available` on.
    pub version: Option<String>,
    /// The download's rounded percentage while `downloading`.
    pub percent: Option<u8>,
}

impl Event for UpdateState {
    const NAME: &'static str = "update_state";
}

/// What a check, a download or an install came to, for the page's
/// dialog (docs/08, "Messages"). Sent as `update_message`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateMessage {
    /// A newer version: Download, Release notes, Later.
    Found { latest: String, current: String },
    /// A manual check found nothing newer.
    UpToDate { current: String },
    /// A manual check failed.
    CheckFailed { reason: String },
    /// A development build asked.
    DevBuild,
    /// No public key: Open the release page, Close.
    NotSetUp,
    /// The download failed: Try again, Close.
    DownloadFailed { reason: String },
    /// The installer is ready: Restart now, Later.
    Downloaded { version: String },
    /// Something stops the install before anything closes.
    CannotInstall { problem: String },
    /// The install failed.
    InstallFailed { reason: String },
}

impl Event for UpdateMessage {
    const NAME: &'static str = "update_message";
}

/// Whether the quiet check may run now (docs/08, "When it checks").
/// `set_up` is an installed build with a public key; `env_off` is
/// `OWLBUM_NO_UPDATE_CHECK` set and not empty.
pub fn quiet_allowed(set_up: bool, env_off: bool, settings: &Settings, now_ms: i64) -> bool {
    set_up
        && !env_off
        && settings.updates_auto
        && settings.updates_notice_seen
        && settings
            .updates_last_check
            .is_none_or(|last| last > now_ms || now_ms - last >= DAY_MS)
}

/// Whether an environment value turns something off: any non-empty value.
pub fn env_set(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// The feed address for this run: `OWLBUM_UPDATE_FEED` when set, else
/// [`FEED_URL`]. A release build accepts only `https://`; a development
/// build also `http://127.0.0.1`. An address that is refused answers why.
pub fn feed(over: Option<&str>, dev: bool) -> Result<String, String> {
    let Some(url) = over.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok(FEED_URL.to_owned());
    };
    let local = url.starts_with("http://127.0.0.1:") || url.starts_with("http://127.0.0.1/");
    if url.starts_with("https://") || (dev && local) {
        Ok(url.to_owned())
    } else {
        Err(format!("{FEED_VAR} must be an https:// address: {url}"))
    }
}

/// The words of an error for a message: its first non-empty line,
/// trimmed, at most 300 characters.
pub fn reason(error: &str) -> String {
    let line = error
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("unknown error");
    line.chars().take(300).collect()
}

/// Whether `latest` is newer than `current`: an older or equal version is
/// never offered.
pub fn is_newer(latest: &str, current: &str) -> bool {
    super::whats_new::compare(latest, current) == std::cmp::Ordering::Greater
}

/// The taskbar's progress for a state (issue 178): the download's
/// percentage while `downloading`, none otherwise (the bar is cleared).
pub fn taskbar_percent(state: &UpdateState) -> Option<u8> {
    (state.state == UpdateStateKind::Downloading).then(|| state.percent.unwrap_or(0).min(100))
}

/// What the feed answered when the updater found no release there, asked
/// again (issue 178): the plugin reports every answer that is not a
/// success as "no release". 404 (and 410) means nothing is published yet,
/// so up to date; any other failure status is a failed check. A success
/// now means the feed changed in between: up to date until the next check.
///
/// # Errors
///
/// The words of a failed check for any other status.
pub fn feed_status(status: u16) -> Result<(), String> {
    match status {
        200..=299 | 404 | 410 => Ok(()),
        _ => Err(format!("The update server answered HTTP {status}.")),
    }
}

/// The file in the app data folder that tells the next start an install
/// failed (issue 178). It holds the reason, one line.
pub const INSTALL_FAILED_FILE: &str = "update-install-failed.txt";

/// Records that the install failed, for the copy that starts next. Best
/// effort: the log has the reason too.
pub fn record_install_failed(data_dir: &std::path::Path, reason: &str) {
    let _ = std::fs::write(data_dir.join(INSTALL_FAILED_FILE), self::reason(reason));
}

/// Takes the failed install's reason recorded by the copy before, once.
pub fn take_install_failed(data_dir: &std::path::Path) -> Option<String> {
    let path = data_dir.join(INSTALL_FAILED_FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Some(reason(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn settings(auto: bool, seen: bool, last: Option<i64>) -> Settings {
        Settings {
            updates_auto: auto,
            updates_notice_seen: seen,
            updates_last_check: last,
            ..Settings::default()
        }
    }

    #[test]
    fn the_quiet_check_needs_every_condition() {
        let ok = settings(true, true, None);
        assert!(quiet_allowed(true, false, &ok, NOW));
        assert!(!quiet_allowed(false, false, &ok, NOW), "not set up");
        assert!(!quiet_allowed(true, true, &ok, NOW), "environment");
        assert!(
            !quiet_allowed(true, false, &settings(false, true, None), NOW),
            "turned off"
        );
        assert!(
            !quiet_allowed(true, false, &settings(true, false, None), NOW),
            "notice unanswered"
        );
    }

    #[test]
    fn the_quiet_check_runs_once_a_day_and_after_the_clock_goes_back() {
        let recent = settings(true, true, Some(NOW - DAY_MS + 1));
        assert!(!quiet_allowed(true, false, &recent, NOW));
        let day = settings(true, true, Some(NOW - DAY_MS));
        assert!(quiet_allowed(true, false, &day, NOW));
        let future = settings(true, true, Some(NOW + 1));
        assert!(quiet_allowed(true, false, &future, NOW));
    }

    #[test]
    fn the_feed_can_be_replaced_by_https_or_a_local_address_in_development() {
        assert_eq!(feed(None, false).unwrap(), FEED_URL);
        assert_eq!(feed(Some(""), false).unwrap(), FEED_URL);
        assert_eq!(
            feed(Some("https://x.test/l.json"), false).unwrap(),
            "https://x.test/l.json"
        );
        assert!(feed(Some("http://127.0.0.1:8000/l.json"), false).is_err());
        assert!(feed(Some("http://127.0.0.1:8000/l.json"), true).is_ok());
        assert!(feed(Some("http://example.com/l.json"), true).is_err());
        assert!(feed(Some("http://127.0.0.1.example.com/l.json"), true).is_err());
    }

    #[test]
    fn a_reason_is_the_first_line_at_most_300_characters() {
        assert_eq!(reason("\n  first line  \nsecond"), "first line");
        assert_eq!(reason(&"x".repeat(400)).chars().count(), 300);
        assert_eq!(reason(""), "unknown error");
    }

    #[test]
    fn only_a_newer_version_is_offered() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.2", "0.1.2"));
        assert!(!is_newer("0.1.1", "0.1.2"));
    }

    #[test]
    fn release_notes_name_the_tag() {
        assert_eq!(
            release_notes_url("0.2.0"),
            "https://github.com/funvill/Owlbum/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn the_taskbar_shows_only_a_download() {
        let at = |kind, percent| UpdateState {
            state: kind,
            version: Some("0.2.0".into()),
            percent,
        };
        assert_eq!(
            taskbar_percent(&at(UpdateStateKind::Downloading, Some(42))),
            Some(42)
        );
        assert_eq!(
            taskbar_percent(&at(UpdateStateKind::Downloading, None)),
            Some(0)
        );
        assert_eq!(
            taskbar_percent(&at(UpdateStateKind::Ready, Some(100))),
            None
        );
        assert_eq!(taskbar_percent(&at(UpdateStateKind::Available, None)), None);
    }

    #[test]
    fn only_404_reads_as_nothing_published() {
        assert_eq!(feed_status(404), Ok(()));
        assert_eq!(feed_status(410), Ok(()));
        assert_eq!(feed_status(200), Ok(()));
        assert_eq!(
            feed_status(500),
            Err("The update server answered HTTP 500.".to_owned())
        );
        assert!(feed_status(403).is_err());
        assert!(feed_status(503).is_err());
    }

    #[test]
    fn a_failed_install_is_told_to_the_next_start_once() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(take_install_failed(tmp.path()), None);
        record_install_failed(
            tmp.path(),
            "the installer could not start
more",
        );
        assert_eq!(
            take_install_failed(tmp.path()).as_deref(),
            Some("the installer could not start")
        );
        assert_eq!(take_install_failed(tmp.path()), None);
    }
}
