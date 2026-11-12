//! The value of the last failure on this thread.
//!
//! A failure crosses as a status and a message. When the library failed with
//! plain data of its own, the bytes of that are kept here, and the binding,
//! having caught what the message was thrown as, takes them and throws the
//! typed value instead. Every failure replaces what was kept, so what is
//! taken belongs to the failure just caught.

use std::cell::RefCell;

thread_local! {
    static KEPT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Keeps `value` as the last failure's, in place of any before it.
pub fn keep(value: Vec<u8>) {
    // A thread that is ending has nowhere to keep it, and nothing to read it.
    let _ = KEPT.try_with(|kept| {
        if let Ok(mut kept) = kept.try_borrow_mut() {
            *kept = value;
        }
    });
}

/// The last failure's value, which is then kept no longer. Empty if it had
/// none.
pub fn take() -> Vec<u8> {
    KEPT.try_with(|kept| {
        kept.try_borrow_mut()
            .map(|mut kept| std::mem::take(&mut *kept))
    })
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_taken_once_and_on_its_own_thread() {
        keep(vec![7]);
        assert!(std::thread::spawn(take).join().unwrap().is_empty());
        assert_eq!(take(), [7]);
        assert!(take().is_empty());
    }
}
