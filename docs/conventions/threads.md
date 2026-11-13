# Threads and delivery

[Design](../design.md) · [Conventions](../conventions.md)

- A library's own threads never call into a runtime or touch its heap.
- `hxn-hl`'s functions are only for the thread the runtime called in on.
- A reactive graph belongs to one thread.
- A library's results are delivered on one thread: the one that first
  requested delivery, or the one that calls `Loop.sync()`.
- Waiting from Haxe is done in bounded slices with a short sleep between, so
  runtimes whose other threads run only while one sleeps still make progress.
- Where two threads each write one atomic and then read the other's, every
  one of those accesses is `SeqCst`, or the two are made under one lock.
  With anything weaker both may read the old value.

See also: [Futures, streams and scheduling](../design/async.md), [Reactivity](../design/reactivity.md).
