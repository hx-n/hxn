//! The crate a library written for hxn depends on.
//!
//! ```
//! #[hxn::interface]
//! pub struct Counter {
//!     value: i32,
//! }
//!
//! #[hxn::methods]
//! impl Counter {
//!     pub fn new(start: i32) -> Self {
//!         Self { value: start }
//!     }
//!
//!     pub fn add(&mut self, amount: i32) -> i32 {
//!         self.value += amount;
//!         self.value
//!     }
//!
//!     #[hxn(blocking)]
//!     pub fn settle(&self) {}
//! }
//!
//! assert_eq!(Counter::new(40).add(2), 42);
//! ```
//!
//! A struct or an enum marked `#[hxn::data]` is plain data and is passed by
//! value. It becomes a Haxe class with the same fields, or an `enum abstract`
//! with the same names.
//!
//! The library itself stays plain Rust with no runtime code in it. `hxn-gen`
//! reads the marked items and writes the adapter for each runtime, the C
//! header, and the Haxe classes.

/// Puts the library's Haxe classes in a package: `hxn::package!("window");`
/// at the crate root makes them `window.Window` and so on. Without it they
/// are in the top-level package. It expands to nothing.
#[macro_export]
macro_rules! package {
    ($name:literal) => {};
}

pub use hxn_abi::memory::collect_soon;
pub use hxn_abi::{AtomicFlag, Weight};
pub use hxn_macros::{data, function, interface, methods};
