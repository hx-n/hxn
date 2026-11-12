//! hxn's HashLink support: only the parts an `.hdll` built on [`hxn_abi`]
//! needs from the VM's garbage collector.
//!
//! - [`manage`] transfers a [`Handle`] to the garbage collector, which
//!   releases it.
//! - [`Root`] keeps a VM value alive while Rust holds it.
//! - [`string`], [`utf16`] and [`bytes`] copy text and bytes across the
//!   boundary, because a Haxe `String` is UTF-16 on HashLink and a Rust string
//!   is UTF-8.
//! - [`prim`] throws a failure to Haxe after Rust has dropped everything it
//!   held.
//! - [`blocking`] lets the garbage collector run while a call blocks.
//! - [`call`], with [`box_i32`] and [`unbox_i32`], calls a Haxe function
//!   value. A callback's trampoline is built from these.
//!
//! Like `hl_abi`, it imports the runtime's symbols and defines none, so the
//! library that links it must leave them undefined for the VM to resolve.
//!
//! Every function here must be called on the thread the VM called in on. Stock
//! HashLink aborts when a thread not registered with it allocates or changes a
//! root.

use std::ffi::{c_int, c_void};

use hl_abi::hl_type;
use hxn_abi::{Failure, Fault, Handle, RawHandle, Slice, SliceMut, catch, release};

// Re-exports the `hl_abi` items a generated adapter uses, so that its crate
// depends on this one alone.
pub use hl_abi::{define_prim, vbyte, vdynamic, vstring};

unsafe extern "C" {
    // On Windows a DLL's data symbol is read through its `__imp_` import
    // pointer instead of being linked directly.
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

/// The object Haxe holds for a handle: an abstract value with a finalizer.
/// The finalizer pointer is the first word, where the garbage collector
/// expects it.
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

/// Moves `value` into a handle the garbage collector owns. Returns null if the
/// VM cannot allocate it, in which case `value` is dropped.
///
/// If `value` is never taken, it is dropped during a collection, so its `Drop`
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

/// Returns the handle `managed` owns, if it is a handle to a `T`.
///
/// # Safety
/// `managed` is null or was returned by [`manage`] in this library, and the VM
/// keeps it reachable for `'a`, as it does for an argument during the call.
pub unsafe fn handle<'a, T: Send + 'static>(
    managed: *const Managed,
) -> Result<&'a Handle<T>, Fault> {
    if managed.is_null() {
        return Err(Fault::Null);
    }
    unsafe { Handle::from_raw((*managed).handle) }
}

/// A GC root: keeps a VM value alive, and at this address, for as long as the
/// `Root` exists.
///
/// Not `Send`: it must be dropped on the thread that created it.
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

/// Converts a Haxe `String` to a Rust `String`. Returns `None` for null.
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

/// The VM's layout of a `haxe.io.Bytes`: its fields in the order the class
/// declares them. A primitive that accepts one declares the parameter type as
/// `OiB_`.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct vbuffer {
    t: *mut hl_type,
    length: c_int,
    bytes: *mut vbyte,
}

/// Returns a copy of the contents of a `haxe.io.Bytes`, or `None` for null.
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

/// Wraps an `hl.Bytes` pointer and a length as a slice borrowed for the call.
///
/// # Safety
/// `data` is readable for `length` bytes for `'a`, or `length` is not positive.
pub unsafe fn slice<'a>(data: *const vbyte, length: c_int) -> Slice<'a> {
    unsafe { Slice::from_raw(data, usize::try_from(length).unwrap_or(0)) }
}

/// Wraps an `hl.Bytes` pointer and a length as bytes the call writes into.
/// The garbage collector does not move them, so the Haxe `Bytes` sees what
/// was written.
///
/// # Safety
/// `data` is writable for `length` bytes for `'a` and nothing else reads or
/// writes them during `'a`, or `length` is not positive.
pub unsafe fn slice_mut<'a>(data: *mut vbyte, length: c_int) -> SliceMut<'a> {
    unsafe { SliceMut::from_raw(data, usize::try_from(length).unwrap_or(0)) }
}

/// Copies `text` into VM-owned, null-terminated UTF-16, the input
/// `String.fromUCS2` accepts. Returns null if it is too long for the VM.
pub fn utf16(text: &str) -> *mut vbyte {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let Some(size) = units.len().checked_mul(size_of::<u16>()) else {
        return std::ptr::null_mut();
    };
    unsafe { copy(units.as_ptr().cast(), size) }
}

/// Returns a VM-owned copy of `data`, or null if it is too long for the VM.
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

/// Implements the body of a primitive: returns the value `body` returns, or
/// throws its failure or panic to Haxe as an exception carrying the message.
/// A failure's value is stored for the binding to retrieve: see
/// `hxn_abi::thrown`.
///
/// The VM throws with a non-local jump to its handler, which skips every Rust
/// frame in between without running destructors. `body` has returned and the
/// failure has been dropped before the jump.
///
/// # Safety
/// The primitive's own frame holds no value with a destructor while this
/// runs. That holds if this call is the primitive's entire body.
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

// Builds the value `hl_error` throws: a dynamic holding UTF-16 bytes.
fn exception(message: &str) -> *mut vdynamic {
    let text = utf16(message);
    let value = unsafe { hl_alloc_dynamic(bytes_type()) };
    if !value.is_null() {
        unsafe { (*value).v.bytes = text };
    }
    value
}

/// Runs `body` with this thread marked as blocked, so the garbage collector
/// can run without waiting for it. For a call that blocks.
///
/// # Safety
/// `body` does not allocate from the VM, call into it, or write to memory the
/// VM owns. Reading arguments the caller still holds is allowed.
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

/// A Haxe function value. Opaque: it is only passed back to the VM.
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

/// Boxes `value` in a dynamic, which is how the VM passes an `hl.Bytes` whose
/// type is not statically known.
pub fn box_bytes(value: *mut vbyte) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(bytes_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.bytes = value };
    }
    boxed
}

/// Boxes `value` in a dynamic, which is how the VM passes an `Int` whose type
/// is not statically known.
pub fn box_i32(value: i32) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(i32_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.i = value };
    }
    boxed
}

/// Returns the `Int` in a `Null<Int>`, or `None` for null.
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

/// Boxes `value` in a dynamic, which is how the VM passes a `Float` whose type
/// is not statically known.
pub fn box_f64(value: f64) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(f64_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.d = value };
    }
    boxed
}

/// Returns the `Float` in a `Null<Float>`, or `None` for null.
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

/// Boxes `value` in a dynamic, which is how the VM passes a `Bool` whose type
/// is not statically known.
pub fn box_bool(value: bool) -> *mut vdynamic {
    let boxed = unsafe { hl_alloc_dynamic(bool_type()) };
    if !boxed.is_null() {
        unsafe { (*boxed).v.b = value };
    }
    boxed
}

/// Returns the `Bool` in a `Null<Bool>`, or `None` for null.
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

/// Calls a Haxe function value and returns its result boxed by the VM: null
/// for `Void` or for a null result.
///
/// # Safety
/// `closure` is live, each of `args` is boxed to match the type of its
/// parameter, and the closure does not throw. A binding guarantees the last
/// condition by wrapping the function the user passed: a throw here would jump
/// past every Rust frame on the way out.
pub unsafe fn call(closure: *mut vclosure, args: &mut [*mut vdynamic]) -> *mut vdynamic {
    unsafe { hl_dyn_call(closure, args.as_mut_ptr(), args.len() as c_int) }
}
