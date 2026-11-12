//! The two libunwind functions Rust's `panic=unwind` calls on WASI, for a
//! side module that unwinds with wasm exceptions. On WASI Rust expects them
//! from the toolchain's libunwind, which neither the module nor Ash's
//! runtime has, so the module defines them, as Rust's own wasm shim does
//! for `wasm32-unknown-unknown`. A panic never leaves the module, so the
//! exception tag is the module's own.

use std::ffi::c_int;

// The tag LLVM's wasm exception handling throws and catches C++ exceptions
// with, which Rust's unwinding also uses.
core::arch::global_asm!(
    ".globl __cpp_exception",
    ".tagtype __cpp_exception i32",
    "__cpp_exception:",
);

unsafe extern "C-unwind" {
    /// LLVM lowers this intrinsic to the `throw` instruction.
    #[link_name = "llvm.wasm.throw"]
    fn wasm_throw(tag: i32, ptr: *mut u8) -> !;
}

// LLVM's tag number for a C++ exception.
const CPP_EXCEPTION_TAG: i32 = 0;

// `_URC_FOREIGN_EXCEPTION_CAUGHT`.
const FOREIGN_EXCEPTION_CAUGHT: c_int = 1;

/// The start of libunwind's `_Unwind_Exception`, as far as these use it.
#[repr(C)]
pub struct Exception {
    class: u64,
    cleanup: Option<unsafe extern "C" fn(c_int, *mut Exception)>,
}

/// # Safety
/// `exception` points to an `_Unwind_Exception`.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn _Unwind_RaiseException(exception: *mut Exception) -> c_int {
    unsafe { wasm_throw(CPP_EXCEPTION_TAG, exception.cast()) }
}

/// # Safety
/// `exception` points to an `_Unwind_Exception`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_DeleteException(exception: *mut Exception) {
    if let Some(cleanup) = unsafe { (*exception).cleanup } {
        unsafe { cleanup(FOREIGN_EXCEPTION_CAUGHT, exception) };
    }
}
