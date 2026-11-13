# Reactivity

[Design](../design.md) · [Conventions](../conventions.md)

`hxn.Signal`, `hxn.Computed` and `hxn.Reactive` provide fine-grained
reactivity in the style of SolidJS. Dependencies are tracked at run time, so
they follow the branches the code takes.

The dependency graph is implemented in Haxe, so it behaves identically on
every runtime. It marks on write and recomputes lazily on read: a write marks
direct dependents dirty and their transitive dependents as needing a check,
and queues effects; an effect that runs, or a computed value that is read,
first updates its own sources and recomputes only if one changed. A value is
computed once however many paths lead to it, and never from a mix of old and
new values.

`Reactive.follow` creates a signal that tracks a value held in Rust. Three
mechanisms keep the two sides consistent:

- a delivery is one batch, so effects rerun once however many values arrived;
- a binding updates the followed Haxe signal when it sets the Rust value,
  making the change immediately visible to Haxe code;
- `Inbox.follow` accepts a sink function, allowing an app to update signals
  in its own reactive system.

See also: [Futures, streams and scheduling](async.md), [Threads and delivery](../conventions/threads.md).
