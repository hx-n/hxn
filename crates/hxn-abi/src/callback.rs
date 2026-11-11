use std::ffi::c_void;

use crate::Fault;

/// A host closure: the function to call, the state to call it with, and how
/// to let go of that state.
///
/// `F` is the C signature, an `unsafe extern "C" fn` whose first parameter is
/// `env`. `drop` is null for a callback lent for one call, and otherwise runs
/// once when this is dropped, which is when the host may collect the closure.
///
/// Neither `Send` nor `Sync`: a host closure is called on the thread that
/// passed it, which is the only one its runtime is known to accept.
///
/// `call` must return normally. A host exception raised inside it is the
/// adapter's to catch, because it would otherwise cross Rust frames.
#[repr(C)]
pub struct Callback<F: Copy> {
    call: Option<F>,
    env: *mut c_void,
    drop: Option<unsafe extern "C" fn(*mut c_void)>,
}

impl<F: Copy> Callback<F> {
    /// # Safety
    /// `F` is an `unsafe extern "C" fn` pointer type. `call` may be called
    /// with `env` on this thread until `drop` has run, and `drop` at most once.
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

    /// The function, for a signature [`call`](Self::call) does not cover, such
    /// as one with a borrowed parameter. It is called with [`env`](Self::env).
    pub fn func(&self) -> Result<F, Fault> {
        // A function pointer is the only `F` whose `Option` is one word.
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
