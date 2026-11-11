//! The calling thread's queue of stepped work, for a runtime with no other
//! thread to do it on.

use std::cell::RefCell;
use std::time::Duration;

use krio_core::Task;
use krio_runtime::{RoundRobin, Scheduler};

/// How long a wait keeps this thread before handing it back to the host.
pub(crate) const SLICE: Duration = Duration::from_millis(4);

thread_local! {
    // Tasks arrive here and move to the scheduler at the start of a round, so
    // a step may queue more work while the scheduler is running it.
    static ARRIVED: RefCell<Vec<Box<dyn Task>>> = const { RefCell::new(Vec::new()) };
    static SCHEDULER: RefCell<RoundRobin> = RefCell::new(RoundRobin::new());
}

#[cfg(any(test, all(target_family = "wasm", not(target_feature = "atomics"))))]
pub(crate) fn queue(task: Box<dyn Task>) {
    ARRIVED.with(|arrived| arrived.borrow_mut().push(task));
}

/// Runs one round of this thread's queued work: every task takes one step.
/// Returns whether any is left.
///
/// Nothing is queued where the library has threads, so there this returns
/// `false`. Called from inside a step it also returns `false`, the round
/// already under way being the one that will reach the rest.
pub fn pump() -> bool {
    SCHEDULER.with(|scheduler| {
        let Ok(mut scheduler) = scheduler.try_borrow_mut() else {
            return false;
        };
        for task in ARRIVED.with(|arrived| std::mem::take(&mut *arrived.borrow_mut())) {
            scheduler.spawn(task);
        }
        let left = scheduler.tick();
        left || ARRIVED.with(|arrived| !arrived.borrow().is_empty())
    })
}
