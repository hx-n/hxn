# Crate boundaries

[Design](../design.md) · [Conventions](../conventions.md)

- Every library links its own copy of `hxn-abi` for each runtime. Keep the
  crate free of dependencies and exported symbols to prevent duplicate
  definitions in one process. Each library re-exports host-callable
  functions such as `release` under its own name.
- `hxn-future` enforces safe Rust with `#![forbid(unsafe_code)]`.
- Only `hxn-hl` imports a runtime's symbols, and it defines none of the
  runtime's. In an Ash side module that unwinds it defines the two libunwind
  functions Rust's unwinding calls, which neither the module nor Ash has.
- A HashLink primitive that can throw is `extern "C-unwind"`. On Ash's
  wasm build the throw is a wasm exception that passes through its frame.
- A library's core is plain Rust. It depends on `hxn` for the marker
  attributes and on `hxn-future` if it has asynchronous results, and on no
  runtime. It never selects a runtime with a cargo feature.
- Adapter crates contain only generated adapter code. Keep library
  implementation code in the core crate.
- During binding builds, `hxn-cli` writes the manifest in the library's
  directory and all other generated files under Cargo's target directory,
  in `hxn/<library>`.
- `hxn-cli` embeds every Haxe class and header of hxn's own. A class
  added to `haxe/` is added to its list, which a test checks.

See also: [Architecture and runtimes](../design/architecture.md), [Binding generation](../design/generation.md).
