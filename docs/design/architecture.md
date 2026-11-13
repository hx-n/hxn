# Architecture and runtimes

[Design](../design.md) · [Conventions](../conventions.md)

## Structure

HashLink, hxcpp and their wasm builds all support C calls. hxn uses a C ABI to
pass values between Rust and each runtime. Runtime-specific adapters convert
those values to the runtime's own representation.

- `hxn-abi` defines the `#[repr(C)]` boundary types and their safe Rust APIs.
- `hxn-hl` implements HashLink primitives over `hl_abi`.
- `include/hxn.h` provides the C declarations and hxcpp's C++ glue helpers.
- Generated Haxe classes expose the same public API on every runtime.

Each library has a core crate and two adapter crates. Cargo compiles the
library's implementation separately from its runtime adapters:

| Crate | Purpose |
|---|---|
| `<name>-core` | The library's Rust implementation, with no runtime dependency. |
| `<name>-c` | A thin static library exporting the plain C ABI, for hxcpp. |
| `<name>-hl` | A thin shared library of HashLink primitives. Its static archive is also linked into an Ash wasm side module. |

Runtime selection belongs in the adapters. Selecting a runtime with a Cargo
feature on the core would compile every dependency twice.

## Runtimes

| Runtime | Adapter | Threads | A panic | Notes |
|---|---|---|---|---|
| HashLink | `-hl` as an `.hdll` | yes | becomes a thrown error | |
| Ash, native | `-hl` as an `.hdll` | yes; cooperative in its interpreter mode | becomes a thrown error | Tested in its hybrid, JIT and interpreter modes. |
| Ash, wasm | `-hl` as a side module | Haxe threads as fibers; no Rust threads | becomes a thrown error | The module uses the program's allocator. |
| hxcpp | `-c` with C++ glue | yes | becomes a thrown error | |
| CPPIA | none of its own: a script runs in an hxcpp host that contains the binding | yes | becomes a thrown error | The host is built with `-D scriptable` and dead-code elimination off. |
| hxcpp through Emscripten | `-c` with C++ glue | none | becomes a thrown error | Rust's prebuilt standard library unwinds with wasm exceptions, which hxcpp from git also uses; hxcpp 4.3.2 uses JavaScript exceptions and does not link with it. hxcpp's collector does not complete a forced collection there, so `hxn.Gc.collect()` does nothing and a meter only counts. |
| hxcpp on iOS | `-c` with C++ glue, cross-compiled with `--target` | yes | not tested | hxcpp produces a static library for an app to link, and merges the Rust library into it. Tested with the examples in a simulator only, not on a device. |
| hxcpp on Android | `-c` with C++ glue, cross-compiled with `--target` | yes | not tested | hxcpp produces a shared library for Java to load, with the Rust library linked in. Program output goes to logcat. Tested with the examples on an emulator only. |

Rayzor's plugin ABI informed part of the manifest format. hxn currently has no
Rayzor adapter.

On runtimes without threads, hxn queues work on the calling thread. Waits
return after a time slice, and delivery polls once per event-loop iteration.
These builds use neither blocking waits nor spawned threads.

See also: [Crate boundaries](../conventions/crates.md).
