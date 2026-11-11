//! counter-core as HashLink primitives. Each is one `prim` call and nothing
//! else, so a throw leaves no Rust value behind.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::time::Duration;

use counter_core::Counter;
use hl_abi::{vbyte, vstring};
use hxn_abi::{Fault, Handle};
use hxn_hl::{Managed, Root, blocking, define_prim, manage, prim, slice, string, utf16};

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
