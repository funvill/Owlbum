//! Decode slots shared by the page's picture requests and the scan
//! (docs/03-media-and-scanning.md, "Order of work"): at most `n` decodes
//! at once, as many as the scan pool has threads. A free slot goes to the
//! newest waiting request from the page first, so what is on screen now
//! comes before what was on screen a moment ago; the scan's pixels pass
//! and the warm pass get a slot only when no request waits.

use std::sync::{Condvar, Mutex};

/// Who asks for a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// A picture the page asked for: newest first.
    Page,
    /// The scan's pixels pass and the warm pass: only when no page request
    /// waits, oldest first.
    Background,
}

pub struct Slots {
    state: Mutex<State>,
    freed: Condvar,
}

struct State {
    free: usize,
    /// Tickets of the page requests waiting, in the order they came.
    page: Vec<u64>,
    /// Tickets of the background jobs waiting, in the order they came.
    background: Vec<u64>,
    next: u64,
}

impl State {
    /// Whether the waiter holding `ticket` is the one a free slot goes to.
    fn is_next(&self, priority: Priority, ticket: u64) -> bool {
        if self.free == 0 {
            return false;
        }
        match priority {
            Priority::Page => self.page.last() == Some(&ticket),
            Priority::Background => {
                self.page.is_empty() && self.background.first() == Some(&ticket)
            }
        }
    }
}

impl Slots {
    pub fn new(n: usize) -> Self {
        Self {
            state: Mutex::new(State {
                free: n.max(1),
                page: Vec::new(),
                background: Vec::new(),
                next: 0,
            }),
            freed: Condvar::new(),
        }
    }

    /// Runs `job` in a slot, waiting for one first.
    pub fn run<T>(&self, priority: Priority, job: impl FnOnce() -> T) -> T {
        {
            let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let ticket = s.next;
            s.next += 1;
            match priority {
                Priority::Page => s.page.push(ticket),
                Priority::Background => s.background.push(ticket),
            }
            while !s.is_next(priority, ticket) {
                s = self.freed.wait(s).unwrap_or_else(|p| p.into_inner());
            }
            let queue = match priority {
                Priority::Page => &mut s.page,
                Priority::Background => &mut s.background,
            };
            queue.retain(|&t| t != ticket);
            s.free -= 1;
            // Another slot may be free for the next waiter.
            if s.free > 0 {
                self.freed.notify_all();
            }
        }
        struct Release<'a>(&'a Slots);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                self.0.state.lock().unwrap_or_else(|p| p.into_inner()).free += 1;
                self.0.freed.notify_all();
            }
        }
        let _release = Release(self);
        job()
    }

    /// Waiters of each priority, for tests.
    #[cfg(test)]
    pub fn waiting(&self) -> (usize, usize) {
        let s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        (s.page.len(), s.background.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// Waits until `slots` has `page` and `background` waiters.
    fn until_waiting(slots: &Slots, page: usize, background: usize) {
        let start = Instant::now();
        while slots.waiting() != (page, background) {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "{:?}",
                slots.waiting()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn page_requests_go_before_background_jobs_and_newest_first() {
        let slots = Arc::new(Slots::new(1));
        let (order_tx, order) = mpsc::channel();
        let (release_tx, release) = mpsc::channel::<()>();
        std::thread::scope(|s| {
            // Hold the only slot.
            let holder = Arc::clone(&slots);
            let (held_tx, held) = mpsc::channel();
            s.spawn(move || {
                holder.run(Priority::Background, || {
                    held_tx.send(()).unwrap();
                    release.recv().unwrap();
                });
            });
            held.recv().unwrap();
            // A background job waits first, then three page requests.
            let spawn = |name: &'static str, priority: Priority| {
                let slots = Arc::clone(&slots);
                let order_tx = order_tx.clone();
                s.spawn(move || slots.run(priority, || order_tx.send(name).unwrap()));
            };
            spawn("scan", Priority::Background);
            until_waiting(&slots, 0, 1);
            spawn("page 1", Priority::Page);
            until_waiting(&slots, 1, 1);
            spawn("page 2", Priority::Page);
            until_waiting(&slots, 2, 1);
            spawn("page 3", Priority::Page);
            until_waiting(&slots, 3, 1);
            release_tx.send(()).unwrap();
        });
        drop(order_tx);
        let order: Vec<_> = order.iter().collect();
        assert_eq!(order, ["page 3", "page 2", "page 1", "scan"]);
    }

    #[test]
    fn no_more_than_n_jobs_run_at_once() {
        let slots = Slots::new(3);
        let running = Mutex::new((0usize, 0usize));
        std::thread::scope(|s| {
            for i in 0..12 {
                let (slots, running) = (&slots, &running);
                s.spawn(move || {
                    let priority = if i % 2 == 0 {
                        Priority::Page
                    } else {
                        Priority::Background
                    };
                    slots.run(priority, || {
                        {
                            let mut r = running.lock().unwrap();
                            r.0 += 1;
                            r.1 = r.1.max(r.0);
                        }
                        std::thread::sleep(Duration::from_millis(5));
                        running.lock().unwrap().0 -= 1;
                    });
                });
            }
        });
        let most = running.lock().unwrap().1;
        assert!((1..=3).contains(&most), "{most}");
    }
}
