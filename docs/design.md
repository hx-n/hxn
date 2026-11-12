# How hxn works, and why

hxn lets a library written in Rust be used from Haxe on several runtimes
without the library containing any runtime-specific code. This document
describes how each part works and why. [conventions.md](conventions.md)
lists the rules that follow from it.

## Structure

HashLink, hxcpp and their wasm builds differ in object layout, garbage
collection, strings and closures. All of them support plain C calls. So hxn
has no shared value model. It has:

- **a runtime-neutral core** (`hxn-abi`): the few `#[repr(C)]` types passed
  between a library and a runtime, and a safe Rust API for each;
- **one adapter per runtime family**: HashLink primitives (`hxn-hl`, over
  `hl_abi`) and a plain C ABI with C++ glue for hxcpp (`include/hxn.h`);
- **Haxe classes** with the same public API on every runtime.

A library is one Rust codebase built once per runtime. It is split into three
crates so that the library itself is compiled once, whichever runtime is
being built:

| Crate | What it is |
|---|---|
| `<name>-core` | The library. Plain Rust with no runtime dependency. |
| `<name>-c` | A thin static library exporting the plain C ABI, for hxcpp. |
| `<name>-hl` | A thin shared library of HashLink primitives. Its static archive is also linked into an Ash wasm side module. |

The core never selects a runtime with a cargo feature. A feature would compile
every dependency twice.

## Boundary types and ownership

| Value | Wire type | Owner |
|---|---|---|
| bytes or UTF-8 text, borrowed for the call | `Slice` | the caller |
| bytes the library writes into, borrowed for the call | `SliceMut` | the caller |
| bytes or UTF-8 text, returned | `Buf` | the library, until the host frees it |
| a Rust value held by the host | `Handle<T>` behind an opaque pointer | the host's garbage collector |
| a host callback, borrowed for the call | `Callback` | the host |
| a cell both sides read and write | an atomic `i32` | the side that created it |
| plain data | bytes, serialized field by field | as for bytes |
| an error | `Status`, with a message in a `Buf` | - |

Numbers are passed as `i32`, `i64` or `f64` only, the types every runtime
supports: a 64-bit integer is `hl.I64` on HashLink and `cpp.Int64` on hxcpp,
and `haxe.Int64` is that type on both. A library function may also accept or
return a narrower integer, `isize` or `usize`, `u32`, `u64` or `f32`; its
adapter converts the value, and a value that does not fit fails the call with
`Status::RANGE`, before the function runs for a parameter or after it returns
for a return value. So the boundary and the generated Haxe are the same
whichever of these the Rust declares, except for `u32`, which Haxe sees as a
`UInt`. Haxe has no unsigned 64-bit type: a `u64` arrives as a `haxe.Int64`
with the same bits, and values from 2^63 up read as negative.

**On wasm32** pointers are 32 bits, but `i64` and `u64` are still 64 bits and
are passed exactly as they are natively. What changes is `usize` and
`isize`, which are 32 bits inside the library: they are still passed as a
Haxe `Int` with the same range check, but arithmetic on them in the library
wraps at 4 GiB where a native build would not. On both wasm runtimes a
64-bit value stays inside the wasm module; hxn never passes one to
JavaScript, where it would be a `BigInt`.

Text is UTF-8 at the boundary. HashLink strings are UTF-16, so the HashLink
adapter converts at the boundary, strictly: invalid text in either encoding is
an error and is never replaced with a replacement character.

Bytes a library writes into are a `&mut [u8]` parameter. The library is
given the memory of the Haxe `haxe.io.Bytes` itself, not a copy, so the
caller sees what was written: `hl.Bytes` on HashLink and the array's base on
hxcpp, which neither collector moves. A `&mut [u8]` may not alias any other
reference, so before making either the adapter checks that the bytes are not
also another bytes argument of the same call, which Haxe can pass by giving
one `Bytes` twice, and fails the call with `Status::ALIASED` if they are.

A `Buf` carries its own `free` function, because each library is linked with
its own allocator and memory must be freed by the allocator that allocated it.

## Data

An interface crosses as a handle: one value, held by reference, with an owner.
Plain data crosses by value. A library marks it with `#[hxn::data]`:

| Rust | Haxe |
|---|---|
| a struct of public fields | a class with the same fields |
| a C-like enum (no variant has fields) | an `enum abstract` over `Int` with the same values |
| a tagged enum (some variants have fields) | an `enum` with the same constructors |
| `Option<T>`, `Vec<T>` | `Null<T>`, `Array<T>` |
| `HashMap<K, V>`, `BTreeMap<K, V>` | `Map<K, V>`; keys are `String` or `Int` |
| `Box<T>` | `T` |

- **A C-like enum crosses as its integer value.** The adapter converts in
  both directions with a `match` on the variant names, so it does not depend
  on the enum's layout, and an integer with no matching variant fails the
  call.
- **Composite data crosses as bytes.** Records, tagged enums, `Option`,
  `Vec` and maps are serialized field by field in declaration order: a
  tagged enum as its variant index followed by that variant's fields, an
  `Option` as a presence byte followed by the value, a list or a map as a
  count followed by the entries. Both sides are generated from the same
  declaration, so the encoding carries no type information. Every runtime
  can already pass bytes, so this needs no per-runtime support, and nested
  types need nothing extra.
- **Data can be recursive** through a `Vec` or a `Box`, which is how a tree
  is expressed. The generated reader for such a type is recursive, so it
  tracks nesting depth and fails with `Status::DEPTH` beyond
  `wire::Reader::DEPTH`.
- **A reader takes exactly what was written.** Bytes that run out, a count
  the bytes left cannot back, text that is not UTF-8 and bytes left over all
  fail the call, with the same status as badly encoded text.
- **Data is copied, both ways.** A function takes it by value or by
  reference, and the reference is to the adapter's copy; what it returns is
  written out and dropped. So there is no owner to track and no `close()`,
  and changing a record in Haxe after passing it changes nothing in the
  library.
- **The Haxe class is plain Haxe**, the same file for HashLink, hxcpp and a
  script. It is made from its fields by name, `{label: "x", enabled: true}`,
  and a field that is an `Option` can be left out. A null where a value is
  needed is refused in Haxe, before anything is written. A Haxe enum
  cannot have methods, so a tagged enum's serializer is a separate
  generated class: `EventWire` for `Event`.
- **Data is accepted everywhere a value is:** as a parameter or a return
  value, carried by a future, a stream or a signal, returned from an
  `async fn`, and as a parameter or return value of a callback. The
  encoding is the same in every position: the adapter serializes or
  deserializes on the Rust side and the binding on the Haxe side.
- **The cost is a copy and a pass over the fields on every call.** That suits what
  data is for: a descriptor, a result, a short list. A large buffer is bytes,
  and a thing with identity is an interface.

## Failures

Nothing unwinds across the boundary in either direction.

- A library function returns a `Result`. The adapter turns `Err` into a status
  and a message, which the Haxe binding throws as a `haxe.Exception`.
- **A `#[hxn::data]` error type is thrown as a typed exception.** If the
  error type of the `Result` is one of the library's `#[hxn::data]` types,
  Haxe gets an exception class named after it (`PantryException` for
  `PantryError`) with the value in its `error` field and the error's
  `Display` output as its message. The value is passed separately from the
  message: the adapter attaches the serialized error to the failure, `guard`
  and `prim` store it in a thread-local (`hxn_abi::thrown`) and throw the
  message as usual, and the generated Haxe method catches that exception,
  fetches the stored bytes and throws the typed exception instead. The
  message path is therefore the same as for any other error on every
  runtime, and native code never constructs a Haxe object. Futures work the
  same way: the stored failure carries the bytes, and they are moved to the
  thread-local when a Haxe thread takes the result. Each failure overwrites
  the stored value, so the bytes the binding fetches always belong to the
  failure it just caught.
- The runtime's own exception is raised **after** the Rust call has returned
  and dropped everything it owned. HashLink throws by jumping past frames
  without running destructors, and a C++ exception must not cross a Rust
  frame, so the throw is always made by the adapter, never from inside the
  library. Ash's wasm build throws a wasm exception, which unwinds through
  the adapter's own frame; so a HashLink primitive that can throw is
  `extern "C-unwind"`, which leaves it free to pass, where `extern "C"`
  would abort the program.
- A panic is caught at the boundary and reported as `Status::PANIC`, on every
  runtime. On wasm the library unwinds with wasm exceptions: Rust's prebuilt
  standard library does for Emscripten, and an Ash side module rebuilds it
  that way. On WASI Rust takes two functions from the toolchain's libunwind,
  which an Ash side module does not have, so `hxn-hl` defines them for that
  build (`unwind.rs`), throwing with the wasm `throw` instruction as Rust's
  own shim for `wasm32-unknown-unknown` does.

So an error is an ordinary return value until the call has returned, which
is why errors work the same in builds that cannot unwind.

## Handles

A `Handle<T>` separates two lifetimes:

- the **value** is dropped when the host calls `close()`;
- the **allocation** is freed when the host's garbage collector finalizes the
  object.

So a pointer the host can still reach never dangles, and a use after close
returns an error instead of crashing. Each handle carries a type tag, so a
handle of the wrong type is rejected.

A host does not respect `&` and `&mut`: it can call from two threads, or
re-enter from a callback. Borrows are checked at run time, and a conflicting
borrow fails with `Busy` instead of blocking or aliasing.

`T` must be `Send`, because the collector runs finalizers on whatever thread
it runs on.

### Passing handles

Each handle is owned by exactly one Haxe object. The rules below maintain
that invariant.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&T` parameter | borrowed for the call, shared | an object |
| `&mut T` parameter | borrowed for the call, exclusive | an object |
| `T` result, from any function | a new value; ownership passes to Haxe | a new object |
| `self` by value | the value is consumed | the object is closed by the call |
| `T` parameter | the value is moved out of its handle | the object passed is closed by the call |
| `Option` of any of these | may be absent | `null` |
| `Vec<T>` result, `Vec<Arc<T>>` of a shared interface | new values, each passed to Haxe | an `Array` of new objects |

- **A parameter is always borrowed.** Passing the same object twice with
  conflicting borrows, for example as the receiver of a `&mut self` method and
  as its argument, fails with "in use" like any other conflicting borrow.
- **A return value is always owned.** There are no borrowed return values: a
  lifetime tying one value to another cannot be expressed across the
  boundary. A child that needs its parent holds a shared reference to it.
- **Sharing is opt-in**: the library marks the interface shared (see below).
  Without the mark a value has a single owner.
- **Haxe values stay in Haxe.** A library that needs to refer to a host value
  is given an id, and the binding keeps the table from ids to values. Rooting
  a host value from native code has the same cycle problem as rooting a
  function.
- **A handle belongs to the library that created it.** Passing it to another
  library fails the type check.
- **A list of objects crosses as a stream that has already ended.** The
  adapter sends each element to a new stream and returns the stream; the
  Haxe method takes every element into an `Array` before it returns. So a
  list uses the handles and the take function a stream of objects already
  has, on every runtime, and no element waits on anything.

### Shared values

An interface marked `#[hxn::interface(shared)]` is held in an `Arc`. Each
handle then holds one `Arc` reference to the value, and the value lives while
the library or any Haxe object holds a reference.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&T` parameter | borrowed for the call | an object |
| `Arc<T>` parameter | a reference the library can keep | an object |
| `T` result | a new value, wrapped in an `Arc` by the adapter | a new object |
| `Arc<T>` result | a reference to a value the library also holds | a new object |

- **Each handle is still owned by one Haxe object.** Returning the same value
  twice gives two objects, each holding a reference, so Haxe object identity
  is not meaningful. `same(other)` reports whether two objects refer to the
  same value.
- **`close()` releases one reference.** The object is closed; the value is
  dropped when its last reference is released.
- **`weak()` returns a weak reference**, an instance of a separate class named
  `<Interface>Weak`. Its `upgrade()` returns a new object holding a
  reference, or null once the value has been dropped. A child uses one to
  refer to its parent without creating a reference cycle.
- **A shared value is never borrowed mutably.** Its functions take `&self`,
  and mutable state is behind a cell or lock in the library, as in any Rust
  type shared between threads. The value is `Send + Sync`.
- **References held by Haxe objects are released by the collector**, like any
  handle, so the timing of collection does not affect a value the library
  also holds.

## Atomics

A Haxe atomic integer and Rust's `AtomicI32` have the same 4-byte layout, and
each runtime's atomic operations work on a plain address. So a cell is shared
by passing its address: each side uses its own atomic operations on it, from
any thread, with no call across the boundary.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&AtomicI32`, `&AtomicFlag` parameter | a cell borrowed for the call | `hxn.AtomicInt`, `hxn.AtomicBool` |
| `Arc<AtomicI32>`, `Arc<AtomicFlag>` result | a cell owned by the library | the same, backed by the library's memory |

- **A borrowed cell** is allocated by Haxe. Rust uses it only for the
  duration of the call, as the borrow expresses, so nothing has to keep
  Haxe's memory alive afterwards. The typical use is a cancellation flag for
  a call in progress.
- **A library-owned cell** is allocated in the library. The Haxe object holds
  the reference the library returned, which keeps the memory alive for as
  long as the object is reachable; it has no `close()`, so the memory cannot
  be freed while the address is in use.
- **A flag is an integer that is 0 or 1.** Rust's `AtomicBool` is one byte and
  Haxe has no 1-byte atomic, so a flag passed across the boundary is an
  `hxn::AtomicFlag`.
- `hxn.AtomicInt` has the same operations as `haxe.atomic.AtomicInt`. On
  hxcpp it is a `haxe.atomic.AtomicInt` backed by the library's memory; on HashLink it calls the VM's atomic
  primitives directly, because the standard type there owns its storage.

## Garbage collection

A native value is released by `close()`, or by the collector once its object
is unreachable. A collector runs when the memory it manages has grown, and an
object that owns a handle counts as a few words of that memory however much
native memory the value holds. A program can drop gigabytes of native memory
without triggering a collection. hxn compensates in three ways, none of which
has native code call the runtime.

| | Rust | Haxe |
|---|---|---|
| force a collection | | `hxn.Gc.collect()` |
| report a value's native memory | an `hxn::Weight` kept in the value | `hxn.Gc.budget` |
| request a collection | `hxn::collect_soon()` | honoured at the next check |

- **`hxn.Gc.collect()`** calls each runtime's own collection function. It
  returns false on a runtime where a collection cannot be forced.
- **A weight is counted while its value is alive.** A `Weight` adds to a total
  when created, updates it on `set`, and subtracts from it when dropped, at
  `close()` or finalization. The total is per library, because each library has its own
  copy of `hxn-abi`.
- **The binding polls; the library never calls the binding.** Each library's
  classes share one `hxn.Gc.Meter`, which reads the total. An interface
  marked `#[hxn::interface(weighed)]` checks the meter whenever one of its
  objects is created, and every delivery checks it too. A check triggers a
  collection when the total exceeds `hxn.Gc.budget`, and then not again until
  the total exceeds twice what remained, so a program whose live values
  exceed the budget does not collect on every allocation.
- **A request sets a flag.** `collect_soon()` can be called from any thread;
  the next check clears the flag and collects.
- **hxn implements the policy itself on every runtime.** hxcpp can be told
  about memory it does not manage, but releases must be reported too; a
  finalizer is not allowed to make that call, and a release reported later is
  missed by the threshold hxcpp computes during that same collection.
- **`hxn.Loop.exit` ends an hxcpp program without running the C runtime's
  destructors.** hxcpp keeps zeroing memory on its own threads after a
  collection, and `exit` destroys a lock those threads take when they
  finish, so on Windows a program that exits shortly after a collection
  crashes. A meter triggers a collection when a library's memory exceeds its
  budget, at a time the program does not choose, so hxn provides an exit
  that does not depend on that timing. On other runtimes it calls
  `Sys.exit`. The crash is in hxcpp 4.3.2 and in hxcpp's git history before
  HaxeFoundation/hxcpp#1298 (commit `0e9183de`), which made that lock a
  `std::mutex`; it does not occur from that commit on. It was reported as
  HaxeFoundation/hxcpp#1470. hxn is built and tested against hxcpp from git
  at a commit after #1298, which CI pins; `hxn doctor` warns about an hxcpp
  from before it. `hxn.Loop.exit` remains the safe way to exit with hxcpp
  4.3.2.

## Calling a Haxe function

An exception cannot be thrown through native code, a Haxe function cannot be
called from a thread the runtime does not know about, and native code should
not keep a host function alive. Three approaches follow.

**Borrowed for the call.** A callback passed to a native call is called during
that call, on the calling thread. The binding wraps it: the wrapper catches an
exception, stores it (`hxn.Callback.failed`), and returns a value that tells
the library the callback failed. The library stops and returns, and the
binding rethrows the original exception (`hxn.Callback.during`). The exception
is caught on the Haxe side, so native code needs no exception handling and the
mechanism is the same on every runtime.

In Rust the library accepts such a callback as `impl Fn(..) -> Result<T, E>`,
generic in `E`. It returns a `Result` because the Haxe function can throw,
which the library sees as an `Err` to return. The error is a type parameter so
the library depends on no hxn type and can be tested with any error type; the
adapter instantiates it with its own.

A callback can have numbers, bools, text, bytes and data as parameters and
return value, and objects as parameters. Text and bytes are passed to it as
borrowed `&str` and `&[u8]`; the host copies them during the call. They are
returned as `String` and `Vec<u8>`: the host copies the result into a buffer
with its own `free`, which the library reads and then frees, so nothing owned
by the collector is referenced after the callback returns. A callback that
returns text or bytes cannot return null, because its wrapper uses null to
report an exception. On HashLink the native side receives a `haxe.io.Bytes` object and
reads its two fields; on hxcpp it receives the underlying array.

Data is passed to a callback by value or by reference (a list also as a
slice) and returned by value. The adapter's closure serializes each argument
before the call and deserializes the result; the Haxe wrapper does the
reverse. Two callbacks that differ only in their data types therefore share
one trampoline, as do two that pass objects of different interfaces.

An object passed to a callback is moved, as `T` or as `Arc<T>` of a shared
interface, and the Haxe side owns it as it owns a return value. A borrowed
object is rejected: Haxe could keep the object after the call. The wrapper
creates the Haxe object before it calls the user's function, so the value has
an owner whatever that function does. HashLink calls a closure with untyped
arguments, so there the pointer is passed as bytes and a primitive of the
interface converts it back to a typed object.

**Stored across calls.** A function needed by later calls is stored in a field
of the Haxe object and passed to each call that uses it. Native code holds no
reference to it.

**Not rooted in native code.** A GC root held by native code is invisible to
the collector, so a function that can reach the object that roots it is never
collected. Event handlers usually reference their emitter. Keeping the
function on the Haxe side leaves every reference visible to the collector, so
such cycles are collected normally.

## Futures, streams and signals

`hxn-future` provides three types of asynchronous result, each with a Haxe
class.

| | Rust | Haxe |
|---|---|---|
| one result, later | `pending()` returns a `Completer` and a `Future` | `hxn.Future`: `await()`, `then()` |
| many results, in order | `channel()` returns a `Sender` and a `Stream` | `hxn.Stream`: an iterator, `listen()` |
| a value that changes | `signal()` returns a `Source` and a `Signal` | `hxn.Signal`, through `Reactive.follow` |

Two rules hold for all three.

**The result stays in Rust until a Haxe thread takes it.** Settling, sending
or setting does not touch any runtime. The thread that does it does not need
to be registered with a collector, and no runtime object has to be kept alive
meanwhile.

**The library never calls the host to report a result.** It queues or sets a
value; the host takes each result on its own thread and does what it wants
with it. This is what makes events work from any thread and on wasm.

A future is settled once; later attempts to settle it return `false`. If
everything that could settle it is dropped, it is rejected as cancelled, so a
waiter is never left waiting on a future nothing can settle. A stream ends
when every sender is dropped, or with an error that the reader receives after
the items sent before it. A sender or source detects that its reader is gone
when `send` or `set` returns `false`.

A stream delivers every item, in order. A signal delivers only the latest
value. Use a stream for events and a signal for state.

A function exposes one by returning `Future<T>`, `Stream<T>` or `Signal<T>`,
and the Haxe method returns the class of the same name. The generated adapter
returns each as a type-erased handle. So a library has one set of exports to
wait on and watch a future, whatever its value type and whichever function
returned it, plus one take export for each value type its futures carry. A future or stream can carry a number, a bool,
text, bytes, data or an object; bytes and objects taken from one are owned by
the Haxe side. It can also carry `()`, which Haxe sees as `hxn.Unit`, to
signal completion or an event with no payload. A signal carries a number, a
bool, text or data, but not a single-owner value, because it is read
repeatedly. Data is serialized when a Haxe thread takes it, by the take function
generated for that type; the library's future holds the library's own Rust
type.

An `async fn` is exposed as a function that returns a future of its return
value; the error of a `Result` fails the future, as a typed exception if the
error type is `#[hxn::data]`.
The adapter starts it with `spawn`. The work runs after the starting call has
returned, so it must own its parameters: text and bytes are copied, a shared
object is kept by its own `Arc` reference, and the receiver is `self` or the
`&self` of a shared interface. Anything only borrowed for the call is
rejected: a borrowed object that is not shared, a cell, a Haxe function.

## Running work

| Entry point | For | With threads | Without |
|---|---|---|---|
| `spawn(async { … })` | async work | a worker pool; polled only when woken | the calling thread's queue |
| `stepped(\|\| …)` | work done in steps | a worker pool | the calling thread's queue |
| `spawn_blocking(\|\| …)` | a blocking closure | a dedicated thread | runs before returning |

Each piece of work is a `krio` task. With threads it runs on a
`krio-parallel` cluster of a few workers per library, started on first use;
idle workers sleep. Without threads it is queued on the calling thread
(`krio-runtime`) and advanced by whichever caller waits: a wait runs the queue
for a short time slice and returns whether or not the result is ready, so the
host can yield and poll again.

`spawn` and `stepped` run the first step before returning, so work that
completes immediately does not wait for a worker or a waiter.

An `hxn_future::Future` can itself be awaited, and that is the source of
wakeups: settling it wakes the task awaiting it. A waiting task uses no CPU
on either scheduler.

## Delivery

Every result a library produces reaches the app through a single queue.

- **Outbox (Rust).** A future, stream or signal the host is watching adds
  itself to the library's queue when it has something new, at most once until
  the host has read it. Only watched values are queued.
- **Inbox (Haxe).** `hxn.Inbox`, one per library, drains that queue and calls
  the watchers of each entry, in the order the library produced them, in one
  batch. `Future.then`, `Stream.listen` and `Reactive.follow` register
  watchers.
- **Loop.** `hxn.Loop` decides when delivery happens. By default it runs from
  the Haxe event loop of the thread that first requested delivery. An app
  with its own main loop sets `Loop.manual` and calls `Loop.sync()` where it
  wants deliveries, for instance once per frame; the Haxe event loop is then
  neither used nor kept alive.

A single queue preserves order across futures, streams and signals, lets the
app choose when delivery happens, and needs only one wait for all of them.

## Reactivity

`hxn.Signal`, `hxn.Computed` and `hxn.Reactive` provide fine-grained
reactivity in the style of SolidJS. Dependencies are tracked at run time, so
they follow the branches the code takes.

The dependency graph is implemented in Haxe, so it behaves identically on
every runtime. It marks on write and recomputes lazily on read: a write marks
direct dependents dirty and their transitive dependents as needing a check,
and queues effects; an effect that runs, or a computed value that is read,
first updates its own sources and recomputes only if one changed. A value is
computed once however many paths lead to it, and never from a mix of old and
new values.

`Reactive.follow` creates a signal that tracks a value held in Rust. Three
mechanisms keep the two sides consistent:

- a delivery is one batch, so effects rerun once however many values arrived;
- a binding **writes through**: a set made through the binding also sets the
  followed signal, so code sees its own writes immediately;
- the graph is **pluggable**: `Inbox.follow` takes the sink as a parameter, so
  an app with its own reactive system can feed its own signals.

## Generation

`hxn-gen` parses a library's source for the items marked `#[hxn::interface]`,
`#[hxn::methods]` and `#[hxn::function]` and generates the manifest, both Rust adapters, the C
header, the hxcpp glue and the Haxe classes. It starts at the crate's `lib.rs`
and follows every module declaration from there, so a library can use any
normal crate layout. Names are unique across the library, not per module: the
adapters refer to each interface at the crate's root.

The attributes expand to nothing. A macro expanding inside the library cannot
write files or add code to the separate adapter crates, so the generator
parses the source instead, and the attributes exist only so that the markers
compile.

A marked function outside an `impl` has no receiver type, and Haxe has no
free functions, so it becomes a static method of a class: by default the
class named after the library, which is an interface's class when the names
match and otherwise a class generated for the purpose. Such a class has no
handle and no `close`; the adapters contain only its functions, which they
call by a path from the crate's root.

A generated Haxe class is a plain class with the library's public API, the
same on both runtimes, that calls native functions. On HashLink each class
declares its own natives, in an `@:hlNative` class private to its file. On
hxcpp one extern class per library declares every function of the glue
header with its types, and carries the build's include and link settings; no
generated class contains inline C++.

The manifest is JSON, generated, committed and reviewed: a change to it is a
change to the library's ABI.

## The `hxn` command

`hxn` wraps the generator with the rest of the build, so that a library is a
single crate and its author writes no adapter crate, build script or compiler
flags.

| | |
|---|---|
| `hxn new <name>` | creates a library as a copy of `examples/greeter`: a crate, and a Haxe program that uses it |
| `hxn build <hl\|cpp>` | generates the binding for a runtime and builds it |
| `hxn run <hl\|cpp>` | builds, then compiles and runs the library's Haxe program |
| `hxn package [hl] [cpp]` | builds for both runtimes, or the ones named, and collects what a Haxe program needs into one directory |
| `hxn check [hl] [cpp] [cppia]` | runs the program on each runtime and checks that the outputs match |
| `hxn doctor` | reports which targets can be built and what the others are missing |
| `hxn import <header.h>` | creates a Rust library crate for a C library, from its header |
| `hxn version` | prints the version, which is the tag `hxn new` depends on |
| `--wasm` on any of these | does the same for the runtime's wasm form |
| `--target <triple>` on `build cpp` or `package` | cross-compiles the hxcpp binding |
| `cppia` in place of `hl` or `cpp` | builds for hxcpp and a host executable that contains the binding, and runs the program as a script |

- **The adapter crates are generated, not stored in the library.** A build
  generates the runtime's adapter as a crate under cargo's target directory,
  with a path dependency on the library, and builds it there. The only file
  added to the library's directory is its manifest.
- **The adapter uses the same hxn source as the library.** The command asks
  cargo where the library's `hxn` dependency comes from (a path, a git
  repository or a registry) and takes `hxn-abi` and `hxn-hl` from the same
  source. Two different copies of `hxn-abi` would have incompatible handles.
- **The command embeds hxn's Haxe classes and C header** and writes them next
  to the generated ones, so a library needs nothing installed except the
  command.
- **The compiler arguments are written to a file** that `haxe` accepts: the
  class paths, and for hxcpp the header and library paths. `run` passes the
  same arguments directly.
- **On Windows a HashLink library is linked against the VM's own library.**
  Its adapter's build script links `libhl.lib` and searches for it in
  `HXN_HL_DIR`, which the command sets to the VM's directory. A library
  linked into an hxcpp program on Windows is built with the static C runtime
  that hxcpp uses, the program links the system libraries that Rust's
  standard library needs, and hxcpp is told to build 64-bit for the host,
  which is not its default.
- **Program-specific settings are specified by the crate**, under
  `[package.metadata.hxn]`: the program's directory and main class, and
  `haxe-flags`, extra compiler arguments such as a `-lib` or a `-D`. The VM
  is the user's choice, so it is given on the command line or in `HXN_VM`,
  followed by any options.
- **A wasm build uses the same adapter, compiled for another target.** For
  HashLink it is built as a static archive for `wasm32-wasip1` and linked
  into a module that exports the primitives' resolvers and imports functions
  the program provides; Ash builds the program and loads the module from the
  same directory. For hxcpp it is a static archive for
  `wasm32-unknown-emscripten` that hxcpp links, and node runs the result.
  For HashLink, Rust's standard library is rebuilt position-independent and
  unwinding with wasm exceptions, in the encoding Ash emits
  (`-wasm-use-legacy-eh=false`), which requires a nightly toolchain; the
  command reports this when the toolchain is not nightly. For hxcpp the prebuilt one for Emscripten is
  used: it unwinds with wasm exceptions, as hxcpp's Emscripten build does,
  so a panic is caught there as it is natively.

- **`check` runs the same program on every runtime.** It runs the library's
  Haxe program on each runtime given, or on HashLink and hxcpp if none is,
  and passes only if every run succeeds and all outputs are identical: a
  binding behaves the same whichever runtime calls it, so the program's
  output is the same too. Before building anything it compares the committed
  manifest with one generated from the source, and stops if they differ,
  because a build would overwrite the evidence that the API changed.
- **`doctor` reports missing prerequisites before a build fails on them.** A
  target is ready when cargo, haxe and the target's own tools are installed:
  a HashLink VM, hxcpp, and for wasm either a nightly toolchain with Rust's
  source and Ash, or Rust's Emscripten target, the Emscripten SDK and an
  hxcpp that builds for it with wasm exceptions. It mentions optional tools that would make
  builds faster, and does not require any of them.
- **A script calls a library through its host.** A CPPIA script cannot call
  C++ or hold a pointer, so no part of the binding is compiled into it. The
  host is an hxcpp program built to run scripts, containing the library's
  classes, the same ones any hxcpp program uses, and hxn's own. The generator
  also writes a declaration of each class, an `extern class` with the public
  methods and no implementation, and a script is compiled against those
  declarations and against the list of classes its host contains. For
  `cppia` the command builds such a host, compiles the library's Haxe program
  as a script and has the host run it; the host passes the arguments after
  the script's name to the script as its program arguments.
- **A package lets a Haxe program use the library without its Rust source.**
  It is a directory that is also a haxelib:

  | | |
  |---|---|
  | `<library>.hxn.json` | the manifest |
  | `hxn/` | hxn's own Haxe classes |
  | `binding/hl/`, `binding/cpp/` | the library's generated classes for each runtime |
  | `binding/cppia/` | declarations of the hxcpp classes, for a script |
  | `include/` | `hxn.h` and the library's headers, for hxcpp |
  | `native/<triple>/` | the built libraries for one platform, named by its Rust target triple, or for `wasm32-wasip1` or `wasm32-unknown-emscripten` |
  | `haxelib.json`, `extraParams.hxml` | the haxelib name and version, and the single line that runs `hxn.Bind.lib` |

  Only `native/` differs between machines, so packaging again on another
  machine, with `--wasm` or with `--target`, adds a directory to it.
- **`--target` cross-compiles the hxcpp binding.** The binding is a static
  library that is only linked when the program is, so cross-compiling needs
  the Rust target installed and no linker or SDK for the target. This is
  how a package gets libraries for iOS (`aarch64-apple-ios`), Android
  (`aarch64-linux-android`) or another desktop platform. An `.hdll` is a
  shared library, linked at build time against the system it runs on, so it
  has to be built on that system. `run` and `check` reject `--target`,
  since the result cannot be executed locally.
- **`haxelib.json` is filled in from the crate's metadata.** Name, version
  and description come from the crate, `url` from `repository`, `license`
  from the crate's SPDX expression mapped to one of the names haxelib
  accepts, and `tags` from `keywords`. `contributors` (haxelib account
  names) and `releasenote` have no Cargo equivalent and are set under
  `[package.metadata.hxn]`. `package` prints the fields `haxelib submit`
  still requires.
- **`hxn.Bind.lib("<library>")` configures a compile to use a package.** It
  is a macro that runs before typing, called from a package's
  `extraParams.hxml`, so a program's build specifies only `-lib <library>`
  and no paths. It locates the package by finding its manifest on the class
  path and reads it, rejecting a manifest of a different version. Based on
  the compile's defines it adds `binding/hl` or `binding/cpp` to the class
  path; for hxcpp it defines the header and archive paths, and the word size
  the archive was built for where hxcpp would default to a different one;
  for HashLink it copies the `.hdll`, or the `.wasm` module, next to the
  program's output once that is written, which is where the VM's loader and
  Ash look for it. The native library used is the one for the host machine,
  the wasm one when `wasm` or `emscripten` is defined, the iOS or Android one
  when hxcpp's defines for those platforms are set (`iphoneos`, `iphonesim`,
  or `android` with an architecture define), or the one given by
  `-D hxn_triple=<triple>`. A compile for `cppia` gets only `binding/cppia`;
  an hxcpp compile with `-D scriptable`, which builds a host, keeps every
  class of the binding and of hxn whether or not the host uses it.
- **The macro selects classes; it does not generate them.** A library's Haxe
  classes and the adapter they call are generated by the same run of the
  generator, so their names and signatures cannot disagree. Generating
  classes at compile time from the manifest would need a second emitter,
  possibly of a different version than the one that generated the package's
  adapter.

A library that keeps its adapter crates in its own repository, as the fixture
in `tests/counter` does, gives each one a build script that calls `hxn-gen`.

## Binding a C library

`hxn import` turns a C library into an hxn library: a Rust crate with a safe
API marked with hxn's attributes. The Haxe bindings then come from that
crate as they do for any other library, and Rust programs can use the crate
directly. `hxn-c` does the work in three steps:

1. **Header to WebIDL.** clang reads the header and writes its AST as JSON
   (`-ast-dump=json`), so hxn needs clang installed but does not link
   libclang. Only declarations in the header itself are imported; included
   headers are read to resolve typedefs. The importer maps what it can:
   - A struct declared without its fields is an object. A function whose
     first parameter points to it is a method, named without the object's
     prefix (`tally_add` becomes `add`). A function named `free`, `destroy`
     and so on frees it, and one named `new`, `create` and so on that
     returns it is the constructor.
   - A struct with fields is passed by value if every field is a boolean or
     a number. An enum becomes a C-like `#[hxn::data]` enum.
   - `const char *` is text, and a `const` byte pointer followed by an
     integer length is one bytes parameter. A length other than a `size_t`
     is `[HxnLength]`, and bytes that do not fit it are an error before the
     call.
   - A non-`const` byte pointer followed by an integer length is bytes C
     writes into, `[HxnWritten]`: a `&mut [u8]` in Rust and a `haxe.io.Bytes`
     in Haxe that sees the writes. It is taken to be written only during the
     call, like a lent callback, and listed for checking. The length must be
     named as one (`n`, `len`, `size`, `count`, `*_len` and so on): a buffer
     C keeps, such as the one `sqlite3_deserialize` takes, comes with a size
     named otherwise.
   - A pointer to fixed-width integers, `float` or `double` followed by a
     count named the same way is an array: `sequence<double>`, a slice in
     Rust and an `Array` in Haxe, copied in for a `const` pointer. A
     non-`const` one is an array C fills, `[HxnOut]`: the caller passes how
     many elements and gets them back, and the C function returns nothing or
     a status. An integer whose width depends on the platform, `long` or
     `size_t`, is not an element, since Haxe's array cannot say which.
   - A `void (*)(void *)` after a text or bytes parameter is taken as its
     destructor, `[HxnFrees]`, as in `sqlite3_bind_text`. C is given a copy
     and a destructor that frees the copy, so it may keep the text or bytes
     for as long as it wants. Any other function pointer passed without user
     data is not imported; one that C is always passed the same value for,
     such as `SQLITE_TRANSIENT`, is added by hand with `[HxnPass=n]`. A returned `char *` is freed with the library's
     only function that takes a `char *` and frees it, if it has exactly
     one.
   - A non-`const` pointer to a number, an enum, a struct, text or an
     object pointer after all the passed parameters is an output: the
     function returns what C writes through it, and an `int` return value
     becomes a status. Outputs after the first are passed `NULL`. A function
     that writes an object through a pointer is that object's constructor
     or static function, so `sqlite3_open(path, &db)` becomes the
     constructor of `Sqlite3`.
   - An object returned by a method of another object is taken as borrowed:
     it belongs to that object, and is not freed by Haxe.
   - A function pointer followed by a `void *` for user data is a callback,
     declared as a `callback` WebIDL definition of its type. It takes
     booleans, integers, floating-point numbers, strings, enums or structs,
     and returns nothing, a boolean or an `int`. A callback is lent for the
     call: a Haxe function that takes at most four values, all `int`s and
     not other integers. A function whose name suggests it keeps the
     callback to call later (`set`, `register`, `hook`, `handler` and so
     on) keeps it instead, `[HxnKept]`: an `int` it returns is taken as a
     status, and a pointer, usually the callback it replaced, is ignored.
   - Each of these guesses is listed under "Check:" at the end of the
     WebIDL, and anything that cannot be mapped is listed as not imported.
   - A header may declare functions the installed library leaves out, as
     sqlite3's does for its optional features, and a binding that refers to
     one does not load. For each library given to `--link` that the C
     compiler finds as a file, `nm` lists what it defines, and a function it
     does not define is listed as not imported.
2. **The WebIDL is reviewed and edited.** A header cannot record which
   `int` is a status code, who frees a returned pointer, or which pointer
   may be null. That is added as extended attributes: `[HxnStatus]` makes a
   non-zero return value an exception, `[HxnFree=f]` names the function
   that frees an object or returned text, `[HxnOut]` and `[HxnNull]` mark
   outputs, `[HxnBorrowed]` a returned object the caller must not free,
   `[HxnThreadSafe]` an object the library allows calls on from several
   threads at once, `[HxnUser]` where C passes a callback its user data,
   `[HxnStop]` what C is returned when the Haxe function throws,
   `[HxnKept]` a callback C keeps, and `[HxnPass]` a pointer C is always
   passed the same value for (see
   `hxn-c`'s `idl` module for the full list). The file is committed. `hxn-c` reads only
   this subset of WebIDL, and reports the line of anything else.
3. **WebIDL to Rust.** Each build generates `src/bindings.rs` from the
   WebIDL again: `extern "C"` declarations, and safe wrappers over them. An
   object owns its C pointer and frees it in `Drop`, unless it is borrowed.
   An object C writes through a pointer is owned before the status is
   checked, so it is freed even when the call fails. Because a C library is
   not assumed to be thread-safe, every method takes `&mut self`, so the
   handle allows one call at a time, and the object is only `Send`; a
   `[HxnThreadSafe]` object's methods take `&self` and it is `Sync`. A
   wrapper returns `Result<_, String>` where the call can fail: text with a
   NUL, a platform-sized integer that does not fit, a C enum value with no
   variant, a null object pointer, or a status code.
   A callback is passed to C as a generated `extern "C"` trampoline, with
   the Haxe function as its user data. C cannot be unwound through, so the
   trampoline catches a panic and keeps it, keeps a failure, and returns
   `[HxnStop]` to C, or does nothing when C cannot be told to stop. Once C
   returns, the panic is resumed or the Haxe function's failure returned,
   ahead of anything the call itself reports.
   A callback C keeps cannot be a Haxe function, because native code does
   not keep a host function alive or call one after the call returns. The
   function that registers it returns a `Stream` instead. Its trampoline
   copies what C passes, sends it to the stream, and returns `[HxnStop]` to
   C on every call; the program reads the calls on its own thread. With more
   than one value, each call is a record, `<Callback>Event`. The stream's
   sender is C's user data. A method's object owns it and drops it after its
   C free function runs, when C can no longer call the callback, and the
   stream ends. A sender given to a function with no object, or to a
   borrowed object, is never freed, since nothing reports when C stops
   calling it. A callback registered again replaces the earlier one in C,
   and the earlier stream ends when the object is freed.

The crate's `build.rs` compiles the C sources given to `--source` with the
`cc` crate, or links the libraries given to `--link`. A static library does
not carry the system libraries it links to the program's link, so for
hxcpp the build reads them from cargo's messages (`linked_libs` and
`linked_paths` of each build script) into `link.xml`, which the generated
glue includes and a package carries beside the archive. This applies to any
library whose dependencies link a system library, not only to imported
ones.

The glue the hxcpp build compiles against is in headers, and hxcpp decides
whether to recompile a file by its own timestamp alone. So the generated
build XML lists `hxn.h` and the library's two headers as dependencies of
every file: a change to the glue recompiles what includes it, instead of
linking objects built against the old one. For `cpp --wasm`, hxn
tells `cc` to compile with `emcc`. `hl --wasm` is not supported, because
compiling C for `wasm32-wasip1` needs the WASI SDK.

`hxn import` reads C only. A C++ library is bound through a C API over it,
which the library's author provides: hxn does not generate one, since how
classes, overloads, templates and exceptions map onto C is the author's
decision.

Some C is left out by design, and listed as not imported:

- **An object passed to a callback.** C owns it and may free it once the
  callback returns, and a Haxe object for it could be kept past that. A
  lent function is given objects to keep, which this is not.
- **A callback whose user data is neither first nor last**, or that takes
  other untyped pointers, as `sqlite3_trace_v2`'s does: what those point to
  depends on the other arguments.
- **Variadic functions and `va_list`.**
- **Pointers whose meaning the header does not give**, such as a `void *`
  with no length, or a buffer C keeps. A function that needs one is
  written in the library's own Rust beside the generated bindings.

## Runtimes

| Runtime | Adapter | Threads | A panic | Notes |
|---|---|---|---|---|
| HashLink | `-hl` as an `.hdll` | yes | becomes a thrown error | |
| Ash, native | the same `.hdll` | yes; cooperative in its interpreter mode | becomes a thrown error | Tested in its hybrid, JIT and interpreter modes. |
| Ash, wasm | `-hl` as a side module | Haxe threads as fibers; no Rust threads | becomes a thrown error | The module uses the program's allocator. |
| hxcpp | `-c` with C++ glue | yes | becomes a thrown error | |
| CPPIA | none of its own: a script runs in an hxcpp host that contains the binding | yes | becomes a thrown error | The host is built with `-D scriptable` and dead-code elimination off. |
| hxcpp through Emscripten | the same | none | becomes a thrown error | Rust's prebuilt standard library unwinds with wasm exceptions, which hxcpp from git also uses; hxcpp 4.3.2 uses JavaScript exceptions and does not link with it. hxcpp's collector does not complete a forced collection there, so `hxn.Gc.collect()` does nothing and a meter only counts. |
| hxcpp on iOS | the same, cross-compiled with `--target` | yes | not tested | hxcpp produces a static library for an app to link, and merges the Rust library into it. Tested with the examples in a simulator only, not on a device. |
| hxcpp on Android | the same, cross-compiled with `--target` | yes | not tested | hxcpp produces a shared library for Java to load, with the Rust library linked in. Program output goes to logcat. Tested with the examples on an emulator only. |

The manifest format is partly based on Rayzor's plugin ABI, but there is no
Rayzor adapter.

On a runtime without threads, nothing in hxn blocks or starts a thread: waits
return after a time slice, work is queued on the calling thread, and delivery
polls once per event loop iteration.
