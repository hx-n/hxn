---
name: hxn
description: Expose a Rust library to Haxe with hxn, so one set of generated Haxe classes runs on HashLink, Ash, hxcpp and CPPIA, natively and on wasm. Use when writing or changing a crate annotated with #[hxn::interface], #[hxn::methods], #[hxn::function] or #[hxn::data], running the hxn command (new, build, run, check, package, import, doctor), packaging an hxn library as a haxelib, binding a C header with hxn import, or extending the hxn CLI with haxelib plugins.
---

# hxn

hxn reads a Rust crate's annotated types and functions, generates Haxe
classes shared by every runtime, and builds a native adapter for each
runtime. Run every `hxn` command in the library crate's directory.

## Workflow

1. `hxn new <name>` makes a crate and a Haxe program from the greeter
   template. An existing crate needs `hxn = { git = "https://github.com/hx-n/hxn", tag = "<version>" }`,
   and the `hxn` command at the same version: they must match.
2. Write the API in `src/lib.rs` (see below). The Haxe program is in `haxe/`
   by default; `[package.metadata.hxn]` in Cargo.toml can set its directory,
   main class and `haxe-flags`.
3. `hxn run hl` or `hxn run cpp` builds and runs it. `hxn build <hl|cpp>`
   only builds, and prints the `haxe` arguments for compiling against the
   binding.
4. `hxn check` runs the program on HashLink and hxcpp, or on the runtimes
   named (`hxn check hl --vm ash`, `hxn check cpp cppia`), and passes only
   when every run passes and prints the same output. This is the test for a
   library; CI runs it per runtime. It also fails when the committed API
   manifest (`<library>.hxn.json`) does not match the source.
5. `hxn package [hl] [cpp] --release --out <dir>` writes a haxelib that
   needs no Rust. Running it again into the same directory on another
   machine, or with `--wasm` or `--target <triple>`, adds that platform.
6. `hxn doctor` reports what this machine can build for.

Run runtimes under a timeout: a wrong wait hangs rather than failing.

## The Rust API

```rust
#[hxn::interface]          // a Haxe class over a Rust value
pub struct Greeter { greeting: String }

#[hxn::methods]            // its public methods; `new` is the constructor
impl Greeter {
    pub fn new(greeting: &str) -> Self { Self { greeting: greeting.into() } }
    pub fn greet(&self, name: &str) -> Result<String, &'static str> {
        if name.is_empty() { return Err("nobody to greet"); }
        Ok(format!("{}, {name}!", self.greeting))
    }
}

#[hxn::function]           // a static method on the library's CamelCase class
pub fn version() -> String { "1".into() }

#[hxn::data]               // copied by value; no close()
pub struct Item { pub name: String, pub count: i32 }
```

- Interfaces, data and functions are public, reachable at the crate root,
  and uniquely named. No general generics. No name of a Haxe top-level type
  (`Math`, `String`, `Map`, `Date`). `hxn::package!("name")` puts the classes
  in a Haxe package.
- Values: `bool`; `&str`/`String` as text; `&[u8]`/`Vec<u8>` as `Bytes`;
  `&mut [u8]` as writable `Bytes` (synchronous calls only); `i32` and
  smaller as range-checked `Int`; `u32` as `UInt`; `i64`/`u64` as
  `haxe.Int64` with the same bits; `f32`/`f64` as `Float`.
- Data: structs with public fields become record classes; C-like enums
  become `enum abstract`; tagged enums become Haxe enums. `Option<T>`,
  `Vec<T>`, maps, sets and nonempty tuples are supported.
  `#[hxn(default)]` gives a field a default.
- Ownership: take interfaces as `&T`, `&mut T` or moved `T` (which closes
  the Haxe object, even on failure); return owned `T`, `Option<T>` or
  `Vec<T>`. No borrowed returns. `#[hxn::interface(shared)]` keeps an `Arc`,
  needs `Send + Sync` and `&self` methods, and accepts `&T` or `Arc<T>`.
- Errors: `Result<T, E>` with `E: Display` throws `haxe.Exception`. A named
  `#[hxn::data]` error throws `<Name>Exception` with the data in `.error`.
- Async: return `hxn_future::Future<T>`, `Stream<T>` or `Signal<T>` at once,
  from `pending()`, `channel()` or `signal()`. An `async fn` becomes a future.
  Use `spawn` for work that waits, `stepped` for computation, and mark a call
  that blocks `#[hxn(blocking)]`. Do not start threads in a portable library.
- Callbacks: `impl Fn`/`FnMut`/`FnOnce` returning `Result<T, E>`, called
  during the call on the Haxe thread. Native code never keeps a Haxe
  function: Haxe passes it again on each call.

## The Haxe side

- Every interface object holds its Rust value until `close()`; call it when
  done. A class with a `close` method of its own is released by the
  collector instead.
- `Future`: `await()` or `then(callback)`; `map`, `flatMap` and `recover`
  compose futures, and `Future.resolved` and `Future.rejected` make settled
  ones. `Stream`: iterate or `listen`.
  `Signal`: `Reactive.follow`. Results arrive through `hxn.Loop` on the
  Haxe event loop; an application with its own loop sets `Loop.manual` and
  calls `Loop.sync()`.
- A packaged library is used with `-lib <library>`; its `extraParams.hxml`
  runs `hxn.Bind.lib`, which adds the classes, passes hxcpp its paths, and
  copies the `.hdll` beside the `.hl` output. It depends on the `hxn`
  haxelib at the version that generated it.
- Hand-written Haxe goes in the crate's `sources/` directory: local builds
  and the package put it on the class path. `init = ["pkg.Class.fn()"]` under
  `[package.metadata.hxn]` lists macros the package's `extraParams.hxml` runs
  after `hxn.Bind.lib`, such as one registering the library's own macros.

## Other platforms

- `--wasm`: `hxn run hl --wasm` built by Ash (nightly Rust with `rust-src`),
  `hxn run cpp --wasm` built by hxcpp with Emscripten
  (`rustup target add wasm32-unknown-emscripten`). A packaged wasm library
  is `lib<library>.a`, which Ash links with
  `ash --build main.wasm --target wasm32-wasip1 --native-lib lib<library>.a main.hl`.
  `--threads` builds both threaded over shared memory (`-D hxn_threads`
  selects that library), and runs the program on a thread of its own;
  `--browser` runs the program in a web page in headless Chrome. An hxcpp
  program built without `--threads` runs on the web page's thread, where it
  delivers futures with `then`: `await` blocks that thread. A threaded hxcpp
  program can `await` a future its library's threads settle, but still needs
  `then` for one the browser settles on its own thread, such as WebGPU's.
- iOS and Android: `hxn package cpp --target aarch64-apple-ios` (or
  `aarch64-linux-android`, `aarch64-apple-ios-sim`) after
  `rustup target add <triple>`. For HashLink the package holds
  `lib<library>.a`, which Ash links:
  `ash --build main --target <triple> --native-lib lib<library>.a main.hl`,
  with `-D hxn_triple=<triple>` when compiling the Haxe.
- `hxn run cppia` builds an hxcpp host with the binding and runs the program
  as a script.

## Binding a C library

`hxn import header.h --source file.c` (or `--link <library>`) uses clang to
write a Rust crate and `<name>.webidl`. Review the WebIDL and add what a
header cannot say: `[HxnStatus]` on an `int` status return,
`[HxnFree=function]` on a result that must be freed. `hxn build` regenerates
the bindings from it.

For real-time audio, inspect hxn-av's existing PCM/decoder support before
adding conversion or codec code. Keep rendering and resource retirement
independent of Haxe callbacks and scheduling. Validate browser activation,
mobile suspend/resume and device changes on the actual platforms; a native
or compile-only pass does not cover them.

## CLI extensions

`hxn plugins` lists extensions from installed `hxn-*` haxelibs. A library
owns a namespace, such as `hxn window ...`; consumers need no source checkout
or shell helpers. Workflow hooks activate only for the source library and
haxelibs selected by its `haxe-flags`, including resolved dependencies.

For a provider, declare `[package.metadata.hxn.cli]` with `version = 1`,
`namespace`, an argv-array `command`, and `hooks`. `{root}` in an argument
expands to the package directory. Ship a Haxe tool in `sources/`, for example
`command = ["haxe", "-cp", "{root}/sources", "--run", "assets.Tool"]`.
Packaging preserves this as `hxn.cli` in `haxelib.json`. Optional `files`
lists relative tool files/directories to ship, including native executables.
Binary page assets can ship through `files` and build-hook `page_files`.
Keep test-only runners, mobile app hosts and fixtures under `tests/`, outside
published CLI commands and assets. Run these tests against installed packages;
keep library test hosts distinct from application packaging/deployment tooling.
Also test direct `haxe -lib` use: an installed library must arrange its
assets through an init macro when it needs them without CLI hooks, including
unthreaded Emscripten.

Tools receive one versioned JSON request on stdin. Commands print ordinary
output and return their exit status. Hooks log to stderr and return JSON
with `version: 1`; build hooks may add `haxe_args` pairs (`-D`, `-cp`,
`--macro`) and `page_files`. Hooks are `before-check`, `before-build`, `after-build`,
`before-run`, `after-run`, `before-package` and `after-package`. Built-in
namespaces are reserved, providers run in haxelib-name order, and failures
stop the workflow. Read `docs/design/cli-extensions.md` and
`docs/conventions/cli-extensions.md` for the protocol and testing rules.

## Reference

In the hxn repository: `docs/cli.md`, `docs/packaging.md`,
`docs/conventions/rust-api.md` (the full rules), `docs/design/` (data,
handles, async, callbacks) and `examples/`, each a crate with a Haxe program.
