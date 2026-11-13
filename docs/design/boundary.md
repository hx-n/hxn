# Boundary types and failures

[Design](../design.md) · [Conventions](../conventions.md)

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

See also: [C ABI and generated names](../conventions/boundary.md).
