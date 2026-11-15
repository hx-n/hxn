//! counter-core as HashLink primitives, using the adapter hxn-gen generates.

// Every export has the ABI's safety contract: its arguments are whatever the
// VM passes.
#![allow(clippy::missing_safety_doc)]

// On wasm the library shares the Ash program's heap, so it uses the
// program's allocator.
#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOCATOR: hl_abi::ProgramAllocator = hl_abi::ProgramAllocator;

include!(concat!(env!("OUT_DIR"), "/adapter.rs"));
