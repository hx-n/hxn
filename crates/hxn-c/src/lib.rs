//! Bindings for C libraries.
//!
//! A C header does not record what a binding needs to know: who owns a
//! returned pointer, which function frees it, whether an `int` return value
//! is a status code. So a header is imported into WebIDL, which is edited
//! and committed, and the Rust bindings are generated from the WebIDL:
//!
//! | Step | Function |
//! |---|---|
//! | C header to the model, through clang's JSON AST | [`import`] |
//! | model to WebIDL | [`write`] |
//! | WebIDL to the model | [`parse`] |
//! | model to Rust: `extern "C"` declarations and a safe API marked for hxn | [`bindings`] |
//!
//! The generated Rust is an ordinary hxn library, so the Haxe bindings come
//! from it through the rest of hxn unchanged.

mod header;
mod idl;
mod model;
mod rust;

pub use header::import;
pub use idl::{parse, write};
pub use model::{
    Api, Callback, Enum, Function, Ignored, Int, Object, Param, Role, Status, Struct, Ty,
};
pub use rust::bindings;
