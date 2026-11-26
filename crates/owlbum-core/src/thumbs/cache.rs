//! The size-limited cache of pictures under app data (docs/02-data.md,
//! "Thumbnail keys and the cache"): the `cache_entries` table of
//! `app.sqlite`, its size limit, pruning and Clear cache.
//!
//! Writes to `app.sqlite` are batched, because it runs with
//! `synchronous = FULL` and a scan makes thousands of thumbnails:
//!
//! - a new entry waits in memory until 200 wait or a second has passed;
//! - `last_used` is kept in memory when a picture is served and written
//!   every 60 seconds, never once per request.
//!
//! A background thread does both on time; [`Cache::flush`] does them now.
//! The total size is kept in memory, so the size limit and the 70% rule for
//! medium thumbnails cost no query.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Kind, store};
use crate::appdata::{AppData, clock};
use crate::error::AppResult;

/// One gigabyte, as `cache.limit_gb` counts it.
pub const GB: u64 = 1_000_000_000;
/// New entries written at once.
const BATCH_ROWS: usize = 200;
/// The longest a new entry waits in memory, in milliseconds.
const BATCH_WAIT_MS: i64 = 1_000;
/// How often `last_used` is written, in milliseconds.
const TOUCH_EVERY_MS: i64 = 60_000;
/// Entries read per step while pruning.
const PRUNE_STEP: i64 = 256;

/// A clock in milliseconds since the epoch; tests pass their own.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// `app_cache_usage`'s reply (docs/08-desktop-and-release.md, "Commands").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CacheUsage {
    /// Bytes held by `thumbs/` and `full/`.
    #[ts(type = "number")]
    pub bytes: u64,
    /// `cache.limit_gb` in bytes.
    #[ts(type = "number")]
    pub limit_bytes: u64,
}

/// `app_cache_clear`'s reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CacheCleared {
    /// What the cache held before it was emptied.
    #[ts(type = "number")]
    pub bytes_freed: u64,
}

/// The cache. Cloning is cheap; every clone shares one state.
#[derive(Clone)]
pub struct Cache {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

struct Inner {
    data: AppData,
    clock: Clock,
    state: Mutex<State>,
}

struct State {
    /// The sum of `bytes` in `cache_entries`.
    written: u64,
    limit: u64,
    /// Entries made but not yet written, and when the first was made.
    pending: Vec<(String, Kind, u64)>,
    pending_since: Option<i64>,
    /// `last_used` values not yet written.
    touched: HashMap<(String, Kind), i64>,
    touched_written_at: i64,
}

impl State {
    fn bytes(&self) -> u64 {
        self.written + self.pending.iter().map(|p| p.2).sum::<u64>()
    }
}

impl Cache {
    /// Opens the cache of the app data folder `data`, with the real clock,
    /// and starts the thread that writes batched changes on time.
    pub fn open(data: AppData) -> AppResult<Self> {
        Self::with_clock(data, Arc::new(clock::now_ms))
    }

    /// [`Cache::open`] with a given clock.
    pub fn with_clock(data: AppData, clock: Clock) -> AppResult<Self> {
        let limit = u64::from(data.settings()?.cache_limit_gb) * GB;
        let written: i64 = data
            .conn()
            .query_row(
                "SELECT coalesce(sum(bytes), 0) FROM cache_entries",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let now = clock();
        let inner = Arc::new(Inner {
            data,
            clock,
            state: Mutex::new(State {
                written: u64::try_from(written).unwrap_or(0),
                limit,
                pending: Vec::new(),
                pending_since: None,
                touched: HashMap::new(),
                touched_written_at: now,
            }),
        });
        let weak = Arc::downgrade(&inner);
        let _ = std::thread::Builder::new()
            .name("owlbum-cache".into())
            .spawn(move || ticker(&weak));
        Ok(Self { inner })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner.state()
    }

    /// The app data folder.
    pub fn dir(&self) -> &std::path::Path {
        self.inner.data.dir()
    }

    /// Notes a picture just written: its `cache_entries` row is written
    /// with the next batch, and the cache is pruned if it is now over its
    /// limit.
    pub fn record(&self, key: &str, kind: Kind, bytes: u64) {
        let now = (self.inner.clock)();
        let mut st = self.state();
        st.pending.push((key.to_owned(), kind, bytes));
        st.pending_since.get_or_insert(now);
        st.touched.remove(&(key.to_owned(), kind));
        let due = st.pending.len() >= BATCH_ROWS
            || st.pending_since.is_some_and(|t| now - t >= BATCH_WAIT_MS);
        if due {
            self.inner.write_pending(&mut st);
        }
        if st.bytes() > st.limit {
            self.inner.prune(&mut st);
        }
    }

    /// Notes that a picture was served. Kept in memory; written with the
    /// next 60-second flush.
    pub fn touch(&self, key: &str, kind: Kind) {
        let now = (self.inner.clock)();
        self.state().touched.insert((key.to_owned(), kind), now);
    }

    /// Writes what waits now: new entries and `last_used` values.
    pub fn flush(&self) {
        let mut st = self.state();
        self.inner.write_pending(&mut st);
        self.inner.write_touched(&mut st);
    }

    /// Writes what is due: new entries a second old, `last_used` values
    /// every 60 seconds. The background thread calls this every second.
    pub fn tick(&self) {
        self.inner.tick();
    }

    /// The bytes held and the limit.
    pub fn usage(&self) -> CacheUsage {
        let st = self.state();
        CacheUsage {
            bytes: st.bytes(),
            limit_bytes: st.limit,
        }
    }

    /// True while the cache is under 70% of its limit: a scan makes medium
    /// thumbnails ahead of need only then.
    pub fn has_room_for_medium(&self) -> bool {
        let st = self.state();
        u128::from(st.bytes()) * 10 < u128::from(st.limit) * 7
    }

    /// Sets the limit to `gb` gigabytes and prunes at once if the cache is
    /// over it.
    pub fn set_limit_gb(&self, gb: u32) {
        self.set_limit_bytes(u64::from(gb) * GB);
    }

    /// Sets the limit in bytes, for tests that need a small cache.
    pub fn set_limit_bytes(&self, bytes: u64) {
        let mut st = self.state();
        st.limit = bytes;
        if st.bytes() > st.limit {
            self.inner.prune(&mut st);
        }
    }

    /// Reads `cache.limit_gb` again (it may have changed in Settings) and
    /// prunes if the cache is now over it.
    pub fn refresh_limit(&self) -> AppResult<()> {
        let gb = self.inner.data.settings()?.cache_limit_gb;
        self.set_limit_gb(gb);
        Ok(())
    }

    /// Prunes now: deletes the least recently used entries, `full` first,
    /// until the cache is within its limit.
    pub fn prune(&self) {
        let mut st = self.state();
        self.inner.prune(&mut st);
    }

    /// Clear cache: deletes `thumbs/`, `full/` and every entry. Answers
    /// what the cache held.
    pub fn clear(&self) -> CacheCleared {
        let mut st = self.state();
        let freed = st.bytes();
        st.pending.clear();
        st.pending_since = None;
        st.touched.clear();
        let dir = self.inner.data.dir();
        for folder in [store::thumbs_dir(dir), store::full_dir(dir)] {
            if let Err(e) = std::fs::remove_dir_all(&folder)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(target: "thumbs", "clearing {}: {e}", folder.display());
            }
        }
        if let Err(e) = self
            .inner
            .data
            .conn()
            .execute("DELETE FROM cache_entries", [])
        {
            tracing::warn!(target: "thumbs", "clearing cache entries: {e}");
        }
        st.written = 0;
        CacheCleared { bytes_freed: freed }
    }

    /// The keys that have a small thumbnail entry, for the warm pass: one
    /// query. Entries still in memory are included.
    pub fn small_keys(&self) -> std::collections::HashSet<String> {
        let st = self.state();
        let mut keys: std::collections::HashSet<String> = st
            .pending
            .iter()
            .filter(|p| p.1 == Kind::Small)
            .map(|p| p.0.clone())
            .collect();
        let conn = self.inner.data.conn();
        let read = conn
            .prepare("SELECT thumb_key FROM cache_entries WHERE kind = 's'")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            });
        match read {
            Ok(rows) => keys.extend(rows),
            Err(e) => tracing::warn!(target: "thumbs", "reading cache entries: {e}"),
        }
        keys
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn tick(&self) {
        let now = (self.clock)();
        let mut st = self.state();
        if st.pending_since.is_some_and(|t| now - t >= BATCH_WAIT_MS) {
            self.write_pending(&mut st);
        }
        if now - st.touched_written_at >= TOUCH_EVERY_MS {
            self.write_touched(&mut st);
        }
    }

    /// Writes the waiting entries in one transaction. An entry written
    /// again (a damaged file made again) replaces the old one's size.
    fn write_pending(&self, st: &mut State) {
        if st.pending.is_empty() {
            return;
        }
        let now = (self.clock)();
        let pending = std::mem::take(&mut st.pending);
        st.pending_since = None;
        let mut conn = self.data.conn();
        let result = (|| -> rusqlite::Result<(u64, u64)> {
            let tx = conn.transaction()?;
            let (mut added, mut replaced) = (0u64, 0u64);
            {
                let mut old = tx.prepare_cached(
                    "SELECT bytes FROM cache_entries WHERE thumb_key = ?1 AND kind = ?2",
                )?;
                let mut put = tx.prepare_cached(
                    "INSERT OR REPLACE INTO cache_entries (thumb_key, kind, bytes, last_used)
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for (key, kind, bytes) in &pending {
                    let before: Option<i64> = old
                        .query_row(params![key, kind.as_str()], |r| r.get(0))
                        .optional()?;
                    replaced += before.map_or(0, |b| u64::try_from(b).unwrap_or(0));
                    put.execute(params![
                        key,
                        kind.as_str(),
                        i64::try_from(*bytes).unwrap_or(i64::MAX),
                        now
                    ])?;
                    added += bytes;
                }
            }
            tx.commit()?;
            Ok((added, replaced))
        })();
        match result {
            Ok((added, replaced)) => {
                st.written = (st.written + added).saturating_sub(replaced);
            }
            Err(e) => tracing::warn!(target: "thumbs", "writing cache entries: {e}"),
        }
    }

    /// Writes the `last_used` values kept in memory.
    fn write_touched(&self, st: &mut State) {
        st.touched_written_at = (self.clock)();
        if st.touched.is_empty() {
            return;
        }
        let touched = std::mem::take(&mut st.touched);
        let mut conn = self.data.conn();
        let result = (|| -> rusqlite::Result<()> {
            let tx = conn.transaction()?;
            {
                let mut up = tx.prepare_cached(
                    "UPDATE cache_entries SET last_used = max(last_used, ?3)
                     WHERE thumb_key = ?1 AND kind = ?2",
                )?;
                for ((key, kind), at) in &touched {
                    up.execute(params![key, kind.as_str(), at])?;
                }
            }
            tx.commit()
        })();
        if let Err(e) = result {
            tracing::warn!(target: "thumbs", "writing last used times: {e}");
        }
    }

    /// Deletes the least recently used entries, `full` first, until the
    /// cache is down to 90% of its limit, so a full cache is not pruned on
    /// every new thumbnail. Everything waiting is written first, so the
    /// order sees every entry and every `last_used`. Each step's deletes
    /// are one transaction.
    fn prune(&self, st: &mut State) {
        self.write_pending(st);
        self.write_touched(st);
        let target = low_water(st.limit);
        while st.written > target {
            let mut conn = self.data.conn();
            let batch = conn
                .prepare(
                    "SELECT thumb_key, kind, bytes FROM cache_entries
                     ORDER BY kind <> 'full', last_used, thumb_key, kind LIMIT ?1",
                )
                .and_then(|mut stmt| {
                    stmt.query_map([PRUNE_STEP], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()
                });
            let batch = match batch {
                Ok(b) if !b.is_empty() => b,
                Ok(_) => {
                    // Nothing left to delete: the count was off.
                    st.written = 0;
                    return;
                }
                Err(e) => {
                    tracing::warn!(target: "thumbs", "reading cache entries to prune: {e}");
                    return;
                }
            };
            let dir = self.data.dir();
            let mut deleted = Vec::new();
            for (key, kind, bytes) in batch {
                if st.written <= target {
                    break;
                }
                if let Some(k) = Kind::parse(&kind)
                    && store::is_thumb_key(&key)
                {
                    store::remove(&store::path(dir, &key, k));
                }
                st.written = st.written.saturating_sub(u64::try_from(bytes).unwrap_or(0));
                deleted.push((key, kind));
            }
            let result = conn.transaction().and_then(|tx| {
                {
                    let mut del = tx.prepare_cached(
                        "DELETE FROM cache_entries WHERE thumb_key = ?1 AND kind = ?2",
                    )?;
                    for (key, kind) in &deleted {
                        del.execute(params![key, kind])?;
                    }
                }
                tx.commit()
            });
            if let Err(e) = result {
                tracing::warn!(target: "thumbs", "deleting cache entries: {e}");
                return;
            }
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let mut st = self.state();
        let mut st = std::mem::replace(
            &mut *st,
            State {
                written: 0,
                limit: 0,
                pending: Vec::new(),
                pending_since: None,
                touched: HashMap::new(),
                touched_written_at: 0,
            },
        );
        self.write_pending(&mut st);
        self.write_touched(&mut st);
    }
}

/// Where pruning stops: 90% of the limit.
fn low_water(limit: u64) -> u64 {
    u64::try_from(u128::from(limit) * 9 / 10).unwrap_or(limit)
}

/// Every second, writes what is due, until the cache is dropped.
fn ticker(cache: &Weak<Inner>) {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        match cache.upgrade() {
            Some(inner) => inner.tick(),
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    struct Fixture {
        data: AppData,
        now: Arc<AtomicI64>,
        cache: Cache,
        /// Last, so it is dropped last: fields drop in order, and on Windows
        /// the folder cannot be deleted while a database in it is still open.
        _tmp: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let data = AppData::open(tmp.path()).unwrap();
        let now = Arc::new(AtomicI64::new(1_000_000));
        let clock = {
            let now = Arc::clone(&now);
            Arc::new(move || now.load(Ordering::SeqCst)) as Clock
        };
        let cache = Cache::with_clock(data.clone(), clock).unwrap();
        Fixture {
            _tmp: tmp,
            data,
            now,
            cache,
        }
    }

    impl Fixture {
        fn advance(&self, ms: i64) {
            self.now.fetch_add(ms, Ordering::SeqCst);
        }

        /// Writes a file of `bytes` bytes for `key` and records it.
        fn put(&self, key: &str, kind: Kind, bytes: usize) {
            let mut body = vec![0u8; bytes.max(4)];
            body[..2].copy_from_slice(&[0xFF, 0xD8]);
            let n = body.len();
            body[n - 2..].copy_from_slice(&[0xFF, 0xD9]);
            store::write(&store::path(self.data.dir(), key, kind), &body).unwrap();
            self.cache.record(key, kind, body.len() as u64);
        }

        fn exists(&self, key: &str, kind: Kind) -> bool {
            store::path(self.data.dir(), key, kind).is_file()
        }

        fn rows(&self) -> Vec<(String, String, i64, i64)> {
            let conn = self.data.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT thumb_key, kind, bytes, last_used FROM cache_entries
                     ORDER BY thumb_key, kind",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        }
    }

    #[test]
    fn new_entries_are_written_in_batches() {
        let f = fixture();
        f.put(A, Kind::Small, 100);
        assert!(f.rows().is_empty(), "waits in memory");
        assert_eq!(f.cache.usage().bytes, 100, "but counts at once");
        f.advance(1_000);
        f.cache.tick();
        assert_eq!(f.rows(), [(A.into(), "s".into(), 100, 1_001_000)]);
        for i in 0..BATCH_ROWS {
            f.cache.record(&format!("{i:040}"), Kind::Small, 1);
        }
        assert_eq!(f.rows().len(), 1 + BATCH_ROWS, "a full batch is written");
    }

    #[test]
    fn the_total_survives_a_restart_and_a_rewrite_replaces_the_size() {
        let f = fixture();
        f.put(A, Kind::Small, 100);
        f.put(A, Kind::Medium, 300);
        f.cache.flush();
        f.put(A, Kind::Small, 120);
        f.cache.flush();
        assert_eq!(f.cache.usage().bytes, 420);
        let again = Cache::open(f.data.clone()).unwrap();
        assert_eq!(
            again.usage(),
            CacheUsage {
                bytes: 420,
                limit_bytes: 20 * GB
            }
        );
    }

    #[test]
    fn last_used_is_written_every_minute_not_per_request() {
        let f = fixture();
        f.put(A, Kind::Small, 10);
        f.cache.flush();
        f.advance(5_000);
        f.cache.touch(A, Kind::Small);
        f.cache.tick();
        assert_eq!(f.rows()[0].3, 1_000_000, "not yet");
        f.advance(60_000);
        f.cache.tick();
        assert_eq!(f.rows()[0].3, 1_005_000, "the time it was served");
    }

    #[test]
    fn pruning_deletes_full_first_then_least_recently_used() {
        let f = fixture();
        f.cache.set_limit_gb(1);
        f.put(A, Kind::Small, 10);
        f.advance(10);
        f.put(B, Kind::Small, 10);
        f.advance(10);
        f.put(C, Kind::Full, 10);
        f.cache.flush();
        // A is used again, so B is now the least recently used thumbnail.
        f.advance(10);
        f.cache.touch(A, Kind::Small);
        // Shrink the limit by hand: just under the total.
        f.cache.set_limit_bytes(25);
        assert!(!f.exists(C, Kind::Full), "full goes first");
        assert!(f.exists(A, Kind::Small) && f.exists(B, Kind::Small));
        f.cache.set_limit_bytes(15);
        assert!(!f.exists(B, Kind::Small), "then the least recently used");
        assert!(f.exists(A, Kind::Small));
        assert_eq!(f.rows().len(), 1);
        assert_eq!(f.cache.usage().bytes, 10);
    }

    #[test]
    fn going_over_the_limit_prunes_at_once() {
        let f = fixture();
        f.cache.set_limit_bytes(25);
        f.put(A, Kind::Small, 10);
        f.advance(1);
        f.put(B, Kind::Small, 10);
        f.advance(1);
        f.put(C, Kind::Small, 10);
        // Down to 90% of the limit, not just under it.
        assert!(f.cache.usage().bytes <= low_water(25));
        assert!(!f.exists(A, Kind::Small), "the oldest went");
        assert!(f.exists(C, Kind::Small));
    }

    #[test]
    fn a_lower_limit_from_settings_prunes_at_once() {
        let f = fixture();
        f.put(A, Kind::Small, 10);
        f.data
            .set_settings(
                serde_json::json!({ "cache.limit_gb": 1 })
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        f.cache.refresh_limit().unwrap();
        assert_eq!(f.cache.usage().limit_bytes, GB);
        assert!(f.exists(A, Kind::Small), "well under 1 GB");
    }

    #[test]
    fn medium_thumbnails_stop_at_seventy_percent() {
        let f = fixture();
        f.cache.set_limit_bytes(100);
        assert!(f.cache.has_room_for_medium());
        f.put(A, Kind::Small, 69);
        assert!(f.cache.has_room_for_medium());
        f.put(B, Kind::Small, 1);
        assert!(!f.cache.has_room_for_medium(), "70 of 100");
    }

    #[test]
    fn clear_deletes_both_folders_and_every_entry() {
        let f = fixture();
        f.put(A, Kind::Small, 10);
        f.put(A, Kind::Medium, 20);
        f.put(B, Kind::Full, 30);
        f.cache.flush();
        f.put(C, Kind::Small, 5);
        let other = f.data.dir().join("models");
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(f.cache.clear(), CacheCleared { bytes_freed: 65 });
        assert!(!f.data.dir().join("thumbs").exists());
        assert!(!f.data.dir().join("full").exists());
        assert!(other.is_dir(), "nothing else is touched");
        assert!(f.data.dir().join("app.sqlite").is_file());
        f.cache.flush();
        assert!(f.rows().is_empty());
        assert_eq!(f.cache.usage().bytes, 0);
    }

    #[test]
    fn small_keys_lists_written_and_waiting_entries() {
        let f = fixture();
        f.put(A, Kind::Small, 1);
        f.cache.flush();
        f.put(B, Kind::Small, 1);
        f.put(C, Kind::Medium, 1);
        let keys = f.cache.small_keys();
        assert!(keys.contains(A) && keys.contains(B) && !keys.contains(C));
    }

    #[test]
    fn dropping_the_cache_writes_what_waits() {
        let f = fixture();
        f.put(A, Kind::Small, 7);
        let Fixture {
            _tmp, data, cache, ..
        } = f;
        drop(cache);
        // The ticker may hold the cache for a moment; the last holder
        // writes.
        let count = || -> i64 {
            data.conn()
                .query_row("SELECT count(*) FROM cache_entries", [], |r| r.get(0))
                .unwrap()
        };
        let start = std::time::Instant::now();
        while count() == 0 && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(count(), 1);
    }
}
