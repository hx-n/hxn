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
- **Sharing is the library's choice.** To hand the same thing out twice, the
  library returns something clonable, an `Arc` for instance, and each return
  is a handle of its own. Two Haxe objects may then refer to one value; Haxe
  object identity is not preserved.
- **Haxe values stay in Haxe.** A library that must remember something of the
  host's is given an id, and the binding keeps the table. Rooting a host
  value from native code has the same cycle problem as rooting a function.
- **A handle belongs to the library that made it.** One passed to another
  library is refused by the type check.

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

## Running work

| Entry point | For | With threads | Without |
|---|---|---|---|
| `spawn(async { … })` | work that awaits | a worker pool; polled only when woken | this thread's queue |
| `stepped(|| …)` | work done a step at a time | a worker pool | this thread's queue |
| `spawn_blocking(|| …)` | a closure that blocks | a thread of its own | runs before returning |

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

`hxn-gen` reads a library's source for the items marked `#[hxn::interface]`
and `#[hxn::methods]` and writes the manifest, both Rust adapters, the C
header, the hxcpp glue and the Haxe classes.

The attributes generate nothing. A macro expanding inside the library could
not write files or put code into the separate adapter crates, so the generator
parses the source instead and the attributes only make the markers legal Rust.

Each thin adapter crate has a build script that writes its adapter from the
library's source. The manifest is JSON, generated, committed and reviewed: a
change to it is a change to the library's ABI.

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
