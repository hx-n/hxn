# How hxn works, and why

hxn lets a library written in Rust be used from Haxe on several runtimes
without the library knowing which one it is running under. This document
describes the approach each part takes and the reason for it. The rules that
follow from these approaches are in [conventions.md](conventions.md).

## The shape

HashLink, hxcpp and their wasm forms disagree on object layout, garbage
collection, strings and closures. They agree on plain C calls. So hxn has no
shared value model. It has:

- **a runtime-neutral core** (`hxn-abi`): the few `#[repr(C)]` shapes a library
  and a runtime agree on, and the safe Rust side of each;
- **one adapter per runtime family**: HashLink primitives (`hxn-hl`, over
  `hl_abi`) and a plain C ABI with C++ glue for hxcpp (`include/hxn.h`);
- **Haxe classes** with the same public API on every runtime.

A library is one Rust source built once per runtime. It is split into three
crates so that the library itself compiles once whatever is being built:

| Crate | What it is |
|---|---|
| `<name>-core` | The library. Plain Rust, no runtime in it. |
| `<name>-c` | A thin static library exporting the plain C ABI, for hxcpp. |
| `<name>-hl` | A thin dynamic library of HashLink primitives. Its archive is also what links into an Ash wasm side module. |

The core never switches runtime with a cargo feature. A feature would compile
every dependency twice.

## What crosses, and who owns it

| Crossing | Shape | Owner |
|---|---|---|
| bytes or UTF-8 text, for one call | `Slice` | the caller |
| bytes or UTF-8 text, returned | `Buf` | the library, until the host frees it |
| a Rust value the host holds | `Handle<T>` behind an opaque pointer | the host's collector |
| a host function, for one call | `Callback` | the host |
| a cell both sides read and write | an atomic `i32` | whoever made it |
| failure | `Status`, with a message in a `Buf` | - |

Numbers cross as an `i32`, an `i64` or an `f64` and nothing else, which is
what every runtime has: a 64-bit integer is `hl.I64` to HashLink and
`cpp.Int64` to hxcpp, and `haxe.Int64` is that type on both. A library's
function may also take or give a narrower integer, a size, an unsigned 32 or
64 bits or an `f32`; its adapter converts on the way, and a value that does
not fit fails the call with `Status::RANGE` before the function runs or
after it has returned. So the boundary and the generated Haxe are the same
whichever of these the Rust says, but for `u32`, which Haxe sees as a
`UInt`. Haxe has no unsigned 64 bits: a `u64` arrives in a `haxe.Int64` with
its bits as they were, and reads as negative from 2^63 up.

**On wasm32** pointers are 32 bits and integers are not. An `i64` or a `u64`
crosses there exactly as it does natively. What changes is `usize` and
`isize`, which are 32 bits inside the library: they still cross as a Haxe
`Int`, checked the same way, but arithmetic the library does in one wraps at
4 GiB where the native build has room. A 64-bit value stays inside the wasm
module on both wasm runtimes; nothing hxn does hands one to JavaScript,
where it would be a `BigInt`.

Text is UTF-8 at the boundary. HashLink strings are UTF-16, so the HashLink
adapter converts at the edge, strictly: text that is not valid in its encoding
is an error, not a replacement character.

A `Buf` carries its own `free` function. Each library is linked with its own
allocator, and only the one that allocated can release.

## Failures

Nothing unwinds across the boundary in either direction.

- A library function returns a `Result`. The adapter turns `Err` into a status
  and a message.
- The runtime's own exception is raised **after** the Rust call has returned
  and dropped everything it owned. HashLink throws by jumping past frames
  without running destructors, and a C++ exception must not cross a Rust
  frame, so the throw is always made by the adapter, never from inside the
  library.
- A panic is caught at the boundary and reported as `Status::PANIC`, where the
  build can catch one. A wasm build aborts on a panic; see the runtime table
  at the end.

So a thrown error is an ordinary value until the call is over. That is why
errors work the same in builds that cannot unwind.

## Handles

A `Handle<T>` separates two lifetimes:

- the **value** ends when the host calls `close()`;
- the **allocation** lives until the host's collector finalizes the object.

A pointer the host can still reach is therefore never dangling, and a use
after close is an error, not a crash. Each handle carries a type tag, so the
wrong kind of handle is refused.

A host does not obey `&` and `&mut`: it can call from two threads, or call
back in from a callback. Borrows are counted at run time, and a conflicting
one fails with `Busy` instead of waiting or aliasing.

`T` must be `Send`, because a collector finalizes on whichever thread it runs.

### Passing handles

One Haxe object owns one handle. Everything else follows from keeping that
true.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&T` parameter | borrowed for the call, shared | an object |
| `&mut T` parameter | borrowed for the call, exclusive | an object |
| `T` result, from any function | a new value, handed over | a new object |
| `self` by value | the value is consumed | the object is closed by the call |
| `Option` of any of these | may be absent | `null` |

- **A parameter is only ever borrowed.** Passing the same object twice in ways
  that conflict, as the receiver of a `&mut self` method and as its argument,
  fails with "in use" like any other conflicting borrow.
- **A result is always owned.** There are no borrowed results: a value that
  refers into another cannot be tied to it across the boundary. A child that
  needs its parent holds a shared reference to it.
- **Sharing is the library's choice**, made by marking the interface shared;
  see below. Without the mark a value has one owner.
- **Haxe values stay in Haxe.** A library that must remember something of the
  host's is given an id, and the binding keeps the table. Rooting a host
  value from native code has the same cycle problem as rooting a function.
- **A handle belongs to the library that made it.** One passed to another
  library is refused by the type check.

### Shared values

An interface marked `#[hxn::interface(shared)]` is held through an `Arc`. A
handle is then one share of the value, and the value lives while the library
or any Haxe object has one.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&T` parameter | the value, for the call | an object |
| `Arc<T>` parameter | a share for the library to keep | an object |
| `T` result | a new value, which the adapter puts in an `Arc` | a new object |
| `Arc<T>` result | a share of a value the library also keeps | a new object |

- **One Haxe object still owns one handle.** Returning the same value twice
  gives two objects with a share each, so Haxe object identity says nothing.
  `same(other)` says whether two objects hold one value.
- **`close()` gives up one share.** The object is closed; the value goes with
  its last holder.
- **`weak()` gives a reference that holds nothing**, a class of its own named
  `<Interface>Weak`. Its `upgrade()` is a new object with a share, or null
  once the value is gone. It is how a child refers to its parent without the
  two keeping each other alive.
- **Nothing borrows a shared value exclusively.** Its functions take `&self`,
  and what changes is behind a cell or a lock of the library's own, as in any
  Rust shared between threads. The value is `Send + Sync`.
- **Shares held by Haxe objects are released by the collector**, like any
  handle, so a value the library also holds is unaffected by when that is.

## Atomics

A Haxe atomic integer and Rust's `AtomicI32` are the same four bytes, and
each runtime's own atomic operations take a plain address. So a cell is
shared by sharing its address: each side works on it with its own operations,
from any thread, and there is no call between them.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&AtomicI32`, `&AtomicFlag` parameter | a cell lent for the call | `hxn.AtomicInt`, `hxn.AtomicBool` |
| `Arc<AtomicI32>`, `Arc<AtomicFlag>` result | a cell the library keeps | the same, over the library's memory |

- **A lent cell** is one Haxe made. Rust has it for the call only, which the
  borrow says, so nothing has to keep Haxe's memory alive afterwards. A
  cancel flag for a call in progress is the usual use.
- **A kept cell** lives in the library. The Haxe object holds what the
  library handed over, which keeps the memory alive for as long as the object
  is referenced; it has no `close()`, so the address cannot outlive it.
- **A flag is an integer that is 0 or 1.** Haxe has no atomic of one byte,
  which Rust's `AtomicBool` is, so a flag that crosses is `hxn::AtomicFlag`.
- `hxn.AtomicInt` has the operations of `haxe.atomic.AtomicInt`. On hxcpp it
  is one, over the library's memory; on HashLink it calls the VM's atomic
  primitives directly, because the standard type there owns its storage.

## Collection

A native value is released by `close()`, or by the collector once its object
is unreachable. A collector runs when the memory it manages has grown, and an
object that owns a handle is a few words of that whatever the value behind it
holds. A program can drop gigabytes of native memory and never be due a
collection. hxn makes up for that in three ways, and none of them has native
code call the runtime.

| | Rust | Haxe |
|---|---|---|
| collect now | | `hxn.Gc.collect()` |
| say what a value holds | an `hxn::Weight` kept in the value | `hxn.Gc.budget` |
| ask for a collection | `hxn::collect_soon()` | honoured at the next check |

- **`hxn.Gc.collect()`** is one call for every runtime's own. It returns
  false where the runtime cannot be made to collect.
- **A weight is counted while its value lives.** `Weight` adds to a total when
  made, follows `set`, and subtracts when dropped, which is at `close()` or
  finalization. The total is per library, because each library has its own
  copy of `hxn-abi`.
- **The binding asks; the library never tells.** Each library's classes share
  one `hxn.Gc.Meter`, which reads the total. An interface marked
  `#[hxn::interface(weighed)]` checks the meter whenever one of its objects
  is made, and every delivery checks it. A check collects when the total has
  passed `hxn.Gc.budget`, and then not again until it passes twice what was
  left, so a program whose live values exceed the budget does not collect on
  every allocation.
- **A request is a flag.** `collect_soon()` can be called from any thread; the
  next check takes it and collects.
- **The policy is hxn's on every runtime.** hxcpp can be told of memory it
  does not manage, but a release has to be reported too, a finalizer is not
  allowed to make that call, and reported any later the release is missed by
  the threshold hxcpp sets during the same collection.

## Calling a Haxe function

Native code cannot be thrown through, cannot be called from a thread the
runtime does not know, and should not keep a host function alive. Three
approaches follow.

**Lent for the call.** A function passed to a native call is called during
that call, on the calling thread. The binding wraps it: the wrapper catches an
exception, keeps it (`hxn.Callback.failed`), and returns the value that tells
the library its callback failed. The library stops and returns, and the
binding rethrows the original exception (`hxn.Callback.during`). The catch is
on the Haxe side, so no trap is needed in native code and it is the same on
every runtime.

In Rust the library takes such a function as `impl Fn(..) -> Result<T, E>`,
generic in `E`. It is a `Result` because the Haxe function can throw, and the
library sees that as an `Err` to return with. The error is a type parameter so
the library names no type of hxn's and can be tested with any error it likes;
the adapter calls it with its own.

**Kept across calls.** A function needed by later calls is kept in a field of
the Haxe object and lent to each call that uses it. Native code holds nothing.

**Not rooted in native code.** A root held by native code is invisible to the
collector, so a function that can reach the object rooting it is never
collected. Event handlers usually can reach their emitter. Keeping the
function on the Haxe side leaves every reference where the collector can see
it, and such cycles are collected normally.

## Results over time

`hxn-future` has three kinds, with a Haxe class for each.

| | Rust | Haxe |
|---|---|---|
| one result, later | `pending()` gives a `Completer` and a `Future` | `hxn.Future`: `await()`, `then()` |
| many results, in order | `channel()` gives a `Sender` and a `Stream` | `hxn.Stream`: an iterator, `listen()` |
| a value that changes | `signal()` gives a `Source` and a `Signal` | `hxn.Signal`, through `Reactive.follow` |

Two rules hold for all three.

**The result stays in Rust until a Haxe thread takes it.** Settling, sending
or setting touches no runtime. The thread that does it needs no standing with
a collector, and nothing of the runtime's has to be kept alive meanwhile.

**The library never calls the host to report something.** It queues or sets;
the host takes each result on a thread of its own and calls whatever it likes
with it. This is what makes events work from any thread and on wasm.

A future is settled once and later settles return `false`. If everything that
could settle it is dropped, it is rejected as cancelled, so a waiter is never
left waiting on nothing. A stream ends when every sender is dropped, or with a
failure the reader meets after the items before it. A sender or source learns
its reader is gone because `send` or `set` returns `false`.

A stream delivers everything, in order. A signal delivers the latest value and
nothing else. Events are a stream; state is a signal.

A function exposes one by returning it: `Future<T>`, `Stream<T>` or
`Signal<T>`, and the Haxe method returns the class of the same name. The
generated adapter hands each over type-erased, as a handle. So a library has
one set of exports to wait on and watch a future, whatever it carries and
whichever function returned it, and one more for each kind of value its
futures carry, to take it. A future or stream can carry a number, a bool, text
or an object; an object taken from one is owned by the Haxe side from then on.
A signal carries a number, a bool or text, because it is read any number of
times.

## Running work

| Entry point | For | With threads | Without |
|---|---|---|---|
| `spawn(async { … })` | work that awaits | a worker pool; polled only when woken | this thread's queue |
| `stepped(\|\| …)` | work done a step at a time | a worker pool | this thread's queue |
| `spawn_blocking(\|\| …)` | a closure that blocks | a thread of its own | runs before returning |

Work is a `krio` task. With threads it runs on a `krio-parallel` cluster of a
few workers per library, started on first use, whose idle workers sleep.
Without threads it is queued on the calling thread (`krio-runtime`) and
advanced by whoever waits: a wait runs the queue for a short slice and returns,
pending or not, so the host can yield and ask again.

`spawn` and `stepped` take their first step before returning, so work that
finishes at once never waits for anyone.

An `hxn_future::Future` can itself be awaited, which is where waking comes
from: settling it wakes whoever awaits it. A waiting task costs nothing on
either scheduler.

## Delivery

Everything a library produces reaches the app through one door.

- **Outbox (Rust).** A future, stream or signal the host is watching notes
  itself in the library's one queue when it has something new, once until the
  host has looked. Only watched things are queued.
- **Inbox (Haxe).** `hxn.Inbox`, one per library, drains that queue and calls
  the watchers for each entry, in the order the library produced them, inside
  one batch. `Future.then`, `Stream.listen` and `Reactive.follow` are watchers.
- **Loop.** `hxn.Loop` decides when. By default deliveries come from the Haxe
  event loop of the thread that first asked. An app with a loop of its own
  sets `Loop.manual` and calls `Loop.sync()` where it wants them, once a frame
  for instance; the Haxe event loop is then neither used nor kept alive.

One queue is what preserves order across kinds, lets the app choose the
moment, and needs one wait for everything.

## Reactivity

`hxn.Signal`, `hxn.Computed` and `hxn.Reactive` give fine-grained reactivity
in the style of SolidJS. Dependencies are found by running the code, so they
follow its branches.

The graph is in Haxe, so it is identical on every runtime. It marks on write
and computes on read: a write marks direct readers dirty and everything below
them as needing a check, and queues effects; an effect that runs, or a
computed value that is read, first brings its own sources up to date and
recomputes only if one changed. A value is computed once however many paths
lead to it, and never from a mix of old and new.

`Reactive.follow` makes a signal that follows a value held in Rust. Three
things keep the two sides consistent:

- a delivery is one batch, so effects rerun once however many values arrived;
- a binding **writes through**: when a set goes through it, it sets the
  followed signal as well, so code reads its own writes at once;
- the graph is **pluggable**: `Inbox.follow` takes the sink as a parameter, so
  an app with a reactive system of its own feeds its own signals.

## Generation

`hxn-gen` reads a library's source for the items marked `#[hxn::interface]`,
`#[hxn::methods]` and `#[hxn::function]` and writes the manifest, both Rust adapters, the C
header, the hxcpp glue and the Haxe classes. It starts at the crate's `lib.rs`
and reads the file of every module declared from there down, so a library is
laid out as any crate is. Names are the library's, not a module's: the
adapters find each interface at the crate's root.

The attributes generate nothing. A macro expanding inside the library could
not write files or put code into the separate adapter crates, so the generator
parses the source instead and the attributes only make the markers legal Rust.

A function marked outside an `impl` has no value to belong to, and Haxe has
no function outside a class, so it becomes a static of one: by default the
class named after the library, which is an interface's class when the two
share a name and otherwise one made for the purpose. Such a class has
nothing behind it: no handle, no `close`, and nothing in the adapters but
its functions, which they call by a path from the crate's root.

A Haxe class is a plain class with the library's public API, the same on both
runtimes, over natives it calls. On HashLink each class declares its own, as
an `@:hlNative` class private to its file. On hxcpp one extern class a library
declares every function of the glue header, typed, and is where the build is
told what to include and link; no generated class writes C++ inline.

The manifest is JSON, generated, committed and reviewed: a change to it is a
change to the library's ABI.

## The command

`hxn` is the generator with the rest of the work around it, so that a library
is one crate and its author writes no adapter crate, build script or compiler
flag.

| | |
|---|---|
| `hxn new <name>` | makes a library, a copy of `examples/greeter`: a crate, and a Haxe program that uses it |
| `hxn build <hl\|cpp>` | writes the binding for a runtime and builds it |
| `hxn run <hl\|cpp>` | builds, then compiles and runs the library's Haxe program |
| `--wasm` on either | does the same for the runtime's wasm form |

- **The adapter crates are written, not kept.** A build writes the adapter
  for the runtime as a crate under cargo's target directory, depending on the
  library by path, and builds it there. The library's own directory gains
  only its manifest.
- **The adapter gets hxn from where the library does.** The command asks
  cargo where the library's `hxn` comes from, a path, a repository or a
  registry, and names `hxn-abi` and `hxn-hl` from the same place. Two copies
  of `hxn-abi` would not agree on what a handle is.
- **The command carries hxn's Haxe classes and C header** and writes them
  beside the generated ones, so a library needs nothing installed but it.
- **What a compile needs is written down**, as a file `haxe` takes: the class
  paths, and for hxcpp where the headers and the library are. `run` passes
  the same itself.
- **What is the program's own is said by the crate**, under
  `[package.metadata.hxn]`: its directory and main class, and `haxe-flags`,
  the arguments its compile takes beside hxn's, a `-lib` or a `-D`. The VM
  is the user's, so it is named on the command line or in `HXN_VM`, with
  any options after it.
- **A wasm build is the same adapter for another target.** For HashLink it is
  built as an archive for `wasm32-wasip1` and linked into a module whose
  exports are the primitives' resolvers and whose imports the program
  answers; Ash builds the program and loads the module from beside it. For
  hxcpp it is an archive for `wasm32-unknown-emscripten` that hxcpp links,
  and node runs the result. Either way Rust's standard library is rebuilt,
  position-independent for the one and aborting for the other, which takes a
  nightly toolchain; the command says so when it has another.

A library that wants its adapter crates in its own repository, as the fixture
in `tests/counter` has them, gives each a build script that calls `hxn-gen`.

## Runtimes

| Runtime | Adapter | Threads | A panic | Notes |
|---|---|---|---|---|
| HashLink | `-hl` as an `.hdll` | yes | becomes a thrown error | |
| Ash, native | the same `.hdll` | yes; cooperative in its interpreter mode | becomes a thrown error | Tested in its hybrid, JIT and interpreter modes. |
| Ash, wasm | `-hl` as a side module | Haxe threads as fibers; no Rust threads | ends the program | The module takes its allocator from the program. |
| hxcpp | `-c` with C++ glue | yes | becomes a thrown error | |
| hxcpp through Emscripten | the same | none | ends the program | hxcpp's collector does not complete a forced collection there, so `hxn.Gc.collect()` does nothing and a meter only counts. |

Rayzor's plugin ABI informed the manifest, but there is no Rayzor adapter.

Where a runtime has no threads, nothing in hxn blocks or starts one: waits
return, work is queued on the caller, and delivery asks once a loop turn.
