//! counter-core over the plain C ABI. Declared for C in `counter_abi.h`.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::ptr::null_mut;
use std::time::Duration;

use counter_core::{Counter, Emitter, Promise};
use hxn_abi::{Buf, Callback, Fault, Handle, RawHandle, Slice, Status, guard, release};
use hxn_future::{AnyFuture, AnyStream};

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

type IntToInt = unsafe extern "C" fn(*mut c_void, i32, *mut i32) -> Status;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_apply(
    this: RawHandle,
    times: i32,
    f: Callback<IntToInt>,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            counter(this)?
                .borrow_mut()?
                .apply(times, |value| f.invoke(value))
        })
    }
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
pub unsafe extern "C" fn counter_sum_later(
    limit: i32,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(Handle::into_raw(AnyFuture::from(counter_core::sum_later(
                limit,
            ))))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_combined_later(
    value: i32,
    limit: i32,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let future = counter_core::combined_later(value, limit);
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

unsafe fn promise<'a>(this: RawHandle) -> Result<&'a Handle<Promise>, Fault> {
    unsafe { Handle::from_raw(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_new(out: *mut RawHandle, err: *mut Buf) -> Status {
    unsafe { guard(out, err, || Ok(Handle::into_raw(Promise::new()))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_future(
    this: RawHandle,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let future = promise(this)?.borrow_mut()?.future().ok_or(Fault::Closed)?;
            Ok(Handle::into_raw(AnyFuture::from(future)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_resolve(
    this: RawHandle,
    value: i32,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(i32::from(promise(this)?.borrow()?.resolve(value)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_doubled(
    this: RawHandle,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let future = promise(this)?
                .borrow_mut()?
                .doubled()
                .ok_or(Fault::Closed)?;
            Ok(Handle::into_raw(AnyFuture::from(future)))
        })
    }
}

unsafe fn stream<'a>(this: RawHandle) -> Result<&'a Handle<AnyStream>, Fault> {
    unsafe { Handle::from_raw(this) }
}

unsafe fn emitter<'a>(this: RawHandle) -> Result<&'a Handle<Emitter>, Fault> {
    unsafe { Handle::from_raw(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_count_up(
    limit: i32,
    fail: i32,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let stream = counter_core::count_up(limit, fail != 0);
            Ok(Handle::into_raw(AnyStream::from(stream)))
        })
    }
}

// The caller is in a GC-free zone, so this only waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_stream_wait(
    this: RawHandle,
    milliseconds: i32,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(stream(this)?.borrow()?.wait_millis(milliseconds) as i32)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_stream_next_int(
    this: RawHandle,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe { guard(out, err, || stream(this)?.borrow()?.next::<i32>()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_emitter_new(out: *mut RawHandle, err: *mut Buf) -> Status {
    unsafe { guard(out, err, || Ok(Handle::into_raw(Emitter::new()))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_emitter_stream(
    this: RawHandle,
    out: *mut RawHandle,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            let stream = emitter(this)?.borrow_mut()?.stream().ok_or(Fault::Closed)?;
            Ok(Handle::into_raw(AnyStream::from(stream)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_emitter_emit(
    this: RawHandle,
    value: i32,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe {
        guard(out, err, || {
            Ok(i32::from(emitter(this)?.borrow()?.emit(value)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_emitter_end(
    this: RawHandle,
    out: *mut i32,
    err: *mut Buf,
) -> Status {
    unsafe { guard(out, err, || Ok(i32::from(emitter(this)?.borrow()?.end()))) }
}
