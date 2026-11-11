use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

/// A flag both sides set and read: an int that is 0 or 1.
///
/// A host's atomic integer and Rust's `AtomicI32` are the same four bytes, so
/// each side works on the other's cell with its own operations and no call
/// between them. A host has no atomic of one byte, which Rust's `AtomicBool`
/// is, so a flag that crosses is this.
///
/// Borrowed as `&AtomicFlag` it is a cell lent for a call; owned as
/// `Arc<AtomicFlag>` it is one the library keeps and hands to the host.
/// Integers cross as `&AtomicI32` and `Arc<AtomicI32>` themselves.
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

    /// Sets the flag and returns what it was.
    pub fn swap(&self, set: bool) -> bool {
        self.0.swap(i32::from(set), Ordering::SeqCst) != 0
    }

    /// The cell a host lent, as a flag.
    ///
    /// # Safety
    /// As [`AtomicI32::from_ptr`]: `cell` is a live, aligned `i32` for `'a`
    /// that is only reached atomically meanwhile.
    pub unsafe fn from_ptr<'a>(cell: *mut i32) -> &'a Self {
        // `repr(transparent)` over the type `from_ptr` gives.
        unsafe { &*(AtomicI32::from_ptr(cell) as *const AtomicI32).cast::<Self>() }
    }

    /// The same cell as the integer it is, which is how it is handed to a host.
    pub fn into_int(self: Arc<Self>) -> Arc<AtomicI32> {
        // `repr(transparent)`: the allocation is an `AtomicI32`'s, exactly.
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
