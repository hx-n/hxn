//! An `async` block as a task: polled when it is stepped, and stepped again
//! only once something it awaits has woken it.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use hxn_abi::{Failure, catch};
use krio_core::{Suspension, Task};

use crate::{Completer, local};

// Returned pending; a wake has to put it back in front of a scheduler.
const IDLE: u8 = 0;
// Due a poll.
const SCHEDULED: u8 = 1;
const POLLING: u8 = 2;
// Woken during a poll, which may have missed why.
const NOTIFIED: u8 = 3;
const DONE: u8 = 4;

struct Wakeup {
    state: AtomicU8,
    // What the workers know the task as, or zero on a thread's own queue,
    // which offers every task a step each round and needs no telling.
    task: AtomicU64,
}

impl Wake for Wakeup {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        loop {
            let (seen, next) = match self.state.load(Ordering::Acquire) {
                IDLE => (IDLE, SCHEDULED),
                POLLING => (POLLING, NOTIFIED),
                _ => return,
            };
            if self
                .state
                .compare_exchange(seen, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // Only an idle task is waiting to be woken, so the workers
                // are told once for each time it parks.
                if seen == IDLE {
                    reschedule(self.task.load(Ordering::Acquire));
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
    // Set once a scheduler has it. Before that the thread stepping it may be
    // a worker in the middle of some other task.
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
        // First, so a wake that finds the task idle knows whom to tell.
        let task = if self.scheduled { current() } else { 0 };
        self.wakeup.task.store(task, Ordering::Release);
        if self
            .wakeup
            .state
            .compare_exchange(SCHEDULED, POLLING, Ordering::AcqRel, Ordering::Acquire)
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
                    .compare_exchange(POLLING, IDLE, Ordering::AcqRel, Ordering::Acquire)
                    .is_err();
                if woken {
                    self.wakeup.state.store(SCHEDULED, Ordering::Release);
                    return Suspension::Yielded;
                }
                return Suspension::Pending;
            }
            Ok(Poll::Ready(result)) => result.map_err(Into::into),
            Err(panic) => Err(panic),
        };
        self.wakeup.state.store(DONE, Ordering::Release);
        self.completer.settle(result);
        Suspension::Completed
    }
}
