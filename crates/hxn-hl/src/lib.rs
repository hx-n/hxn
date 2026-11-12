//! hxn's HashLink side: what an `.hdll` built on [`hxn_abi`] needs from the
//! VM's collector and nothing else.
//!
//! - [`manage`] gives a [`Handle`] to the collector, which releases it.
//! - [`Root`] keeps a VM value alive while Rust holds it.
//! - [`string`], [`utf16`] and [`bytes`] copy text and bytes across, since a
//!   Haxe `String` is UTF-16 here and Rust's is UTF-8.
//! - [`prim`] throws a failure to Haxe once Rust has let go of everything.
//! - [`blocking`] lets the collector run while a call waits.
//! - [`call`], with [`box_i32`] and [`unbox_i32`], calls a Haxe function
//!   value, which is what a callback's trampoline is made of.
//!
//! Like `hl_abi` it imports the runtime's symbols and defines none, so the
//! library linking it must leave them undefined for the VM to resolve.
//!
//! Every function here is for the thread the VM called in on. Stock HashLink
//! aborts when a thread it does not know allocates or changes a root.

use std::ffi::{c_int, c_void};

use hl_abi::hl_type;
use hxn_abi::{Failure, Fault, Handle, RawHandle, Slice, catch, release};

// What a generated adapter names, so that its crate needs this one alone.
pub use hl_abi::{define_prim, vbyte, vdynamic, vstring};

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
    #[cfg(not(windows))]
    static mut hlt_i32: hl_type;
    #[cfg(windows)]
    #[link_name = "__imp_hlt_i32"]
    static mut hlt_i32_import: *mut hl_type;
    #[cfg(not(windows))]
    static mut hlt_f64: hl_type;
    #[cfg(windows)]
    #[link_name = "__imp_hlt_f64"]
    static mut hlt_f64_import: *mut hl_type;
    #[cfg(not(windows))]
    static mut hlt_bool: hl_type;
    #[cfg(windows)]
    #[link_name = "__imp_hlt_bool"]
    static mut hlt_bool_import: *mut hl_type;

    fn hl_gc_alloc_gen(ty: *mut hl_type, size: c_int, flags: c_int) -> *mut c_void;
    fn hl_alloc_dynamic(ty: *mut hl_type) -> *mut vdynamic;
    fn hl_add_root(slot: *mut c_void);
    fn hl_remove_root(slot: *mut c_void);
    fn hl_blocking(blocking: bool);
    fn hl_throw(value: *mut vdynamic) -> !;
    fn hl_dyn_call(closure: *mut vclosure, args: *mut *mut vdynamic, count: c_int)
    -> *mut vdynamic;
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

/// A `haxe.io.Bytes` as the VM lays it out: its fields in the order the
/// class declares them. A primitive that takes one writes it `OiB_`.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct vbuffer {
    t: *mut hl_type,
    length: c_int,
    bytes: *mut vbyte,
}

/// A copy of what a `haxe.io.Bytes` holds. `None` for null.
///
/// # Safety
/// `value` is null or a live `haxe.io.Bytes`.
pub unsafe fn buffer(value: *const vbuffer) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let (data, length) = unsafe { ((*value).bytes, (*value).length) };
    if length <= 0 || data.is_null() {
        return Some(Vec::new());
    }
    Some(unsafe { std::slice::from_raw_parts(data, length as usize) }.to_vec())
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
/// failure or panic to Haxe as the message. A failure's value is kept for
/// the binding to ask for: see `hxn_abi::thrown`.
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
        Err(failure) => {
            let thrown = exception(&failure.message);
            hxn_abi::thrown::keep(failure.value);
            thrown
        }
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

/// A Haxe function value. Opaque: it is only handed back to the VM.
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct vclosure {
    _private: [u8; 0],
}

unsafe fn i32_type() -> *mut hl_type {
    #[cfg(windows)]
    unsafe {
        hlt_i32_import
    }
    #[cfg(not(windows))]
    {
        &raw mut hlt_i32
    }
}

/// `value` as the VM passes `hl.Bytes` it does not know the type of.
pub fn box_bytes(value: *mut vbyte) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(bytes_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.bytes = value };
    }
    boxed
}

/// `value` as the VM passes an `Int` it does not know the type of.
pub fn box_i32(value: i32) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(i32_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.i = value };
    }
    boxed
}

/// The `Int` in a `Null<Int>`, or `None` for null.
///
/// # Safety
/// `value` is null or a live boxed `Int`.
pub unsafe fn unbox_i32(value: *const vdynamic) -> Option<i32> {
    if value.is_null() {
        None
    } else {
        Some(unsafe { (*value).v.i })
    }
}

unsafe fn f64_type() -> *mut hl_type {
    #[cfg(windows)]
    unsafe {
        hlt_f64_import
    }
    #[cfg(not(windows))]
    {
        &raw mut hlt_f64
    }
}

unsafe fn bool_type() -> *mut hl_type {
    #[cfg(windows)]
    unsafe {
        hlt_bool_import
    }
    #[cfg(not(windows))]
    {
        &raw mut hlt_bool
    }
}

/// `value` as the VM passes a `Float` it does not know the type of.
pub fn box_f64(value: f64) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(f64_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.d = value };
    }
    boxed
}

/// The `Float` in a `Null<Float>`, or `None` for null.
///
/// # Safety
/// `value` is null or a live boxed `Float`.
pub unsafe fn unbox_f64(value: *const vdynamic) -> Option<f64> {
    if value.is_null() {
        None
    } else {
        Some(unsafe { (*value).v.d })
    }
}

/// `value` as the VM passes a `Bool` it does not know the type of.
pub fn box_bool(value: bool) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(bool_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.b = value };
    }
    boxed
}

/// The `Bool` in a `Null<Bool>`, or `None` for null.
///
/// # Safety
/// `value` is null or a live boxed `Bool`.
pub unsafe fn unbox_bool(value: *const vdynamic) -> Option<bool> {
    if value.is_null() {
        None
    } else {
        Some(unsafe { (*value).v.b })
    }
}

/// Calls a Haxe function value and returns its result as the VM boxes it:
/// null for `Void` or a null.
///
/// # Safety
/// `closure` is live, `args` are boxed as its parameters are typed, and it
/// does not throw. A binding sees to the last by wrapping what the user
/// passed: a throw here would jump past every Rust frame on the way out.
pub unsafe fn call(closure: *mut vclosure, args: &mut [*mut vdynamic]) -> *mut vdynamic {
    unsafe { hl_dyn_call(closure, args.as_mut_ptr(), args.len() as c_int) }
}
