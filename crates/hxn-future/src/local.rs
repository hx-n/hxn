//! The calling thread's queue of stepped work, for a runtime with no other
//! thread to do it on.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use krio_core::Task;
use krio_runtime::{RoundRobin, Scheduler};

/// How long a wait keeps this thread before handing it back to the host.
pub(crate) const SLICE: Duration = Duration::from_millis(4);

/// Runs rounds until `done`, or until there is no point going on: the slice
/// is used up, nothing is left, or everything left is waiting on something
/// and would get nowhere by being asked again.
pub(crate) fn drive(timeout: Option<Duration>, done: impl Fn() -> bool) {
    let slice = timeout.map_or(SLICE, |timeout| timeout.min(SLICE));
    let started = Instant::now();
    while !done() {
        let round = round();
        if !round.left || !round.progressed || started.elapsed() >= slice {
            return;
        }
    }
}

thread_local! {
    // Tasks arrive here and move to the scheduler at the start of a round, so
    // a step may queue more work while the scheduler is running it.
    static ARRIVED: RefCell<Vec<Box<dyn Task>>> = const { RefCell::new(Vec::new()) };
    static SCHEDULER: RefCell<RoundRobin> = RefCell::new(RoundRobin::new());
    static PROGRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Called by a task that did something with its step, as opposed to finding
/// it was still waiting.
pub(crate) fn progressed() {
    PROGRESSED.set(true);
}

pub(crate) struct Round {
    /// Whether any task is left.
    pub left: bool,
    /// Whether any task did anything.
    pub progressed: bool,
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
    round().left
}

pub(crate) fn round() -> Round {
    SCHEDULER.with(|scheduler| {
        let Ok(mut scheduler) = scheduler.try_borrow_mut() else {
            return Round {
                left: false,
                progressed: false,
            };
        };
        for task in ARRIVED.with(|arrived| std::mem::take(&mut *arrived.borrow_mut())) {
            scheduler.spawn(task);
        }
        PROGRESSED.set(false);
        let left = scheduler.tick();
        Round {
            left: left || ARRIVED.with(|arrived| !arrived.borrow().is_empty()),
            progressed: PROGRESSED.get(),
        }
    })
}
