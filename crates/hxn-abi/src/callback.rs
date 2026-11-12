use std::ffi::c_void;
use std::mem::MaybeUninit;

use crate::{Failure, Fault, Status};

/// A host closure: a function pointer, the environment pointer passed to it,
/// and a function that releases the environment.
///
/// `F` is the C signature, an `unsafe extern "C" fn` whose first parameter is
/// `env`. `drop` is null for a callback borrowed for one call. Otherwise it
/// runs once when this value is dropped, after which the host may collect the
/// closure.
///
/// Neither `Send` nor `Sync`: a host closure is called on the thread that
/// passed it, the only thread its runtime is known to accept the call on.
///
/// `call` must return normally. The binding is responsible for catching a host
/// exception raised inside it, because the exception would otherwise unwind
/// through Rust frames. By convention a callback returns a [`Status`] and
/// writes its result through a final `out` parameter.
/// [`invoke`](Self::invoke) calls a callback with that signature.
#[repr(C)]
pub struct Callback<F: Copy> {
    call: Option<F>,
    env: *mut c_void,
    drop: Option<unsafe extern "C" fn(*mut c_void)>,
}

impl<F: Copy> Callback<F> {
    /// # Safety
    /// `F` is an `unsafe extern "C" fn` pointer type. `call` may be called
    /// with `env` on this thread until `drop` has run, and `drop` may be
    /// called at most once.
    pub const unsafe fn from_raw(
        call: F,
        env: *mut c_void,
        drop: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> Self {
        Self {
            call: Some(call),
            env,
            drop,
        }
    }

    /// Returns the function pointer, for a signature [`call`](Self::call) does
    /// not cover, such as one with a borrowed parameter. Pass
    /// [`env`](Self::env) as its first argument.
    pub fn func(&self) -> Result<F, Fault> {
        // `Option<F>` is one word only when `F` is a function pointer.
        const { assert!(size_of::<Option<F>>() == size_of::<usize>()) };
        self.call.ok_or(Fault::Null)
    }

    pub fn env(&self) -> *mut c_void {
        self.env
    }
}

impl<F: Copy> Drop for Callback<F> {
    fn drop(&mut self) {
        if let Some(drop) = self.drop.take() {
            unsafe { drop(self.env) };
        }
    }
}

macro_rules! call {
    ($($arg:ident),*) => {
        impl<R $(, $arg)*> Callback<unsafe extern "C" fn(*mut c_void $(, $arg)*) -> R> {
            #[allow(non_snake_case, clippy::too_many_arguments)]
            pub fn call(&self $(, $arg: $arg)*) -> Result<R, Fault> {
                let call = self.func()?;
                Ok(unsafe { call(self.env $(, $arg)*) })
            }
        }
    };
}

macro_rules! invoke {
    ($($arg:ident),*) => {
        impl<R $(, $arg)*> Callback<unsafe extern "C" fn(*mut c_void $(, $arg)*, *mut R) -> Status> {
            /// Calls a callback that follows the convention and returns the
            /// value it wrote to `out`, or the status it failed with. After
            /// [`Status::CALLBACK`] the host has a pending exception, so the
            /// library should stop and return the failure.
            #[allow(non_snake_case, clippy::too_many_arguments)]
            pub fn invoke(&self $(, $arg: $arg)*) -> Result<R, Failure> {
                let call = self.func()?;
                let mut out = MaybeUninit::<R>::uninit();
                let status = unsafe { call(self.env $(, $arg)*, out.as_mut_ptr()) };
                if status.is_ok() {
                    Ok(unsafe { out.assume_init() })
                } else {
                    Err(Failure::new(status, "callback failed"))
                }
            }
        }
    };
}

invoke!();
invoke!(A);
invoke!(A, B);
invoke!(A, B, C);
invoke!(A, B, C, D);

call!();
call!(A);
call!(A, B);
call!(A, B, C);
call!(A, B, C, D);
call!(A, B, C, D, E);
call!(A, B, C, D, E, G);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Slice;

    struct Env {
        total: i32,
        dropped: bool,
    }

    unsafe extern "C" fn add(env: *mut c_void, amount: i32) -> i32 {
        let env = unsafe { &mut *env.cast::<Env>() };
        env.total += amount;
        env.total
    }

    unsafe extern "C" fn length(_: *mut c_void, text: Slice<'_>) -> usize {
        text.as_bytes().map_or(0, <[u8]>::len)
    }

    unsafe extern "C" fn mark_dropped(env: *mut c_void) {
        unsafe { (*env.cast::<Env>()).dropped = true };
    }

    #[test]
    fn calls_with_its_env_and_drops_it_once() {
        let mut env = Env {
            total: 0,
            dropped: false,
        };
        let pointer = (&raw mut env).cast::<c_void>();
        let callback = unsafe {
            Callback::from_raw(
                add as unsafe extern "C" fn(_, _) -> _,
                pointer,
                Some(mark_dropped),
            )
        };
        assert_eq!(callback.call(2), Ok(2));
        assert_eq!(callback.call(3), Ok(5));
        drop(callback);
        assert!(env.dropped);
    }

    #[test]
    fn a_borrowed_parameter_goes_through_func() {
        type Length = for<'a> unsafe extern "C" fn(*mut c_void, Slice<'a>) -> usize;
        let callback = unsafe { Callback::from_raw(length as Length, std::ptr::null_mut(), None) };
        let call = callback.func().unwrap();
        assert_eq!(unsafe { call(callback.env(), Slice::from("four")) }, 4);
    }

    unsafe extern "C" fn halve(_: *mut c_void, value: i32, out: *mut i32) -> Status {
        if value % 2 != 0 {
            return Status::CALLBACK;
        }
        unsafe { out.write(value / 2) };
        Status::OK
    }

    #[test]
    fn invoke_returns_what_was_written_or_the_status() {
        type Halve = unsafe extern "C" fn(*mut c_void, i32, *mut i32) -> Status;
        let callback = unsafe { Callback::from_raw(halve as Halve, std::ptr::null_mut(), None) };
        assert_eq!(callback.invoke(8), Ok(4));
        assert_eq!(callback.invoke(7).unwrap_err().status, Status::CALLBACK);
    }

    #[test]
    fn a_null_function_is_an_error() {
        let callback = Callback::<unsafe extern "C" fn(*mut c_void) -> i32> {
            call: None,
            env: std::ptr::null_mut(),
            drop: None,
        };
        assert_eq!(callback.call(), Err(Fault::Null));
    }
}
