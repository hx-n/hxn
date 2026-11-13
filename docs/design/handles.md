# Handles and shared state

[Design](../design.md) · [Conventions](../conventions.md)

`Handle<T>` drops its value at `close()` and frees its allocation at
finalization. Access to closed, foreign or wrongly typed handles fails.
Conflicting runtime borrows return `Busy`, including re-entry from callbacks.
Values must be `Send` because finalizers may run on another thread.

## Ownership

| Rust position | Effect |
|---|---|
| `&T`, `&mut T` parameter | Shared or exclusive borrow for the call. |
| `T` parameter or `self` receiver | Moves the value; closes the Haxe object even if the call fails. |
| `T` result | Creates a new Haxe owner. |
| `Option<T>` | Allows `null`. |
| `Vec<T>` result | Produces an array of owned Haxe objects. |

Borrowed returns are unsupported. A child needing its parent keeps a shared
reference. Object lists transfer through a completed stream that Haxe drains
before returning; untaken values remain owned by Rust.

Host objects stay in Haxe fields or tables; Rust may retain their ids.
Native GC roots would hide reference cycles from the collector.

## Shared interfaces

`#[hxn::interface(shared)]` stores one `Arc` per handle and requires
`Send + Sync`. Methods take `&self`; mutation uses interior mutability.

| Operation | Effect |
|---|---|
| Accept `&T` | Borrows for the call. |
| Accept `Arc<T>` | Lets Rust retain a reference. |
| Return `T` or `Arc<T>` | Creates a Haxe object owning one reference. |
| `same(other)` | Compares underlying identity. |
| `close()` or finalization | Releases that object's reference. |
| `weak()` / `upgrade()` | Produces `<Interface>Weak` / a new owner, or null after destruction. |

The value is dropped after the last reference, including references held by
Rust. Use weak references for parent links that would otherwise form cycles.

## Atomics

| Rust | Haxe | Lifetime |
|---|---|---|
| `&AtomicI32`, `&AtomicFlag` | `hxn.AtomicInt`, `hxn.AtomicBool` | Borrowed for the call. |
| `Arc<AtomicI32>`, `Arc<AtomicFlag>` | The same classes over Rust memory | Kept alive by the Haxe object; no `close()`. |

Both sides operate on the same four-byte cell. Flags use integer 0/1;
Rust's one-byte `AtomicBool` is incompatible. See [thread rules](../conventions/threads.md).
