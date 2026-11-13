# Design

hxn generates adapters that let Haxe programs use a Rust library on several
runtimes. The library code is shared across runtimes; adapters handle each
runtime's object layout, strings, garbage collection and calls. This document
describes the implementation and its constraints.
[conventions.md](conventions.md) lists the coding and testing rules.

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
for a return value. The generated Haxe uses these same boundary types for
narrower Rust numbers; `u32` is exposed as a `UInt`. Haxe has no unsigned
64-bit type: a `u64` arrives as a `haxe.Int64` with the same bits, and values
from 2^63 up read as negative.

On wasm32, pointers, `usize` and `isize` are 32 bits. `i64` and `u64` remain
64 bits and use the native encoding. The adapter still represents a size as a
Haxe `Int` and checks its range, while arithmetic on sizes inside the 32-bit
library wraps at 4 GiB. Both wasm runtimes keep 64-bit values inside the wasm
module, avoiding a JavaScript `BigInt` conversion.

Text is UTF-8 at the boundary. HashLink strings are UTF-16, so the HashLink
adapter converts at the boundary, strictly: invalid text in either encoding is
an error and is never replaced with a replacement character.

A `&mut [u8]` parameter gives Rust direct access to the storage of a Haxe
`haxe.io.Bytes`: `hl.Bytes` on HashLink and the array's base address on hxcpp.
Both collectors leave that storage in place, and writes are visible to the
Haxe caller. A `&mut [u8]` may not alias any other reference, so before making
either the adapter checks that the bytes are not also another bytes argument
of the same call, which Haxe can pass by giving one `Bytes` twice, and fails
the call with `Status::ALIASED` if they are.

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
| a nonempty tuple `(A, B, ...)` | `hxn.Tuple<A, B, ...>`, with typed `_0`, `_1`, ... fields |
| `HashMap<K, V>`, `BTreeMap<K, V>` | `Map<K, V>`; keys are `String`, `Int` or a C-like enum |
| `HashSet<T>`, `BTreeSet<T>` | `hxn.Set<T>`; a `BTreeSet` returns Rust's `Ord` order |
| `Box<T>` | `T` |

- The adapter converts a C-like enum between variant names and declared
  integer values with a `match` in each direction. An unknown integer fails
  the call. This conversion is independent of Rust's enum layout.
- The generated serializers write records, tagged enums, `Option`, `Vec`
  and maps to bytes in declaration order: a
  tagged enum as its variant index followed by that variant's fields, an
  `Option` as a presence byte followed by the value, a list or a map as a
  count followed by the entries. A set uses a list's count and elements. An
  empty record occupies one zero byte, so the count remains bounded by the
  bytes available. The Rust and Haxe serializers use the same declaration;
  the encoding contains field values and structural markers, with no type
  information. Each runtime's byte-buffer support transports composite and
  nested data.
- `Vec` and `Box` support recursive data such as trees. Generated readers
  track nesting depth and fail with `Status::DEPTH` beyond
  `wire::Reader::DEPTH`.
- A nonempty Rust tuple becomes `hxn.Tuple<A, B, ...>`, a Haxe abstract over
  an array. Construct one with `hxn.Tuple.of("name", 42)` or
  `new hxn.Tuple<String, Int>("name", 42)`; numbered record literals are also
  accepted. `pair[index]` reads and writes with runtime bounds checks. Since
  positions can have different types, indexing returns `Dynamic`. For a
  statically typed position, `pair.at(1)` and `pair.set(1, value)` require an
  integer literal and check its bounds and type during compilation. Typed
  `_0`, `_1`, ... properties remain available. Positions are mutable, but
  their declared types and the tuple's `length` are fixed. `copy` makes a
  shallow copy; `toArray` returns a separate `Array<Dynamic>` for inspection.
  A generic-build macro defines the typed properties for each arity. The
  common indexing abstract and each arity's abstract erase to the same array,
  including in a CPPIA host and its scripts, with no wrapper allocation.
  Elements use the data encoding in position order, with no count or tag.
  The reader reads each element before constructing the tuple, so evaluation
  order cannot reorder the input. Tuples can nest anywhere data is accepted;
  `()` denotes a return value with no data; tuple data requires at least one
  element.
- For `Map<Mode, T>` with a C-like enum key, Haxe retains the enum type and
  uses integer key storage. The wire contains each
  key's declared discriminant, checked by Rust's enum reader, followed by
  the value. Tagged enums are not map keys.
- `hxn.Set<T>` provides `add`, `has`, `remove`,
  `clear`, `length`, iteration, `copy` and `toArray`. Array literals convert
  to sets, and construction and `toArray` copy the array. Its array storage
  preserves insertion order and membership checks are linear. Membership
  uses Haxe `==` by default, including object identity; an optional equality
  function compares data values differently. Rust applies its own `Eq + Hash`
  or `Ord` when reading elements, so it may remove more duplicates. A
  `BTreeSet` writes elements in Rust's `Ord` order, and a `HashSet` promises
  no order. The wrapper preserves that order, but later additions do not
  sort it. Sets, including a set of bytes, use the data encoding rather
  than a byte buffer.
- A reader rejects truncated data, counts that exceed the available bytes,
  invalid UTF-8 and trailing bytes. Each fails the call with the same status
  as malformed text.
- Adapters copy data in both directions. A Rust function takes the
  deserialized value or a reference to that copy; returned data is serialized
  and then dropped. Haxe records therefore need no `close()`. Editing a Haxe
  record after a call leaves the library's copy unchanged.
- Generated data classes use the same Haxe file for HashLink, hxcpp and
  CPPIA. Construct a record with named fields, `{label: "x", enabled: true}`,
  and a field that is an `Option` can be left out. A null where a value is
  needed is refused in Haxe, before anything is written. A Haxe enum
  cannot have methods, so a tagged enum's serializer is a separate
  generated class: `EventWire` for `Event`.
- On a record field, `#[hxn(default)]` calls
  `Default::default()` when Haxe omits the field; `#[hxn(default =
  "Item::default_unit")]` calls that public zero-argument function, whose
  path is relative to the library root. The generated Haxe field is
  nullable and starts as null. Null requests the default on every crossing
  into Rust; false, zero and empty collections are explicit values. Defaults
  are constructed lazily by the adapter, without changing the Haxe input.
  Rust always writes the actual value back, so returned fields are filled.
  Each defaulted field adds a presence byte before its usual encoding;
  bindings and adapters must be regenerated together after changing a
  default annotation. The manifest records `default: true` or the function
  path. Defaults apply to non-nullable record fields. An `Option` field
  already uses `None` when omitted; tagged enum fields remain required.
- Data can be passed as function or callback parameters and return values,
  returned from async functions, and stored in futures, streams and signals. The
  encoding is the same in every position: the adapter serializes or
  deserializes on the Rust side and the binding on the Haxe side.
- Serializing data copies and visits every field on each call. Use records
  for descriptors, results and short lists; use byte buffers for large data
  transfers and interfaces for values that require identity.

## Failures

Adapters raise runtime exceptions after Rust library calls return and release
their local values.

- A library function returns a `Result`. The adapter turns `Err` into a status
  and a message, which the Haxe binding throws as a `haxe.Exception`.
- For a `Result` with a named `#[hxn::data]` error, Haxe generates an
  exception class such as `PantryException` for `PantryError`. Its `error`
  field holds the data value, and its message is the error's `Display`
  output. The adapter attaches the serialized value to the failure. `guard`
  and `prim` store those bytes in `hxn_abi::thrown`, a thread-local, and
  report the message through the usual exception path. The generated Haxe
  method catches the exception, retrieves the bytes and constructs the
  typed exception. Haxe constructs the exception object after Rust returns.
  Each failure replaces the thread-local value, so a handler reads the
  payload of the failure it just caught.
- A failed future stores the error bytes with its failure. Taking the
  result moves those bytes to the thread-local for the Haxe handler.
- A `Stream<Result<T, E>>` with a named `#[hxn::data]` error exposes its `Ok`
  values as `hxn.Stream<T>`. Taking the first `Err` closes the read side,
  drops later queued values outside the queue lock and refuses further
  sends. The adapter serializes the error on the taking Haxe thread.
  Iteration throws the typed exception; `listen` delivers it to `onError`.
  A channel failure from `Sender::fail` retains its status, message and
  optional payload.
- Rust finishes the call and drops its local values before the adapter
  raises the runtime exception. HashLink throws by jumping past frames
  without running destructors; C++ exceptions must stay outside Rust frames.
  Ash's wasm exception unwinds through the adapter's own frame, so a
  HashLink primitive that can throw uses `extern "C-unwind"`. Declaring that
  primitive `extern "C"` would make the exception abort the program.
- A panic is caught at the boundary and reported as `Status::PANIC`, on every
  runtime. On wasm the library unwinds with wasm exceptions: Rust's prebuilt
  standard library does for Emscripten, and an Ash side module rebuilds it
  that way. On WASI Rust takes two functions from the toolchain's libunwind,
  which an Ash side module does not have, so `hxn-hl` defines them for that
  build (`unwind.rs`), throwing with the wasm `throw` instruction as Rust's
  own shim for `wasm32-unknown-unknown` does.

## Handles

A `Handle<T>` separates two lifetimes:

- the value is dropped when the host calls `close()`;
- the allocation is freed when the host's garbage collector finalizes the
  object.

The allocation remains valid while the host can reach its pointer. Access
checks reject a closed handle or a handle with the wrong type tag.

The host can call from two threads or re-enter from a callback, so the adapter
checks shared and exclusive borrows at run time. A conflicting borrow returns
`Busy` immediately, preventing blocking and aliased access.

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

- `&T` and `&mut T` parameters borrow the object for the call. Conflicting
  borrows fail with "in use", including when the same object is both a
  `&mut self` receiver and an argument. A `T` parameter transfers ownership
  as described in the table.
- Return values transfer ownership to Haxe. The boundary cannot express a
  lifetime tying a borrowed result to another value. A child that needs its
  parent must hold a shared reference to it.
- An interface has a single owner unless the library marks it shared.
- Haxe owns host values. When Rust needs to refer to one, the binding stores
  the value in a table and gives Rust its id. Keeping host references in
  native GC roots would prevent the collector from detecting cycles.
- The library that created a handle validates its type tag. Passing the
  handle to another library fails that check.
- For an object list, the adapter puts the elements in a new stream and
  closes the sender. The Haxe method drains that completed stream into an
  `Array` before returning. This reuses the stream handle and element-take
  functions on every runtime; all elements are ready when the call returns.

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

- Returning the same shared value twice creates two Haxe objects, each
  owning a handle with one `Arc` reference. Use `same(other)` to compare the
  underlying values' identity.
- `close()` closes the Haxe object and releases its reference. Rust drops
  the value when the last reference is released.
- `weak()` returns a weak reference, an instance of a separate class named
  `<Interface>Weak`. Its `upgrade()` returns a new object holding a
  reference, or null once the value has been dropped. A child uses one to
  refer to its parent without creating a reference cycle.
- Shared interfaces require `Send + Sync`. Their functions take `&self` and
  keep mutable state behind a cell or lock, following Rust's rules for types
  shared between threads.
- The collector releases unreachable Haxe objects' references. A reference
  held by the library keeps the shared value alive through collection.

## Atomics

Haxe atomic integers and Rust's `AtomicI32` share a 4-byte layout. Passing a
cell's address lets each side use its own atomic operations from any thread,
without a cross-runtime call.

| In Rust | Meaning | In Haxe |
|---|---|---|
| `&AtomicI32`, `&AtomicFlag` parameter | a cell borrowed for the call | `hxn.AtomicInt`, `hxn.AtomicBool` |
| `Arc<AtomicI32>`, `Arc<AtomicFlag>` result | a cell owned by the library | the same, backed by the library's memory |

- Haxe allocates borrowed cells and keeps them alive for the call. Rust's
  borrow ends when the call returns. A cancellation flag for a call in
  progress can use this storage.
- Rust allocates library-owned cells. The Haxe object holds the returned
  reference for as long as the object is reachable. These objects have no
  `close()`, keeping the cell's address valid throughout use.
- A flag is an integer that is 0 or 1. Rust's `AtomicBool` is one byte and
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
without triggering a collection. hxn provides explicit collection, native
memory accounting and collection requests through the binding:

| Operation | Rust | Haxe |
|---|---|---|
| force a collection | | `hxn.Gc.collect()` |
| report a value's native memory | an `hxn::Weight` kept in the value | `hxn.Gc.budget` |
| request a collection | `hxn::collect_soon()` | honoured at the next check |

- `hxn.Gc.collect()` calls each runtime's own collection function. It
  returns false on a runtime where a collection cannot be forced.
- A weight is counted while its value is alive. A `Weight` adds to a total
  when created, updates it on `set`, and subtracts from it when dropped, at
  `close()` or finalization. The total is per library, because each library has its own
  copy of `hxn-abi`.
- Each library's Haxe classes share one `hxn.Gc.Meter`, which polls the
  library's native-memory total. An interface
  marked `#[hxn::interface(weighed)]` checks the meter whenever one of its
  objects is created and before each of its methods runs, since an earlier
  call may have grown what it holds; every delivery checks it too. A check triggers a
  collection when the total exceeds `hxn.Gc.budget`, and then not again until
  the total exceeds twice what remained, so a program whose live values
  exceed the budget does not collect on every allocation.
- `collect_soon()` sets a flag from any thread. The next meter check clears
  the flag and collects.
- hxn applies this collection policy on every runtime. hxcpp's external
  memory accounting requires reports for allocations and releases. Finalizers
  cannot report releases, and deferred reports arrive after hxcpp computes
  the threshold for that collection.
- `hxn.Loop.exit` ends an hxcpp program without running the C runtime's
  destructors. hxcpp keeps zeroing memory on its own threads after a
  collection, and `exit` destroys a lock those threads take when they
  finish, so on Windows a program that exits shortly after a collection
  crashes. A meter triggers a collection when a library's memory exceeds its
  budget. `hxn.Loop.exit` remains safe when that collection occurs shortly
  before program exit. On other runtimes it calls
  `Sys.exit`. The crash is in hxcpp 4.3.2 and in hxcpp's git history before
  HaxeFoundation/hxcpp#1298 (commit `0e9183de`), which made that lock a
  `std::mutex`; it does not occur from that commit on. It was reported as
  HaxeFoundation/hxcpp#1470. hxn is built and tested against hxcpp from git
  at a commit after #1298, which CI pins; `hxn doctor` warns about an hxcpp
  from before it. `hxn.Loop.exit` remains the safe way to exit with hxcpp
  4.3.2.

## Calling a Haxe function

The binding invokes Haxe callbacks during the native call, on the runtime
thread that made the call. A wrapper catches each Haxe exception before
control returns to Rust.

The wrapper stores the exception with `hxn.Callback.failed` and returns a
failure value to Rust. The library stops and returns, and the binding rethrows
the original exception (`hxn.Callback.during`). Both adapters use this protocol
to keep Haxe exceptions on the Haxe side of the call.

Rust functions accept callbacks as `impl Fn(..) -> Result<T, E>`, generic in
`E`. A Haxe exception becomes an `Err` for the library to return. The adapter
supplies the callback's error type, while core tests can use any error type.

A callback can have numbers, bools, text, bytes and data as parameters and
return value, and objects as parameters. Text and bytes are passed to it as
borrowed `&str` and `&[u8]`; the host copies them during the call. They are
returned as `String` and `Vec<u8>`: the host copies the result into a buffer
with its own `free`. Rust reads and frees that copy after the callback
returns; the host retains ownership of the collector-managed storage. A
callback that returns text or bytes cannot return null, because its wrapper
uses null to report an exception. On HashLink the native side receives a
`haxe.io.Bytes` object and reads its two fields; on hxcpp it receives the
underlying array.

A callback's `i64` is a `haxe.Int64`, as it is in an ordinary call. In a
callback it crosses as eight serialized bytes, using the same encoding as
an `i64` field in data. The wrapper reads and writes those bytes, so a
runtime's dynamic numeric conversions cannot lose bits by narrowing it or
passing it through floating point.

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

Haxe objects store functions needed across calls in fields and pass them to
each native call that uses them. The collector can trace those references and
collect cycles, including an event handler that refers to its emitter. A
native GC root would keep such a cycle alive because the collector cannot
trace references through native storage.

## Futures, streams and signals

`hxn-future` provides three types of asynchronous result, each with a Haxe
class.

| Result | Rust | Haxe |
|---|---|---|
| one result, later | `pending()` returns a `Completer` and a `Future` | `hxn.Future`: `await()`, `then()` |
| many results, in order | `channel()` returns a `Sender` and a `Stream` | `hxn.Stream`: an iterator, `listen()` |
| a value that changes | `signal()` returns a `Source` and a `Signal` | `hxn.Signal`, through `Reactive.follow` |

Futures, streams and signals store their results in Rust until a Haxe thread
retrieves them. Producer threads settle futures, send stream items and update
signals through Rust-owned storage. They use no runtime objects and need no
collector registration. The Haxe thread retrieves the values and runs their
registered handlers. This delivery protocol also works on wasm.

A future is settled once; later attempts to settle it return `false`. If every
producer is dropped, the future is rejected as cancelled and wakes its
waiters. A stream ends when every sender is dropped, or with an error that the
reader receives after the items sent before it. A sender or source detects
that its reader is gone when `send` or `set` returns `false`.

Use a stream when consumers need every item in order, and a signal when they
need the latest state.

A function exposes one by returning `Future<T>`, `Stream<T>` or `Signal<T>`,
and the Haxe method returns the class of the same name. The generated adapter
returns each as a type-erased handle. All futures in a library share one set
of wait/watch exports, with a separate take export for each value type. A
future or stream can carry a number, a bool, text, bytes, data or an object;
bytes and objects taken from one are owned by the Haxe side. It can also carry
`()`, which Haxe sees as `hxn.Unit`, to signal completion or an event with no
payload. A signal carries a number, a bool, text or data, but not a
single-owner value, because it is read repeatedly. Data is serialized when a
Haxe thread takes it, by the take function generated for that type; the
library's future holds the library's own Rust type.

A future or stream can also carry a list of objects, `Vec<T>` or
`Vec<Arc<T>>` of a shared interface. Haxe receives `Array<T>`. The Rust
result keeps the vector until it is taken; the take call transfers its
values into an ended stream, and the Haxe wrapper drains that stream into
the array before delivering it. The element take exports are generated even
when no function returns a stream of individual objects. Until an element
is taken it remains owned by Rust, so dropping an untaken result releases
its values without creating host objects.

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

Settling an `hxn_future::Future` wakes the task awaiting it. Both schedulers
suspend waiting tasks until a wakeup, using no CPU while they wait.

`stepped` is for work that is all computation, split so a single thread can
interleave it with other work: `Poll::Pending` from a step means there is
more to do, and the next step runs at once. Work that waits on something is
an `async` block given to `spawn`. A task never calls a Haxe function; what
it produces reaches Haxe as the future's, stream's or signal's value.

## Delivery

Each library uses one queue to notify Haxe of changes to watched results.

- Rust queues a watched future, stream or signal's identifier when its
  result changes. Each identifier is queued at most once until Haxe reads
  it; unwatched results produce no queue entries.
- `hxn.Inbox`, one per library, drains the queue and calls each entry's
  watchers in production order, as one batch. `Future.then`, `Stream.listen` and `Reactive.follow` register
  watchers.
- `hxn.Loop` schedules delivery. By default it runs from
  the Haxe event loop of the thread that first requested delivery. An app
  with its own main loop sets `Loop.manual` and calls `Loop.sync()` where it
  wants deliveries, for instance once per frame; the Haxe event loop is then
  neither used nor kept alive.

A single queue preserves order across futures, streams and signals, lets the
app choose when delivery happens, and needs only one wait for all of them.

Wrap related signal updates in `hxn_future::together(|| { … })` to deliver
them as one change. The entries the scope's changes would add are held back
until it returns and then added at once, and the host does not read a signal's
value while any scope is running. An inbox takes every entry before it runs
effects, so effects never see one of those values changed and another not.
Scopes on different threads run one at a time, and the library's own reads of
a signal never wait: only the host's do.

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
- a binding updates the followed Haxe signal when it sets the Rust value,
  making the change immediately visible to Haxe code;
- `Inbox.follow` accepts a sink function, allowing an app to update signals
  in its own reactive system.

## Generation

`hxn-gen` parses a library's source for the items marked `#[hxn::interface]`,
`#[hxn::methods]` and `#[hxn::function]` and generates the manifest, both Rust
adapters, the C header, the hxcpp glue and the Haxe classes. It starts at the
crate's `lib.rs` and follows every module declaration from there, so a library
can use any normal crate layout. Names are unique across the whole library.
Adapters refer to each interface at the crate's root.

The generator reads the marked items from source and writes the files for both
adapter crates. Attribute macros remove the marker notes and retain the Rust
items. Macro expansion inside the library cannot add code to the separate
adapter crates or reliably write their files.

A marked function outside an `impl` becomes a static Haxe method. By default,
the generator puts the method in the class named after the library. It uses an
existing interface class when the names match, or generates a class for the
functions. A function-only class needs no handle or `close`; the adapters call
its functions through paths from the crate's root.

A generated Haxe class is a plain class with the library's public API, the
same on both runtimes, that calls native functions. On HashLink each class
declares its own natives, in an `@:hlNative` class private to its file. On
hxcpp one extern class per library declares every function of the glue
header with its types, and carries the build's include and link settings; no
generated class contains inline C++.

The manifest is JSON, generated, committed and reviewed: a change to it is a
change to the library's ABI.

## The `hxn` command

`hxn` runs generation and builds the runtime adapters for a library defined in
a single Rust crate. It supplies the adapter crates, build scripts and
compiler arguments.

| Command or option | Behavior |
|---|---|
| `hxn new <name>` | creates a library as a copy of `examples/greeter`: a crate, and a Haxe program that uses it |
| `hxn build <hl\|cpp>` | generates the binding for a runtime and builds it |
| `hxn run <hl\|cpp>` | builds, then compiles and runs the library's Haxe program |
| `hxn package [hl] [cpp]` | builds for both runtimes, or the ones named, and collects what a Haxe program needs into one directory |
| `hxn check [hl] [cpp] [cppia]` | runs the program on each runtime and checks that the outputs match |
| `hxn doctor [--target <triple>]` | reports tools, runtime prerequisites and installed Rust standard libraries; checks a requested binding target |
| `hxn import <header.h>` | creates a Rust library crate for a C library, from its header |
| `hxn version` | prints the version, which is the tag `hxn new` depends on |
| `--wasm` on any of these | does the same for the runtime's wasm form |
| `--target <triple>` on `build cpp` or `package` | cross-compiles the hxcpp binding |
| `cppia` in place of `hl` or `cpp` | builds for hxcpp and a host executable that contains the binding, and runs the program as a script |

- A build creates the runtime's adapter crate under Cargo's target
  directory, adds a path dependency on the library and builds the adapter
  there. It writes the API manifest in the library's directory.
- The command uses Cargo metadata to locate the library's `hxn` dependency
  in a path, git repository or registry. It takes `hxn-abi` and `hxn-hl`
  from that same source to keep their handle types compatible.
- Before building, the command compares its version with the library's
  `hxn` version. A mismatch stops the build and reports the dependency tag
  or command version to install. Generated adapters require the matching
  version of the `hxn` crates.
- The first adapter build copies the library's `Cargo.lock`. Later builds
  copy it again when the library's lock file changes, preserving the
  library's resolved dependency versions.
- The command embeds hxn's Haxe support classes and C header and writes
  them alongside the generated binding. The command supplies these support
  files during each build.
- The command writes Haxe class paths and hxcpp header/library paths to a
  compiler argument file. `run` passes the same arguments directly to Haxe.
- On Windows a HashLink library is linked against the VM's own library.
  Its adapter's build script links `libhl.lib` and searches for it in
  `HXN_HL_DIR`, which the command sets to the VM's directory. A library
  linked into an hxcpp program on Windows is built with the static C runtime
  that hxcpp uses, the program links the system libraries that Rust's
  standard library needs, and hxcpp is told to build 64-bit for the host,
  which is not its default.
- Set the program directory, main class and extra `haxe-flags` under
  `[package.metadata.hxn]`. Flags can include `-lib` and `-D`. Select the
  VM and its options on the command line or through `HXN_VM`.
- A wasm build uses the same adapter, compiled for another target. For
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

- `check` runs the library's Haxe program on the requested runtimes,
  defaulting to HashLink and hxcpp. It requires every run to succeed and all
  outputs to match. Before building, it compares the committed manifest with
  the current source and stops on a difference, preserving the API change
  for review.
- `doctor` checks Cargo, Haxe and the tools required by each target:
  a HashLink VM, hxcpp, and for wasm either a nightly toolchain with Rust's
  source and Ash, or Rust's Emscripten target, the Emscripten SDK and an
  hxcpp that builds for it with wasm exceptions. It also lists optional
  tools that can speed up builds. To find installed standard libraries,
  `doctor` looks for `std` archives in the active rustc sysroot. This works
  with custom toolchains independently of rustup. It excludes empty target
  directories and core-only installations and reports an unreadable
  inventory as unavailable.
  `doctor --target <triple>` checks cargo and that target's standard library
  for a Rust static binding, and its exit status reflects that check instead
  of whether a local HashLink or hxcpp program can run. A missing built-in
  target produces a `rustup target add` command; an unknown name produces a
  `rustc --print target-list` hint. Bindings require `std`. Dependencies of
  the library and the Haxe app may require additional platform tools and
  SDKs beyond this check.
- A CPPIA script calls the binding through an hxcpp host. The host contains
  the library's generated classes and hxn's support classes. The script uses
  class declarations because CPPIA cannot call C++ or hold native pointers. The generator
  also writes a declaration of each class, an `extern class` with the public
  methods and no implementation, and a script is compiled against those
  declarations and against the list of classes its host contains. For
  `cppia` the command builds such a host, compiles the library's Haxe program
  as a script and has the host run it; the host passes the arguments after
  the script's name to the script as its program arguments.
- `package` creates a haxelib directory containing the built library and its
  bindings:

  | Path | Contents |
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
- `--target` cross-compiles the hxcpp binding. The binding is a static
  library that is only linked when the program is, so cross-compiling needs
  the Rust target installed. The linker and SDK are needed later, when
  linking the application. A package can include libraries for iOS (`aarch64-apple-ios`), Android
  (`aarch64-linux-android`) or another desktop platform. An `.hdll` is a
  shared library, linked at build time against the system it runs on, so it
  has to be built on that system. `run` and `check` reject `--target`,
  since the result cannot be executed locally.
- `haxelib.json` is filled in from the crate's metadata. Name, version
  and description come from the crate, `url` from `repository`, `license`
  from the crate's SPDX expression mapped to one of the names haxelib
  accepts, and `tags` from `keywords`. `contributors` (haxelib account
  names) and `releasenote` have no Cargo equivalent and are set under
  `[package.metadata.hxn]`. `package` prints the fields `haxelib submit`
  still requires.
- `hxn.Bind.lib("<library>")` configures a compile to use a package. It
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
- The generator produces a library's Haxe classes and adapters together,
  keeping their names and signatures consistent. `hxn.Bind.lib` selects
  those packaged classes during compilation. Generating them again from the
  manifest would require a second emitter whose version could differ from
  the package's adapter generator.

A library that keeps its adapter crates in its own repository, as the fixture
in `tests/counter` does, gives each one a build script that calls `hxn-gen`.

## Binding a C library

`hxn import` creates a Rust crate with safe wrappers around a C library's API
and marks those wrappers for hxn generation. Both Rust programs and the
generated Haxe classes use this crate. `hxn-c` imports it in three steps:

1. clang reads the header and writes its AST as JSON
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
2. Review the generated WebIDL and add ownership, nullability and status
   information through extended attributes: `[HxnStatus]` makes a
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
3. Each build regenerates `src/bindings.rs` from the reviewed WebIDL: `extern "C"` declarations, and safe wrappers over them. An
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
   For a retained C callback, the registration function returns a `Stream`.
   The generated trampoline
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
`cc` crate, or links the libraries given to `--link`. For hxcpp, the build
reads system-library dependencies from Cargo's `linked_libs` and
`linked_paths` messages and writes them to `link.xml`. The generated glue
includes this file, and packages keep it beside the archive. A static archive
requires these separate link settings for its system-library dependencies. The
same process applies to imported libraries and other crates that link system
libraries.

The generated build XML lists `hxn.h` and the library's two glue headers as
dependencies of every compiled file. Header changes therefore recompile the
affected objects, compensating for hxcpp's timestamp-only rebuild check. For
`cpp --wasm`, hxn tells `cc` to compile with `emcc`. `hl --wasm` is not
supported, because compiling C for `wasm32-wasip1` needs the WASI SDK.

For a C++ library, provide a C API that specifies how its classes, overloads,
templates and exceptions are exposed. `hxn import` reads that C API; the
library author defines the C++ conversion.

The importer lists these unsupported declarations as not imported:

- Objects passed to callbacks remain owned by C, which may free them when
  the callback returns. A Haxe function could retain an object beyond that
  lifetime, so these arguments cannot use hxn's ownership-transfer protocol.
- A callback whose user data is neither first nor last, or that takes
  other untyped pointers, as `sqlite3_trace_v2`'s does: what those point to
  depends on the other arguments.
- Variadic functions and functions that use `va_list` are unsupported.
- Pointers whose meaning the header does not give, such as a `void *`
  with no length, or a buffer C keeps. A function that needs one is
  written in the library's own Rust beside the generated bindings.

## Website

`site/` is the static landing page and Quick Start, served by Cloudflare
Workers Static Assets. `wrangler.jsonc` points at that directory; there is no
build step or Worker script. Missing paths return 404. The Worker named
`hxn-site` serves `hxn.dev` through a Custom Domain route, with its
`workers.dev` address disabled. The page's canonical URL, social preview URLs,
structured data and sitemap all use `https://hxn.dev/`.

`site/og-hero.png` is a browser capture of the hero, framed at 1200 by
630 CSS pixels and saved at twice that resolution. Open Graph, Twitter
cards and the page's structured data use it as the preview image.

With Node.js 24 installed, these commands run from the repository root:

```
npx --yes wrangler@4.161.0 dev
npx --yes wrangler@4.161.0 deploy --dry-run --outdir target/site-deploy
npx --yes wrangler@4.161.0 login
npx --yes wrangler@4.161.0 deploy
```

Cloudflare Workers Builds deploys the site directly from GitHub. Connect
the repository under the Worker's Settings > Builds, using these settings:

| Setting | Value |
|---|---|
| Worker name | `hxn-site` |
| Production branch | `main` |
| Root directory | repository root (`/`) |
| Build command | empty |
| Deploy command | `npx --yes wrangler@4.161.0 deploy` |

The root directory contains `wrangler.jsonc`, which selects `site/` as the
assets directory. Cloudflare manages deployment credentials through its
Git integration. `.github/workflows/site.yml` only validates the static
deployment on pull requests and changes on `main`, or when run by hand;
it needs no Cloudflare credentials and does not publish the site.

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
