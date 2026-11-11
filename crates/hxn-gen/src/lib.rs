//! From a library's source to everything a runtime needs to call it.
//!
//! [`parse`] reads the items a library marked with `#[hxn::interface]` and
//! `#[hxn::methods]` into a [`Library`]. The emitters write from that:
//!
//! | Output | From |
//! |---|---|
//! | the manifest, which is committed and reviewed | [`manifest`] |
//! | the adapter over the plain C ABI, for hxcpp | [`c_adapter`] |
//! | the adapter of HashLink primitives | [`hl_adapter`] |
//! | the C header for the first | [`c_header`] |
//! | the C++ glue hxcpp calls it through | [`cpp_glue`] |
//! | the Haxe classes | [`haxe_hl`], [`haxe_cpp`] |
//!
//! The library itself is not changed and gains no runtime. Each adapter is
//! the text of a file that a thin crate of its own includes, usually from a
//! build script, so the library is compiled once for whatever is built.

mod c;
mod haxe;
mod later;
mod lent;
mod manifest;
mod model;
mod names;
mod parse;
mod rust;

pub use c::{c_header, cpp_glue};
pub use haxe::{File, haxe_cpp, haxe_hl};
pub use manifest::manifest;
pub use model::{Error, Interface, Library, Method, Number, Param, Receiver, Type};
pub use parse::{parse, parse_crate};
pub use rust::{c_adapter, hl_adapter};
