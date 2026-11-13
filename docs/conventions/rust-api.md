# Rust API

[Design](../design.md) · [Conventions](../conventions.md)

## Interfaces and functions

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
## Primitive values

- Text is accepted as `&str` and returned as `String`; bytes are accepted as
  `&[u8]` and returned as `Vec<u8>`. Bytes the function writes into are
  `&mut [u8]`, and the Haxe `Bytes` passed for them sees the writes. An
  `async fn` cannot take them, and they are not a lent function's
  parameter.
- A number is `i32`, `i64` or `f64`, or one the adapter converts: `i8`,
  `i16`, `u8`, `u16`, `isize` and `usize` are passed as a Haxe `Int`, `u32` as
  a `UInt` with the same bits, `u64` as a `haxe.Int64` with the same bits,
  `f32` as a `Float`. A value that does not fit, as a parameter or as a return
  value, fails the call. No type wider than 64 bits is passed.
- Use `u64` or `i64` for quantities that can exceed 4 GiB, including file
  lengths, offsets and nanosecond counts. These types remain 64 bits on
  wasm32. `usize` is 32 bits there and can overflow even when the native
  build handles the same value.
## Data and collections

- Plain data is a struct or an enum marked `#[hxn::data]`, `pub` and
  reachable at the crate's root, with a name that no interface uses. A struct has
  named fields, all `pub`, each a `bool`, a fixed-width number,
  `String`, `Vec<u8>`, other data, a nonempty tuple, or an `Option`, `Vec`, `Box`, `HashMap`,
  `BTreeMap`, `HashSet` or `BTreeSet` of these. Map keys are `String`, an
  integer of at most 32 bits, or a C-like enum. An enum is either C-like
  (no variant has fields; explicit discriminants are allowed) or tagged
  (some variants have fields, named or unnamed; no explicit discriminants).
  Data is accepted by value or by `&`, a list also as `&[T]`, and returned
  by value. `Option`, `Vec`, maps and sets of data are accepted wherever data is.
- A nonempty Rust tuple `(A, B, ...)` is `hxn.Tuple<A, B, ...>`, constructed
  with `hxn.Tuple.of(...)`, `new hxn.Tuple<A, B, ...>(...)`, or a numbered
  record literal. It is an array-backed abstract with fixed `length`, `copy`
  and `toArray`. `tuple[index]` reads or writes `Dynamic` and checks bounds
  at runtime. `tuple.at(1)` and `tuple.set(1, value)` preserve the position's
  static type and check literal positions at compile time. Typed `_0`, `_1`,
  ... properties remain available. Each position is required, including `_0`
  for `(T,)`; a value can be null only when that position is an `Option`.
  Elements are encoded and decoded in position order. Tuples are data
  wherever data is accepted, but `()` is not a data field.
- A `HashSet<T>` or `BTreeSet<T>` is an `hxn.Set<T>` in Haxe. The wrapper
  accepts array literals and provides membership, insertion, removal and
  iteration. Haxe membership uses `==` unless constructed with an equality
  function; Rust applies its own `Eq + Hash` or `Ord` on every call. A
  returned `BTreeSet` is in Rust's `Ord` order; a `HashSet` has no promised
  order. Adding elements in Haxe does not sort the set. Sets are data
  everywhere data is accepted, and a set of `u8` is `hxn.Set<Int>`, not `Bytes`.
- A non-nullable record field can use `#[hxn(default)]`, requiring its Rust
  type to implement `Default`, or `#[hxn(default = "Item::default_unit")]`.
  The custom function is public, takes no arguments, returns the field's
  type, and its path is relative to the library root. Haxe may omit the field
  or set it to null to request its Rust default; zero, false and empty values
  override the default. The Haxe input stays unchanged, and Rust returns
  materialized fields. A default adds a presence byte to that field's wire
  encoding and is recorded in the manifest; regenerate both sides together.
  `Option` fields already default to `None` and do not accept this annotation.
  Tagged enum fields do not have defaults.
- An empty data record occupies one zero byte, including inside a collection.
  This lets the reader bound every collection count by the available bytes.
- Recursive data goes through a `Vec` or a `Box` and may nest at most
  `hxn_abi::wire::Reader::DEPTH` levels. Use a `Vec` for a long sequence,
  not a chain of boxes.
- Data carried by a signal must be `Clone`.
## Object ownership

- Another interface is accepted as `&T` or `&mut T` and returned as `T`, with
  `Option` for one that may be absent and `Vec` for a list, which Haxe gets
  as an `Array`. Accepted as `T`, it is moved out of its handle and the Haxe
  object passed is closed, whether or not the call succeeds. It is never
  returned by reference.
- A value with more than one owner is an interface marked
  `#[hxn::interface(shared)]`. Its functions take `&self`, never `&mut self`
  or `self`, and it is `Send + Sync`. It is accepted as `&T` to use it and as
  `Arc<T>` to keep a reference, never as `&Arc<T>` or `&mut T`, and returned
  as `T` or `Arc<T>`; a list of them is `Vec<Arc<T>>`. `Arc` of an interface
  not marked shared is not passed across the boundary.
## Asynchronous results

- An asynchronous result is returned as a `Future<T>`, a `Stream<T>` or a
  `Signal<T>` of `hxn-future`, by a function that itself returns immediately.
  It carries `bool`, a number, `String` or data; a future or a stream may
  also carry `Vec<u8>`, an interface, a shared one as `Arc<T>`, an `Option`
  or `Vec` of either, or `()`. A list is taken as a Haxe `Array` of owned
  objects; a signal never carries objects.
- A typed stream failure is `Stream<Result<T, E>>`, where `T` is a supported
  stream value and `E` names a `#[hxn::data]` type implementing `Display`.
  Haxe sees `hxn.Stream<T>`. Its first `Err` is terminal: earlier values are
  delivered, later queued values are dropped and further sends are refused.
  Iteration throws the generated exception; `listen` sends it to `onError`
  instead of `onEnd`. A plain `Stream<T>` still uses `Sender::fail` for an
  untyped channel failure. Only streams carry `Result` items; a fallible
  future is written as an `async fn` returning `Result<T, E>`.
- An `async fn` is exposed as a function returning a future of its return
  value. Its receiver is `self` or, for a shared interface, `&self`; its
  parameters are numbers, `bool`, `&str` and `&[u8]`, which are copied, and
  shared interfaces. It does not accept anything borrowed only for the call,
  and it is not `new` or `#[hxn(blocking)]`. Its future must be `Send`.
## Callbacks

- A Haxe callback is accepted as `impl Fn(..) -> Result<T, E>`, or `FnMut` or
  `FnOnce`, with `E` a type parameter of the function used only by that
  callback.
  Its parameters are `bool`, `i32`, `i64`, `f64`, `&str`, `&[u8]`, data (by value
  or by `&`), or an object whose ownership passes to Haxe (`T`, or `Arc<T>`
  of a shared interface). It returns `bool`, `i32`, `i64`, `f64`, `String`,
  `Vec<u8>`, data or `()`. When the callback fails, the library returns. A function marked `#[hxn(blocking)]` does not accept a callback.
## Shared cells and collection

- A cell shared with Haxe is an `AtomicI32` or an `hxn::AtomicFlag`, never
  an `AtomicBool`. It is accepted as a reference, borrowed for the call, and
  returned as an `Arc`, owned by the library.
- A value that holds a large amount of memory keeps an `hxn::Weight` recording
  the amount, and its interface is marked `#[hxn::interface(weighed)]`. The
  weight is updated when the size changes. Without the mark the weight is
  counted, but creating the object does not check the meter.
- Request collection with `hxn::collect_soon()`. The binding handles the
  request at its next check; the request returns before collection occurs.
## Errors

- Report expected failures with `Result<T, E>`, where `E` implements
  `Display`.
- For errors that Haxe code needs to distinguish, use a `#[hxn::data]` type
  as the error: an enum with one variant per cause, or a record. Name it
  directly in the `Result`, not through a type alias. Haxe catches it as
  `<Name>Exception`; a trailing `Error` is dropped first, so `PantryError`
  gives `PantryException`. Do not use that class name, or `<Name>Wire` for
  a tagged enum, for anything else.
## Scheduling and delivery

- Mark a function that waits with `#[hxn(blocking)]`.
- Return `Poll::Pending` from `stepped` when another computation step can
  run immediately. Use `spawn` with `async` for work that waits.
- Do not start threads or block in a library that should run on wasm. Use
  `spawn` or `stepped`.
- Release hxn's locks before running library code. Move or swap values out
  under the lock, then drop them after releasing it.
- Queue results or update stored values for the host to retrieve. Delivery
  handlers run on the host's thread.

See also: [Data and serialization](../design/data.md), [Handles and shared state](../design/handles.md), [Haxe callbacks](../design/callbacks.md), [Futures, streams and scheduling](../design/async.md).
