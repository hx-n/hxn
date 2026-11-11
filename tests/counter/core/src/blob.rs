//! A value that says how much it holds, kept in a file of its own: the
//! generator reads every module of a library.

use std::sync::atomic::{AtomicI32, Ordering};

use hxn::{Weight, collect_soon};

static BLOBS: AtomicI32 = AtomicI32::new(0);

/// Bytes the collector cannot see, which it is told of: to the host, one of
/// these is a few words whatever its size here.
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

    /// Another blob with the same bytes, as heavy as this one.
    pub fn copy(&self) -> Blob {
        BLOBS.fetch_add(1, Ordering::SeqCst);
        Blob {
            bytes: self.bytes.clone(),
            weight: self.weight.clone(),
        }
    }

    /// How many blobs exist.
    pub fn live() -> i32 {
        BLOBS.load(Ordering::SeqCst)
    }

    /// Asks the host to collect at its next chance.
    pub fn crowded() {
        collect_soon();
    }
}

impl Drop for Blob {
    fn drop(&mut self) {
        BLOBS.fetch_sub(1, Ordering::SeqCst);
    }
}
