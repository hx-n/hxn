# Runtime bindings

[Design](../design.md) · [Conventions](../conventions.md)

- Create one `hxn.Inbox` per library and share it across the binding's
  futures, streams and signals.
- Wrap a primitive in a lambda where a function value is needed. Pass
  `raw -> Native.take(raw)`, not `Native.take`.
- Wrap a callback passed to native code so that it catches exceptions,
  calls `hxn.Callback.failed` and returns the failure value, and make the
  native call inside `hxn.Callback.during`. The failure value is null, or
  false for a callback that returns nothing.
- Store a function used by later calls in a field of the Haxe object and
  pass it to each call. Native code does not root host functions.
- When a binding sets a Rust value, also update its followed Haxe signal.
- Every class that owns a handle has `close()` and a finalizer. The
  finalizer only frees: it runs during a collection.
- Share one `hxn.Gc.Meter` across all binding classes so that every program
  using the library creates the meter. A weighed class checks it
  once the new object owns its value and at the start of each of its methods,
  never before the object owns its value and never in a finalizer.
- A binding collects through `hxn.Gc`, not through a runtime's own call.
- Two objects are compared with `same`, never with `==`, when their
  interface is shared.
- A return value is wrapped in a new object of its interface's class,
  which becomes its owner only once the library has transferred the value.
- A cell the library returns has no `close()`. The object holding it
  keeps the memory alive, so its address is valid for as long as it is used.
- A null object is passed to native code as a null handle, even when the
  parameter is not an `Option`. The adapter then rejects it with the same
  error on every runtime. The binding must not read the object's handle
  field itself: hxcpp does not check for null before a field access.
- On hxcpp, a raw pointer field is `@:unreflective`, and a call that waits is
  made inside `hxn::Blocking`. Failures are thrown by `hxn::check` after the
  library has returned.
- On HashLink, put a fallible primitive's entire body inside one `prim` call,
  so a throw leaves no Rust value behind, and a call that waits is made
  inside `blocking`.

See also: [Binding generation](../design/generation.md), [Garbage collection and native memory](../design/memory.md).
