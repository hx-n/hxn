# Rust API

[Design](../design.md) · [Conventions](../conventions.md)

## Interfaces and functions

- Interfaces and data are public, reachable at the crate root and uniquely
  named across the library. `#[hxn::methods]` uses the bare interface name.
- Receivers are `&self`, `&mut self`, `self` or absent; `new` is the constructor.
  Moving `self` closes the Haxe object. Exposed APIs have no general generics.
- `#[hxn::function]` becomes a static method on the library's CamelCase class
  or `class = "Name"`, including an existing interface. It is public and
  reachable at the crate root. Its name is unique
  within that class and cannot be `new`.
- No interface, data type or class is named like a type of Haxe's top-level
  package (`Math`, `String`, `Map`, `Date`, ...), which the generated code
  refers to. A library named `math` puts its functions on `class = "Name"`.
- `hxn::package!("name")` puts the generated Haxe classes in the package
  `name`. Declare it once in the crate. Each dot-separated part of the name
  starts with a lowercase letter or an underscore, and the package cannot
  be `hxn`.
- The build script can write part of the API. The library includes the
  file it writes with `include!(concat!(env!("OUT_DIR"), "/name.rs"))`, and
  `hxn build` runs the build script before it reads the library's source.

## Values

| Rust | Haxe / restriction |
|---|---|
| `bool` | `Bool`. |
| `&str` / `String` | Text input / output. |
| `&[u8]` / `Vec<u8>` | Bytes input / output. |
| `&mut [u8]` | Writable `Bytes`; synchronous calls only, not callback parameters. |
| `i8`, `i16`, `u8`, `u16`, `i32`, `isize`, `usize` | `Int`, range-checked. |
| `u32` | `UInt`, same bits. |
| `i64`, `u64` | `haxe.Int64`, same bits. |
| `f32`, `f64` | `Float`, range-checked. |

Use 64-bit quantities above 4 GiB; wasm32 `usize` is 32-bit. Numbers wider
than 64 bits are unsupported.

Data structs have named public fields; enums are C-like with discriminants
or tagged without explicit discriminants. Fields may contain bools, fixed-width
numbers, text, bytes, data, nonempty tuples and their supported containers.
Pass data by value or `&`, lists also by `&[T]`; return it by value.
See [data types, collections and defaults](../design/data.md) for the contract.
Data in signals must be `Clone`; `()` is not a data field.

## Ownership

- Accept ordinary interfaces as `&T`, `&mut T` or moved `T`; return owned `T`.
  A moved parameter closes its Haxe object even on failure. `Option` allows
  absence; object lists return as Haxe arrays. No borrowed returns.
- Shared interfaces require `Send + Sync` and `&self` methods. Accept `&T`
  or `Arc<T>`; return `T`, `Arc<T>` or `Vec<Arc<T>>`. Neither `&Arc<T>` nor
  `&mut T` is accepted. Ordinary interfaces cannot cross as `Arc`.
- Shared cells use borrowed `AtomicI32`/`hxn::AtomicFlag` or returned `Arc`s.
  Rust's `AtomicBool` is incompatible.

## Asynchronous results and callbacks

- Return `hxn_future::Future<T>`, `Stream<T>` or `Signal<T>` immediately.
  See [carried values and scheduling](../design/async.md).
- `async fn` receivers are moved `self` or shared `&self`. Parameters are
  scalars, copied text/bytes or shared interfaces. The future is `Send`;
  no call-only borrows, `new` or `#[hxn(blocking)]`.
- Callbacks use `impl Fn`, `FnMut` or `FnOnce` returning `Result<T, E>`;
  `E` is a function type parameter used only by that callback. Parameters
  are bools, `i32`, `i64`, `f64`, borrowed text/bytes, data or owned objects.
  Results are the same scalar types, owned text/bytes, data or `()`.
  Propagate callback failures; blocking functions cannot accept callbacks.

## Errors and execution

- Use `Result<T, E>` with `E: Display`. Name data error types directly, not
  through aliases; see [typed exceptions](../design/boundary.md#failures).
- Typed stream errors use `Stream<Result<T, E>>` with a named data error
  implementing `Display`; only streams carry `Result` items. The first error terminates delivery after earlier values; `listen`
  calls `onError`, not `onEnd`. Plain streams may use `Sender::fail`.
- Mark waits `#[hxn(blocking)]`. Use `spawn`/`stepped` instead of blocking or
  starting threads in portable libraries. `stepped`'s `Poll::Pending` means
  another computation step is ready.
- Release locks before running or dropping library values. Queue results
  for delivery on the host thread.
- Track large allocations with `hxn::Weight` and `#[hxn::interface(weighed)]`.
  Update the weight as size changes; `collect_soon()` requests a later check.
