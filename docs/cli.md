# CLI usage

[Design](design.md) · [Conventions](conventions.md)

Run build, run, check and package commands in the library crate. `run`
and `check` also run a [program file](#program-files) in any directory. See
the [README](../README.md#quick-setup) for installation and a first example.

## Commands

| Command | Use |
|---|---|
| `hxn new <name>` | Create a library and Haxe program from the greeter template. |
| `hxn build <hl\|cpp>` | Generate and build a binding. |
| `hxn run <hl\|cpp\|cppia> [<file.hxml>]` | Build and run the Haxe program, or compile and run a program file. |
| `hxn check [hl] [cpp] [cppia] [<file.hxml>]` | Compare program output across runtimes; defaults to HashLink and hxcpp. |
| `hxn package [hl] [cpp]` | Create a [haxelib package](packaging.md). |
| `hxn doctor [--target <triple>]` | Check runtime prerequisites or a Rust binding target. |
| `hxn import <header.h>` | [Import a C library](design/c-import.md). |
| `hxn plugins` | List commands supplied by installed hxn libraries. |
| `hxn <namespace> ...` | Run an installed library's CLI extension. |
| `hxn version` | Print the command version. |

## Targets and settings

| Option | Commands | Effect |
|---|---|---|
| `--vm <command>`, `HXN_VM` | `run`, `check` | The VM that runs HashLink bytecode, with its options. `hl` by default. `ash` is Ash's hybrid mode, which starts at once, for development and server workloads. `"ash --mode jit"` and `"ash --mode interp"` are its other modes. |
| `--ash <command>`, `HXN_ASH` | `build`, `run`, `check` | The Ash command and its options for what Ash builds: a program file that `build` builds ahead of time, and every `hl --wasm` program. For example `"ash --no-fma"`. |
| `--wasm` | `build`, `run`, `check`, `package` | Builds the runtime's wasm form. |
| `--threads` | with `--wasm` | Builds a threaded library and program over one shared memory, and passes `-D hxn_threads`, by which `hxn.Bind.lib` selects that library. The library's workers run on threads of their own: Web Workers in a browser, worker threads under node. |
| `--browser` | `run`, `check`, with `--wasm` | Runs the program in a web page in headless Chrome, and prints what it prints. |
| `--target <triple>` | `build`, `package` | Builds the binding for another platform, such as `aarch64-linux-android`, as a static library. |

### Wasm

| | `hl --wasm`, built by Ash | `cpp --wasm`, built by hxcpp with Emscripten |
|---|---|---|
| Needs | Nightly Rust with `rust-src`: hxn rebuilds Rust's standard library. | The Emscripten SDK and a compatible hxcpp. |
| Library | `lib<library>.a`, which Ash links into the program with `--native-lib`. | Linked by hxcpp. |
| `--threads` | Target `wasm32-wasip1-threads`, which needs an Ash with that target's runtime object. A library that serves a browser API through `ash_host_agent` needs it, since the page's browser agent reads the program's memory. | Rust's standard library rebuilt with atomics (nightly Rust with `rust-src`), and `-D HXCPP_MULTI_THREADED_APP`. The program links with Emscripten's `-pthread`, and `main` runs on a pthread (`-sPROXY_TO_PTHREAD`), as an Ash program runs in a Worker. |
| `--browser` | Ash writes the page. `ash serve` serves it with the cross-origin isolation headers that a threaded program needs. node 22 or later drives Chrome over the DevTools protocol. | hxcpp writes the page and Emscripten's `emrun` serves it, with the cross-origin isolation headers shared memory needs. |
| Futures in a web page | The program runs on a thread of its own, where `await` works on a future the library's threads settle. | Without `--threads`, the program runs on the page's thread, and the browser settles a future only while that thread's event loop is free: use `then`, because `await` blocks it. With `--threads`, as on Ash. |

On either runtime, use `then` for a future that the browser settles on the
program's own thread. The promise of a browser API called from that thread is
an example.

A threaded hxcpp library that draws on the web page's canvas has it
transferred to its thread with its `link-args` (see
[packaging](packaging.md)). A server other than `emrun` must send
`Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`.

| Variable | Effect with `--browser` |
|---|---|
| `HXN_BROWSER` | For `cpp`, another browser that emrun knows. For `hl`, the path of Chrome, if Chrome is not in its usual place. |
| `HXN_BROWSER_ARGS` | Flags added to Chrome's command line, such as `--enable-unsafe-webgpu`. |
| `HXN_PAGE_TIMEOUT` | Seconds to wait for the program to finish, 300 by default. |
| `HXN_PAGE_CORES` | For `hl`, the core count the web page sees, which sizes its pool of thread agents, to reproduce a smaller machine. |

### Other platforms

Install the triple's Rust standard library with `rustup target add <triple>`
before `--target`. hxn needs no linker for this. The program is linked
with the platform's linker and SDK. hxcpp links the `cpp` library into
the app. For HashLink, Ash links `lib<library>.a` into the program it builds
for that platform:

```
ash --build <program> --target <triple> --native-lib lib<library>.a main.hl
```

`run` and `check` do not accept `--target`, because they run the program on
this machine.

### Other settings

`cppia` builds an hxcpp host and runs the program as a script. Arguments
after the script name go to the program. Set the Haxe directory, main class
and `haxe-flags` under `[package.metadata.hxn]`.

The command and library's `hxn` versions must match. `check` also requires
the committed API manifest to match the source before building.

`doctor --target` checks Cargo and the target's `std` archive in the active
rustc sysroot. It does not check the application's SDKs or dependencies.

## Program files

A program file is an `.hxml` file for a Haxe program that uses installed hxn
libraries and has no crate of its own, such as an application or a test
program of a Haxe library:

```
-lib greeter
-cp src
-main Main
```

```
hxn run hl app.hxml
hxn run cpp --wasm --threads app.hxml
hxn check hl cpp cppia app.hxml
```

The file specifies the class paths, the main class, the libraries and the
defines. It has no output line, because hxn adds one for each runtime. hxn
runs haxe in the directory of the file, so relative paths in the file
resolve there. The output goes to `bin/<file name>/<target>` in that
directory.

hxn builds no binding for a program file. `hxn.Bind.lib`, which the `-lib`
of each hxn library runs, copies the library's native file next to the
program, and hxn adds the defines by which it selects that file:

- `--wasm` and `--threads` work as they do for a library. For `hl --wasm`,
  hxn gives each `lib*.a` file next to the program to Ash with
  `--native-lib`.
- `--browser`, `--ash` and `--vm` work as they do for a library.
- `hxn build <hl|cpp|cppia> <file.hxml>` builds the program for distribution
  without running it, and prints the command that runs it. For `hl`, Ash
  builds it ahead of time into a native executable in the output directory.
  hxn gives Ash each `lib*.a` file next to the program with `--native-lib`,
  and adds the system libraries and frameworks that each one links to
  `ASH_LINK_ARGS`. `hxn.Bind.lib` copies that list from the package's
  `link.xml` to `lib<library>.link.xml` next to the archive. For an iOS
  target, hxn also writes the app bundle `<name>.app`, with the executable,
  an `Info.plist` and the files next to the program, and signs it without an
  identity, which the simulator accepts. `-D hxn_bundle_id=<id>` sets its
  bundle identifier, which is `dev.hxn.<file name>` by default. hxn prints
  the commands that install and start it in the booted simulator.
  The executable and the files beside it are the program you distribute.
  For `cpp` it is hxcpp's executable, and with `--wasm` the wasm program.
- `hxn package <hl|cpp|cppia> <file.hxml>` builds the program the same way
  and copies it into one directory, an app package. The package holds the
  executable and every file it loads, and runs after it is moved. `--out`
  names the directory, by default `bin/<file name>/<target>-package`. See
  [packaging an app](packaging.md#packaging-an-app).
- `--release` and `--timings` apply to a binding, so a program file ignores
  them.
- For `cppia`, the host contains the classes of the libraries that the
  program uses. The script contains the classes of the program's own class
  paths. hxn finds the library classes with a first compile of the script,
  which lists every module that the program uses.

The hooks of the libraries that the file selects with `-lib` run as they do
for a library program. A program has no binding to build, so both build
hooks run before the compile.

## Library extensions

Installed hxn libraries can provide commands such as `hxn window ...` and
hooks for build, run and package. `hxn plugins` lists the active haxelib
repository's registrations. Commands work without a Rust source checkout;
hooks activate only for the source library and libraries selected by its
Haxe flags. See the [extension protocol](design/cli-extensions.md) and
[conventions](conventions/cli-extensions.md) when implementing one.
