//! counter-core over the plain C ABI, as hxn-gen writes it.

// Every export's contract is the ABI's: arguments are what the host's glue passes.
#![allow(clippy::missing_safety_doc)]

include!(concat!(env!("OUT_DIR"), "/adapter.rs"));
