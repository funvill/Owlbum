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
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::album::db::LostLock;

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
    let Some(lock) = existing else {
        return Verdict::Free;
    };
    if lock.host.eq_ignore_ascii_case(&me.host) && (lock.pid == me.pid || !is_running(lock.pid)) {
        return Verdict::Free;
    }
    let age = now.saturating_sub(lock.heartbeat).max(0);
    if age < LIVE_FOR_MS {
        Verdict::Live {
            host: lock.host.clone(),
        }
    } else {
        Verdict::Old {
            host: lock.host.clone(),
            last_seen: lock.heartbeat,
        }
    }
}

/// Reads `<dir>/lock`. `Ok(None)` when there is none; a file that is not a
/// lock (empty, or not the JSON above) reads as `Ok(None)` too. It may be a
/// lock another Owlbum has just created and is still filling, so
/// `Album::open` reads it again a moment later before replacing it.
pub fn read(dir: &Path) -> io::Result<Option<LockInfo>> {
    match std::fs::read(dir.join(LOCK_FILE)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Writes `info` to `<dir>/lock` by writing a temporary file and renaming it
/// into place, so a reader never sees half a lock.
fn write_whole(dir: &Path, info: &LockInfo) -> io::Result<()> {
    let tmp = dir.join(format!("{LOCK_FILE}.{}.tmp", info.pid));
    let result =
        std::fs::write(&tmp, json(info)).and_then(|()| std::fs::rename(&tmp, dir.join(LOCK_FILE)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn json(info: &LockInfo) -> Vec<u8> {
    serde_json::to_vec(info).unwrap_or_default()
}

/// True when two locks were written by the same open.
fn same_holder(a: &LockInfo, b: &LockInfo) -> bool {
    a.host == b.host && a.pid == b.pid && a.opened_at == b.opened_at
}

/// True when the lock on disk is still the one `info` describes.
fn still_ours(dir: &Path, info: &LockInfo) -> bool {
    matches!(read(dir), Ok(Some(on_disk)) if same_holder(&on_disk, info))
}

/// A held lock. It rewrites the heartbeat in a background thread and
/// deletes the lock file when dropped, if the file is still its own.
#[derive(Debug)]
pub struct AlbumLock {
    path: PathBuf,
    info: LockInfo,
    lost: LostLock,
    on_lost: OnLost,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

/// Called once, on the heartbeat thread, when the lock is lost.
type OnLost = Arc<OnLostSlot>;

#[derive(Default)]
struct OnLostSlot(Mutex<Option<Arc<dyn Fn() + Send + Sync>>>);

impl std::fmt::Debug for OnLostSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OnLostSlot")
    }
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
        let at = now();
        let info = LockInfo {
            host: me.host.clone(),
            pid: me.pid,
            user: me.user.clone(),
            app_version: crate::app::version().version,
            opened_at: at,
            heartbeat: at,
        };
        if replace {
            write_whole(dir, &info)?;
        } else {
            // `create_new` makes "is there a lock?" and "take it" one step.
            // The file is filled in place here; a reader that catches it
            // empty reads it again a moment later (`Album::open`).
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(LOCK_FILE))?;
            let written = io::Write::write_all(&mut file, &json(&info));
            drop(file);
            if let Err(e) = written {
                let _ = std::fs::remove_file(dir.join(LOCK_FILE));
                return Err(e);
            }
        }

        let lost = LostLock::default();
        let on_lost = OnLost::default();
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = {
            let dir = dir.to_path_buf();
            let mut beat = info.clone();
            let lost = Arc::clone(&lost);
            let on_lost = Arc::clone(&on_lost);
            std::thread::Builder::new()
                .name("owlbum-album-lock".into())
                .spawn(move || {
                    while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(every) {
                        match read(&dir) {
                            Ok(Some(on_disk)) if !same_holder(&on_disk, &beat) => {
                                *lost.lock().unwrap_or_else(|p| p.into_inner()) =
                                    Some(on_disk.host);
                                let tell =
                                    on_lost.0.lock().unwrap_or_else(|p| p.into_inner()).clone();
                                if let Some(tell) = tell {
                                    tell();
                                }
                                break;
                            }
                            // Ours, or gone (someone deleted it): beat, which
                            // puts it back.
                            Ok(_) => {}
                            // Unreadable for a moment: try at the next beat.
                            Err(_) => continue,
                        }
                        beat.heartbeat = now();
                        // A failed beat (a share that dropped for a moment)
                        // is tried again at the next one.
                        let _ = write_whole(&dir, &beat);
                    }
                })?
        };
        Ok(Self {
            path: dir.join(LOCK_FILE),
            info,
            lost,
            on_lost,
            stop: Some(stop),
            thread: Some(thread),
        })
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
        self.lost
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
    }

    /// Calls `tell` once when the lock is lost, on the heartbeat thread; at
    /// once, here, when it is lost already.
    pub fn on_lost(&self, tell: Arc<dyn Fn() + Send + Sync>) {
        let mut slot = self.on_lost.0.lock().unwrap_or_else(|p| p.into_inner());
        if self.is_lost() {
            drop(slot);
            tell();
        } else {
            *slot = Some(tell);
        }
    }

    /// Set to the other computer's name once the lock is lost. The album
    /// database shares it to refuse writes from then on.
    pub fn lost_to(&self) -> LostLock {
        Arc::clone(&self.lost)
    }
}

impl Drop for AlbumLock {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(dir) = self.path.parent()
            && still_ours(dir, &self.info)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// This computer's name, or `"unknown"`.
pub fn this_host() -> String {
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(not(windows))]
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok());
    name.map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// The user this process runs as, or `""`.
pub fn this_user() -> String {
    std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default()
}

/// Whether process `pid` of this computer is running.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn is_running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, GetLastError};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    /// `GetExitCodeProcess` answers this while the process runs.
    const STILL_ACTIVE: u32 = 259;

    // SAFETY: plain calls with no pointers kept; the handle is closed before
    // returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // A process we may not look at still exists.
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

/// Whether process `pid` of this computer is running.
#[cfg(not(windows))]
pub fn is_running(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

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
            [
                "app_version",
                "heartbeat",
                "host",
                "opened_at",
                "pid",
                "user"
            ]
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
        let err =
            AlbumLock::take(tmp.path(), &other, false, HEARTBEAT_EVERY, now_ms()).unwrap_err();
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
        let held = AlbumLock::take(
            tmp.path(),
            &me(),
            false,
            Duration::from_millis(20),
            now_ms(),
        )
        .unwrap();
        let theirs = lock("THERE", 7, NOW);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !held.is_lost() {
            assert!(std::time::Instant::now() < deadline, "loss not noticed");
            // Written again each time, as the other computer's heartbeat
            // would: a beat of ours can land on one of theirs.
            std::fs::write(
                tmp.path().join(LOCK_FILE),
                serde_json::to_vec(&theirs).unwrap(),
            )
            .unwrap();
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
