//! The library's worker threads: a `krio` cluster that stepped work is
//! submitted to where the build has threads.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use krio_core::{Clock, Task};
use krio_parallel::{Cluster, Park};
use krio_runtime::{AgentId, ParallelScheduler};

// A library is one of several in a process, each with a pool of its own.
const MOST_WORKERS: usize = 4;

/// Parks an idle worker on a condition variable.
///
/// One for every slot: a wake reaches all parked workers, and those it was
/// not meant for look at their own slot and park again.
struct Blocking {
    lock: Mutex<()>,
    woken: Condvar,
}

impl Park for Blocking {
    fn park(&self, slot: &AtomicU32, expected: u32) {
        let guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        // Read under the lock a waker takes, so a wake that lands after this
        // read finds the worker already waiting.
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

type Pool = Cluster<Blocking, Elapsed>;

// `None` if no worker could be started.
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
        let cluster = Arc::new(Cluster::new(
            workers as u32,
            parker,
            Elapsed(Instant::now()),
        ));
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

/// Gives `task` to the workers. With none, it is dropped here, which
/// cancels the future it would have settled.
pub(crate) fn submit(task: Box<dyn Task + Send>) {
    if let Some(pool) = pool() {
        pool.spawn(task);
    }
}
