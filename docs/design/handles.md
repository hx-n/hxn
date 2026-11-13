# Handles and shared state

[Design](../design.md) · [Conventions](../conventions.md)

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

See also: [Rust API](../conventions/rust-api.md), [Threads and delivery](../conventions/threads.md).
