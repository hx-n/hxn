//! Accounting for native memory the host's garbage collector cannot see.
//!
//! A garbage collector schedules collections by the amount of memory it
//! manages. A handle takes a few words of that memory regardless of how much
//! the value behind it holds, so a host can keep creating values without ever
//! triggering a collection of the ones it dropped. A library declares how many
//! bytes each value holds with a [`Weight`]. The binding reads the total and
//! triggers a collection when it has grown.
//!
//! The total is per library, because this crate is linked into each library
//! separately. Nothing here calls the host: the host polls.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static HELD: AtomicUsize = AtomicUsize::new(0);
static WANTED: AtomicBool = AtomicBool::new(false);

/// The number of bytes a value holds that the garbage collector cannot see.
/// Stored as a field of the value, so the bytes are counted exactly while the
/// value is alive.
#[derive(Debug)]
pub struct Weight(usize);

impl Weight {
    pub fn new(bytes: usize) -> Self {
        HELD.fetch_add(bytes, Ordering::Relaxed);
        Self(bytes)
    }

    pub fn bytes(&self) -> usize {
        self.0
    }

    /// Updates the count, for a value whose size changes.
    pub fn set(&mut self, bytes: usize) {
        if bytes >= self.0 {
            HELD.fetch_add(bytes - self.0, Ordering::Relaxed);
        } else {
            HELD.fetch_sub(self.0 - bytes, Ordering::Relaxed);
        }
        self.0 = bytes;
    }
}

impl Default for Weight {
    fn default() -> Self {
        Self::new(0)
    }
}

// A clone of a value holds the same number of bytes again, so it is counted
// separately.
impl Clone for Weight {
    fn clone(&self) -> Self {
        Self::new(self.0)
    }
}

impl Drop for Weight {
    fn drop(&mut self) {
        HELD.fetch_sub(self.0, Ordering::Relaxed);
    }
}

/// The total bytes declared by this library's live values.
pub fn held() -> usize {
    HELD.load(Ordering::Relaxed)
}

/// Requests a garbage collection at the host's next opportunity: the next
/// time the binding creates an object of a weighed interface or runs a
/// delivery. Intended for a library that has just released a lot of memory,
/// or is short of a resource that a dropped value still holds. Safe to call
/// from any thread.
pub fn collect_soon() {
    WANTED.store(true, Ordering::Relaxed);
}

/// Returns whether a collection was requested since the last call. Called by
/// the adapter.
pub fn take_request() -> bool {
    WANTED.swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // The total is process-wide, so the tests that read it run one at a time.
    static TURN: Mutex<()> = Mutex::new(());

    #[test]
    fn a_weight_counts_while_it_lives() {
        let _turn = TURN.lock().unwrap();
        let before = held();
        let weight = Weight::new(1000);
        assert_eq!(held(), before + 1000);
        assert_eq!(weight.bytes(), 1000);
        drop(weight);
        assert_eq!(held(), before);
    }

    #[test]
    fn a_weight_follows_a_value_that_grows_and_shrinks() {
        let _turn = TURN.lock().unwrap();
        let before = held();
        let mut weight = Weight::default();
        assert_eq!(held(), before);
        weight.set(4096);
        assert_eq!(held(), before + 4096);
        weight.set(96);
        assert_eq!(held(), before + 96);
        assert_eq!(weight.bytes(), 96);
        drop(weight);
        assert_eq!(held(), before);
    }

    #[test]
    fn a_copy_counts_as_much_again() {
        let _turn = TURN.lock().unwrap();
        let before = held();
        let weight = Weight::new(10);
        let copy = weight.clone();
        assert_eq!(held(), before + 20);
        drop(weight);
        assert_eq!(held(), before + 10);
        drop(copy);
        assert_eq!(held(), before);
    }

    #[test]
    fn a_request_is_taken_once() {
        let _turn = TURN.lock().unwrap();
        assert!(!take_request());
        collect_soon();
        collect_soon();
        assert!(take_request());
        assert!(!take_request());
    }
}
