//! counter-core over the plain C ABI. Declared for C in `counter_abi.h`.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::ptr::null_mut;
use std::time::Duration;

use counter_core::Counter;
use hxn_abi::{Buf, Handle, RawHandle, Slice, Status, guard, release};

unsafe fn counter<'a>(this: RawHandle) -> Result<&'a Handle<Counter>, hxn_abi::Fault> {
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
