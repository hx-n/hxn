//! A queue of notifications from the library to the host, in the order the
//! events happened.
//!
//! A future that is settled, a stream that is sent to, and a signal that is
//! set each post their id here, once, until the host takes it. The host waits
//! on this single queue and takes from it, so results of different kinds
//! reach it in the order the library produced them, at a time the host
//! chooses, with one wait for all of them.
//!
//! Only objects the host registered with `watch` are posted, so a library
//! whose results are all awaited directly queues nothing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::{THREADS, local};

static QUEUE: Mutex<VecDeque<Arc<Tag>>> = Mutex::new(VecDeque::new());
static ARRIVED: Condvar = Condvar::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

// A host represents ids as a non-negative `Int`.
const IDS: u64 = i32::MAX as u64;

/// The outbox entry for a future, stream or signal.
pub(crate) struct Tag {
    id: i32,
    watched: AtomicBool,
    // Set while in the queue or about to be added. Cleared when the host
    // takes it, so the next change posts it again.
    queued: AtomicBool,
}

impl Tag {
    pub(crate) fn new() -> Arc<Self> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) % IDS;
        Arc::new(Self {
            id: id as i32,
            watched: AtomicBool::new(false),
            queued: AtomicBool::new(false),
        })
    }

    /// Returns the id the host watches. The first call starts the watching.
    pub(crate) fn watch(&self) -> i32 {
        self.watched.store(true, Ordering::Release);
        self.id
    }
}

fn queue() -> MutexGuard<'static, VecDeque<Arc<Tag>>> {
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Posts `tag` to the queue: it has something new for the host.
pub(crate) fn publish(tag: &Arc<Tag>) {
    if !tag.watched.load(Ordering::Acquire) || tag.queued.swap(true, Ordering::AcqRel) {
        return;
    }
    queue().push_back(tag.clone());
    ARRIVED.notify_all();
}

/// Blocks until the queue is non-empty or `timeout` has elapsed, and returns
/// whether it is non-empty. `None` waits indefinitely. Without threads it runs
/// queued work for a short time slice instead of blocking.
pub fn wait(timeout: Option<Duration>) -> bool {
    if !THREADS {
        local::drive(timeout, || !queue().is_empty());
        return !queue().is_empty();
    }
    let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
    let mut queue = queue();
    loop {
        if !queue.is_empty() {
            return true;
        }
        queue = match deadline {
            None => ARRIVED.wait(queue).unwrap_or_else(PoisonError::into_inner),
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return false;
                }
                ARRIVED
                    .wait_timeout(queue, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
        };
    }
}

/// Like [`wait`], with the timeout in the form a host passes it:
/// milliseconds, or negative for no limit.
pub fn wait_millis(milliseconds: i32) -> bool {
    wait(u64::try_from(milliseconds).ok().map(Duration::from_millis))
}

/// Pops the id of the next future, stream or signal with something new, in
/// the order they were posted, or returns -1 when the queue is empty.
pub fn next() -> i32 {
    match queue().pop_front() {
        Some(tag) => {
            tag.queued.store(false, Ordering::Release);
            tag.id
        }
        None => -1,
    }
}
