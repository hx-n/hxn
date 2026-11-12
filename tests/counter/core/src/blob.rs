//! A type that reports how much memory it holds. It is defined in a separate
//! file: the generator reads every module of a library.

use std::sync::atomic::{AtomicI32, Ordering};

use hxn::{Weight, collect_soon};

static BLOBS: AtomicI32 = AtomicI32::new(0);

/// A byte buffer the garbage collector cannot see, whose size is reported
/// to it. To the host, a blob is a few words regardless of how much native
/// memory it holds.
#[hxn::interface(weighed)]
pub struct Blob {
    bytes: Vec<u8>,
    weight: Weight,
}

fn blob_size(size: i32) -> Result<usize, &'static str> {
    usize::try_from(size).map_err(|_| "a blob has no negative size")
}

#[hxn::methods]
impl Blob {
    pub fn new(size: i32) -> Result<Self, &'static str> {
        let size = blob_size(size)?;
        BLOBS.fetch_add(1, Ordering::SeqCst);
        Ok(Self {
            bytes: vec![0; size],
            weight: Weight::new(size),
        })
    }

    pub fn size(&self) -> i32 {
        i32::try_from(self.bytes.len()).unwrap_or(i32::MAX)
    }

    pub fn resize(&mut self, size: i32) -> Result<(), &'static str> {
        let size = blob_size(size)?;
        self.bytes.resize(size, 0);
        self.weight.set(size);
        Ok(())
    }

    /// A copy of this blob with the same bytes and the same reported size.
    pub fn copy(&self) -> Blob {
        BLOBS.fetch_add(1, Ordering::SeqCst);
        Blob {
            bytes: self.bytes.clone(),
            weight: self.weight.clone(),
        }
    }

    /// The number of live blobs.
    pub fn live() -> i32 {
        BLOBS.load(Ordering::SeqCst)
    }

    /// Asks the host to run a garbage collection at its next opportunity.
    pub fn crowded() {
        collect_soon();
    }
}

impl Drop for Blob {
    fn drop(&mut self) {
        BLOBS.fetch_sub(1, Ordering::SeqCst);
    }
}
