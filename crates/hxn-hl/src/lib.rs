//! hxn's HashLink side: what an `.hdll` built on [`hxn_abi`] needs from the
//! VM's collector and nothing else.
//!
//! - [`manage`] gives a [`Handle`] to the collector, which releases it.
//! - [`Root`] keeps a VM value alive while Rust holds it.
//! - [`string`], [`utf16`] and [`bytes`] copy text and bytes across, since a
//!   Haxe `String` is UTF-16 here and Rust's is UTF-8.
//! - [`prim`] throws a failure to Haxe once Rust has let go of everything.
//! - [`blocking`] lets the collector run while a call waits.
//!
//! Like `hl_abi` it imports the runtime's symbols and defines none, so the
//! library linking it must leave them undefined for the VM to resolve.
//!
//! Every function here is for the thread the VM called in on. Stock HashLink
//! aborts when a thread it does not know allocates or changes a root.

use std::ffi::{c_int, c_void};

use hl_abi::{hl_type, vbyte, vdynamic, vstring};
use hxn_abi::{Failure, Fault, Handle, RawHandle, Slice, catch, release};

pub use hl_abi::define_prim;

unsafe extern "C" {
    // A DLL's data is reached through its import pointer, not linked directly.
    #[cfg(not(windows))]
    static mut hlt_abstract: hl_type;
    #[cfg(not(windows))]
    static mut hlt_bytes: hl_type;
    #[cfg(windows)]
    #[link_name = "__imp_hlt_abstract"]
    static mut hlt_abstract_import: *mut hl_type;
    #[cfg(windows)]
    #[link_name = "__imp_hlt_bytes"]
    static mut hlt_bytes_import: *mut hl_type;

    fn hl_gc_alloc_gen(ty: *mut hl_type, size: c_int, flags: c_int) -> *mut c_void;
    fn hl_alloc_dynamic(ty: *mut hl_type) -> *mut vdynamic;
    fn hl_add_root(slot: *mut c_void);
    fn hl_remove_root(slot: *mut c_void);
    fn hl_blocking(blocking: bool);
    fn hl_throw(value: *mut vdynamic) -> !;
}

const MEM_KIND_FINALIZER: c_int = 3;

unsafe fn abstract_type() -> *mut hl_type {
    #[cfg(windows)]
    unsafe {
        hlt_abstract_import
    }
    #[cfg(not(windows))]
    {
        &raw mut hlt_abstract
    }
}

unsafe fn bytes_type() -> *mut hl_type {
    #[cfg(windows)]
    unsafe {
        hlt_bytes_import
    }
    #[cfg(not(windows))]
    {
        &raw mut hlt_bytes
    }
}

/// What Haxe holds for a handle: an abstract the collector finalizes. The
/// finalizer is its first word, which is where the collector looks for it.
#[repr(C)]
pub struct Managed {
    finalize: Option<unsafe extern "C" fn(*mut Managed)>,
    handle: RawHandle,
}

unsafe extern "C" fn finalize(managed: *mut Managed) {
    unsafe {
        release((*managed).handle);
        (*managed).handle = std::ptr::null_mut();
    }
}

/// Moves `value` into a handle the collector owns. Null if the VM has no
/// memory for it, in which case `value` is dropped.
///
/// `value` is dropped inside a collection if it is never taken, so its `Drop`
/// must not allocate from the VM or call into it.
pub fn manage<T: Send + 'static>(value: T) -> *mut Managed {
    let managed = unsafe {
        hl_gc_alloc_gen(
            abstract_type(),
            size_of::<Managed>() as c_int,
            MEM_KIND_FINALIZER,
        )
    }
    .cast::<Managed>();
    if !managed.is_null() {
        unsafe {
            managed.write(Managed {
                finalize: Some(finalize),
                handle: Handle::into_raw(value),
            })
        };
    }
    managed
}

/// The handle `managed` owns, if it is one of `T`.
///
/// # Safety
/// `managed` is null or came from [`manage`] in this library, and the VM
/// keeps it reachable for `'a`, as it does an argument for the call.
pub unsafe fn handle<'a, T: Send + 'static>(
    managed: *const Managed,
) -> Result<&'a Handle<T>, Fault> {
    if managed.is_null() {
        return Err(Fault::Null);
    }
    unsafe { Handle::from_raw((*managed).handle) }
}

/// Keeps a VM value alive, and at this address, for as long as it exists.
///
/// Not `Send`: it is dropped on the thread that made it.
pub struct Root<T>(Box<*mut T>);

impl<T> Root<T> {
    /// # Safety
    /// `value` is null or a live value the VM allocated.
    pub unsafe fn new(value: *mut T) -> Self {
        let mut slot = Box::new(value);
        unsafe { hl_add_root((&raw mut *slot).cast()) };
        Self(slot)
    }

    pub fn get(&self) -> *mut T {
        *self.0
    }
}

impl<T> Drop for Root<T> {
    fn drop(&mut self) {
        unsafe { hl_remove_root((&raw mut *self.0).cast()) };
    }
}

/// A Haxe `String` as Rust text. `None` for null.
///
/// # Safety
/// `value` is null or a live `String`.
pub unsafe fn string(value: *const vstring) -> Result<Option<String>, Fault> {
    if value.is_null() {
        return Ok(None);
    }
    let (units, length) = unsafe { ((*value).bytes, (*value).length) };
    if length <= 0 || units.is_null() {
        return Ok(Some(String::new()));
    }
    let units = unsafe { std::slice::from_raw_parts(units, length as usize) };
    String::from_utf16(units)
        .map(Some)
        .map_err(|_| Fault::Encoding)
}

/// `hl.Bytes` and a length, borrowed for the call.
///
/// # Safety
/// `data` is readable for `length` bytes for `'a`, or `length` is not positive.
pub unsafe fn slice<'a>(data: *const vbyte, length: c_int) -> Slice<'a> {
    unsafe { Slice::from_raw(data, usize::try_from(length).unwrap_or(0)) }
}

/// `text` as VM-owned, null-terminated UTF-16, which is what
/// `String.fromUCS2` takes. Null if it is too long for the VM.
pub fn utf16(text: &str) -> *mut vbyte {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let Some(size) = units.len().checked_mul(size_of::<u16>()) else {
        return std::ptr::null_mut();
    };
    unsafe { copy(units.as_ptr().cast(), size) }
}

/// A VM-owned copy of `data`. Null if it is too long for the VM.
pub fn bytes(data: &[u8]) -> *mut vbyte {
    unsafe { copy(data.as_ptr(), data.len()) }
}

unsafe fn copy(data: *const u8, size: usize) -> *mut vbyte {
    let Ok(length) = c_int::try_from(size) else {
        return std::ptr::null_mut();
    };
    let out = unsafe { hl_abi::hl_alloc_bytes(length) };
    if !out.is_null() {
        unsafe { std::ptr::copy_nonoverlapping(data, out, size) };
    }
    out
}

/// The body of a primitive: returns what `body` returns, or throws its
/// failure or panic to Haxe as the message.
///
/// The VM throws by jumping to its handler, past every Rust frame on the way
/// without running a destructor. `body` and the failure are finished before
/// that happens.
///
/// # Safety
/// The primitive's own frame holds nothing with a destructor while this
/// runs, which is true if this call is all it does.
pub unsafe fn prim<T>(body: impl FnOnce() -> Result<T, Failure>) -> T {
    let thrown = match catch(body) {
        Ok(value) => return value,
        Err(failure) => exception(&failure.message),
    };
    unsafe { hl_throw(thrown) }
}

// What `hl_error` throws: a dynamic holding UTF-16 bytes.
fn exception(message: &str) -> *mut vdynamic {
    let text = utf16(message);
    let value = unsafe { hl_alloc_dynamic(bytes_type()) };
    if !value.is_null() {
        unsafe { (*value).v.bytes = text };
    }
    value
}

/// Runs `body` with the collector free to run without this thread, for a
/// call that waits.
///
/// # Safety
/// `body` does not allocate from the VM, call into it, or write to memory it
/// owns. Reading arguments the caller still holds is fine.
pub unsafe fn blocking<T>(body: impl FnOnce() -> T) -> T {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            unsafe { hl_blocking(false) };
        }
    }
    unsafe { hl_blocking(true) };
    let _leave = Leave;
    body()
}
