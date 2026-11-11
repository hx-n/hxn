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
//!
//! # Without threads
//!
//! A wasm build without threads has nowhere to run work beside its caller.
//! There [`spawn`] runs its work before it returns, and work written as
//! steps with [`stepped`] is queued on the calling thread and advanced by
//! whoever waits: [`Future::wait`] runs the queue for a short slice and
//! returns, pending or not, so the host can yield and ask again. The queue
//! is a `krio` scheduler; [`pump`] runs one round of it directly.

#![forbid(unsafe_code)]

use std::any::Any;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::task::Poll;
use std::time::{Duration, Instant};

use hxn_abi::{Failure, Fault, Status, catch};
use krio_core::{Suspension, Task};

mod local;
#[cfg(not(all(target_family = "wasm", not(target_feature = "atomics"))))]
mod pool;

pub use local::pump;

// False for wasm built without atomics, where std has no thread to start and
// no condition variable to wait on.
const THREADS: bool = cfg!(not(all(
    target_family = "wasm",
    not(target_feature = "atomics")
)));

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
        if !THREADS {
            return self.wait_driving(timeout);
        }
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

    // Waiting where nothing else can run: this thread does the queued work
    // itself, a round at a time, and gives up at the end of its slice so the
    // host gets the thread back.
    fn wait_driving(&self, timeout: Option<Duration>) -> State {
        let slice = timeout.map_or(local::SLICE, |timeout| timeout.min(local::SLICE));
        let started = Instant::now();
        loop {
            let state = self.state();
            if state != State::Pending || !local::pump() || started.elapsed() >= slice {
                return self.state();
            }
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
    /// returns its state then. `None` waits without limit. Without threads
    /// it runs queued work for a short slice instead of blocking.
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
/// returns. A panic rejects it. Without threads `work` runs here, and the
/// future is settled when this returns.
pub fn spawn<T, E>(work: impl FnOnce() -> Result<T, E> + Send + 'static) -> Future<T>
where
    T: Send + 'static,
    E: Into<Failure>,
{
    let (completer, future) = pending();
    let settle = move || {
        completer.settle(catch(|| work().map_err(Into::into)));
    };
    if THREADS {
        // If no thread starts, the closure is dropped and the future cancelled.
        let _ = std::thread::Builder::new().spawn(settle);
    } else {
        settle();
    }
    future
}

/// Work done a step at a time: `step` is called until it returns
/// [`Poll::Ready`], and that result settles the future. A panic rejects it.
///
/// The first step runs before this returns, so work that finishes at once
/// never waits for anyone. With threads the rest goes to the library's pool
/// of workers. Without them it is queued on this thread and advances only
/// while the host waits on a future from this library or calls [`pump`],
/// which is what lets one thread interleave it with everything else.
///
/// `Poll::Pending` means there is more to do, not that the work is waiting
/// on something: a worker takes the next step straight away.
pub fn stepped<T, E>(step: impl FnMut() -> Poll<Result<T, E>> + Send + 'static) -> Future<T>
where
    T: Send + 'static,
    E: Into<Failure> + 'static,
{
    let (completer, future) = pending();
    let mut task = Stepped { step, completer };
    if task.step().is_done() {
        return future;
    }
    hand_over(Box::new(task));
    future
}

#[cfg(not(all(target_family = "wasm", not(target_feature = "atomics"))))]
fn hand_over(task: Box<dyn Task + Send>) {
    pool::submit(task);
}

#[cfg(all(target_family = "wasm", not(target_feature = "atomics")))]
fn hand_over(task: Box<dyn Task + Send>) {
    local::queue(task);
}

struct Stepped<S, T> {
    step: S,
    completer: Completer<T>,
}

impl<S, T, E> Task for Stepped<S, T>
where
    S: FnMut() -> Poll<Result<T, E>>,
    E: Into<Failure>,
{
    fn step(&mut self) -> Suspension {
        match catch(|| Ok((self.step)())) {
            Ok(Poll::Pending) => Suspension::Yielded,
            Ok(Poll::Ready(result)) => {
                self.completer.settle(result.map_err(Into::into));
                Suspension::Completed
            }
            Err(panic) => {
                self.completer.reject(panic);
                Suspension::Completed
            }
        }
    }
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

    #[test]
    fn stepped_work_settles_with_its_last_step() {
        let mut total = 0;
        let mut next = 1;
        let sum = stepped(move || {
            total += next;
            next += 1;
            if next > 100 {
                Poll::Ready(Ok::<_, Failure>(total))
            } else {
                Poll::Pending
            }
        });
        assert_eq!(sum.wait(None), State::Resolved);
        assert_eq!(sum.take(), Ok(5050));

        let at_once = stepped(|| Poll::Ready(Ok::<_, Failure>("first step")));
        assert_eq!(at_once.state(), State::Resolved);

        let failed = stepped(|| Poll::Ready("x".parse::<i32>()));
        assert_eq!(failed.take().unwrap_err().status, Status::ERROR);

        let mut calls = 0;
        let panicked = stepped(move || -> Poll<Result<i32, Failure>> {
            calls += 1;
            if calls == 3 {
                panic!("on the third step");
            }
            Poll::Pending
        });
        assert_eq!(panicked.wait(None), State::Rejected);
        assert_eq!(
            panicked.take(),
            Err(Failure::new(Status::PANIC, "on the third step"))
        );
    }

    #[test]
    fn the_workers_take_more_work_than_there_are_of_them_and_survive_a_panic() {
        let panicked = stepped(move || -> Poll<Result<i32, Failure>> {
            if std::thread::current().name() == Some("hxn-worker") {
                panic!("on a worker");
            }
            Poll::Pending
        });
        assert_eq!(panicked.wait(None), State::Rejected);

        let sums: Vec<_> = (0..64)
            .map(|extra| stepped(counting(1000 + extra)))
            .collect();
        for (extra, sum) in sums.iter().enumerate() {
            assert_eq!(sum.wait(None), State::Resolved);
            assert_eq!(sum.take(), Ok(1000 + extra as i32));
        }
    }

    // What `stepped` does where there are no threads, on this thread's queue.
    fn queued<T: Send + 'static>(
        step: impl FnMut() -> Poll<Result<T, Failure>> + 'static,
    ) -> Future<T> {
        let (completer, future) = pending();
        local::queue(Box::new(Stepped { step, completer }));
        future
    }

    fn counting(to: i32) -> impl FnMut() -> Poll<Result<i32, Failure>> {
        let mut at = 0;
        move || {
            at += 1;
            if at == to {
                Poll::Ready(Ok(at))
            } else {
                Poll::Pending
            }
        }
    }

    #[test]
    fn waiting_drives_queued_work_a_round_at_a_time() {
        let future = queued(counting(3));
        assert_eq!(future.state(), State::Pending);
        assert_eq!(
            future.shared.wait_driving(Some(Duration::ZERO)),
            State::Pending
        );
        assert_eq!(
            future.shared.wait_driving(Some(Duration::ZERO)),
            State::Pending
        );
        assert_eq!(
            future.shared.wait_driving(Some(Duration::ZERO)),
            State::Resolved
        );
        assert_eq!(future.take(), Ok(3));
        assert!(!pump());
    }

    #[test]
    fn a_wait_without_limit_finishes_short_work_and_gives_up_on_long() {
        let short = queued(counting(500));
        assert_eq!(short.shared.wait_driving(None), State::Resolved);

        let endless = queued(|| Poll::<Result<(), Failure>>::Pending);
        let started = Instant::now();
        assert_eq!(endless.shared.wait_driving(None), State::Pending);
        assert!(started.elapsed() >= local::SLICE);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn queued_work_takes_turns() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let record = |name: &'static str, steps: i32| {
            let order = order.clone();
            let mut inner = counting(steps);
            queued(move || {
                order.lock().unwrap().push(name);
                inner()
            })
        };
        let first = record("a", 2);
        let second = record("b", 3);
        assert_eq!(second.shared.wait_driving(None), State::Resolved);
        assert_eq!(first.state(), State::Resolved);
        assert_eq!(*order.lock().unwrap(), ["a", "b", "a", "b", "b"]);
    }

    #[test]
    fn a_step_may_queue_work_and_may_not_drive_it() {
        let mut inner = None;
        let outer = queued(move || match &inner {
            None => {
                // A round is under way, so this one cannot start another.
                assert!(!pump());
                inner = Some(queued(counting(2)));
                Poll::Pending
            }
            Some(future) => match future.state() {
                State::Pending => Poll::Pending,
                _ => Poll::Ready(future.take()),
            },
        });
        assert_eq!(outer.shared.wait_driving(None), State::Resolved);
        assert_eq!(outer.take(), Ok(2));
    }
}
