//! Memory the host's collector cannot see.
//!
//! A collector decides when to run from the memory it manages. A handle is a
//! few words of that, whatever the value behind it holds, so a host can keep
//! making values and never think it is time to collect the ones it dropped.
//! A library declares what its values hold with a [`Weight`], and the binding
//! reads the total and collects when it has grown.
//!
//! The total is the library's: this crate is linked into each one separately.
//! Nothing here calls the host. The host asks.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static HELD: AtomicUsize = AtomicUsize::new(0);
static WANTED: AtomicBool = AtomicBool::new(false);

/// What a value holds outside the collector's sight, in bytes. Kept in the
/// value, so it is counted for as long as the value lives and no longer.
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

    /// For a value whose size changes.
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

// A copy of a value holds as much again.
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

/// The bytes this library's live values declare.
pub fn held() -> usize {
    HELD.load(Ordering::Relaxed)
}

/// Asks the host to collect at its next chance: the next time the binding
/// makes a weighed object, or delivers. For a library that knows it has just
/// let go of much, or is short of something a dropped value still holds.
/// Safe from any thread.
pub fn collect_soon() {
    WANTED.store(true, Ordering::Relaxed);
}

/// Whether a collection was asked for since the last call. For the adapter.
pub fn take_request() -> bool {
    WANTED.swap(false, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // The total is one for the process, so tests of it take turns.
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
