# Haxe callbacks

[Design](../design.md) · [Conventions](../conventions.md)

Haxe callbacks run on the calling runtime thread, during the native call.
Rust accepts `impl Fn(..) -> Result<T, E>`, `FnMut` or `FnOnce`, with the
callback error as a function type parameter.

1. The Haxe wrapper catches exceptions, calls `hxn.Callback.failed` and
   returns a failure value.
2. Rust propagates the failure and returns normally.
3. `hxn.Callback.during` rethrows the original Haxe exception.

| Callback value | Transfer |
|---|---|
| Numbers and bools | By value; `i64` uses eight serialized bytes to preserve its bits. |
| `&str`, `&[u8]`, data arguments | Copied during the call; data may also be passed by reference or slice. |
| `String`, `Vec<u8>`, data results | Copied back; temporary buffers use their own `free`. |
| `T` or shared `Arc<T>` argument | Ownership passes to Haxe before invoking the callback. |

Borrowed objects are rejected because Haxe could retain them. Text and byte
results cannot be null: null reports failure. For a void callback, false
reports failure.

Store callbacks needed across calls in Haxe fields and pass them on each call.
The collector can then trace and collect cycles. Native code never roots or
retains a host function. See [Rust API rules](../conventions/rust-api.md).
