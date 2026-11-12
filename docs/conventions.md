# Conventions

The rules that hxn's code, its bindings and its tests follow.
[design.md](design.md) explains the reasons; this document is the checklist.

## Crates

- **`hxn-abi` has no dependencies and exports no symbols.** It is linked into
  every library, once per runtime, so any symbol it exported would be defined
  once per library in the same process. A function the host must call, such
  as `release`, is re-exported by each library under its own name.
- **`hxn-future` contains no `unsafe`.** It is `#![forbid(unsafe_code)]`.
- **Only `hxn-hl` imports a runtime's symbols**, and it defines none.
- **A library's core is plain Rust.** It depends on `hxn` for the marker
  attributes and on `hxn-future` if it has asynchronous results, and on no
  runtime. It never selects a runtime with a cargo feature.
- **Adapter crates are thin.** They contain only the generated adapter, and
  no code written by the library author.
- **`hxn-cli` writes nothing into a library's directory except its
  manifest.** Everything else it generates goes under cargo's target
  directory, in `hxn/<library>`.
- **`hxn-cli` embeds every Haxe class and header of hxn's own.** A class
  added to `haxe/` is added to its list, which a test checks.

## The C boundary

- An exported function returns an `hxn_status`. Zero is success, negative
  values are hxn's, positive values are the library's.
- A result is written through a final `out` parameter, and an error message
  through `err`. `out` is not written on failure, and `err` is not written on
  success.
- A `bool` is an `int32_t` at the boundary.
- Text and bytes are passed as an `hxn_slice` by value and are UTF-8.
- Returned text and bytes are an `hxn_buf`, freed by the host through the
  buffer's own `free`, exactly once.
- A handle is an opaque `hxn_handle *`. Only the library that created it reads it.
- A callback is `{call, env, drop}`. `call` takes `env` as its first
  parameter, returns an `hxn_status` and writes its result through a final
  `out` parameter. It must return normally.
- Waits take a timeout in milliseconds; a negative value means no timeout.
- Versions and ids the host sees are non-negative `int32_t`; `-1` means
  "closed" or "nothing".

## Names

| Thing | Name |
|---|---|
| exported function | `<library>_<interface>_<method>`, the interface in snake case |
| HashLink primitive resolver | `hlp_<interface>_<method>` |
| HashLink abstract | `<library>_<interface>` |
| a shared interface's built-in functions | `<library>_<interface>_same`, `_weak`, `_weak_upgrade`, `_weak_close` |
| weak reference to a shared interface | Haxe class `<Interface>Weak`, HashLink abstract `<library>_<interface>_weak` |
| release function for any handle | `<library>_release` |
| the library's queue of ready results | `<library>_outbox_wait`, `<library>_outbox_next` |
| waiting on and watching a future, stream or signal | `<library>_future_wait`, `_future_state`, `_future_watch`; the same for `stream` and `signal` |
| taking the value one carries | `<library>_future_take_<kind>`, `<library>_stream_next_<kind>`, `<library>_signal_get_<kind>`; the kind is `unit`, `bool`, `int`, `long`, `float`, `text`, `bytes`, the name of a narrower number type, or the snake-case name of an interface or data type, prefixed with `option_`, `list_` or `map_` for those wrappers. Two carried types that map to the same name are rejected |
| fetching the typed error of the last failure | `<library>_thrown_take`, generated only when a function has a `#[hxn::data]` error type |
| exception class for a typed error | Haxe class `<Name>Exception`, with a trailing `Error` removed from the name first |
| serializer for a tagged enum | Haxe class `<Name>Wire` |
| native memory held by a library's values, in bytes | `<library>_memory_held` |
| whether the library requested a collection | `<library>_memory_wanted` |
| C header | `<library>_abi.h` |
| hxcpp glue header | `<library>_glue.h`, namespace `<library>_glue` |
| hxcpp extern class declaring the glue | `Hxn<Library>Native`, each function under its glue name |
| Haxe method | the Rust name in camel case |
| a function outside an `impl` | `<library>_<class>_<function>`, named as a method of that class would be |
| Haxe class a binding needs in addition to the library's own | `Hxn<Library><Role>`: `HxnCounterMemory` |

An interface is not named `Atomic`, `Memory`, `Future`, `Stream`, `Signal` or
`Outbox`: the adapters use those names for their own exports. No interface is named `<Interface>Weak` when a shared
`<Interface>` exists, and a shared interface has no function named `same` or `weak`.

A C header's name never differs from a Haxe class's name only by case. hxcpp
generates `Counter.h` for a class `Counter`, and on a case-insensitive
filesystem `#include "counter.h"` would then find the generated header.

## Rust side

- An interface's name is unique within a library, whatever module it is in,
  and the interface is reachable at the crate's root: one defined in a module
  is re-exported there. A `#[hxn::methods]` block refers to its interface by
  its bare name.
- A function exposed to Haxe takes `&self`, `&mut self` or `self`, or no
  receiver. `self` by value closes the Haxe object. `new` is the constructor.
  No generics.
- A function outside an `impl` is marked `#[hxn::function]`. Haxe has no
  free functions, so it is a static method of a class: the class named after
  the library (`audio_tools` becomes `AudioTools`), or the class given by
  `class = "Name"` in the attribute. Either may be an interface's class, which
  the function is then added to, or a class that holds only functions.
  The function is `pub`, reachable at the crate's root as an interface is,
  not named `new`, and its name is unique within its class.
- Text is accepted as `&str` and returned as `String`; bytes are accepted as
  `&[u8]` and returned as `Vec<u8>`.
- A number is `i32`, `i64` or `f64`, or one the adapter converts: `i8`,
  `i16`, `u8`, `u16`, `isize` and `usize` are passed as a Haxe `Int`, `u32` as
  a `UInt` with the same bits, `u64` as a `haxe.Int64` with the same bits,
  `f32` as a `Float`. A value that does not fit, as a parameter or as a return
  value, fails the call. No type wider than 64 bits is passed.
- **On wasm32 a size is 32 bits.** `i64` and `u64` are 64 bits there as
  everywhere, so a quantity that can exceed 4 GiB, such as a file length,
  an offset into a file or a count of nanoseconds, is a `u64` or an `i64`,
  not a `usize`: a `usize` would be correct natively and wrap in the wasm
  build of the same library.
- Plain data is a struct or an enum marked `#[hxn::data]`, `pub` and
  reachable at the crate's root, with a name that no interface uses. A struct has
  named fields, all `pub`, each a `bool`, a fixed-width number,
  `String`, `Vec<u8>`, other data, or an `Option`, `Vec`, `Box`, `HashMap`
  or `BTreeMap` of these. Map keys are `String` or an integer of at most 32
  bits. An enum is either C-like (no variant has fields; explicit
  discriminants are allowed) or tagged (some variants have fields, named or
  unnamed; no explicit discriminants). Data is accepted by value or by `&`, a
  list also as `&[T]`, and returned by value. `Option`, `Vec` and maps of
  data are accepted wherever data is.
- Recursive data goes through a `Vec` or a `Box` and may nest at most
  `hxn_abi::wire::Reader::DEPTH` levels. Use a `Vec` for a long sequence,
  not a chain of boxes.
- Data carried by a signal must be `Clone`.
- Another interface is accepted as `&T` or `&mut T` and returned as `T`, with
  `Option` for one that may be absent. It is never accepted by value and never
  returned by reference.
- A value with more than one owner is an interface marked
  `#[hxn::interface(shared)]`. Its functions take `&self`, never `&mut self`
  or `self`, and it is `Send + Sync`. It is accepted as `&T` to use it and as
  `Arc<T>` to keep a reference, never as `&Arc<T>` or `&mut T`, and returned
  as `T` or `Arc<T>`. `Arc` of an interface not marked shared is not passed
  across the boundary.
- An asynchronous result is returned as a `Future<T>`, a `Stream<T>` or a
  `Signal<T>` of `hxn-future`, by a function that itself returns immediately.
  It carries `bool`, a number, `String` or data; a future or a stream may
  also carry `Vec<u8>`, an interface, a shared one as `Arc<T>`, or `()`.
- An `async fn` is exposed as a function returning a future of its return
  value. Its receiver is `self` or, for a shared interface, `&self`; its
  parameters are numbers, `bool`, `&str` and `&[u8]`, which are copied, and
  shared interfaces. It does not accept anything borrowed only for the call,
  and it is not `new` or `#[hxn(blocking)]`. Its future must be `Send`.
- A Haxe callback is accepted as `impl Fn(..) -> Result<T, E>`, or `FnMut` or
  `FnOnce`, with `E` a type parameter of the function used only by that
  callback.
  Its parameters are `bool`, `i32`, `f64`, `&str`, `&[u8]`, data (by value
  or by `&`), or an object whose ownership passes to Haxe (`T`, or `Arc<T>`
  of a shared interface). It returns `bool`, `i32`, `f64`, `String`,
  `Vec<u8>`, data or `()`. When the callback fails, the library returns. A function marked `#[hxn(blocking)]` does not accept a callback.
- A cell shared with Haxe is an `AtomicI32` or an `hxn::AtomicFlag`, never
  an `AtomicBool`. It is accepted as a reference, borrowed for the call, and
  returned as an `Arc`, owned by the library.
- A value that holds a large amount of memory keeps an `hxn::Weight` recording
  the amount, and its interface is marked `#[hxn::interface(weighed)]`. The
  weight is updated when the size changes. Without the mark the weight is
  counted, but creating the object does not check the meter.
- A library that wants a collection calls `hxn::collect_soon()`. It does not
  call the runtime, and does not assume the collection has happened.
- Errors are a `Result` whose error implements `Display`. Do not panic to
  report a failure: a wasm build cannot catch it.
- For errors that Haxe code needs to distinguish, use a `#[hxn::data]` type
  as the error: an enum with one variant per cause, or a record. Name it
  directly in the `Result`, not through a type alias. Haxe catches it as
  `<Name>Exception`; a trailing `Error` is dropped first, so `PantryError`
  gives `PantryException`. Do not use that class name, or `<Name>Wire` for
  a tagged enum, for anything else.
- Mark a function that waits with `#[hxn(blocking)]`.
- In `stepped`, `Poll::Pending` means there is more to do now, not that the
  work is waiting on something. Work that waits uses `spawn` with `async`.
- Do not start threads or block in a library that should run on wasm. Use
  `spawn` or `stepped`.
- Inside hxn, no library code runs while one of hxn's locks is held. A value
  is swapped or taken out under the lock and dropped after the lock is
  released.
- hxn does not call a host function to report a result. It queues or sets the
  value, and the host retrieves it.

## Bindings

- **One `hxn.Inbox` per library**, shared by every future, stream and signal
  the binding creates.
- **Wrap a primitive in a lambda where a function value is needed.** Pass
  `raw -> Native.take(raw)`, not `Native.take`.
- **Wrap a callback passed to native code** so that it catches exceptions,
  calls `hxn.Callback.failed` and returns the failure value, and make the
  native call inside `hxn.Callback.during`. The failure value is null, or
  false for a callback that returns nothing.
- **Store a function used by later calls in a field of the Haxe object** and
  pass it to each call. Native code does not root host functions.
- **Write through.** When a set is made through the binding, also set the
  followed signal.
- **Every class that owns a handle has `close()` and a finalizer.** The
  finalizer only frees: it runs during a collection.
- **One `hxn.Gc.Meter` per library**, referenced by every class of the binding
  so that it exists whichever classes a program uses. A weighed class checks it
  once the new object owns its value, never before and never in a finalizer.
- **A binding collects through `hxn.Gc`**, not through a runtime's own call.
- **Two objects are compared with `same`**, never with `==`, when their
  interface is shared.
- **A return value is wrapped in a new object of its interface's class**,
  which becomes its owner only once the library has transferred the value.
- **A cell the library returns has no `close()`.** The object holding it
  keeps the memory alive, so its address is valid for as long as it is used.
- **A null object is passed to native code as a null handle**, even when the
  parameter is not an `Option`. The adapter then rejects it with the same
  error on every runtime. The binding must not read the object's handle
  field itself: hxcpp does not check for null before a field access.
- On hxcpp, a raw pointer field is `@:unreflective`, and a call that waits is
  made inside `hxn::Blocking`. Failures are thrown by `hxn::check` after the
  library has returned.
- On HashLink, a primitive that can fail is one `prim` call and nothing else,
  so a throw leaves no Rust value behind, and a call that waits is made
  inside `blocking`.

## Threads

- A library's own threads never call into a runtime or touch its heap.
- `hxn-hl`'s functions are only for the thread the runtime called in on.
- A reactive graph belongs to one thread.
- A library's results are delivered on one thread: the one that first
  requested delivery, or the one that calls `Loop.sync()`.
- Waiting from Haxe is done in bounded slices with a short sleep between, so
  runtimes whose other threads run only while one sleeps still make progress.
- Where two threads each write one atomic and then read the other's, every
  one of those accesses is `SeqCst`, or the two are made under one lock.
  With anything weaker both may read the old value.

## Tests

- `tests/counter` is the fixture: one small library, hand-written bindings for
  both runtimes, and one Haxe test (`haxe/Main.hx`) that every runtime runs.
  `haxe/Generated.hx` checks the classes the generator writes.
- **A change is run on every runtime before it is considered done:**

  ```
  tests/counter/run.sh hl <vm>      # stock HashLink, and Ash
  tests/counter/run.sh cpp
  tests/counter/run.sh wasm <ash>
  tests/counter/run.sh emcc
  tests/counter/run.sh gen-hl <vm>
  tests/counter/run.sh gen-cpp
  tests/counter/run.sh gen-cppia
  tests/counter/run.sh gen-wasm <ash>
  tests/counter/run.sh gen-emcc
  ```

  Ash is run in its hybrid, JIT and interpreter modes, which differ in how
  threads and native calls behave.
- `tests/cli/run.sh` makes a library with `hxn new` and runs it with
  `hxn run`, as the readme describes, runs it again under `hxn check`, then
  packages it with `hxn package` and builds the same program against the
  package alone.
  `tests/examples/run.sh` runs each library under `examples/` and compares
  what it prints with its `expected.txt`. Both take `hl <vm>`, `cpp`,
  `cppia`, `wasm <ash>` and `emcc`, and a VM is named with its options, so
  `hl "ash --mode jit"` runs Ash's JIT.
- `tests/mobile/run.sh` runs each example on mobile toolchains: `ios-sim`
  in an iOS simulator, `android` on a device or emulator reachable by `adb`.
  It packages each library with `--target`, compiles the program against
  the package with hxcpp's defines for the platform, links the result into
  an executable (hxcpp only builds a library on these platforms) and runs
  it. Run it after changing how an hxcpp program is compiled or linked.
- **The readme's example is `examples/greeter`, which is what `hxn new`
  creates.** A test fails when the readme differs from it.
- **An example is a realistic library:** a crate, a Haxe program, its
  committed manifest, and the program's expected output, which is the same
  on every run and every runtime. It demonstrates a few features and is not
  a test; anything that has to be checked belongs in the fixture.
- **CI runs what a single machine cannot.** `.github/workflows/ci.yml` runs the
  unit tests on Linux, Windows and macOS, the lints and Miri on Linux, and
  the fixture's native modes and the CLI and example tests on HashLink and
  hxcpp on Linux and Windows, and the fixture and the CLI test as CPPIA
  scripts. It also runs the generated fixture and the examples
  through Emscripten on Linux, and the examples in an iOS simulator on macOS.
  Windows is run nowhere else. Ash, wasm under Ash, and Android are run by
  hand, as above.
- Run a runtime test under a timeout. A wrong wait hangs instead of failing.
- Anything concurrent is run repeatedly, on more than one machine.
- `cargo miri test -p hxn-abi` checks the unsafe core, and
  `cargo miri test -p hxn-future` finds interleavings a native run does not,
  more so with `-Zmiri-many-seeds`. Under Miri, tests that depend on how much
  work fits in a time slice are skipped, because Miri runs too slowly for
  them.
- A test that depends on a runtime's limitations documents this next to the
  check it skips, with the reason.
- **The WebIDL emitter's output is parsed in its tests**, by a WebIDL parser
  that is a dev-dependency only. The fixture's WebIDL is committed, like its
  JSON manifest, and a test fails when either is out of date.
- A test program ends with `hxn.Loop.exit`, not `Sys.exit`: a program that
  exits right after a collection crashes in hxcpp on Windows.
- A check does not depend on when the collector runs on its own: it forces a
  collection, or asserts a bound that holds either way.
- The pure crates have unit tests; `hxn-hl` and the adapters are tested only
  in a real runtime.
- The fixture's manifest and each example's are committed, and a test fails
  when one no longer matches its source.

## Comments, commits and issues

- A comment describes what the code does, and why it is shaped that way when
  that is not obvious. No history of what was tried, and no measurements.
- A commit message is short: what changed, and a few sentences of why when
  that is not self-evident.
- Detail lives in a `git-bug` issue: reproducers, measurements, alternatives
  weighed, what is still open. A commit cites its issue in a `git-bug:`
  trailer with the full id.
- No planning, backlog or status documents. These two documents describe the
  current design, not history or plans.
- An issue has a kind (`bug`, `perf`, `debt`) and an area
  (`area:abi`, `area:hl`, `area:cpp`, `area:future`, `area:reactive`,
  `area:wasm`, `area:gen`, `area:bind`, `area:cli`).
