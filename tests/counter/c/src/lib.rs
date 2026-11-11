//! counter-core over the plain C ABI. Declared for C in `counter_abi.h`.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::ptr::null_mut;
use std::time::Duration;

use counter_core::Counter;
use hxn_abi::{Buf, Fault, Handle, RawHandle, Slice, Status, guard, release};
use hxn_future::AnyFuture;

unsafe fn counter<'a>(this: RawHandle) -> Result<&'a Handle<Counter>, Fault> {
    unsafe { Handle::from_raw(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_new(start: i32, out: *mut RawHandle, err: *mut Buf) -> Status {
    unsafe { guard(out, err, || Ok(Handle::into_raw(Counter::new(start)))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_add(
    this: RawHandle,
    amount: i32,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe { guard(out, err, || Ok(counter(this)?.borrow_mut()?.add(amount)?)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_label(
    this: RawHandle,
    name: Slice<'_>,
    out: *mut Buf,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(counter(this)?.borrow()?.label(name.as_str()?).into())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_sum(
    this: RawHandle,
    data: Slice<'_>,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(counter(this)?.borrow()?.sum(data.as_bytes()?))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_close(this: RawHandle, err: *mut Buf) -> Status {
    unsafe { guard(null_mut(), err, || Ok(counter(this)?.take().map(drop)?)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_release(this: RawHandle) {
    unsafe { release(this) }
}

#[unsafe(no_mangle)]
pub extern "C" fn counter_live() -> i32 {
    counter_core::live()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_fail(err: *mut Buf) -> Status {
    unsafe { guard::<()>(null_mut(), err, || panic!("counter failed on request")) }
}

#[unsafe(no_mangle)]
pub extern "C" fn counter_nap(milliseconds: i32) {
    std::thread::sleep(Duration::from_millis(milliseconds.max(0) as u64));
}

fn delay(milliseconds: i32) -> Duration {
    Duration::from_millis(milliseconds.max(0) as u64)
}

unsafe fn future<'a>(this: RawHandle) -> Result<&'a Handle<AnyFuture>, Fault> {
    unsafe { Handle::from_raw(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_square_later(
    value: i32,
    milliseconds: i32,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let future = counter_core::square_later(value, delay(milliseconds));
            Ok(Handle::into_raw(AnyFuture::from(future)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_greet_later(
    name: Slice<'_>,
    milliseconds: i32,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let future = counter_core::greet_later(name.as_str()?.to_owned(), delay(milliseconds));
            Ok(Handle::into_raw(AnyFuture::from(future)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_abandoned(out: *mut RawHandle, err: *mut Buf) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(Handle::into_raw(AnyFuture::from(counter_core::abandoned())))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_state(
    this: RawHandle,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe { guard(out, err, || Ok(future(this)?.borrow()?.state() as i32)) }
}

// The caller is in a GC-free zone, so this only waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_wait(
    this: RawHandle,
    milliseconds: i32,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(future(this)?.borrow()?.wait_millis(milliseconds) as i32)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_take_int(
    this: RawHandle,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe { guard(out, err, || future(this)?.borrow()?.take::<i32>()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_take_text(
    this: RawHandle,
    out: *mut Buf,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(future(this)?.borrow()?.take::<String>()?.into())
        })
    }
}
