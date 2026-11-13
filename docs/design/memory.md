# Garbage collection and native memory

[Design](../design.md) · [Conventions](../conventions.md)

Collectors see a handle's small wrapper, not the Rust memory behind it.
hxn accounts for that memory per library.

| API | Purpose |
|---|---|
| `hxn::Weight` | Counts native bytes while its value lives; update with `set` when the size changes. |
| `#[hxn::interface(weighed)]` | Checks the library's shared `hxn.Gc.Meter` after construction and before methods. |
| `hxn.Gc.budget` | Sets the collection threshold. |
| `hxn::collect_soon()` | Requests collection at the next meter check, from any thread. |
| `hxn.Gc.collect()` | Forces collection; returns false where unsupported. |

Delivery also checks the meter. After collection, the next threshold is twice
the remaining native memory, avoiding repeated collections for live values.
Finalizers only free; they never trigger collection. hxn uses its own policy
because hxcpp cannot safely account for native releases inside finalizers.

End programs with `hxn.Loop.exit`. It avoids an hxcpp 4.3.2 shutdown crash
after collection; newer hxcpp versions are pinned by CI.

See [binding rules](../conventions/bindings.md).
