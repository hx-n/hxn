//! A thread-local queue of stepped work, for a runtime with no other thread
//! to run it on.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use krio_core::Task;
use krio_runtime::{RoundRobin, Scheduler};

/// How long a wait keeps this thread before returning it to the host.
pub(crate) const SLICE: Duration = Duration::from_millis(4);

/// Runs rounds until `done` returns true, or until continuing is pointless:
/// the time slice is used up, no task is left, or every remaining task is
/// blocked and would make no progress if stepped again.
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
    // New tasks are added here and moved to the scheduler at the start of a
    // round, so a step can queue more work while the scheduler is running it.
    static ARRIVED: RefCell<Vec<Box<dyn Task>>> = const { RefCell::new(Vec::new()) };
    static SCHEDULER: RefCell<RoundRobin> = RefCell::new(RoundRobin::new());
    static PROGRESSED: Cell<bool> = const { Cell::new(false) };
}

/// Called by a task whose step made progress, as opposed to finding it was
/// still blocked.
pub(crate) fn progressed() {
    PROGRESSED.set(true);
}

pub(crate) struct Round {
    /// Whether any task is left.
    pub left: bool,
    /// Whether any task made progress.
    pub progressed: bool,
}

#[cfg(any(test, all(target_family = "wasm", not(target_feature = "atomics"))))]
pub(crate) fn queue(task: Box<dyn Task>) {
    ARRIVED.with(|arrived| arrived.borrow_mut().push(task));
}

/// Runs one round of this thread's queued work: every task takes one step.
/// Returns whether any task is left.
///
/// Where the library has threads nothing is queued, so this returns `false`.
/// Called from inside a step it also returns `false`, because the round
/// already running will step the remaining tasks.
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
