# Architecture and runtimes

[Design](../design.md) · [Conventions](../conventions.md)

hxn uses a C ABI shared by every runtime. Each library has a plain Rust core
and two generated adapters; runtime selection never changes the core's Cargo
features.

| Component | Responsibility |
|---|---|
| `hxn-abi` | Boundary layouts, handles and ownership. |
| `hxn-hl` | HashLink primitives over `hl_abi`. |
| `include/hxn.h` | C declarations and hxcpp glue helpers. |
| `<name>-core` | Library implementation, without runtime dependencies. |
| `<name>-c` | Static C adapter for hxcpp. |
| `<name>-hl` | Shared HashLink adapter, or static Ash wasm side module. |
| Generated Haxe | The same public API on each runtime. |

## Runtime support

| Runtime | Execution and limits |
|---|---|
| HashLink | Native `.hdll`, with threads. |
| Ash native | Same `.hdll`; hybrid, JIT and cooperative interpreter modes. |
| Ash wasm | Side module using the program's allocator; Haxe fibers, no Rust threads. |
| hxcpp | Static C adapter and C++ glue, with threads. |
| CPPIA | Script in an hxcpp host built with `-D scriptable` and DCE off. |
| hxcpp / Emscripten | C adapter compiled to wasm; no threads or forced GC. Requires wasm-exception-compatible hxcpp. |
| hxcpp / iOS | Rust merged into the app's static library. Examples tested on simulator only; panics untested. |
| hxcpp / Android | Rust linked into the shared library; output through logcat. Examples tested on emulator only; panics untested. |

On Windows, HashLink links `libhl.lib` from `HXN_HL_DIR`. hxcpp uses its
static C runtime, Rust's required system libraries and a 64-bit host build.

Ash wasm rebuilds Rust's standard library with nightly, position-independent
code and wasm exceptions (`-wasm-use-legacy-eh=false`). Emscripten uses its
prebuilt standard library. Both catch panics at the boundary.

Threadless builds queue work locally and use bounded waits; see
[scheduling](async.md). Windows is validated by CI. There is no Rayzor adapter.
