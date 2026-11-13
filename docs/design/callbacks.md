# Haxe callbacks

[Design](../design.md) · [Conventions](../conventions.md)

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

See also: [Rust API](../conventions/rust-api.md), [Runtime bindings](../conventions/bindings.md).
