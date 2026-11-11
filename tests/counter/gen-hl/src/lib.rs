//! counter-core as HashLink primitives, as hxn-gen writes them.

// Every export's contract is the ABI's: arguments are what the VM passes.
#![allow(clippy::missing_safety_doc)]

// A side module shares the program's heap, so it allocates from the program.
#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOCATOR: hl_abi::ProgramAllocator = hl_abi::ProgramAllocator;

include!(concat!(env!("OUT_DIR"), "/adapter.rs"));
