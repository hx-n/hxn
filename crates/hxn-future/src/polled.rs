//! Runs an `async` block as a task: it is polled when stepped, and stepped
//! again only after something it awaits has woken it.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use hxn_abi::{Failure, catch};
use krio_core::{Suspension, Task};

use crate::{Completer, local};

// Returned pending; a wake must reschedule it.
const IDLE: u8 = 0;
// Waiting to be polled.
const SCHEDULED: u8 = 1;
const POLLING: u8 = 2;
// Woken while a poll was running. That poll may not have seen the change
// that caused the wake, so the task is polled again.
const NOTIFIED: u8 = 3;
const DONE: u8 = 4;

// A step writes `task` and then reads `state`; a wake writes `state` and
// then reads `task`. Every access is sequentially consistent, so at least one
// of the two sees the other's write. With weaker orderings both could read
// the old value, and the task would park with no worker told to wake it.
struct Wakeup {
    state: AtomicU8,
    // The task's id in the worker pool, or zero on a thread-local queue,
    // which steps every task each round and needs no notification.
    task: AtomicU64,
}

impl Wake for Wakeup {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        loop {
            let (seen, next) = match self.state.load(Ordering::SeqCst) {
                IDLE => (IDLE, SCHEDULED),
                POLLING => (POLLING, NOTIFIED),
                _ => return,
            };
            if self
                .state
                .compare_exchange(seen, next, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                // Only an idle task needs rescheduling, so the workers are
                // notified once each time it parks.
                if seen == IDLE {
                    reschedule(self.task.load(Ordering::SeqCst));
                }
                return;
            }
        }
    }
}

#[cfg(not(all(target_family = "wasm", not(target_feature = "atomics"))))]
use crate::pool::{current, wake as reschedule};

#[cfg(all(target_family = "wasm", not(target_feature = "atomics")))]
fn current() -> u64 {
    0
}

#[cfg(all(target_family = "wasm", not(target_feature = "atomics")))]
fn reschedule(_task: u64) {}

pub(crate) struct Polled<F, T> {
    work: Pin<Box<F>>,
    completer: Completer<T>,
    wakeup: Arc<Wakeup>,
    waker: Waker,
    // Set once the task is owned by a scheduler. Before that the thread
    // stepping it may be a worker in the middle of another task.
    scheduled: bool,
}

impl<F, T> Polled<F, T> {
    pub(crate) fn new(work: F, completer: Completer<T>) -> Self {
        let wakeup = Arc::new(Wakeup {
            state: AtomicU8::new(SCHEDULED),
            task: AtomicU64::new(0),
        });
        Self {
            work: Box::pin(work),
            completer,
            waker: Waker::from(wakeup.clone()),
            wakeup,
            scheduled: false,
        }
    }

    pub(crate) fn scheduled(mut self) -> Self {
        self.scheduled = true;
        self
    }
}

impl<F, T, E> Task for Polled<F, T>
where
    F: Future<Output = Result<T, E>>,
    E: Into<Failure>,
{
    fn step(&mut self) -> Suspension {
        // Stored first, so a wake that finds the task idle knows which task
        // to reschedule.
        let task = if self.scheduled { current() } else { 0 };
        self.wakeup.task.store(task, Ordering::SeqCst);
        if self
            .wakeup
            .state
            .compare_exchange(SCHEDULED, POLLING, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Suspension::Pending;
        }
        local::progressed();
        let mut context = Context::from_waker(&self.waker);
        let polled = catch(|| Ok(self.work.as_mut().poll(&mut context)));
        let result = match polled {
            Ok(Poll::Pending) => {
                let woken = self
                    .wakeup
                    .state
                    .compare_exchange(POLLING, IDLE, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err();
                if woken {
                    self.wakeup.state.store(SCHEDULED, Ordering::SeqCst);
                    return Suspension::Yielded;
                }
                return Suspension::Pending;
            }
            Ok(Poll::Ready(result)) => result.map_err(Into::into),
            Err(panic) => Err(panic),
        };
        self.wakeup.state.store(DONE, Ordering::SeqCst);
        self.completer.settle(result);
        Suspension::Completed
    }
}
