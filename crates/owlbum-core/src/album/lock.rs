//! The lock file (docs/02-data.md, "Opening, locking and view-only").
//!
//! `.owlbum/lock` says who has the album open for writing. It is JSON:
//! `{ "host", "pid", "user", "app_version", "opened_at", "heartbeat" }`,
//! with times in milliseconds since the Unix epoch, like every other
//! timestamp Owlbum stores. The holder rewrites `heartbeat` every 30 seconds
//! and deletes the file on close.
//!
//! [`judge`] decides what an existing lock means for an open; it is a pure
//! function so every row of the spec's table is tested without a second
//! computer. [`AlbumLock`] is a held lock: it beats in a background thread
//! and removes the file when dropped.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The lock file's name inside `.owlbum`.
pub const LOCK_FILE: &str = "lock";

/// How often the holder rewrites `heartbeat`.
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);

/// A lock from another computer whose heartbeat is younger than this is
/// live: the album opens view-only. An older one is asked about.
pub const LIVE_FOR_MS: i64 = 2 * 60 * 1000;

/// The lock file's contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    /// The computer's name.
    pub host: String,
    /// The process that holds the lock.
    pub pid: u32,
    /// The user the process runs as.
    pub user: String,
    /// The Owlbum version of the holder.
    pub app_version: String,
    /// When the album was opened, ms since the epoch.
    pub opened_at: i64,
    /// When the holder last wrote the file, ms since the epoch.
    pub heartbeat: i64,
}

/// Who is opening the album: this process, or a pretend one in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    /// This computer's name.
    pub host: String,
    /// This process's id.
    pub pid: u32,
    /// The user this process runs as.
    pub user: String,
}

impl Owner {
    /// This process on this computer.
    pub fn this_process() -> Self {
        Self {
            host: this_host(),
            pid: std::process::id(),
            user: this_user(),
        }
    }
}

/// What an existing lock means for an open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No lock, or one that may be replaced without asking: a lock left by a
    /// process of this computer that is no longer running, a lock of this
    /// very process, or a file that is not a lock at all.
    Free,
    /// Another Owlbum has the album open and beat within [`LIVE_FOR_MS`].
    Live {
        /// The computer named by the lock.
        host: String,
    },
    /// A lock whose holder has not beaten for [`LIVE_FOR_MS`] or more and
    /// may still be running: the user is asked.
    Old {
        /// The computer named by the lock.
        host: String,
        /// Its last heartbeat, ms since the epoch.
        last_seen: i64,
    },
}

/// Decides what `existing` means for `me` at `now` (ms since the epoch).
/// `is_running` says whether a process id of this computer is running.
///
/// A lock of this computer whose process still runs is judged by its
/// heartbeat like a lock of another computer (another Owlbum, for example
/// a development build, has the album). A heartbeat in the future (another
/// computer's clock is ahead) counts as just now.
pub fn judge(
    existing: Option<&LockInfo>,
    me: &Owner,
    now: i64,
    is_running: impl Fn(u32) -> bool,
) -> Verdict {
    let _ = (existing, me, now, is_running);
    unimplemented!("#4")
}

/// Reads `<dir>/lock`. `Ok(None)` when there is none; a file that is not a
/// lock (empty, or not the JSON above) reads as `Ok(None)` too, because
/// Owlbum writes the lock by renaming a finished file into place, so such a
/// file was never written by a running Owlbum.
pub fn read(dir: &Path) -> io::Result<Option<LockInfo>> {
    let _ = dir;
    unimplemented!("#4")
}

/// A held lock. It rewrites the heartbeat in a background thread and
/// deletes the lock file when dropped, if the file is still its own.
#[derive(Debug)]
pub struct AlbumLock {
    path: PathBuf,
    info: LockInfo,
    lost: Arc<AtomicBool>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

/// The clock the lock reads: ms since the epoch.
pub type NowMs = Arc<dyn Fn() -> i64 + Send + Sync>;

impl AlbumLock {
    /// Takes the lock in `dir` (`.owlbum`, which must exist) for `me`.
    ///
    /// With `replace` false the file is created only if there is none, so
    /// two computers opening at once cannot both win; the loser gets
    /// `AlreadyExists` and judges again. With `replace` true any existing
    /// lock is overwritten. The heartbeat is rewritten every `every`.
    pub fn take(
        dir: &Path,
        me: &Owner,
        replace: bool,
        every: Duration,
        now: NowMs,
    ) -> io::Result<Self> {
        let _ = (dir, me, replace, every, now);
        unimplemented!("#4")
    }

    /// The lock as written.
    pub fn info(&self) -> &LockInfo {
        &self.info
    }

    /// The lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True once the heartbeat found the file no longer names this holder
    /// (another computer chose Open anyway). The album must stop writing.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }
}

impl Drop for AlbumLock {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// This computer's name, or `"unknown"`.
pub fn this_host() -> String {
    unimplemented!("#4")
}

/// The user this process runs as, or `""`.
pub fn this_user() -> String {
    unimplemented!("#4")
}

/// Whether process `pid` of this computer is running.
pub fn is_running(pid: u32) -> bool {
    let _ = pid;
    unimplemented!("#4")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn me() -> Owner {
        Owner {
            host: "HERE".into(),
            pid: 100,
            user: "me".into(),
        }
    }

    fn lock(host: &str, pid: u32, heartbeat: i64) -> LockInfo {
        LockInfo {
            host: host.into(),
            pid,
            user: "someone".into(),
            app_version: "0.1.0".into(),
            opened_at: heartbeat - 1000,
            heartbeat,
        }
    }

    fn now_ms() -> NowMs {
        Arc::new(|| chrono::Utc::now().timestamp_millis())
    }

    #[test]
    fn no_lock_is_free() {
        assert_eq!(judge(None, &me(), NOW, |_| true), Verdict::Free);
    }

    #[test]
    fn a_lock_of_this_computer_whose_process_is_gone_is_stale() {
        let l = lock("HERE", 7, NOW);
        assert_eq!(judge(Some(&l), &me(), NOW, |_| false), Verdict::Free);
    }

    #[test]
    fn a_lock_of_this_very_process_is_free() {
        let l = lock("HERE", 100, NOW);
        assert_eq!(judge(Some(&l), &me(), NOW, |_| true), Verdict::Free);
    }

    #[test]
    fn another_computer_with_a_recent_heartbeat_is_live() {
        let l = lock("THERE", 7, NOW - LIVE_FOR_MS + 1);
        assert_eq!(
            judge(Some(&l), &me(), NOW, |_| false),
            Verdict::Live {
                host: "THERE".into()
            }
        );
    }

    #[test]
    fn another_computer_with_an_old_heartbeat_is_asked_about() {
        let l = lock("THERE", 7, NOW - LIVE_FOR_MS);
        assert_eq!(
            judge(Some(&l), &me(), NOW, |_| false),
            Verdict::Old {
                host: "THERE".into(),
                last_seen: NOW - LIVE_FOR_MS
            }
        );
    }

    #[test]
    fn a_heartbeat_in_the_future_counts_as_now() {
        let l = lock("THERE", 7, NOW + 3_600_000);
        assert_eq!(
            judge(Some(&l), &me(), NOW, |_| false),
            Verdict::Live {
                host: "THERE".into()
            }
        );
    }

    #[test]
    fn another_running_process_of_this_computer_is_judged_by_its_heartbeat() {
        let live = lock("HERE", 7, NOW - 1000);
        assert_eq!(
            judge(Some(&live), &me(), NOW, |_| true),
            Verdict::Live {
                host: "HERE".into()
            }
        );
        let old = lock("HERE", 7, NOW - LIVE_FOR_MS - 1);
        assert_eq!(
            judge(Some(&old), &me(), NOW, |_| true),
            Verdict::Old {
                host: "HERE".into(),
                last_seen: NOW - LIVE_FOR_MS - 1
            }
        );
    }

    #[test]
    fn host_names_compare_without_case() {
        // Windows computer names are case-insensitive.
        let l = lock("here", 7, NOW);
        assert_eq!(judge(Some(&l), &me(), NOW, |_| false), Verdict::Free);
    }

    #[test]
    fn the_file_is_the_documented_json() {
        let tmp = tempfile::tempdir().unwrap();
        let held = AlbumLock::take(tmp.path(), &me(), false, HEARTBEAT_EVERY, now_ms()).unwrap();
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(tmp.path().join(LOCK_FILE)).unwrap()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["app_version", "heartbeat", "host", "opened_at", "pid", "user"]
        );
        assert_eq!(v["host"], "HERE");
        assert_eq!(v["pid"], 100);
        assert_eq!(v["app_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(read(tmp.path()).unwrap().as_ref(), Some(held.info()));
    }

    #[test]
    fn taking_without_replace_refuses_an_existing_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let _held = AlbumLock::take(tmp.path(), &me(), false, HEARTBEAT_EVERY, now_ms()).unwrap();
        let other = Owner {
            host: "THERE".into(),
            ..me()
        };
        let err = AlbumLock::take(tmp.path(), &other, false, HEARTBEAT_EVERY, now_ms())
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read(tmp.path()).unwrap().unwrap().host, "HERE");
    }

    #[test]
    fn taking_with_replace_overwrites_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(LOCK_FILE),
            serde_json::to_vec(&lock("THERE", 7, NOW)).unwrap(),
        )
        .unwrap();
        let held = AlbumLock::take(tmp.path(), &me(), true, HEARTBEAT_EVERY, now_ms()).unwrap();
        assert_eq!(read(tmp.path()).unwrap().as_ref(), Some(held.info()));
    }

    #[test]
    fn dropping_the_lock_deletes_the_file_and_leaves_nothing_else() {
        let tmp = tempfile::tempdir().unwrap();
        let held = AlbumLock::take(tmp.path(), &me(), false, HEARTBEAT_EVERY, now_ms()).unwrap();
        drop(held);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_heartbeat_is_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let clock = Arc::new(std::sync::atomic::AtomicI64::new(NOW));
        let now: NowMs = {
            let clock = Arc::clone(&clock);
            Arc::new(move || clock.load(Ordering::SeqCst))
        };
        let held =
            AlbumLock::take(tmp.path(), &me(), false, Duration::from_millis(20), now).unwrap();
        assert_eq!(held.info().heartbeat, NOW);
        clock.store(NOW + 5000, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let on_disk = read(tmp.path()).unwrap().unwrap();
            if on_disk.heartbeat == NOW + 5000 {
                assert_eq!(on_disk.opened_at, NOW, "opened_at stays");
                break;
            }
            assert!(std::time::Instant::now() < deadline, "no heartbeat");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!held.is_lost());
    }

    #[test]
    fn a_replaced_lock_is_lost_and_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let held =
            AlbumLock::take(tmp.path(), &me(), false, Duration::from_millis(20), now_ms()).unwrap();
        let theirs = lock("THERE", 7, NOW);
        std::fs::write(
            tmp.path().join(LOCK_FILE),
            serde_json::to_vec(&theirs).unwrap(),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !held.is_lost() {
            assert!(std::time::Instant::now() < deadline, "loss not noticed");
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(held);
        assert_eq!(read(tmp.path()).unwrap(), Some(theirs), "not clobbered");
    }

    #[test]
    fn a_file_that_is_not_a_lock_reads_as_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
        std::fs::write(tmp.path().join(LOCK_FILE), b"").unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
        std::fs::write(tmp.path().join(LOCK_FILE), b"{\"host\":").unwrap();
        assert_eq!(read(tmp.path()).unwrap(), None);
    }

    #[test]
    fn this_process_is_running_and_a_finished_one_is_not() {
        assert!(is_running(std::process::id()));
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child);
        assert!(!is_running(pid));
    }

    #[test]
    fn this_host_has_a_name() {
        let host = this_host();
        assert!(!host.is_empty());
        assert!(!host.contains('\n'));
    }
}
