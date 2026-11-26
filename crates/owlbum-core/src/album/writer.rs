//! The writer thread (docs/01-architecture.md, "Threads"; docs/02-data.md,
//! "Everything goes through the writer thread").
//!
//! One thread per open album owns the only writable connection. Everything
//! that writes sends it a job; each job runs in its own short transaction and
//! the caller gets the job's answer after the commit, so a typed change is
//! saved before the command that made it returns. Because there is one
//! writer, writes never collide inside the program.
//!
//! The scanner's batching (commit when 200 rows wait or 250 ms have passed,
//! docs/03-media-and-scanning.md) is built on top of this by #8.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::AppError;
use crate::album::db::Gate;
use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::text;

type Job = Box<dyn FnOnce(&mut Gated) + Send>;

/// The writer thread's connection and the gate it holds while it writes.
struct Gated {
    conn: Connection,
    gate: Gate,
}

/// The handle to an album's writer thread. Dropping it lets the queued jobs
/// finish, closes the connection and waits for the thread to end.
pub struct Writer {
    jobs: Option<mpsc::Sender<Job>>,
    /// Held while a job is counted and queued, so its place in the queue
    /// and the count it was given agree.
    queueing: Mutex<()>,
    /// Jobs the writer thread has finished.
    done: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer").finish_non_exhaustive()
    }
}

impl Writer {
    /// Starts the writer thread with the album's writable connection.
    pub fn start(conn: Connection) -> AppResult<Self> {
        Self::start_gated(conn, Gate::default())
    }

    /// [`Writer::start`], holding `gate` alone for each transaction so the
    /// album's readers take turns with it ([`Gate`]).
    pub fn start_gated(conn: Connection, gate: Gate) -> AppResult<Self> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let done = Arc::new(AtomicU64::new(0));
        let finished = Arc::clone(&done);
        let thread = std::thread::Builder::new()
            .name("owlbum-album-writer".into())
            .spawn(move || {
                let mut conn = Gated { conn, gate };
                for job in queue {
                    job(&mut conn);
                    finished.fetch_add(1, Ordering::SeqCst);
                }
                // The connection closes here, after the last job.
            })
            .map_err(|_| AppError::internal(text::album::OPENING_DATABASE))?;
        Ok(Self {
            jobs: Some(jobs),
            queueing: Mutex::new(()),
            done,
            thread: Some(thread),
        })
    }

    /// Queues `job`, which is given how many jobs ran on the writer thread
    /// after it was queued and before it started: the ones it waited for.
    fn queue(&self, job: impl FnOnce(&mut Gated, u64) + Send + 'static) -> AppResult<()> {
        let done = Arc::clone(&self.done);
        let _order = self.queueing.lock().unwrap_or_else(|p| p.into_inner());
        let at_queue = done.load(Ordering::SeqCst);
        let job: Job = Box::new(move |conn: &mut Gated| {
            let waited = done.load(Ordering::SeqCst) - at_queue;
            job(conn, waited);
        });
        self.jobs
            .as_ref()
            .and_then(|jobs| jobs.send(job).ok())
            .ok_or_else(|| AppError::internal(text::album::WRITING_DATABASE))
    }

    /// Runs `job` in a transaction on the writer thread and waits for it.
    ///
    /// The transaction commits when `job` answers `Ok` and rolls back when it
    /// answers `Err` or panics; either way the writer keeps running. The
    /// answer comes back after the commit.
    ///
    /// Never call it from inside a job: the job would wait for itself.
    pub fn write<T, F>(&self, job: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
    {
        self.write_counted(job).map(|(value, _)| value)
    }

    /// [`Writer::write`], also answering how many earlier jobs (other
    /// writes, a scan's batches) the job waited for on the writer thread.
    /// Docs/03 promises a typed change waits for at most one scan batch;
    /// this is how a test checks it without a clock.
    pub fn write_counted<T, F>(&self, job: F) -> AppResult<(T, u64)>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
    {
        let (reply, answer) = mpsc::sync_channel::<AppResult<(T, u64)>>(1);
        self.queue(move |conn, waited| {
            let result = catch_unwind(AssertUnwindSafe(|| in_transaction(conn, job)))
                .unwrap_or_else(|_| Err(AppError::internal(text::album::WRITING_DATABASE)));
            // The caller may have gone; nothing to tell then.
            let _ = reply.send(result.map(|value| (value, waited)));
        })?;
        answer
            .recv()
            .unwrap_or_else(|_| Err(AppError::internal(text::album::WRITING_DATABASE)))
    }
}

impl Writer {
    /// [`Writer::write`], then `after` with the writer's connection and the
    /// job's answer: after the commit, on the writer thread, before the
    /// caller gets the answer. Nothing else writes in between. `after` does
    /// not run when the job fails, and a panic in it does not undo or hide
    /// the committed answer.
    ///
    /// This is how the album index follows each write
    /// ([`crate::album::Album::write_photos`]).
    pub fn write_then<T, F, A>(&self, job: F, after: A) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
        A: FnOnce(&Connection, &T) + Send + 'static,
    {
        let (reply, answer) = mpsc::sync_channel::<AppResult<T>>(1);
        self.queue(move |conn, _| {
            let result = catch_unwind(AssertUnwindSafe(|| in_transaction(conn, job)))
                .unwrap_or_else(|_| Err(AppError::internal(text::album::WRITING_DATABASE)));
            if let Ok(value) = &result {
                let conn: &Connection = &conn.conn;
                let _ = catch_unwind(AssertUnwindSafe(|| after(conn, value)));
            }
            let _ = reply.send(result);
        })?;
        answer
            .recv()
            .unwrap_or_else(|_| Err(AppError::internal(text::album::WRITING_DATABASE)))
    }
}

fn in_transaction<T>(
    gated: &mut Gated,
    job: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
) -> AppResult<T> {
    let failed = sqlite_error(text::album::WRITING_DATABASE);
    // Readers wait for the commit (or the rollback, which drops `tx` before
    // this) on the gate rather than on SQLite's busy handler.
    let _turn = gated.gate.write().unwrap_or_else(|p| p.into_inner());
    let tx = gated
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(&failed)?;
    // On `Err` (or a panic) `tx` is dropped here, which rolls it back.
    let value = job(&tx)?;
    tx.commit().map_err(&failed)?;
    Ok(value)
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Closing the queue ends the thread's loop after the queued jobs.
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn writer() -> Writer {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (n INTEGER NOT NULL)")
            .unwrap();
        Writer::start(conn).unwrap()
    }

    fn count(w: &Writer) -> i64 {
        w.write(|tx| {
            Ok(tx
                .query_row("SELECT count(*) FROM t", [], |r| r.get::<_, i64>(0))
                .unwrap())
        })
        .unwrap()
    }

    #[test]
    fn a_write_is_committed_before_it_answers() {
        let w = writer();
        let id = w
            .write(|tx| {
                tx.execute("INSERT INTO t (n) VALUES (1)", []).unwrap();
                Ok(tx.last_insert_rowid())
            })
            .unwrap();
        assert_eq!(id, 1);
        assert_eq!(count(&w), 1);
    }

    #[test]
    fn a_job_that_fails_is_rolled_back() {
        let w = writer();
        let e = w
            .write(|tx| -> AppResult<()> {
                tx.execute("INSERT INTO t (n) VALUES (1)", []).unwrap();
                Err(AppError::new(ErrorCode::ViewOnly, "no"))
            })
            .unwrap_err();
        assert_eq!(
            e.code,
            ErrorCode::ViewOnly,
            "the job's own error comes back"
        );
        assert_eq!(count(&w), 0);
    }

    #[test]
    fn a_job_that_panics_is_rolled_back_and_the_writer_keeps_going() {
        let w = writer();
        let e = w
            .write(|tx| -> AppResult<()> {
                tx.execute("INSERT INTO t (n) VALUES (1)", []).unwrap();
                panic!("a bug in a job");
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(count(&w), 0);
        w.write(|tx| Ok(tx.execute("INSERT INTO t (n) VALUES (2)", []).unwrap()))
            .unwrap();
        assert_eq!(count(&w), 1);
    }

    #[test]
    fn a_sqlite_failure_is_an_internal_error_with_the_cause_in_details() {
        let w = writer();
        let e = w
            .write(|tx| {
                tx.execute("INSERT INTO nope VALUES (1)", [])
                    .map_err(sqlite_error(text::album::WRITING_DATABASE))
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(
            e.message.contains("saving to the album database"),
            "{}",
            e.message
        );
        let cause = e.details.unwrap()["cause"].as_str().unwrap().to_owned();
        assert!(cause.contains("nope"), "{cause}");
    }

    #[test]
    fn a_write_counts_the_jobs_it_waited_for() {
        let w = writer();
        assert_eq!(w.write_counted(|_| Ok(())).unwrap().1, 0, "an idle writer");

        // One job running and two queued behind it, then the counted one.
        let (release, held) = mpsc::channel::<()>();
        let (running, started) = mpsc::channel::<()>();
        w.queue(move |_, _| {
            running.send(()).unwrap();
            held.recv().unwrap();
        })
        .unwrap();
        started.recv().unwrap();
        w.queue(|_, _| {}).unwrap();
        w.queue(|_, _| {}).unwrap();
        let (tell, waited) = mpsc::channel::<u64>();
        w.queue(move |_, n| tell.send(n).unwrap()).unwrap();
        release.send(()).unwrap();
        assert_eq!(waited.recv().unwrap(), 3);
    }

    #[test]
    fn jobs_from_many_threads_all_land() {
        let w = Arc::new(writer());
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let w = Arc::clone(&w);
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        w.write(move |tx| {
                            Ok(tx.execute("INSERT INTO t (n) VALUES (?1)", [i]).unwrap())
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(count(&w), 200);
    }

    #[test]
    fn the_after_step_runs_after_the_commit_and_before_the_answer() {
        let w = writer();
        let seen = Arc::new(std::sync::Mutex::new(None));
        let id = {
            let seen = Arc::clone(&seen);
            w.write_then(
                |tx| {
                    tx.execute("INSERT INTO t (n) VALUES (5)", []).unwrap();
                    Ok(tx.last_insert_rowid())
                },
                move |conn, id| {
                    assert!(conn.is_autocommit(), "the transaction is over");
                    let n: i64 = conn
                        .query_row("SELECT n FROM t WHERE rowid = ?1", [id], |r| r.get(0))
                        .unwrap();
                    let thread = std::thread::current().name().unwrap_or("").to_owned();
                    *seen.lock().unwrap() = Some((n, thread));
                },
            )
            .unwrap()
        };
        assert_eq!(id, 1);
        assert_eq!(
            seen.lock().unwrap().clone(),
            Some((5, "owlbum-album-writer".to_owned()))
        );
    }

    #[test]
    fn the_after_step_is_skipped_when_the_job_fails_and_cannot_undo_it() {
        let w = writer();
        let e = w
            .write_then(
                |_| -> AppResult<()> { Err(AppError::new(ErrorCode::ViewOnly, "no")) },
                |_, _| panic!("not after a failed job"),
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::ViewOnly);
        w.write_then(
            |tx| Ok(tx.execute("INSERT INTO t (n) VALUES (1)", []).unwrap()),
            |_, _| panic!("a bug in the after step"),
        )
        .unwrap();
        assert_eq!(count(&w), 1, "committed, and the writer keeps going");
    }

    #[test]
    fn jobs_run_on_one_named_thread() {
        let w = writer();
        let names: Vec<String> = (0..3)
            .map(|_| {
                w.write(|_| Ok(std::thread::current().name().unwrap_or("").to_owned()))
                    .unwrap()
            })
            .collect();
        assert_eq!(names, ["owlbum-album-writer"; 3]);
    }

    #[test]
    fn dropping_the_writer_closes_its_connection() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("w.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE t (n INTEGER NOT NULL)")
            .unwrap();
        let w = Writer::start(conn).unwrap();
        w.write(|tx| Ok(tx.execute("INSERT INTO t (n) VALUES (1)", []).unwrap()))
            .unwrap();
        drop(w);
        // With the connection closed the file can be removed, which Windows
        // refuses while any handle is open.
        std::fs::remove_file(&path).unwrap();
    }
}
