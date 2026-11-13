# Garbage collection and native memory

[Design](../design.md) · [Conventions](../conventions.md)

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

See also: [Runtime bindings](../conventions/bindings.md).
