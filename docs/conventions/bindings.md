# Runtime bindings

[Design](../design.md) · [Conventions](../conventions.md)

- Share one `hxn.Inbox` and one `hxn.Gc.Meter` per library.
- Wrap a returned handle in its class: `Thing.hxnAdopt(raw)`.
- Catch callback exceptions with `hxn.Callback.failed`; return null, or
  false for void callbacks. Invoke native code inside `Callback.during`.
- Store retained functions in Haxe fields and pass them on each call.
- Binding setters also update followed Haxe signals.
- Handle owners provide `close()` and a finalizer that only frees. Wrap
  return values only after Rust transfers ownership. Compare shared values
  with `same`, not `==`.
- Weighed objects check the meter after taking ownership and before methods;
  never collect in finalizers or before ownership. Collect through `hxn.Gc`.
- Returned atomic cells have no `close()`; their backing owner keeps memory alive.
- Pass null objects as null handles, including non-`Option` parameters.
  Let the adapter reject them; reading a null object's handle crashes on hxcpp.

| Runtime | Required wrapper |
|---|---|
| hxcpp | Pointer fields are `@:unreflective`; waits use `hxn::Blocking`; `hxn::check` throws after Rust returns. |
| HashLink | A fallible primitive's whole body is inside `prim`; waits use `blocking`. |

See [generation](../design/generation.md) and [memory accounting](../design/memory.md).
