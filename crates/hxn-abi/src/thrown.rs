//! Thread-local storage for the typed value of the last failure.
//!
//! A failure is passed across the boundary as a status and a message. When
//! the library failed with a plain-data error of its own, the serialized bytes
//! are stored here. The binding catches the exception thrown for the message,
//! takes the bytes, and throws the typed error instead. Every failure replaces
//! the stored bytes, so the bytes taken belong to the failure just caught.

use std::cell::RefCell;

thread_local! {
    static KEPT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Stores `value` as the last failure's value, replacing any earlier one.
pub fn keep(value: Vec<u8>) {
    // On an exiting thread the storage is gone and nothing would read the
    // value, so it is dropped.
    let _ = KEPT.try_with(|kept| {
        if let Ok(mut kept) = kept.try_borrow_mut() {
            *kept = value;
        }
    });
}

/// Returns the last failure's value and clears it. Empty if the failure had
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
