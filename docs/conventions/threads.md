# Threads and delivery

[Design](../design.md) · [Conventions](../conventions.md)

- A library's own threads never call into a runtime or touch its heap.
- `hxn-hl`'s functions are only for the thread the runtime called in on.
- A reactive graph belongs to one thread.
- A library's results are delivered on one thread: the one that first
  requested delivery, or the one that calls `Loop.sync()`.
- Static adapters can share an outbox. Forward consumed notifications to other
  interested inboxes and schedule their checks on their own delivery thread.
  Watchers check readiness before taking a value; ids from independent queues
  can overlap. Never discard another binding's notification while draining.
- Device render callbacks use preallocated native data. Do not allocate,
  free the last resource reference, take locks, log or call Haxe there.
  Send bounded commands and retire resources on the control side. A browser
  AudioWorklet keeps rendering independent of the Haxe event loop,
  including when hxcpp is built without threads.
- Device libraries expose suspend/resume and pollable device failures.
  Check mobile interruptions and browser playback activation as well as
  desktop output; compiling an adapter does not verify its device lifecycle.
- Waiting from Haxe is done in bounded slices with a short sleep between, so
  runtimes whose other threads run only while one sleeps still make progress.
- Where two threads each write one atomic and then read the other's, every
  one of those accesses is `SeqCst`, or the two are made under one lock.
  With anything weaker both may read the old value.

See also: [Futures, streams and scheduling](../design/async.md), [Reactivity](../design/reactivity.md).
