# Reactivity

[Design](../design.md) · [Conventions](../conventions.md)

`hxn.Signal`, `Computed` and `Reactive` track dependencies dynamically in
Haxe, identically on every runtime.

- Writes mark dependents dirty and queue effects.
- Reads update sources and recompute only changed dependencies.
- Each computed value runs once per update, using consistent source values.

`Reactive.follow` tracks a Rust signal. Delivery batches updates before
running effects; binding setters also update the Haxe signal immediately.
`Inbox.follow` accepts a sink for integration with another reactive system.

See [delivery](async.md#delivery) and [thread rules](../conventions/threads.md).
