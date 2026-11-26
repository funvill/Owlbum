//! Work in flight, shared: requests for the same key while a job runs wait
//! for that job's result instead of starting another (docs/03, "On
//! demand": requests for the same key share one job).

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};

/// Jobs in flight by key. A job that panics answers `V::default()` to
/// everyone waiting on it, and the panic is logged.
pub struct Flights<V> {
    jobs: Mutex<HashMap<String, Arc<Flight<V>>>>,
}

struct Flight<V> {
    value: Mutex<Option<V>>,
    done: Condvar,
}

impl<V> Default for Flights<V> {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }
}

impl<V> std::fmt::Debug for Flights<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flights").finish_non_exhaustive()
    }
}

impl<V: Clone + Default> Flights<V> {
    /// Runs `job` for `key`, or, when a job for `key` already runs, waits
    /// for it and answers its result.
    pub fn share(&self, key: &str, job: impl FnOnce() -> V) -> V {
        let (flight, mine) = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            match jobs.get(key) {
                Some(f) => (Arc::clone(f), false),
                None => {
                    let f = Arc::new(Flight {
                        value: Mutex::new(None),
                        done: Condvar::new(),
                    });
                    jobs.insert(key.to_owned(), Arc::clone(&f));
                    (f, true)
                }
            }
        };
        if !mine {
            let mut value = flight.value.lock().unwrap_or_else(|p| p.into_inner());
            while value.is_none() {
                value = flight.done.wait(value).unwrap_or_else(|p| p.into_inner());
            }
            return value.clone().unwrap_or_default();
        }
        let value = catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| {
            tracing::error!(target: "thumbs", "a picture job panicked");
            V::default()
        });
        self.jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key);
        *flight.value.lock().unwrap_or_else(|p| p.into_inner()) = Some(value.clone());
        flight.done.notify_all();
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn concurrent_requests_for_one_key_run_one_job() {
        let flights = Flights::<Option<u32>>::default();
        let runs = AtomicUsize::new(0);
        let answers: Vec<_> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    s.spawn(|| {
                        flights.share("k", || {
                            runs.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(200));
                            Some(7)
                        })
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert!(answers.iter().all(|a| *a == Some(7)));
        // Once finished, the next request runs again.
        assert_eq!(flights.share("k", || Some(8)), Some(8));
    }

    #[test]
    fn a_panicking_job_answers_the_default() {
        let flights = Flights::<Option<u32>>::default();
        assert_eq!(flights.share("k", || panic!("boom")), None);
        assert_eq!(flights.share("k", || Some(1)), Some(1));
    }
}
