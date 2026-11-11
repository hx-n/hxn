//! What the library has for the host, in the order it happened.
//!
//! A future that is settled, a stream that is sent to and a signal that is
//! set each note themselves here, once, until the host has looked. The host
//! waits on this one queue and takes from it, so results of different kinds
//! reach it in the order the library produced them, at a moment the host
//! chooses, and with one wait for everything.
//!
//! Only what the host asked to watch is noted, so a library whose results
//! are all awaited directly queues nothing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::{THREADS, local};

static QUEUE: Mutex<VecDeque<Arc<Tag>>> = Mutex::new(VecDeque::new());
static ARRIVED: Condvar = Condvar::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

// A host counts in a non-negative int.
const IDS: u64 = i32::MAX as u64;

/// What a future, stream or signal is known by here.
pub(crate) struct Tag {
    id: i32,
    watched: AtomicBool,
    // In the queue, or about to be. Cleared when the host takes it, so
    // whatever happens after that is noted again.
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

    /// The id the host watches under. Asking is what starts the watching.
    pub(crate) fn watch(&self) -> i32 {
        self.watched.store(true, Ordering::Release);
        self.id
    }
}

fn queue() -> MutexGuard<'static, VecDeque<Arc<Tag>>> {
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Notes that `tag` has something new for the host.
pub(crate) fn publish(tag: &Arc<Tag>) {
    if !tag.watched.load(Ordering::Acquire) || tag.queued.swap(true, Ordering::AcqRel) {
        return;
    }
    queue().push_back(tag.clone());
    ARRIVED.notify_all();
}

/// Blocks until something is queued or `timeout` has passed, and returns
/// whether something is. `None` waits without limit. Without threads it runs
/// queued work for a short slice instead of blocking.
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

/// As [`wait`], with the timeout a host passes: milliseconds, or negative
/// for no limit.
pub fn wait_millis(milliseconds: i32) -> bool {
    wait(u64::try_from(milliseconds).ok().map(Duration::from_millis))
}

/// The id of the next thing with something new, in the order they were
/// noted, or -1 when there is nothing.
pub fn next() -> i32 {
    match queue().pop_front() {
        Some(tag) => {
            tag.queued.store(false, Ordering::Release);
            tag.id
        }
        None => -1,
    }
}
