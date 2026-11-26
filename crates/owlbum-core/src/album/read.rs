//! The read side: a small pool of read-only connections to the album
//! database (docs/01-architecture.md, "Threads").

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags};

use crate::album::{db, sqlite_error};
use crate::error::AppResult;
use crate::text;

/// How many idle read connections are kept open.
pub const POOL_SIZE: usize = 4;

/// Read-only connections to one album database. A connection is opened when
/// none is idle; up to [`POOL_SIZE`] are kept for reuse.
#[derive(Debug)]
pub struct Readers {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
}

impl Readers {
    /// A pool for the database at `path`. Opens nothing yet.
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            idle: Mutex::new(Vec::new()),
        }
    }

    /// Reads through one connection held for the session: the in-memory
    /// database of a folder that cannot be written. Reads take turns.
    pub fn shared(conn: Connection) -> Self {
        let _ = conn;
        unimplemented!("#4")
    }

    /// Runs `f` with a read-only connection. Any number of threads may read
    /// at once; a read waits (up to [`db::BUSY_TIMEOUT`]) only while the
    /// writer commits.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = match self.idle.lock().unwrap_or_else(|p| p.into_inner()).pop() {
            Some(conn) => conn,
            None => open_reader(&self.path)?,
        };
        let result = f(&conn);
        let mut idle = self.idle.lock().unwrap_or_else(|p| p.into_inner());
        if idle.len() < POOL_SIZE {
            idle.push(conn);
        }
        result
    }
}

/// Opens one read-only connection.
fn open_reader(path: &Path) -> AppResult<Connection> {
    let failed = sqlite_error(text::album::OPENING_DATABASE);
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(&failed)?;
    conn.busy_timeout(db::BUSY_TIMEOUT).map_err(&failed)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_cannot_write() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("r.sqlite");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE t (n)")
            .unwrap();
        let readers = Readers::new(&path);
        let n: i64 = readers
            .read(|c| {
                Ok(c.query_row("SELECT count(*) FROM t", [], |r| r.get(0))
                    .unwrap())
            })
            .unwrap();
        assert_eq!(n, 0);
        let write = readers.read(|c| Ok(c.execute("INSERT INTO t VALUES (1)", [])));
        assert!(write.unwrap().is_err());
    }

    #[test]
    fn a_shared_connection_serves_every_read() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (n); INSERT INTO t VALUES (1), (2);")
            .unwrap();
        let readers = std::sync::Arc::new(Readers::shared(conn));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let readers = std::sync::Arc::clone(&readers);
                std::thread::spawn(move || {
                    readers
                        .read(|c| {
                            Ok(c.query_row("SELECT count(*) FROM t", [], |r| r.get::<_, i64>(0))
                                .unwrap())
                        })
                        .unwrap()
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 2);
        }
    }

    #[test]
    fn connections_are_reused_up_to_the_pool_size() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("r.sqlite");
        Connection::open(&path).unwrap();
        let readers = Readers::new(&path);
        for _ in 0..10 {
            readers.read(|_| Ok(())).unwrap();
        }
        assert_eq!(readers.idle.lock().unwrap().len(), 1);
    }
}
