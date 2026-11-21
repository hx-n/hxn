# Boundary types and failures

[Design](../design.md) · [Conventions](../conventions.md)

## Values

| Value | Boundary type | Ownership |
|---|---|---|
| Borrowed text or bytes | `Slice` | Caller; text is UTF-8. |
| Writable bytes | `SliceMut` | Caller; writes affect Haxe `Bytes`. |
| Typed buffer | Its bytes, as `Slice` or `SliceMut`, viewed as `Elements` or `ElementsMut` | Caller; writes affect the Haxe buffer. |
| Returned text or bytes | `Buf` | Host frees exactly once through its `free`. |
| Rust object | Opaque `Handle<T>` pointer | One Haxe owner per handle. |
| Borrowed host callback | `Callback` | Host, for the call. |
| Shared atomic cell | `i32` | Side that allocated it. |
| Plain data | Serialized bytes | Copied in both directions. |
| Failure | `Status` and message `Buf` | Adapter reports it after Rust returns. |

Numbers cross as `i32`, `i64` or `f64`. Adapters range-check conversions;
failures return `Status::RANGE`. `u32` becomes `UInt`; `u64` keeps its bits
in `haxe.Int64`, so values from 2^63 read as negative. On wasm32, pointers
and sizes are 32-bit; use `i64` or `u64` for quantities above 4 GiB.

Invalid UTF-8 or UTF-16 is rejected. Writable bytes must not alias another
bytes argument; adapters reject overlap with `Status::ALIASED` before
creating Rust references. Each `Buf` uses its originating allocator.

## Typed buffers

`hxn.Float32Buffer`, `Float64Buffer`, `Int32Buffer`, `UInt32Buffer`,
`Int16Buffer` and `UInt16Buffer` are abstracts over `haxe.io.Bytes`. Each one
holds a fixed number of elements in little-endian order. That is the native
order on every supported platform.

A library function that takes `&[f32]` or `&mut [f32]` borrows the bytes of
the buffer for the call, the same way that it borrows `Bytes`.
`hxn_abi::Elements` and `ElementsMut` give the bytes to the function as
elements. If the bytes are aligned for the element type, the function reads
and writes them in place. If they are not aligned, the function works on a
copy, and `ElementsMut` writes the copy back when the call returns. The
collectors of HashLink and hxcpp do not move memory during a call, so the
adapter does not pin the buffer.

On HashLink and Ash, indexing reads `hl.Bytes` directly. On hxcpp, it uses
the raw memory functions of hxcpp. Indexing checks bounds only when you
compile with `-debug`. The length of a buffer does not change: `resized`
returns a new buffer. An async function copies the buffer before the call
returns. It cannot take a buffer to write into.

## Failures

| Rust failure | Haxe behavior |
|---|---|
| `Result::Err` with a `Display` error | Throws `haxe.Exception`. |
| Named `#[hxn::data]` error | Throws `<Name>Exception`, dropping a trailing `Error`; `.error` holds the data. |
| Failed future | Throws when its result is taken. |
| `Stream<Result<T, E>>` | Delivers earlier values, then throws or calls `onError`; later values are dropped and sends refused. |
| Panic | Becomes `Status::PANIC` on every runtime. |

Typed error bytes travel with failures and enter `hxn_abi::thrown` when taken.
Each failure replaces that thread-local payload; Haxe reads it immediately.
`Sender::fail` retains the channel failure's status, message and payload.

Runtime exceptions are raised after Rust locals have been dropped. Fallible
HashLink primitives use `extern "C-unwind"` for Ash's wasm exceptions.
Wasm builds unwind with wasm exceptions; `hxn-hl` supplies the missing WASI
unwind shims. See [C ABI rules](../conventions/boundary.md).
