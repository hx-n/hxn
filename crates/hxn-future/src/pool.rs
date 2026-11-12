//! The library's worker thread pool: a `krio` cluster that runs stepped work
//! when the build has threads.

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use krio_core::{Clock, Task, TaskId};
use krio_parallel::{Cluster, Park, TaskObserver};
use krio_runtime::{AgentId, ParallelScheduler};

// A process may load several libraries, each with its own pool.
const MOST_WORKERS: usize = 4;

/// Parks an idle worker on a condition variable.
///
/// One instance serves every slot: a wake notifies all parked workers, and
/// each worker the wake was not meant for checks its own slot and parks again.
struct Blocking {
    lock: Mutex<()>,
    woken: Condvar,
}

impl Park for Blocking {
    fn park(&self, slot: &AtomicU32, expected: u32) {
        let guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        // Read while holding the lock a waker takes, so a wake that happens
        // after this read finds the worker already waiting.
        if slot.load(Ordering::Acquire) == expected {
            drop(self.woken.wait(guard));
        }
    }

    fn unpark(&self, _slot: &AtomicU32) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.woken.notify_all();
    }
}

struct Elapsed(Instant);

impl Clock for Elapsed {
    fn now_ms(&self) -> f64 {
        self.0.elapsed().as_secs_f64() * 1000.0
    }
}

thread_local! {
    static STEPPING: Cell<u64> = const { Cell::new(0) };
}

// Records the id of the task being stepped, so a task can learn its own id,
// which is what a wake targets.
struct Stepping;

impl TaskObserver for Stepping {
    fn on_step_begin(&self, task: TaskId, _agent: AgentId) {
        STEPPING.set(task.0);
    }

    fn on_step_end(&self, _task: TaskId, _agent: AgentId) {
        STEPPING.set(0);
    }
}

/// Returns the id of the task a worker is stepping on this thread, or zero.
pub(crate) fn current() -> u64 {
    STEPPING.get()
}

/// Reschedules a task that returned pending.
pub(crate) fn wake(task: u64) {
    if task != 0
        && let Some(pool) = pool()
    {
        pool.wake(TaskId(task));
    }
}

type Pool = Cluster<Blocking, Elapsed>;

// `None` if no worker thread could be started.
fn pool() -> Option<&'static Arc<Pool>> {
    static POOL: OnceLock<Option<Arc<Pool>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(MOST_WORKERS);
        let parker = Blocking {
            lock: Mutex::new(()),
            woken: Condvar::new(),
        };
        let mut cluster = Cluster::new(workers as u32, parker, Elapsed(Instant::now()));
        cluster.set_observer(Arc::new(Stepping));
        let cluster = Arc::new(cluster);
        let started = (0..workers as u32)
            .filter(|&agent| {
                let cluster = cluster.clone();
                std::thread::Builder::new()
                    .name(String::from("hxn-worker"))
                    .spawn(move || cluster.run(AgentId(agent)))
                    .is_ok()
            })
            .count();
        (started > 0).then_some(cluster)
    })
    .as_ref()
}

/// Submits `task` to the workers. If there are none, it is dropped here,
/// which cancels the future it would have settled.
pub(crate) fn submit(task: Box<dyn Task + Send>) {
    if let Some(pool) = pool() {
        pool.spawn(task);
    }
}
