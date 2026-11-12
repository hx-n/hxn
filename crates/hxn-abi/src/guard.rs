use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{Buf, Failure, Status};

/// Runs `body` and converts a panic into a [`Status::PANIC`] failure.
pub fn catch<T>(body: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    // `AssertUnwindSafe`: this function reads no state the panic interrupted.
    // The library is responsible for its own state.
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => {
            let message = message_of(&*payload);
            // A panic from the payload's `Drop` must not escape either.
            if let Err(second) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                std::mem::forget(second);
            }
            Err(Failure::new(Status::PANIC, message))
        }
    }
}

fn message_of(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        String::from("panic")
    }
}

/// Implements the body of an exported function: runs `body`, then either
/// writes its return value to `out` and returns [`Status::OK`], or writes the
/// error message to `err` and returns the failure's status. It never unwinds.
///
/// `out` is not written on failure and `err` is not written on success. A
/// function with no return value passes a null `out` and a `body` that returns
/// `()`. A failure's value is stored for the host to retrieve: see
/// [`thrown`](crate::thrown).
///
/// # Safety
/// `out` is valid for writing a `T`, or `T` is zero-sized. `err` is null or
/// valid for writing a [`Buf`], and does not hold a buffer that needs freeing.
pub unsafe fn guard<T>(
    out: *mut T,
    err: *mut Buf,
    body: impl FnOnce() -> Result<T, Failure>,
) -> Status {
    let result = if out.is_null() && size_of::<T>() != 0 {
        Err(crate::Fault::Null.into())
    } else {
        catch(body)
    };
    match result {
        Ok(value) => {
            if size_of::<T>() != 0 {
                unsafe { out.write(value) };
            }
            Status::OK
        }
        Err(failure) => {
            crate::thrown::keep(failure.value);
            if !err.is_null() {
                unsafe { err.write(Buf::from(failure.message)) };
            }
            failure.status
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fault;

    fn run<T>(out: &mut T, body: impl FnOnce() -> Result<T, Failure>) -> (Status, String) {
        let mut err = Buf::empty();
        let status = unsafe { guard(out, &mut err, body) };
        (status, String::from_utf8(err.as_bytes().to_vec()).unwrap())
    }

    #[test]
    fn success_writes_the_value_only() {
        let mut out = 0;
        assert_eq!(run(&mut out, || Ok(42)), (Status::OK, String::new()));
        assert_eq!(out, 42);
    }

    #[test]
    fn failure_writes_the_message_only() {
        let mut out = 7;
        let (status, message) = run(&mut out, || Err("abc".parse::<i32>().unwrap_err().into()));
        assert_eq!(status, Status::ERROR);
        assert_eq!(message, "invalid digit found in string");
        assert_eq!(out, 7);

        let (status, message) = run(&mut out, || Err(Fault::Closed.into()));
        assert_eq!(
            (status, message.as_str()),
            (Status::CLOSED, "handle is closed")
        );

        let (status, _) = run(&mut out, || Err(Failure::new(Status(42), "its own code")));
        assert_eq!(status, Status(42));
    }

    #[test]
    fn a_panic_becomes_a_status() {
        let mut out = 0;
        let (status, message) = run(&mut out, || panic!("went {}", "wrong"));
        assert_eq!((status, message.as_str()), (Status::PANIC, "went wrong"));
        let (status, message) = run(&mut out, || std::panic::panic_any(3u8));
        assert_eq!((status, message.as_str()), (Status::PANIC, "panic"));
    }

    #[test]
    fn a_failure_leaves_its_value_to_be_asked_for() {
        let mut out = 0;
        let failed = || Err(Failure::new(Status::ERROR, "no").with_value(vec![1, 2]));
        assert_eq!(run(&mut out, failed).0, Status::ERROR);
        assert_eq!(crate::thrown::take(), [1, 2]);
        assert!(crate::thrown::take().is_empty());

        // The next failure replaces the stored value, whether or not it has
        // one.
        run(&mut out, failed);
        run(&mut out, || Err(Fault::Closed.into()));
        assert!(crate::thrown::take().is_empty());
    }

    #[test]
    fn nothing_to_return_needs_no_out() {
        let status = unsafe { guard(std::ptr::null_mut(), std::ptr::null_mut(), || Ok(())) };
        assert_eq!(status, Status::OK);
        let status =
            unsafe { guard::<()>(std::ptr::null_mut(), std::ptr::null_mut(), || panic!()) };
        assert_eq!(status, Status::PANIC);
    }

    #[test]
    fn a_missing_out_fails_before_the_body_runs() {
        let mut ran = false;
        let status = unsafe {
            guard::<i32>(std::ptr::null_mut(), std::ptr::null_mut(), || {
                ran = true;
                Ok(1)
            })
        };
        assert_eq!(status, Status::NULL);
        assert!(!ran);
    }
}
