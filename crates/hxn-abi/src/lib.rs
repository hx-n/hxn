//! The runtime-neutral core of hxn: the `#[repr(C)]` types a native library
//! and a Haxe runtime share, and the safe Rust API for each.
//!
//! | Passed across the boundary | Type | Owner |
//! |---|---|---|
//! | bytes or UTF-8 text, borrowed for one call | [`Slice`], [`SliceMut`] | the caller |
//! | bytes or UTF-8 text, returned | [`Buf`] | the library, until the host frees it |
//! | a Rust value the host holds | [`Handle`] behind a [`RawHandle`] | the host's garbage collector |
//! | a host closure | [`Callback`] | the library, until it drops it |
//! | an atomic cell both sides read and write | `AtomicI32`, [`AtomicFlag`] | the side that created it |
//! | a failure | [`Status`], with a message in a [`Buf`] | - |
//! | a failure's typed value | serialized bytes, stored by [`thrown`] until the binding takes them | the thread that failed |
//! | a plain-data value | bytes in the [`wire`] format | the same as for bytes |
//!
//! A library uses [`memory`] to report to the host's garbage collector how
//! much memory its values hold.
//!
//! Nothing unwinds across the boundary in either direction: [`guard`] converts
//! a panic into a status, and a runtime's exception is thrown by its adapter
//! after the Rust call has returned.
//!
//! **This crate has no dependencies and exports no symbols.** It is linked
//! into every library built on it, once per runtime, so any symbol it exported
//! would be defined once per library in the same process. A library re-exports
//! under its own name the functions a host needs to call, such as [`release`].
//!
//! `include/hxn.h` declares the same layouts for C and C++.

mod atomic;
mod buf;
mod callback;
mod guard;
mod handle;
pub mod memory;
mod slice;
mod status;
pub mod thrown;
pub mod wire;

pub use atomic::AtomicFlag;
pub use buf::Buf;
pub use callback::Callback;
pub use guard::{catch, guard};
pub use handle::{Handle, Opaque, RawHandle, Ref, RefMut, release};
pub use memory::Weight;
pub use slice::{Slice, SliceMut, apart};
pub use status::{Failure, Fault, Status};

// These sizes match the layouts include/hxn.h declares. Change both together.
const _: () = {
    use std::mem::{align_of, size_of};
    const WORD: usize = size_of::<usize>();
    assert!(size_of::<Status>() == 4);
    assert!(size_of::<Slice<'static>>() == 2 * WORD && align_of::<Slice<'static>>() == WORD);
    assert!(size_of::<SliceMut<'static>>() == 2 * WORD);
    assert!(size_of::<Buf>() == 4 * WORD && align_of::<Buf>() == WORD);
    assert!(size_of::<Callback<unsafe extern "C" fn()>>() == 3 * WORD);
    assert!(size_of::<RawHandle>() == WORD);
};
