//! One result from an asynchronous native operation, the same on every
//! runtime hxn supports.
//!
//! [`pending`] makes a pair. The library keeps the [`Completer`] and settles
//! it from any thread; the host is given the [`Future`], erased to an
//! [`AnyFuture`] inside a handle, and asks it for its state, waits on it, and
//! takes the result. `hxn.Future` is the Haxe side.
//!
//! The result stays in Rust until a Haxe thread takes it. Settling touches no
//! runtime, so the thread that settles needs no standing with a collector,
//! and nothing of the runtime's has to be kept alive in the meantime.
//!
//! A future is settled once: the first [`resolve`](Completer::resolve) or
//! [`reject`](Completer::reject) wins and later ones return `false`. If every
//! completer is dropped first it is rejected as [`Status::CANCELLED`], so a
//! waiter is never left waiting on nothing.

#![forbid(unsafe_code)]

use std::any::Any;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use hxn_abi::{Failure, Fault, Status, catch};

/// Where a future stands. The values are what a host sees.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum State {
    Pending = 0,
    Resolved = 1,
    Rejected = 2,
}

enum Slot<T> {
    Pending,
    Settled(Result<T, Failure>),
    // The result was taken; what it was is still its state.
    Taken(State),
}

impl<T> Slot<T> {
    fn state(&self) -> State {
        match self {
            Self::Pending => State::Pending,
            Self::Settled(Ok(_)) => State::Resolved,
            Self::Settled(Err(_)) => State::Rejected,
            Self::Taken(state) => *state,
        }
    }
}

struct Shared<T> {
    slot: Mutex<Slot<T>>,
    settled: Condvar,
}

impl<T> Shared<T> {
    // No code of the library's runs under the lock, so a poisoned one still
    // holds a whole slot.
    fn lock(&self) -> MutexGuard<'_, Slot<T>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn settle(&self, result: Result<T, Failure>) -> bool {
        let mut slot = self.lock();
        if !matches!(*slot, Slot::Pending) {
            // Unlocked before `result` is dropped: its `Drop` is the library's.
            drop(slot);
            return false;
        }
        *slot = Slot::Settled(result);
        drop(slot);
        self.settled.notify_all();
        true
    }

    fn state(&self) -> State {
        self.lock().state()
    }

    fn wait(&self, timeout: Option<Duration>) -> State {
        let deadline = timeout.and_then(|timeout| Instant::now().checked_add(timeout));
        let mut slot = self.lock();
        loop {
            let state = slot.state();
            if state != State::Pending {
                return state;
            }
            slot = match deadline {
                None => self
                    .settled
                    .wait(slot)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                        return State::Pending;
                    };
                    if left.is_zero() {
                        return State::Pending;
                    }
                    self.settled
                        .wait_timeout(slot, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }

    fn take(&self) -> Result<T, Failure> {
        let mut slot = self.lock();
        let state = slot.state();
        match std::mem::replace(&mut *slot, Slot::Taken(state)) {
            Slot::Settled(result) => result,
            Slot::Pending => {
                *slot = Slot::Pending;
                Err(Failure::new(Status::PENDING, "future is still pending"))
            }
            Slot::Taken(_) => Err(Fault::Closed.into()),
        }
    }
}

// Shared by every clone of a completer; the last one to go cancels.
struct Owner<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Drop for Owner<T> {
    fn drop(&mut self) {
        self.shared.settle(Err(Failure::new(
            Status::CANCELLED,
            "future was dropped before it was settled",
        )));
    }
}

/// The settling side. Clones settle the same future.
pub struct Completer<T> {
    owner: Arc<Owner<T>>,
}

impl<T> Clone for Completer<T> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
        }
    }
}

impl<T> Completer<T> {
    /// `false` if the future was already settled, in which case `value` is
    /// dropped.
    pub fn resolve(&self, value: T) -> bool {
        self.settle(Ok(value))
    }

    /// `false` if the future was already settled.
    pub fn reject(&self, failure: impl Into<Failure>) -> bool {
        self.settle(Err(failure.into()))
    }

    pub fn settle(&self, result: Result<T, Failure>) -> bool {
        self.owner.shared.settle(result)
    }
}

/// The reading side.
pub struct Future<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Future<T> {
    pub fn state(&self) -> State {
        self.shared.state()
    }

    /// Blocks until the future is settled or `timeout` has passed, and
    /// returns its state then. `None` waits without limit.
    pub fn wait(&self, timeout: Option<Duration>) -> State {
        self.shared.wait(timeout)
    }

    /// The result, once. [`Status::PENDING`] before there is one, which
    /// leaves it to be taken later, and [`Fault::Closed`] after it was taken.
    pub fn take(&self) -> Result<T, Failure> {
        self.shared.take()
    }
}

/// A pending future and what settles it.
pub fn pending<T>() -> (Completer<T>, Future<T>) {
    let shared = Arc::new(Shared {
        slot: Mutex::new(Slot::Pending),
        settled: Condvar::new(),
    });
    let owner = Arc::new(Owner {
        shared: shared.clone(),
    });
    (Completer { owner }, Future { shared })
}

/// Runs `work` on a thread of its own and settles a future with what it
/// returns. A panic rejects it.
pub fn spawn<T, E>(work: impl FnOnce() -> Result<T, E> + Send + 'static) -> Future<T>
where
    T: Send + 'static,
    E: Into<Failure>,
{
    let (completer, future) = pending();
    // If no thread starts, the closure is dropped and the future cancelled.
    let _ = std::thread::Builder::new().spawn(move || {
        completer.settle(catch(|| work().map_err(Into::into)));
    });
    future
}

trait Erased: Send + Sync {
    fn state(&self) -> State;
    fn wait(&self, timeout: Option<Duration>) -> State;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Send + 'static> Erased for Shared<T> {
    fn state(&self) -> State {
        Shared::state(self)
    }

    fn wait(&self, timeout: Option<Duration>) -> State {
        Shared::wait(self, timeout)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A future of any type, which is what a host holds: one handle type, and
/// one `state` and `wait` export, serve every future a library returns. Only
/// [`take`](Self::take) needs the type again.
pub struct AnyFuture(Arc<dyn Erased>);

impl<T: Send + 'static> From<Future<T>> for AnyFuture {
    fn from(future: Future<T>) -> Self {
        Self(future.shared)
    }
}

impl AnyFuture {
    pub fn state(&self) -> State {
        self.0.state()
    }

    pub fn wait(&self, timeout: Option<Duration>) -> State {
        self.0.wait(timeout)
    }

    /// As [`wait`](Self::wait), with the timeout a host passes: milliseconds,
    /// or negative for no limit.
    pub fn wait_millis(&self, milliseconds: i32) -> State {
        self.wait(u64::try_from(milliseconds).ok().map(Duration::from_millis))
    }

    /// As [`Future::take`], or [`Fault::InvalidHandle`] if this is not a
    /// future of `T`.
    pub fn take<T: Send + 'static>(&self) -> Result<T, Failure> {
        match self.0.as_any().downcast_ref::<Shared<T>>() {
            Some(shared) => shared.take(),
            None => Err(Fault::InvalidHandle.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_first_settle_wins() {
        let (completer, future) = pending();
        assert_eq!(future.state(), State::Pending);
        assert!(completer.resolve(1));
        assert!(!completer.resolve(2));
        assert!(!completer.clone().reject("late"));
        assert_eq!(future.state(), State::Resolved);
        assert_eq!(future.take(), Ok(1));
    }

    #[test]
    fn a_result_is_taken_once_and_keeps_its_state() {
        let (completer, future) = pending::<i32>();
        assert_eq!(future.take().unwrap_err().status, Status::PENDING);
        completer.reject(Failure::new(Status(7), "its own code"));
        assert_eq!(future.state(), State::Rejected);
        assert_eq!(future.take(), Err(Failure::new(Status(7), "its own code")));
        assert_eq!(future.take().unwrap_err().status, Status::CLOSED);
        assert_eq!(future.state(), State::Rejected);
    }

    #[test]
    fn dropping_every_completer_cancels() {
        let (completer, future) = pending::<i32>();
        let other = completer.clone();
        drop(completer);
        assert_eq!(future.state(), State::Pending);
        drop(other);
        assert_eq!(future.wait(None), State::Rejected);
        assert_eq!(future.take().unwrap_err().status, Status::CANCELLED);
    }

    #[test]
    fn a_settled_future_outlives_its_completer() {
        let (completer, future) = pending();
        completer.resolve("kept");
        drop(completer);
        assert_eq!(future.take(), Ok("kept"));
    }

    #[test]
    fn wait_returns_at_the_timeout_or_the_result() {
        let (completer, future) = pending();
        assert_eq!(future.wait(Some(Duration::ZERO)), State::Pending);
        assert_eq!(future.wait(Some(Duration::from_millis(5))), State::Pending);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                completer.resolve(5);
            });
            assert_eq!(future.wait(None), State::Resolved);
        });
        assert_eq!(future.wait(Some(Duration::MAX)), State::Resolved);
        assert_eq!(future.take(), Ok(5));
    }

    #[test]
    fn every_waiter_wakes() {
        let (completer, future) = pending::<()>();
        let woken = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    future.wait(None);
                    woken.fetch_add(1, Ordering::SeqCst);
                });
            }
            std::thread::sleep(Duration::from_millis(10));
            completer.resolve(());
        });
        assert_eq!(woken.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn spawn_settles_with_the_result_or_the_panic() {
        let done = spawn(|| Ok::<_, Failure>(6 * 7));
        assert_eq!(done.wait(None), State::Resolved);
        assert_eq!(done.take(), Ok(42));

        let failed = spawn(|| "x".parse::<i32>());
        assert_eq!(failed.wait(None), State::Rejected);
        assert_eq!(failed.take().unwrap_err().status, Status::ERROR);

        let panicked = spawn(|| -> Result<i32, Failure> { panic!("in the worker") });
        assert_eq!(panicked.wait(None), State::Rejected);
        assert_eq!(
            panicked.take(),
            Err(Failure::new(Status::PANIC, "in the worker"))
        );
    }

    #[test]
    fn an_erased_future_takes_only_its_own_type() {
        let (completer, future) = pending::<String>();
        let erased = AnyFuture::from(future);
        assert_eq!(erased.wait_millis(0), State::Pending);
        completer.resolve(String::from("typed"));
        assert_eq!(erased.wait_millis(-1), State::Resolved);
        assert_eq!(
            erased.take::<i32>().unwrap_err().status,
            Status::INVALID_HANDLE
        );
        assert_eq!(erased.state(), State::Resolved);
        assert_eq!(erased.take::<String>().as_deref(), Ok("typed"));
    }

    #[test]
    fn a_losing_value_is_dropped_outside_the_lock() {
        struct Reenters(Future<Reenters>);
        impl Drop for Reenters {
            fn drop(&mut self) {
                // Deadlocks if the slot is still locked.
                self.0.state();
            }
        }
        let (completer, future) = pending();
        let (_, unrelated) = pending();
        completer.resolve(Reenters(unrelated));
        let again = Future {
            shared: future.shared.clone(),
        };
        assert!(!completer.resolve(Reenters(again)));
        drop(future.take());
    }
}
