use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

/// A boolean flag both sides set and read, stored as an `i32` that is 0 or 1.
///
/// A host's atomic integer and Rust's `AtomicI32` have the same four-byte
/// layout, so each side uses its own atomic operations on the other's cell,
/// with no call across the boundary. Rust's `AtomicBool` is one byte and a
/// host has no one-byte atomic, so a flag passed across the boundary has this
/// type.
///
/// `&AtomicFlag` is a cell borrowed for the duration of a call.
/// `Arc<AtomicFlag>` is a cell the library owns and shares with the host.
/// Integers are passed as `&AtomicI32` and `Arc<AtomicI32>` directly.
#[repr(transparent)]
#[derive(Default)]
pub struct AtomicFlag(AtomicI32);

impl AtomicFlag {
    pub const fn new(set: bool) -> Self {
        Self(AtomicI32::new(set as i32))
    }

    pub fn load(&self) -> bool {
        self.0.load(Ordering::SeqCst) != 0
    }

    pub fn store(&self, set: bool) {
        self.0.store(i32::from(set), Ordering::SeqCst);
    }

    /// Sets the flag and returns its previous value.
    pub fn swap(&self, set: bool) -> bool {
        self.0.swap(i32::from(set), Ordering::SeqCst) != 0
    }

    /// Views a cell borrowed from a host as a flag.
    ///
    /// # Safety
    /// The same as [`AtomicI32::from_ptr`]: `cell` points to a live, aligned
    /// `i32` for all of `'a`, and every access to it during `'a` is atomic.
    pub unsafe fn from_ptr<'a>(cell: *mut i32) -> &'a Self {
        // `Self` is `repr(transparent)` over the type `from_ptr` returns.
        unsafe { &*(AtomicI32::from_ptr(cell) as *const AtomicI32).cast::<Self>() }
    }

    /// Converts to the underlying `AtomicI32`, the type a host receives.
    pub fn into_int(self: Arc<Self>) -> Arc<AtomicI32> {
        // `repr(transparent)`: the allocation has exactly the layout of an
        // `AtomicI32`.
        unsafe { Arc::from_raw(Arc::into_raw(self).cast::<AtomicI32>()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flag_is_an_int_that_is_zero_or_one() {
        let flag = AtomicFlag::new(false);
        assert!(!flag.load());
        assert!(!flag.swap(true));
        assert!(flag.load());
        flag.store(false);
        assert!(!flag.load());
        assert_eq!(size_of::<AtomicFlag>(), size_of::<i32>());
    }

    #[test]
    fn a_lent_cell_and_a_shared_one_are_the_same_cell() {
        let mut cell = 0i32;
        let flag = unsafe { AtomicFlag::from_ptr(&raw mut cell) };
        flag.store(true);
        assert_eq!(cell, 1);

        let shared = Arc::new(AtomicFlag::new(true));
        let kept = shared.clone();
        let int = shared.into_int();
        assert_eq!(int.load(Ordering::SeqCst), 1);
        int.store(0, Ordering::SeqCst);
        assert!(!kept.load());
        assert_eq!(Arc::strong_count(&int), 2);
    }
}
