//! counter-core as HashLink primitives. Each is one `prim` call and nothing
//! else, so a throw leaves no Rust value behind.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::time::Duration;

use counter_core::{Counter, Promise};
use hl_abi::{vbyte, vstring};
use hxn_abi::{Fault, Handle};
use hxn_future::AnyFuture;
use hxn_hl::{Managed, Root, blocking, define_prim, manage, prim, slice, string, utf16};

// A side module shares the program's heap, so it allocates from the program.
#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOCATOR: hl_abi::ProgramAllocator = hl_abi::ProgramAllocator;

unsafe fn counter<'a>(this: *mut Managed) -> Result<&'a Handle<Counter>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_make(start: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(Counter::new(start)))) }
}
define_prim!(hlp_make, counter_make, "Pi_Xhxn_counter_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_add(this: *mut Managed, amount: i32) -> i32 {
    unsafe { prim(|| Ok(counter(this)?.borrow_mut()?.add(amount)?)) }
}
define_prim!(hlp_add, counter_add, "PXhxn_counter_i_i");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_label(this: *mut Managed, name: *mut vstring) -> *mut vbyte {
    unsafe {
        prim(|| {
            let name = string(name)?.unwrap_or_default();
            Ok(utf16(&counter(this)?.borrow()?.label(&name)))
        })
    }
}
define_prim!(hlp_label, counter_label, "PXhxn_counter_OBi__B");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_sum(this: *mut Managed, data: *mut vbyte, length: i32) -> i32 {
    unsafe {
        prim(|| {
            // An argument needs no root for the call; this one is here to run `Root`.
            let data = Root::new(data);
            Ok(counter(this)?
                .borrow()?
                .sum(slice(data.get(), length).as_bytes()?))
        })
    }
}
define_prim!(hlp_sum, counter_sum, "PXhxn_counter_Bi_i");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_close(this: *mut Managed) {
    unsafe { prim(|| Ok(counter(this)?.take().map(drop)?)) }
}
define_prim!(hlp_close, counter_close, "PXhxn_counter__v");

#[unsafe(no_mangle)]
pub extern "C" fn counter_live() -> i32 {
    counter_core::live()
}
define_prim!(hlp_live, counter_live, "P_i");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_fail() {
    unsafe { prim(|| panic!("counter failed on request")) }
}
define_prim!(hlp_fail, counter_fail, "P_v");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_nap(milliseconds: i32) {
    let time = Duration::from_millis(milliseconds.max(0) as u64);
    unsafe { blocking(|| std::thread::sleep(time)) }
}
define_prim!(hlp_nap, counter_nap, "Pi_v");

fn delay(milliseconds: i32) -> Duration {
    Duration::from_millis(milliseconds.max(0) as u64)
}

unsafe fn future<'a>(this: *mut Managed) -> Result<&'a Handle<AnyFuture>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_square_later(value: i32, milliseconds: i32) -> *mut Managed {
    unsafe {
        prim(|| {
            let future = counter_core::square_later(value, delay(milliseconds));
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(hlp_square_later, counter_square_later, "Pii_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_greet_later(
    name: *mut vstring,
    milliseconds: i32,
) -> *mut Managed {
    unsafe {
        prim(|| {
            let name = string(name)?.unwrap_or_default();
            let future = counter_core::greet_later(name, delay(milliseconds));
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(hlp_greet_later, counter_greet_later, "POBi_i_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_sum_later(limit: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnyFuture::from(counter_core::sum_later(limit))))) }
}
define_prim!(hlp_sum_later, counter_sum_later, "Pi_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_abandoned() -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnyFuture::from(counter_core::abandoned())))) }
}
define_prim!(hlp_abandoned, counter_abandoned, "P_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_state(this: *mut Managed) -> i32 {
    unsafe { prim(|| Ok(future(this)?.borrow()?.state() as i32)) }
}
define_prim!(hlp_future_state, counter_future_state, "PXhxn_future__i");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_wait(this: *mut Managed, milliseconds: i32) -> i32 {
    unsafe {
        prim(|| {
            let future = future(this)?.borrow()?;
            Ok(blocking(|| future.wait_millis(milliseconds)) as i32)
        })
    }
}
define_prim!(hlp_future_wait, counter_future_wait, "PXhxn_future_i_i");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_take_int(this: *mut Managed) -> i32 {
    unsafe { prim(|| future(this)?.borrow()?.take::<i32>()) }
}
define_prim!(
    hlp_future_take_int,
    counter_future_take_int,
    "PXhxn_future__i"
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_future_take_text(this: *mut Managed) -> *mut vbyte {
    unsafe { prim(|| Ok(utf16(&future(this)?.borrow()?.take::<String>()?))) }
}
define_prim!(
    hlp_future_take_text,
    counter_future_take_text,
    "PXhxn_future__B"
);

unsafe fn promise<'a>(this: *mut Managed) -> Result<&'a Handle<Promise>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_new() -> *mut Managed {
    unsafe { prim(|| Ok(manage(Promise::new()))) }
}
define_prim!(hlp_promise_new, counter_promise_new, "P_Xhxn_promise_");

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_future(this: *mut Managed) -> *mut Managed {
    unsafe {
        prim(|| {
            let future = promise(this)?.borrow_mut()?.future().ok_or(Fault::Closed)?;
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(
    hlp_promise_future,
    counter_promise_future,
    "PXhxn_promise__Xhxn_future_"
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn counter_promise_resolve(this: *mut Managed, value: i32) -> bool {
    unsafe { prim(|| Ok(promise(this)?.borrow()?.resolve(value))) }
}
define_prim!(
    hlp_promise_resolve,
    counter_promise_resolve,
    "PXhxn_promise_i_b"
);
