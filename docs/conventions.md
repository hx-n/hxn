# Conventions

The rules hxn's own code, its bindings and its tests follow. The reasons are
in [design.md](design.md); this is the checklist.

## Crates

- **`hxn-abi` has no dependencies and exports no symbols.** It is linked into
  every library, once per runtime, so anything it exported would be defined
  once per library in the same process. What a host must call, such as
  `release`, a library re-exports under its own name.
- **`hxn-future` contains no `unsafe`.** It is `#![forbid(unsafe_code)]`.
- **Only `hxn-hl` imports a runtime's symbols**, and it defines none.
- **A library's core is plain Rust.** It depends on `hxn` for the marker
  attributes and on `hxn-future` if it has asynchronous results, and on no
  runtime. It never selects a runtime with a cargo feature.
- **Adapter crates are thin.** They hold the generated adapter and nothing a
  library author writes.
- **`hxn-cli` writes nothing into a library's directory but its manifest.**
  Everything else it makes goes under cargo's target directory, in
  `hxn/<library>`.
- **A Haxe class or header of hxn's own is carried by `hxn-cli`.** One added
  to `haxe/` is added to its list, which a test checks.

## The C boundary

- An exported function returns an `hxn_status`. Zero is success, negative
  values are hxn's, positive values are the library's.
- A result is written through a last `out` parameter, and a failure's message
  through `err`. `out` is untouched on failure and `err` on success.
- A `bool` is an `int32_t` at the boundary.
- Text and bytes are passed as an `hxn_slice` by value and are UTF-8.
- Returned text and bytes are an `hxn_buf`, freed by the host through the
  buffer's own `free`, exactly once.
- A handle is an opaque `hxn_handle *`. Only the library that made one reads it.
- A callback is `{call, env, drop}`. `call` takes `env` first, returns an
  `hxn_status` and writes its result through a last `out`. It must return
  normally.
- Waits take a limit in milliseconds; negative means no limit.
- Versions and ids the host sees are non-negative `int32_t`; `-1` means
  "closed" or "nothing".

## Names

| Thing | Name |
|---|---|
| exported function | `<library>_<interface>_<method>`, the interface in snake case |
| HashLink primitive resolver | `hlp_<interface>_<method>` |
| HashLink abstract | `<library>_<interface>` |
| a shared interface's own functions | `<library>_<interface>_same`, `_weak`, `_weak_upgrade`, `_weak_close` |
| weak reference to a shared interface | Haxe class `<Interface>Weak`, HashLink abstract `<library>_<interface>_weak` |
| release for any handle | `<library>_release` |
| the library's queue of what is ready | `<library>_outbox_wait`, `<library>_outbox_next` |
| waiting on and watching a result over time | `<library>_future_wait`, `_future_state`, `_future_watch`; the same for `stream` and `signal` |
| taking what one carries | `<library>_future_take_<kind>`, `<library>_stream_next_<kind>`, `<library>_signal_get_<kind>`; the kind is `bool`, `int`, `float`, `text` or an interface in snake case |
| bytes a library's values hold | `<library>_memory_held` |
| whether it asked for a collection | `<library>_memory_wanted` |
| C header | `<library>_abi.h` |
| hxcpp glue header | `<library>_glue.h`, namespace `<library>_glue` |
| hxcpp extern class over the glue | `Hxn<Library>Native`, each function under its glue name |
| Haxe method | the Rust name in camel case |
| a function outside an `impl` | `<library>_<class>_<function>`, as a function of that class would be |
| Haxe class a binding needs beside the library's own | `Hxn<Library><Role>`: `HxnCounterMemory` |

An interface is not named `Atomic`, `Memory`, `Future`, `Stream`, `Signal` or
`Outbox`: the adapters name their own exports after those. Nor is one named `<Interface>Weak` beside a shared
`<Interface>`, and a shared interface has no function named `same` or `weak`.

A C header never has the name of a Haxe class differing only by case. hxcpp
generates `Counter.h` for a class `Counter`, and on a case-insensitive
filesystem `#include "counter.h"` would then find the generated one.

## Rust side

- An interface is named once in a library, whatever module it is in, and is
  reachable at the crate's root: one defined in a module is re-exported
  there. A `#[hxn::methods]` block names its interface by its bare name.
- A function exposed to Haxe takes `&self`, `&mut self` or `self`, or no
  receiver. `self` by value closes the Haxe object. `new` is the constructor.
  No generics.
- A function outside an `impl` is marked `#[hxn::function]`. Haxe has no
  function without a class, so it is a static of one: the class named after
  the library, `audio_tools` giving `AudioTools`, or the one the attribute
  names with `class = "Name"`. Either may be an interface's class, which
  the function then joins, or a class that exists only to hold functions.
  The function is `pub`, reachable at the crate's root as an interface is,
  not named `new`, and named once in its class.
- Text is taken as `&str` and returned as `String`; bytes are taken as
  `&[u8]` and returned as `Vec<u8>`.
- A number is `i32`, `i64` or `f64`, or one the adapter converts: `i8`,
  `i16`, `u8`, `u16`, `isize` and `usize` are a Haxe `Int`, `u32` a `UInt`
  with the same bits, `u64` a `haxe.Int64` with the same bits, `f32` a
  `Float`. One that does not fit, going in or coming back, fails the call.
  Nothing wider than 64 bits crosses.
- **On wasm32 a size is 32 bits.** `i64` and `u64` are 64 bits there as
  everywhere, so a quantity that can pass 4 GiB, a file's length or a
  position in one, a count of nanoseconds, is a `u64` or an `i64` and not a
  `usize`: as a `usize` it would be right natively and wrap in the same
  library built for wasm.
- Another interface is taken as `&T` or `&mut T` and returned as `T`, with
  `Option` for one that may be absent. It is never taken by value and never
  returned by reference.
- A value held from more than one place is an interface marked
  `#[hxn::interface(shared)]`. Its functions take `&self`, never `&mut self`
  or `self`, and it is `Send + Sync`. It is taken as `&T` to use it and as
  `Arc<T>` to keep a share, never as `&Arc<T>` or `&mut T`, and returned as
  `T` or `Arc<T>`. `Arc` of an interface not marked shared does not cross.
- A result that arrives later is returned as a `Future<T>`, a `Stream<T>` or
  a `Signal<T>` of `hxn-future`, by a function that itself returns at once.
  It carries `bool`, `i32`, `f64` or `String`; a future or a stream may carry
  an interface, a shared one as `Arc<T>`. There is no `async fn` across the
  boundary: the function starts the work with `spawn` and returns its future.
- A Haxe function is taken as `impl Fn(..) -> Result<T, E>`, or `FnMut` or
  `FnOnce`, with `E` a type parameter of the function: its only use for one.
  It takes `bool`, `i32` or `f64` and gives one of those or `()`. The library
  returns when it fails. A function marked `#[hxn(blocking)]` is not lent one.
- A cell shared with Haxe is an `AtomicI32` or an `hxn::AtomicFlag`, never
  an `AtomicBool`. It is taken as a reference, lent for the call, and
  returned as an `Arc`, kept by the library.
- A value that holds much memory keeps an `hxn::Weight` saying how much, and
  its interface is marked `#[hxn::interface(weighed)]`. The weight is set
  again when the size changes. Without the mark the weight is counted but
  making the object checks nothing.
- A library that wants a collection calls `hxn::collect_soon()`. It does not
  call the runtime, and does not assume the collection has happened.
- Errors are a `Result` whose error implements `Display`. Do not panic to
  report a failure: a wasm build cannot catch it.
- Mark a function that waits with `#[hxn(blocking)]`.
- In `stepped`, `Poll::Pending` means there is more to do now, not that the
  work is waiting on something. Work that waits uses `spawn` with `async`.
- Do not start threads or block in a library that should run on wasm. Use
  `spawn` or `stepped`.
- Inside hxn itself, none of a library's code runs while one of hxn's locks is
  held. A value is swapped or taken under the lock and dropped after it.
- hxn does not call a host function to report a result. It queues or sets, and
  the host takes.

## Bindings

- **One `hxn.Inbox` per library**, shared by every future, stream and signal
  the binding makes.
- **Wrap a primitive in a lambda where a function value is needed.** Pass
  `raw -> Native.take(raw)`, not `Native.take`.
- **Wrap a function lent to native code** so that it catches, calls
  `hxn.Callback.failed` and returns the failure value, and make the native
  call inside `hxn.Callback.during`. The failure value is null, or false from
  a function that returns nothing.
- **Keep a function used by later calls in a field of the Haxe object** and
  lend it to each call. Native code does not root host functions.
- **Write through.** When a set goes through the binding, set the followed
  signal as well.
- **Every class that owns a handle has `close()` and a finalizer.** The
  finalizer frees and does nothing else: it runs inside a collection.
- **One `hxn.Gc.Meter` per library**, held by every class of the binding so
  that it exists whichever of them a program uses. A weighed class checks it
  once the new object owns its value, never before and never in a finalizer.
- **A binding collects through `hxn.Gc`**, not through a runtime's own call.
- **Two objects are compared with `same`**, never with `==`, when their
  interface is shared.
- **A returned value gets a new object of its interface's class**, which
  becomes its owner only once the library has handed the value over.
- **A cell the library returns has no `close()`.** The object holding it
  keeps the memory alive, so its address is good for as long as it is used.
- **A null object is passed as a null handle**, whether or not the library
  takes one. The library refuses it, with the same error on every runtime;
  hxcpp does not check a null object before reading its field.
- On hxcpp, a raw pointer field is `@:unreflective`, and a call that waits is
  made inside `hxn::Blocking`. Failures are thrown by `hxn::check` after the
  library has returned.
- On HashLink, a primitive that can fail is one `prim` call and nothing else,
  so a throw leaves no Rust value behind, and a call that waits is made
  inside `blocking`.

## Threads

- A library's own threads never call into a runtime or touch its heap.
- `hxn-hl`'s functions are for the thread the runtime called in on.
- A reactive graph belongs to one thread.
- A library's results are delivered on one thread: the one that first asked,
  or the one that calls `Loop.sync()`.
- Waiting from Haxe is done in bounded slices with a short sleep between, so
  runtimes whose other threads run only while one sleeps still make progress.
- Where two threads each write one atomic and then read the other's, every
  one of those accesses is `SeqCst`, or the two are made under one lock.
  With anything weaker both may read the old value.

## Tests

- `tests/counter` is the fixture: one small library, hand-written bindings for
  both runtimes, and one Haxe test (`haxe/Main.hx`) that every runtime runs.
  `haxe/Generated.hx` checks the classes the generator writes.
- **A change is run on every runtime before it is called done:**

  ```
  tests/counter/run.sh hl <vm>      # stock HashLink, and Ash
  tests/counter/run.sh cpp
  tests/counter/run.sh wasm <ash>
  tests/counter/run.sh emcc
  tests/counter/run.sh gen-hl <vm>
  tests/counter/run.sh gen-cpp
  tests/counter/run.sh gen-wasm <ash>
  tests/counter/run.sh gen-emcc
  ```

  Ash is run in its hybrid, JIT and interpreter modes, which differ in how
  threads and native calls behave.
- `tests/cli/run.sh` makes a library with `hxn new` and runs it with
  `hxn run`, as the readme says to, and `tests/examples/run.sh` runs each
  library under `examples/` and compares what it prints with its
  `expected.txt`. Both take `hl <vm>`, `cpp`, `wasm <ash>` and `emcc`, and
  a VM is named with its options, so `hl "ash --mode jit"` runs Ash's JIT.
- **The readme's example is `examples/greeter`, which is what `hxn new`
  makes.** A test fails when the readme differs from it.
- **An example is a library as someone would write one:** a crate, a Haxe
  program, its committed manifest, and what the program prints, which is the
  same on every run and every runtime. It shows a few things and checks
  nothing; what has to be checked is in the fixture.
- Run a runtime test under a timeout. A wrong wait hangs; it does not fail.
- Anything concurrent is run repeatedly, on more than one machine.
- `cargo miri test -p hxn-abi` runs the unsafe core, and
  `cargo miri test -p hxn-future` finds interleavings a native run does not,
  the more with `-Zmiri-many-seeds`. Under Miri the tests that count on what
  fits in a slice of time are skipped: it runs too slowly for them.
- A test that depends on a runtime's limits says so beside the check it
  skips, with the reason.
- A check does not depend on when the collector runs by itself: it forces a
  collection, or asserts a bound that holds either way.
- The pure crates have unit tests; `hxn-hl` and the adapters are tested only
  in a real runtime.
- The fixture's manifest and each example's are committed, and a test fails
  when one no longer matches its source.

## Comments, commits and issues

- A comment says what the code does, and why it is shaped that way when that
  is not obvious. No history of what was tried, and no measurements.
- A commit message is short: what changed, and a few sentences of why when
  that is not self-evident.
- Detail lives in a `git-bug` issue: reproducers, measurements, alternatives
  weighed, what is still open. A commit cites its issue in a `git-bug:`
  trailer with the full id.
- No planning, backlog or status documents. These two documents describe how
  hxn is, not what happened or what is next.
- An issue has a kind (`bug`, `perf`, `debt`) and an area
  (`area:abi`, `area:hl`, `area:cpp`, `area:future`, `area:reactive`,
  `area:wasm`, `area:gen`, `area:bind`, `area:cli`).
