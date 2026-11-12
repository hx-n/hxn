//! Work written as an `async fn`, on a value that the work and any number
//! of Haxe objects share.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use hxn_future::{Failure, spawn_blocking};

/// What several downloads add up to. Shared: a download holds a share of it
/// while it runs, whatever becomes of the object that started it.
#[hxn::interface(shared)]
#[derive(Default)]
pub struct Tally {
    bytes: Arc<AtomicI32>,
}

// Stands in for a read from the network: it takes a moment, on a thread of
// its own where there are threads.
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

    /// Fetches `size` bytes a chunk at a time and adds them to the tally.
    /// Haxe gets a future of the text this returns, or of its failure.
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

    /// The bytes fetched so far, as a cell Haxe reads without a call.
    pub fn bytes(&self) -> Arc<AtomicI32> {
        Arc::clone(&self.bytes)
    }
}
