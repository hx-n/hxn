//! counter-core over the plain C ABI, using the adapter hxn-gen generates.

// Every export has the ABI's safety contract: its arguments are whatever the
// host's glue passes.
#![allow(clippy::missing_safety_doc)]

include!(concat!(env!("OUT_DIR"), "/adapter.rs"));
