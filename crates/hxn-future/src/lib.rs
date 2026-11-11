//! One result from an asynchronous native operation, the same on every
//! runtime hxn supports.
//!
//! [`pending`] makes a pair. The library keeps the [`Completer`] and settles
//! it from any thread; the host is given the [`Future`], erased to an
//! [`AnyFuture`] inside a handle, and asks it for its state, waits on it, and
//! takes the result. `hxn.Future` is the Haxe side.
//!
//! Most libraries never touch a completer. [`spawn`] runs an `async` block,
//! [`stepped`] runs work written as steps, and [`spawn_blocking`] runs a
//! closure that blocks; each returns the future its result settles. A
//! [`Future`] can itself be awaited, so one operation can be built on others.
//!
//! [`channel`] is the same idea for results that arrive more than once: a
//! [`Stream`] the library sends to and the host takes from, item by item.
//! [`signal()`] is for a value that changes, where only the latest matters.
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
//! There [`spawn_blocking`] runs its work before it returns, and [`spawn`]
//! and [`stepped`] queue theirs on the calling thread, to be advanced by
//! whoever waits: [`Future::wait`] runs the queue for a short slice and
//! returns, pending or not, so the host can yield and ask again. The queue
//! is a `krio` scheduler; [`pump`] runs one round of it directly.

#![forbid(unsafe_code)]

use std::any::Any;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use hxn_abi::{Fault, catch};
use krio_core::{Suspension, Task};

pub use hxn_abi::{Failure, Status};

mod local;
pub mod outbox;
mod polled;
#[cfg(not(all(target_family = "wasm", not(target_feature = "atomics"))))]
mod pool;
mod signal;
mod stream;

pub use local::pump;
pub use signal::{AnySignal, CLOSED, Change, Signal, Source, signal};
pub use stream::{AnyStream, Flow, Sender, Stream, channel};

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
    // With whoever is awaiting it from `async` code.
    Pending(Vec<Waker>),
    Settled(Result<T, Failure>),
    // The result was taken; what it was is still its state.
    Taken(State),
}

impl<T> Slot<T> {
    fn state(&self) -> State {
        match self {
            Self::Pending(_) => State::Pending,
            Self::Settled(Ok(_)) => State::Resolved,
            Self::Settled(Err(_)) => State::Rejected,
            Self::Taken(state) => *state,
        }
    }
}

struct Shared<T> {
    slot: Mutex<Slot<T>>,
    settled: Condvar,
    tag: Arc<outbox::Tag>,
}

impl<T> Shared<T> {
    // No code of the library's runs under the lock, so a poisoned one still
    // holds a whole slot.
    fn lock(&self) -> MutexGuard<'_, Slot<T>> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn settle(&self, result: Result<T, Failure>) -> bool {
        let mut slot = self.lock();
        let Slot::Pending(wakers) = &mut *slot else {
            // Unlocked before `result` is dropped: its `Drop` is the library's.
            drop(slot);
            return false;
        };
        let wakers = std::mem::take(wakers);
        *slot = Slot::Settled(result);
        drop(slot);
        self.settled.notify_all();
        outbox::publish(&self.tag);
        for waker in wakers {
            waker.wake();
        }
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
        local::drive(timeout, || self.state() != State::Pending);
        self.state()
    }

    fn take(&self) -> Result<T, Failure> {
        let mut slot = self.lock();
        let state = slot.state();
        match std::mem::replace(&mut *slot, Slot::Taken(state)) {
            Slot::Settled(result) => result,
            Slot::Pending(wakers) => {
                *slot = Slot::Pending(wakers);
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

/// Awaiting one gives its result, which no one else can then take.
impl<T> std::future::Future for Future<T> {
    type Output = Result<T, Failure>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = self.shared.lock();
        if let Slot::Pending(wakers) = &mut *slot {
            if !wakers.iter().any(|waker| waker.will_wake(context.waker())) {
                wakers.push(context.waker().clone());
            }
            return Poll::Pending;
        }
        drop(slot);
        Poll::Ready(self.shared.take())
    }
}

/// A pending future and what settles it.
pub fn pending<T>() -> (Completer<T>, Future<T>) {
    let shared = Arc::new(Shared {
        slot: Mutex::new(Slot::Pending(Vec::new())),
        settled: Condvar::new(),
        tag: outbox::Tag::new(),
    });
    let owner = Arc::new(Owner {
        shared: shared.clone(),
    });
    (Completer { owner }, Future { shared })
}

/// Runs `work` and settles a future with its output. A panic rejects it.
///
/// It is polled once before this returns, so work that never waits is done
/// by then. After that it runs on the library's workers, or without threads
/// on this thread's queue, and only when something it awaits wakes it: an
/// awaited [`Future`] does, when it is settled.
pub fn spawn<F, T, E>(work: F) -> Future<T>
where
    F: std::future::Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Into<Failure> + 'static,
{
    let (completer, future) = pending();
    let mut task = polled::Polled::new(work, completer);
    if !task.step().is_done() {
        hand_over(Box::new(task.scheduled()));
    }
    future
}

/// Runs `work`, which may block, on a thread of its own and settles a future
/// with what it returns. A panic rejects it. Without threads `work` runs
/// here, and the future is settled when this returns.
pub fn spawn_blocking<T, E>(work: impl FnOnce() -> Result<T, E> + Send + 'static) -> Future<T>
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
        local::progressed();
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
    fn watch(&self) -> i32;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Send + 'static> Erased for Shared<T> {
    fn state(&self) -> State {
        Shared::state(self)
    }

    fn wait(&self, timeout: Option<Duration>) -> State {
        Shared::wait(self, timeout)
    }

    fn watch(&self) -> i32 {
        self.tag.watch()
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

    /// Has the future noted in the [`outbox`] when it is settled, and returns
    /// the id it is noted under.
    pub fn watch(&self) -> i32 {
        self.0.watch()
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
    fn spawn_blocking_settles_with_the_result_or_the_panic() {
        let done = spawn_blocking(|| Ok::<_, Failure>(6 * 7));
        assert_eq!(done.wait(None), State::Resolved);
        assert_eq!(done.take(), Ok(42));

        let failed = spawn_blocking(|| "x".parse::<i32>());
        assert_eq!(failed.wait(None), State::Rejected);
        assert_eq!(failed.take().unwrap_err().status, Status::ERROR);

        let panicked = spawn_blocking(|| -> Result<i32, Failure> { panic!("in the worker") });
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

    #[test]
    fn async_work_that_never_waits_is_done_when_spawn_returns() {
        let done = spawn(async { Ok::<_, Failure>(6 * 7) });
        assert_eq!(done.state(), State::Resolved);
        assert_eq!(done.take(), Ok(42));
        let failed = spawn(async { "x".parse::<i32>() });
        assert_eq!(failed.state(), State::Rejected);
    }

    #[test]
    fn async_work_resumes_when_what_it_awaits_is_settled() {
        let (completer, inner) = pending::<i32>();
        let outer = spawn(async move { Ok::<_, Failure>(inner.await? + 1) });
        assert_eq!(outer.wait(Some(Duration::from_millis(20))), State::Pending);
        completer.resolve(41);
        assert_eq!(outer.wait(None), State::Resolved);
        assert_eq!(outer.take(), Ok(42));
    }

    #[test]
    fn async_work_builds_on_the_other_kinds_and_passes_failures_on() {
        let total = spawn(async {
            let blocking = spawn_blocking(|| Ok::<_, Failure>(40)).await?;
            let stepped = stepped(counting(2)).await?;
            Ok::<_, Failure>(blocking + stepped)
        });
        assert_eq!(total.wait(None), State::Resolved);
        assert_eq!(total.take(), Ok(42));

        let (completer, inner) = pending::<i32>();
        let outer = spawn(inner);
        completer.reject(Failure::new(Status(9), "from below"));
        assert_eq!(outer.wait(None), State::Rejected);
        assert_eq!(outer.take(), Err(Failure::new(Status(9), "from below")));

        let (completer, gate) = pending::<bool>();
        let panicked = spawn(async move {
            if gate.await? {
                panic!("after waking");
            }
            Ok::<_, Failure>(0)
        });
        completer.resolve(true);
        assert_eq!(panicked.wait(None), State::Rejected);
        assert_eq!(
            panicked.take(),
            Err(Failure::new(Status::PANIC, "after waking"))
        );
        let after = spawn(stepped(counting(3)));
        assert_eq!(after.wait(None), State::Resolved);
    }

    #[test]
    fn every_wake_reaches_its_task() {
        for _ in 0..20 {
            let (completers, outers): (Vec<_>, Vec<_>) = (0..200)
                .map(|extra| {
                    let (completer, inner) = pending::<i32>();
                    let outer = spawn(async move { Ok::<_, Failure>(inner.await? + extra) });
                    (completer, outer)
                })
                .unzip();
            std::thread::scope(|scope| {
                for chunk in completers.chunks(50) {
                    scope.spawn(move || {
                        for completer in chunk {
                            completer.resolve(1);
                        }
                    });
                }
            });
            for (extra, outer) in outers.iter().enumerate() {
                // A lost wake would leave this pending for good.
                assert_eq!(outer.wait(Some(Duration::from_secs(30))), State::Resolved);
                assert_eq!(outer.take(), Ok(1 + extra as i32));
            }
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

    fn queued_async<T: Send + 'static>(
        work: impl std::future::Future<Output = Result<T, Failure>> + 'static,
    ) -> Future<T> {
        let (completer, future) = pending();
        local::queue(Box::new(polled::Polled::new(work, completer)));
        future
    }

    #[test]
    fn queued_async_work_is_polled_only_when_woken() {
        use std::future::Future as _;

        let polls = Arc::new(AtomicUsize::new(0));
        let counted = polls.clone();
        let (completer, inner) = pending::<i32>();
        let mut inner = Box::pin(inner);
        let outer = queued_async(std::future::poll_fn(move |context| {
            counted.fetch_add(1, Ordering::SeqCst);
            inner.as_mut().poll(context)
        }));
        for _ in 0..5 {
            assert_eq!(
                outer.shared.wait_driving(Some(Duration::ZERO)),
                State::Pending
            );
        }
        assert_eq!(outer.shared.wait_driving(None), State::Pending);
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        completer.resolve(9);
        assert_eq!(outer.shared.wait_driving(None), State::Resolved);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(outer.take(), Ok(9));
    }

    #[test]
    fn queued_async_and_stepped_work_share_the_queue() {
        let sum = queued_async(async {
            let first = queued(counting(3)).await?;
            let second = queued_async(queued(counting(4))).await?;
            Ok(first + second)
        });
        assert_eq!(sum.shared.wait_driving(None), State::Resolved);
        assert_eq!(sum.take(), Ok(7));
        assert!(!pump());
    }

    #[test]
    fn waiting_on_a_stream_drives_the_work_that_feeds_it() {
        let (sender, stream) = channel();
        let mut next = 0;
        drop(queued(move || {
            next += 1;
            sender.send(next);
            if next == 3 {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }));
        let mut seen = Vec::new();
        while stream.wait_driving(None) == Flow::Ready {
            seen.push(stream.take().unwrap().unwrap());
        }
        assert_eq!(seen, [1, 2, 3]);
        assert_eq!(stream.flow(), Flow::Ended);
    }

    // The outbox is the library's one queue, so everything about it is
    // checked in one test: two would take each other's entries.
    #[test]
    fn the_host_is_told_what_happened_in_the_order_it_did() {
        let (unwatched, _future) = pending::<i32>();
        unwatched.resolve(1);

        let (completer, future) = pending::<i32>();
        let future = AnyFuture::from(future);
        let (sender, stream) = channel::<i32>();
        let stream = AnyStream::from(stream);
        let (source, value) = signal(0);
        let value = AnySignal::from(value);
        let (future, stream, value) = (future.watch(), stream.watch(), value.watch());
        assert!(!outbox::wait(Some(Duration::ZERO)));
        assert_eq!(outbox::next(), -1);

        sender.send(1);
        source.set(1);
        completer.resolve(5);
        sender.send(2);
        source.set(2);
        assert!(outbox::wait(None));
        // Each once, however often it changed, in the order they first did.
        let taken = [
            outbox::next(),
            outbox::next(),
            outbox::next(),
            outbox::next(),
        ];
        assert_eq!(taken, [stream, value, future, -1]);

        // Taken, so the next change is noted again.
        sender.send(3);
        drop(source);
        assert_eq!([outbox::next(), outbox::next()], [stream, value]);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                sender.close();
            });
            assert!(!outbox::wait_millis(1));
            assert!(outbox::wait_millis(-1));
        });
        assert_eq!([outbox::next(), outbox::next()], [stream, -1]);
    }
}
