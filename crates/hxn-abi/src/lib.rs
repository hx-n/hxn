//! The runtime-neutral core of hxn: the `#[repr(C)]` shapes a native library
//! and a Haxe runtime agree on, and the safe Rust side of each.
//!
//! | Crossing the boundary | Shape | Owner |
//! |---|---|---|
//! | bytes or UTF-8 text, for one call | [`Slice`], [`SliceMut`] | the caller |
//! | bytes or UTF-8 text, returned | [`Buf`] | the library, until the host frees it |
//! | a Rust value the host holds | [`Handle`] behind a [`RawHandle`] | the host's collector |
//! | a host closure | [`Callback`] | the library, until it drops it |
//! | a cell both sides read and write | `AtomicI32`, [`AtomicFlag`] | whoever made it |
//! | failure | [`Status`], with a message in a [`Buf`] | - |
//! | a failure's typed value | bytes, kept by [`thrown`] until asked for | the thread that failed |
//! | a value made of plain data | bytes, as [`wire`] lays them out | as bytes are |
//!
//! [`memory`] is how a library tells the host's collector what its values
//! hold.
//!
//! Nothing unwinds across the boundary in either direction: [`guard`] turns a
//! panic into a status, and a runtime's own exception is raised by its adapter
//! after the Rust call has returned.
//!
//! **This crate has no dependencies and exports no symbols.** It is linked
//! into every library built on it, once per runtime, so anything it exported
//! would be defined once per library in the same process. What a host needs to
//! call, such as [`release`], a library re-exports under its own name.
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
pub use slice::{Slice, SliceMut};
pub use status::{Failure, Fault, Status};

// What include/hxn.h declares. A change here is a change there.
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
