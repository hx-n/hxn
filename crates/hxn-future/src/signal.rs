//! A value the library keeps current and the host reads when it likes.
//!
//! Where a stream hands over everything that was sent, in order, this hands
//! over the latest and nothing else: a host that looks after ten changes
//! sees the tenth. `hxn.Reactive.follow` turns one into a Haxe signal, which
//! effects and computed values then depend on like any other.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use hxn_abi::{Failure, Fault};

use crate::outbox::{self, Tag};
use crate::{THREADS, local};

struct State<T> {
    value: T,
    version: u64,
    // Every source is gone, so the value will not change again.
    closed: bool,
    // The reading side is gone, which is how a source learns to stop.
    unread: bool,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
    tag: Arc<Tag>,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// Shared by every clone of a source; the last one to go closes the signal.
struct Owner<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Drop for Owner<T> {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.changed.notify_all();
        outbox::publish(&self.shared.tag);
    }
}

/// The setting side. Clones set the same signal, which closes when the last
/// of them is dropped.
pub struct Source<T> {
    owner: Arc<Owner<T>>,
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
        }
    }
}

impl<T> Source<T> {
    /// Replaces the value. `false` if nothing will read it, because the
    /// reader is gone: the sign to stop.
    pub fn set(&self, value: T) -> bool {
        let shared = &self.owner.shared;
        let mut state = shared.lock();
        if state.unread {
            // Unlocked before `value` is dropped: its `Drop` is the library's.
            drop(state);
            return false;
        }
        let old = std::mem::replace(&mut state.value, value);
        state.version += 1;
        drop(state);
        shared.changed.notify_all();
        outbox::publish(&shared.tag);
        drop(old);
        true
    }
}

/// The reading side.
pub struct Signal<T> {
    shared: Arc<Shared<T>>,
}

/// What a wait found.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Change {
    /// The value is at this version, which is not the one the waiter had.
    At(u64),
    /// The time ran out with the value as the waiter had it.
    Unchanged,
    /// The waiter has the last value there will be.
    Closed,
}

impl<T> Signal<T> {
    /// A number that changes with every [`Source::set`].
    pub fn version(&self) -> u64 {
        self.shared.lock().version
    }

    fn change(state: &State<T>, seen: Option<u64>) -> Change {
        if seen != Some(state.version) {
            Change::At(state.version)
        } else if state.closed {
            Change::Closed
        } else {
            Change::Unchanged
        }
    }

    /// Blocks until the value is no longer at version `seen`, the signal is
    /// closed, or `timeout` has passed. `None` for `seen` returns the current
    /// version at once, and `None` for `timeout` waits without limit. Without
    /// threads it runs queued work for a short slice instead of blocking.
    pub fn wait(&self, seen: Option<u64>, timeout: Option<Duration>) -> Change {
        if !THREADS {
            local::drive(timeout, || {
                Self::change(&self.shared.lock(), seen) != Change::Unchanged
            });
            return Self::change(&self.shared.lock(), seen);
        }
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        let mut state = self.shared.lock();
        loop {
            let change = Self::change(&state, seen);
            if change != Change::Unchanged {
                return change;
            }
            state = match deadline {
                None => self
                    .shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                        return Change::Unchanged;
                    };
                    if left.is_zero() {
                        return Change::Unchanged;
                    }
                    self.shared
                        .changed
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }
}

impl<T: Clone> Signal<T> {
    /// The value as it is now.
    pub fn get(&self) -> T {
        self.shared.lock().value.clone()
    }
}

impl<T> Drop for Signal<T> {
    fn drop(&mut self) {
        self.shared.lock().unread = true;
    }
}

/// A signal holding `initial`, and what sets it.
pub fn signal<T>(initial: T) -> (Source<T>, Signal<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            value: initial,
            version: 0,
            closed: false,
            unread: false,
        }),
        changed: Condvar::new(),
        tag: Tag::new(),
    });
    let owner = Arc::new(Owner {
        shared: shared.clone(),
    });
    (Source { owner }, Signal { shared })
}

trait Erased: Send + Sync {
    fn wait(&self, seen: Option<u64>, timeout: Option<Duration>) -> Change;
    fn watch(&self) -> i32;
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: Send + 'static> Erased for Signal<T> {
    fn wait(&self, seen: Option<u64>, timeout: Option<Duration>) -> Change {
        Signal::wait(self, seen, timeout)
    }

    fn watch(&self) -> i32 {
        self.shared.tag.watch()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A signal of any type, which is what a host holds. Only
/// [`get`](Self::get) needs the type again.
pub struct AnySignal(Box<dyn Erased>);

impl<T: Send + 'static> From<Signal<T>> for AnySignal {
    fn from(signal: Signal<T>) -> Self {
        Self(Box::new(signal))
    }
}

// A host counts versions in a non-negative int, and negative ones say
// something else.
const VERSIONS: u64 = i32::MAX as u64;

/// What [`AnySignal::wait_millis`] returns once the host has the last value.
pub const CLOSED: i32 = -1;

impl AnySignal {
    /// Has the signal noted in the [`outbox`](crate::outbox) when it changes,
    /// and returns the id it is noted under.
    pub fn watch(&self) -> i32 {
        self.0.watch()
    }

    /// As [`Signal::wait`], in the terms a host passes: the version it last
    /// saw, or any negative number for none, and a limit in milliseconds, or
    /// negative for none. Returns the version the value is at, which is
    /// `seen` if the time ran out, or [`CLOSED`].
    pub fn wait_millis(&self, seen: i32, milliseconds: i32) -> i32 {
        let timeout = u64::try_from(milliseconds).ok().map(Duration::from_millis);
        let mut deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        let mut known = None;
        loop {
            let left = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            match self.0.wait(known, left) {
                Change::At(version) => {
                    let host = (version % VERSIONS) as i32;
                    // Only a version the host has not seen is news to it.
                    if host != seen {
                        return host;
                    }
                    known = Some(version);
                    if timeout.is_none() {
                        deadline = None;
                    }
                }
                Change::Unchanged => return seen,
                Change::Closed => return CLOSED,
            }
        }
    }

    /// The value as it is now, or [`Fault::InvalidHandle`] for another type.
    pub fn get<T: Clone + Send + 'static>(&self) -> Result<T, Failure> {
        match self.0.as_any().downcast_ref::<Signal<T>>() {
            Some(signal) => Ok(signal.get()),
            None => Err(Fault::InvalidHandle.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_value_is_what_is_read() {
        let (source, signal) = signal(1);
        assert_eq!((signal.get(), signal.version()), (1, 0));
        assert!(source.set(2));
        assert!(source.set(3));
        assert_eq!((signal.get(), signal.version()), (3, 2));
    }

    #[test]
    fn wait_tells_a_change_from_the_time_running_out() {
        let (source, signal) = signal("first");
        assert_eq!(signal.wait(None, Some(Duration::ZERO)), Change::At(0));
        assert_eq!(
            signal.wait(Some(0), Some(Duration::ZERO)),
            Change::Unchanged
        );
        assert_eq!(
            signal.wait(Some(0), Some(Duration::from_millis(5))),
            Change::Unchanged
        );
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                source.set("second");
            });
            assert_eq!(signal.wait(Some(0), None), Change::At(1));
        });
        assert_eq!(signal.get(), "second");
    }

    #[test]
    fn dropping_every_source_closes_after_the_last_value() {
        let (source, signal) = signal(1);
        let other = source.clone();
        other.set(2);
        drop(source);
        assert_eq!(
            signal.wait(Some(1), Some(Duration::ZERO)),
            Change::Unchanged
        );
        drop(other);
        // Not yet seen by a waiter that is a version behind.
        assert_eq!(signal.wait(Some(0), None), Change::At(1));
        assert_eq!(signal.wait(Some(1), None), Change::Closed);
        assert_eq!(signal.get(), 2);
    }

    #[test]
    fn a_source_learns_its_reader_is_gone() {
        let (source, signal) = signal(Arc::new(()));
        let value = Arc::new(());
        drop(signal);
        assert!(!source.set(value.clone()));
        assert_eq!(Arc::strong_count(&value), 1);
    }

    #[test]
    fn a_host_follows_in_its_own_terms() {
        let (source, signal) = signal(10);
        let erased = AnySignal::from(signal);
        assert_eq!(erased.wait_millis(-2, 0), 0);
        assert_eq!(erased.wait_millis(0, 0), 0);
        assert_eq!(erased.wait_millis(0, 5), 0);
        source.set(11);
        source.set(12);
        assert_eq!(erased.wait_millis(0, -1), 2);
        assert_eq!(erased.get::<i32>(), Ok(12));
        assert!(erased.get::<String>().is_err());
        drop(source);
        assert_eq!(erased.wait_millis(2, -1), CLOSED);
        assert_eq!(erased.get::<i32>(), Ok(12));
    }
}
