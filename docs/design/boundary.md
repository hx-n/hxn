# Boundary types and failures

[Design](../design.md) · [Conventions](../conventions.md)

## Values

| Value | Boundary type | Ownership |
|---|---|---|
| Borrowed text or bytes | `Slice` | Caller; text is UTF-8. |
| Writable bytes | `SliceMut` | Caller; writes affect Haxe `Bytes`. |
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
