//! An `async fn` method on a shared interface: the running task and any
//! number of Haxe objects hold references to the same value.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use hxn_future::{Failure, spawn_blocking};

/// The total size of several downloads. A shared interface: a download holds
/// its own reference while it runs, independent of the Haxe object that
/// started it.
#[hxn::interface(shared)]
#[derive(Default)]
pub struct Tally {
    bytes: Arc<AtomicI32>,
}

// Simulates a network read: it sleeps briefly, on a separate thread where
// the build has threads.
async fn chunk() {
    let wait = spawn_blocking(|| {
        std::thread::sleep(Duration::from_millis(1));
        Ok::<_, Failure>(())
    });
    let _ = wait.await;
}

#[hxn::methods]
impl Tally {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetches `size` bytes in chunks and adds them to the tally. In Haxe
    /// this returns a future that resolves to the returned text or rejects
    /// with the error.
    pub async fn fetch(&self, name: &str, size: i32) -> Result<String, String> {
        if size < 0 {
            return Err(format!("{name} has a negative size"));
        }
        let mut left = size;
        while left > 0 {
            let read = left.min(1024);
            chunk().await;
            self.bytes.fetch_add(read, Ordering::SeqCst);
            left -= read;
        }
        Ok(format!("{name}: {size} bytes"))
    }

    /// The number of bytes fetched so far, as an atomic cell that Haxe reads
    /// without a native call.
    pub fn bytes(&self) -> Arc<AtomicI32> {
        Arc::clone(&self.bytes)
    }
}
