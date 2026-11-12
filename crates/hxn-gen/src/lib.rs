//! Generates everything a runtime needs to call a library, from the
//! library's source.
//!
//! [`parse`] reads the items a library marked with `#[hxn::interface]` and
//! `#[hxn::methods]` into a [`Library`]. The emitters write from that:
//!
//! | Output | From |
//! |---|---|
//! | the manifest, which is committed and reviewed | [`manifest`] |
//! | the adapter over the plain C ABI, for hxcpp | [`c_adapter`] |
//! | the adapter exporting HashLink primitives | [`hl_adapter`] |
//! | the C header for the C ABI adapter | [`c_header`] |
//! | the C++ glue hxcpp calls it through | [`cpp_glue`] |
//! | the Haxe classes | [`haxe_hl`], [`haxe_cpp`] |
//! | declarations of the hxcpp classes, for a CPPIA script | [`haxe_cppia`] |
//!
//! The library itself is not changed and depends on no runtime. Each adapter
//! is emitted as the text of a source file that a separate thin crate
//! includes, usually from a build script, so the library is compiled once for
//! whichever binding is built.

mod c;
mod data;
mod haxe;
mod later;
mod lent;
mod manifest;
mod model;
mod names;
mod parse;
mod rust;

pub use c::{c_header, cpp_glue};
pub use haxe::{File, haxe_cpp, haxe_cppia, haxe_hl};
pub use manifest::manifest;
pub use model::{
    Case, Data, Enumeration, Error, Field, Interface, Library, Method, Number, Param, Receiver,
    Record, Tagged, Type, Variant,
};
pub use parse::{parse, parse_crate};
pub use rust::{c_adapter, hl_adapter};
