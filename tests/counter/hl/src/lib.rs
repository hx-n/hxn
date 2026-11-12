//! counter-core as HashLink primitives. Each primitive is a single `prim`
//! call, so a throw leaves no Rust value undropped.

// Every export has the ABI's safety contract: its arguments are whatever the
// host's glue passes.
#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::time::Duration;

use counter_core::{Counter, Emitter, Gauge, Promise};
use hl_abi::{vbyte, vstring};
use hxn_abi::{Callback, Fault, Handle, Status};
use hxn_future::{AnyFuture, AnySignal, AnyStream, outbox};
use hxn_hl::{
    Managed, Root, blocking, box_i32, call, define_prim, manage, prim, slice, string, unbox_i32,
    utf16, vclosure,
};

// A wasm side module shares the main program's heap, so it uses the
// program's allocator.
#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOCATOR: hl_abi::ProgramAllocator = hl_abi::ProgramAllocator;

unsafe fn counter<'a>(this: *mut Managed) -> Result<&'a Handle<Counter>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_make(start: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(Counter::new(start)))) }
}
define_prim!(hlp_make, counter_make, "Pi_Xhxn_counter_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_add(this: *mut Managed, amount: i32) -> i32 {
    unsafe { prim(|| Ok(counter(this)?.borrow_mut()?.add(amount)?)) }
}
define_prim!(hlp_add, counter_add, "PXhxn_counter_i_i");

type IntToInt = unsafe extern "C" fn(*mut c_void, i32, *mut i32) -> Status;

// The trampoline for an `Int->Null<Int>` callback. The binding's wrapper
// returns null when the user's function threw.
unsafe extern "C" fn int_to_int(env: *mut c_void, value: i32, out: *mut i32) -> Status {
    let mut args = [box_i32(value)];
    match unsafe { unbox_i32(call(env.cast(), &mut args)) } {
        Some(result) => {
            unsafe { out.write(result) };
            Status::OK
        }
        None => Status::CALLBACK,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_apply(
    this: *mut Managed,
    times: i32,
    f: *mut vclosure,
) -> i32 {
    unsafe {
        prim(|| {
            // Borrowed: the VM keeps `f` alive for the duration of the call,
            // so the callback has no drop function.
            let f = Callback::from_raw(int_to_int as IntToInt, f.cast(), None);
            counter(this)?
                .borrow_mut()?
                .apply(times, |value| f.invoke(value))
        })
    }
}
define_prim!(hlp_apply, counter_apply, "PXhxn_counter_iPi_Ni_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_label(
    this: *mut Managed,
    name: *mut vstring,
) -> *mut vbyte {
    unsafe {
        prim(|| {
            let name = string(name)?.unwrap_or_default();
            Ok(utf16(&counter(this)?.borrow()?.label(&name)))
        })
    }
}
define_prim!(hlp_label, counter_label, "PXhxn_counter_OBi__B");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_sum(
    this: *mut Managed,
    data: *mut vbyte,
    length: i32,
) -> i32 {
    unsafe {
        prim(|| {
            // An argument needs no GC root during the call. This one exists
            // to exercise `Root`.
            let data = Root::new(data);
            Ok(counter(this)?
                .borrow()?
                .sum(slice(data.get(), length).as_bytes()?))
        })
    }
}
define_prim!(hlp_sum, counter_sum, "PXhxn_counter_Bi_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_close(this: *mut Managed) {
    unsafe { prim(|| Ok(counter(this)?.take().map(drop)?)) }
}
define_prim!(hlp_close, counter_close, "PXhxn_counter__v");

#[unsafe(no_mangle)]
pub extern "C" fn counter_live() -> i32 {
    counter_core::live()
}
define_prim!(hlp_live, counter_live, "P_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_fail() {
    unsafe { prim(|| panic!("counter failed on request")) }
}
define_prim!(hlp_fail, counter_fail, "P_v");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_nap(milliseconds: i32) {
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
pub unsafe extern "C-unwind" fn counter_square_later(
    value: i32,
    milliseconds: i32,
) -> *mut Managed {
    unsafe {
        prim(|| {
            let future = counter_core::square_later(value, delay(milliseconds));
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(hlp_square_later, counter_square_later, "Pii_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_greet_later(
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
pub unsafe extern "C-unwind" fn counter_sum_later(limit: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnyFuture::from(counter_core::sum_later(limit))))) }
}
define_prim!(hlp_sum_later, counter_sum_later, "Pi_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_combined_later(value: i32, limit: i32) -> *mut Managed {
    unsafe {
        prim(|| {
            let future = counter_core::combined_later(value, limit);
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(
    hlp_combined_later,
    counter_combined_later,
    "Pii_Xhxn_future_"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_abandoned() -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnyFuture::from(counter_core::abandoned())))) }
}
define_prim!(hlp_abandoned, counter_abandoned, "P_Xhxn_future_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_future_state(this: *mut Managed) -> i32 {
    unsafe { prim(|| Ok(future(this)?.borrow()?.state() as i32)) }
}
define_prim!(hlp_future_state, counter_future_state, "PXhxn_future__i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_future_wait(this: *mut Managed, milliseconds: i32) -> i32 {
    unsafe {
        prim(|| {
            let future = future(this)?.borrow()?;
            Ok(blocking(|| future.wait_millis(milliseconds)) as i32)
        })
    }
}
define_prim!(hlp_future_wait, counter_future_wait, "PXhxn_future_i_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_future_take_int(this: *mut Managed) -> i32 {
    unsafe { prim(|| future(this)?.borrow()?.take::<i32>()) }
}
define_prim!(
    hlp_future_take_int,
    counter_future_take_int,
    "PXhxn_future__i"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_future_take_text(this: *mut Managed) -> *mut vbyte {
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
pub unsafe extern "C-unwind" fn counter_promise_new() -> *mut Managed {
    unsafe { prim(|| Ok(manage(Promise::new()))) }
}
define_prim!(hlp_promise_new, counter_promise_new, "P_Xhxn_promise_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_promise_future(this: *mut Managed) -> *mut Managed {
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
pub unsafe extern "C-unwind" fn counter_promise_resolve(this: *mut Managed, value: i32) -> bool {
    unsafe { prim(|| Ok(promise(this)?.borrow()?.resolve(value))) }
}
define_prim!(
    hlp_promise_resolve,
    counter_promise_resolve,
    "PXhxn_promise_i_b"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_promise_doubled(this: *mut Managed) -> *mut Managed {
    unsafe {
        prim(|| {
            let future = promise(this)?
                .borrow_mut()?
                .doubled()
                .ok_or(Fault::Closed)?;
            Ok(manage(AnyFuture::from(future)))
        })
    }
}
define_prim!(
    hlp_promise_doubled,
    counter_promise_doubled,
    "PXhxn_promise__Xhxn_future_"
);

unsafe fn stream<'a>(this: *mut Managed) -> Result<&'a Handle<AnyStream>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

unsafe fn emitter<'a>(this: *mut Managed) -> Result<&'a Handle<Emitter>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_count_up(limit: i32, fail: bool) -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnyStream::from(counter_core::count_up(limit, fail))))) }
}
define_prim!(hlp_count_up, counter_count_up, "Pib_Xhxn_stream_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_stream_wait(this: *mut Managed, milliseconds: i32) -> i32 {
    unsafe {
        prim(|| {
            let stream = stream(this)?.borrow()?;
            Ok(blocking(|| stream.wait_millis(milliseconds)) as i32)
        })
    }
}
define_prim!(hlp_stream_wait, counter_stream_wait, "PXhxn_stream_i_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_stream_next_int(this: *mut Managed) -> i32 {
    unsafe { prim(|| stream(this)?.borrow()?.next::<i32>()) }
}
define_prim!(
    hlp_stream_next_int,
    counter_stream_next_int,
    "PXhxn_stream__i"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_emitter_new() -> *mut Managed {
    unsafe { prim(|| Ok(manage(Emitter::new()))) }
}
define_prim!(hlp_emitter_new, counter_emitter_new, "P_Xhxn_emitter_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_emitter_stream(this: *mut Managed) -> *mut Managed {
    unsafe {
        prim(|| {
            let stream = emitter(this)?.borrow_mut()?.stream().ok_or(Fault::Closed)?;
            Ok(manage(AnyStream::from(stream)))
        })
    }
}
define_prim!(
    hlp_emitter_stream,
    counter_emitter_stream,
    "PXhxn_emitter__Xhxn_stream_"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_emitter_emit(this: *mut Managed, value: i32) -> bool {
    unsafe { prim(|| Ok(emitter(this)?.borrow()?.emit(value))) }
}
define_prim!(hlp_emitter_emit, counter_emitter_emit, "PXhxn_emitter_i_b");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_emitter_end(this: *mut Managed) -> bool {
    unsafe { prim(|| Ok(emitter(this)?.borrow()?.end())) }
}
define_prim!(hlp_emitter_end, counter_emitter_end, "PXhxn_emitter__b");

unsafe fn signal<'a>(this: *mut Managed) -> Result<&'a Handle<AnySignal>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

unsafe fn gauge<'a>(this: *mut Managed) -> Result<&'a Handle<Gauge>, Fault> {
    unsafe { hxn_hl::handle(this) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_ramp(limit: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(AnySignal::from(counter_core::ramp(limit))))) }
}
define_prim!(hlp_ramp, counter_ramp, "Pi_Xhxn_signal_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_signal_wait(
    this: *mut Managed,
    seen: i32,
    milliseconds: i32,
) -> i32 {
    unsafe {
        prim(|| {
            let signal = signal(this)?.borrow()?;
            Ok(blocking(|| signal.wait_millis(seen, milliseconds)))
        })
    }
}
define_prim!(hlp_signal_wait, counter_signal_wait, "PXhxn_signal_ii_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_signal_get_int(this: *mut Managed) -> i32 {
    unsafe { prim(|| signal(this)?.borrow()?.get::<i32>()) }
}
define_prim!(
    hlp_signal_get_int,
    counter_signal_get_int,
    "PXhxn_signal__i"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_gauge_new(initial: i32) -> *mut Managed {
    unsafe { prim(|| Ok(manage(Gauge::new(initial)))) }
}
define_prim!(hlp_gauge_new, counter_gauge_new, "Pi_Xhxn_gauge_");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_gauge_signal(this: *mut Managed) -> *mut Managed {
    unsafe {
        prim(|| {
            let signal = gauge(this)?.borrow_mut()?.signal().ok_or(Fault::Closed)?;
            Ok(manage(AnySignal::from(signal)))
        })
    }
}
define_prim!(
    hlp_gauge_signal,
    counter_gauge_signal,
    "PXhxn_gauge__Xhxn_signal_"
);

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_gauge_set(this: *mut Managed, value: i32) -> bool {
    unsafe { prim(|| Ok(gauge(this)?.borrow()?.set(value))) }
}
define_prim!(hlp_gauge_set, counter_gauge_set, "PXhxn_gauge_i_b");

// The library's single queue of items ready for the host.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_outbox_wait(milliseconds: i32) -> bool {
    unsafe { blocking(|| outbox::wait_millis(milliseconds)) }
}
define_prim!(hlp_outbox_wait, counter_outbox_wait, "Pi_b");

#[unsafe(no_mangle)]
pub extern "C" fn counter_outbox_next() -> i32 {
    outbox::next()
}
define_prim!(hlp_outbox_next, counter_outbox_next, "P_i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_future_watch(this: *mut Managed) -> i32 {
    unsafe { prim(|| Ok(future(this)?.borrow()?.watch())) }
}
define_prim!(hlp_future_watch, counter_future_watch, "PXhxn_future__i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_stream_watch(this: *mut Managed) -> i32 {
    unsafe { prim(|| Ok(stream(this)?.borrow()?.watch())) }
}
define_prim!(hlp_stream_watch, counter_stream_watch, "PXhxn_stream__i");

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn counter_signal_watch(this: *mut Managed) -> i32 {
    unsafe { prim(|| Ok(signal(this)?.borrow()?.watch())) }
}
define_prim!(hlp_signal_watch, counter_signal_watch, "PXhxn_signal__i");
