# CLI usage

[Design](design.md) · [Conventions](conventions.md)

Run build, run, check and package commands in the library crate. See the
[README](../README.md#quick-setup) for installation and a first example.

## Commands

| Command | Use |
|---|---|
| `hxn new <name>` | Create a library and Haxe program from the greeter template. |
| `hxn build <hl\|cpp>` | Generate and build a binding. |
| `hxn run <hl\|cpp\|cppia>` | Build and run the Haxe program. |
| `hxn check [hl] [cpp] [cppia]` | Compare program output across runtimes; defaults to HashLink and hxcpp. |
| `hxn package [hl] [cpp]` | Create a [haxelib package](packaging.md). |
| `hxn doctor [--target <triple>]` | Check runtime prerequisites or a Rust binding target. |
| `hxn import <header.h>` | [Import a C library](design/c-import.md). |
| `hxn version` | Print the command version. |

## Targets and settings

- `--wasm` builds the selected runtime's wasm form. For Ash, hxn rebuilds
  Rust's standard library, which needs nightly Rust with `rust-src`, builds
  the library as `lib<library>.a`, and Ash links it into the program with
  `--native-lib`. Emscripten requires its SDK and compatible hxcpp.
- `--threads`, with `--wasm`, builds a threaded library and program over one
  shared memory, and passes `-D hxn_threads` so `hxn.Bind.lib` selects that
  library. The library's workers run on threads of their own: Web Workers in
  a browser, worker threads under node.
  - With `hl --wasm`, the target is `wasm32-wasip1-threads`. A library that
    serves a browser API through `ash_host_agent` needs it, since the page's
    browser agent reads the program's memory. It needs an Ash with that
    target's runtime object.
  - With `cpp --wasm`, hxn rebuilds Rust's standard library with atomics,
    which needs nightly Rust with `rust-src`, and passes
    `-D HXCPP_MULTI_THREADED_APP`; the library's generated classes compile
    and link the program with Emscripten's `-pthread`, and `main` runs on a
    pthread (`-sPROXY_TO_PTHREAD`), as an Ash program runs in a Worker. A
    library that draws on the web page's canvas has it transferred to that
    thread with its `link-args` (see [packaging](packaging.md)). `emrun`
    serves the web page with the cross-origin isolation headers shared memory
    needs; a server of your own has to send
    `Cross-Origin-Opener-Policy: same-origin` and
    `Cross-Origin-Embedder-Policy: require-corp`.
- `--browser` runs a wasm program in a web page in headless Chrome, and prints
  what it prints. With `cpp --wasm`, hxcpp writes the page and Emscripten's
  `emrun` serves it; `HXN_BROWSER` names another browser emrun knows. With
  `hl --wasm`, Ash writes the page, `ash serve` serves it with the
  cross-origin isolation headers a threaded program needs, and node 22 or
  later drives Chrome over the DevTools protocol; `HXN_BROWSER` is Chrome's
  path when it is not installed where Chrome installs. `HXN_PAGE_TIMEOUT` is
  the seconds to wait for the program to finish in either, 300 by default.
  For Ash, `HXN_PAGE_CORES` sets the core count the web page sees, which
  sizes its pool of thread agents, to reproduce a smaller machine.
  `HXN_BROWSER_ARGS` adds flags to Chrome's command line, such as
  `--enable-unsafe-webgpu`. An hxcpp program built without `--threads` runs
  on the web page's thread and delivers a future with `then`: the browser
  settles it only while that thread's event loop is free, and `await` blocks
  it. A threaded hxcpp program, like an Ash one, runs on a thread of its own,
  where `await` works.
- `--target <triple>` makes `build` and `package` build the binding for
  another platform, such as `aarch64-linux-android`, as a static library.
  Install that triple's Rust standard library first with
  `rustup target add <triple>`. hxn needs no linker for this; the
  platform's linker and SDK are used when the program is linked.

  hxcpp links the `cpp` library into the app. For HashLink, Ash links
  `lib<library>.a` into the program it builds for that platform:

  ```
  ash --build <program> --target <triple> --native-lib lib<library>.a main.hl
  ```

  `run` and `check` do not accept `--target`, because they run the program
  on this machine.
- `cppia` builds an hxcpp host and runs the program as a script. Arguments
  after the script name go to the program.
- Set the Haxe directory, main class and `haxe-flags` under
  `[package.metadata.hxn]`. Choose the VM and its options on the command
  line or through `HXN_VM`.

The command and library's `hxn` versions must match. `check` also requires
the committed API manifest to match the source before building.

`doctor --target` checks Cargo and the target's `std` archive in the active
rustc sysroot. It does not check the application's SDKs or dependencies.
