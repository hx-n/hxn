# C ABI and generated names

[Design](../design.md) · [Conventions](../conventions.md)

## The C boundary

- An exported function returns an `hxn_status`. Zero is success, negative
  values are hxn's, positive values are the library's.
- A result is written through a final `out` parameter, and an error message
  through `err`. `out` is not written on failure, and `err` is not written on
  success.
- A `bool` is an `int32_t` at the boundary.
- Pass text and bytes as an `hxn_slice` by value. Encode text as UTF-8.
- Returned text and bytes are an `hxn_buf`, freed by the host through the
  buffer's own `free`, exactly once.
- A handle is an opaque `hxn_handle *`. Only the library that created it reads it.
- A callback is `{call, env, drop}`. `call` takes `env` as its first
  parameter, returns an `hxn_status` and writes its result through a final
  `out` parameter. It must return normally.
- Waits take a timeout in milliseconds; a negative value means no timeout.
- Versions and ids the host sees are non-negative `int32_t`; `-1` means
  "closed" or "nothing".

## Names

| Generated item | Name |
|---|---|
| exported function | `hxn_<library>_<interface>_<method>`, the interface in snake case. The `hxn_` prefix keeps an export from taking the name of a C function the library links: a library `sqlite3` with an interface `Value` would otherwise export `sqlite3_value_subtype`, which is sqlite3's own |
| HashLink primitive resolver | `hlp_<interface>_<method>` |
| HashLink abstract | `<library>_<interface>` |
| a shared interface's built-in functions | `hxn_<library>_<interface>_same`, `_weak`, `_weak_upgrade`, `_weak_close` |
| weak reference to a shared interface | Haxe class `<Interface>Weak`, HashLink abstract `<library>_<interface>_weak` |
| release function for any handle | `hxn_<library>_release` |
| the library's queue of ready results | `hxn_<library>_outbox_wait`, `hxn_<library>_outbox_next` |
| waiting on and watching a future, stream or signal | `hxn_<library>_future_wait`, `_future_state`, `_future_watch`; the same for `stream` and `signal` |
| taking the value one carries | `hxn_<library>_future_take_<kind>`, `hxn_<library>_stream_next_<kind>`, `hxn_<library>_signal_get_<kind>`; the kind is `unit`, `bool`, `int`, `long`, `float`, `text`, `bytes`, the name of a narrower number type, or the snake-case name of an interface or data type, prefixed with `option_`, `list_`, `map_`, `set_` or `sorted_set_` for those wrappers, and with `tuple_<arity>_` followed by each element kind for a tuple, and `result_` followed by the value kind and error kind for a fallible stream. Two carried types that map to the same name are rejected |
| fetching the typed error of the last failure | `hxn_<library>_thrown_take`, generated only when a function has a `#[hxn::data]` error type |
| exception class for a typed error | Haxe class `<Name>Exception`, with a trailing `Error` removed from the name first |
| serializer for a tagged enum | Haxe class `<Name>Wire` |
| native memory held by a library's values, in bytes | `hxn_<library>_memory_held` |
| whether the library requested a collection | `hxn_<library>_memory_wanted` |
| C header | `<library>_abi.h` |
| hxcpp glue header | `<library>_glue.h`, namespace `<library>_glue` |
| hxcpp extern class declaring the glue | `Hxn<Library>Native`, each function under its glue name |
| Haxe method | the Rust name in camel case |
| a function outside an `impl` | `<library>_<class>_<function>`, named as a method of that class would be |
| Haxe class a binding needs in addition to the library's own | `Hxn<Library><Role>`: `HxnCounterMemory` |

An interface is not named `Atomic`, `Memory`, `Future`, `Stream`, `Signal` or
`Outbox`: the adapters use those names for their own exports. No interface is named `<Interface>Weak` when a shared
`<Interface>` exists, and a shared interface has no function named `same` or `weak`.

Give C headers names that remain distinct from generated Haxe headers on
case-insensitive filesystems. hxcpp generates `Counter.h` for a class
`Counter`, and on a case-insensitive filesystem `#include "counter.h"` would
then find the generated header.

See also: [Boundary types and failures](../design/boundary.md).
