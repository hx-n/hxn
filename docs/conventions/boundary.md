# C ABI and generated names

[Design](../design.md) · [Conventions](../conventions.md)

## C ABI

- Return `hxn_status`: zero succeeds, negative codes belong to hxn, positive
  codes to the library. Write final `out` only on success and `err` only on failure.
- `bool` is `int32_t`; text/bytes use by-value `hxn_slice`, with UTF-8 text.
  Returned `hxn_buf` is freed exactly once through its own `free`.
- Handles are opaque `hxn_handle *`, read only by their originating library.
- Callbacks are `{call, env, drop}`. `call` takes `env` first, returns status
  and writes a final `out`. It returns normally.
- Timeouts are milliseconds; negative means unlimited. Host-visible ids and
  versions are non-negative `int32_t`; `-1` means closed or absent.

## Names

In this table, `P` means `hxn_<library>_`; interfaces use snake case in exports.

| Item | Name |
|---|---|
| Method or free function | `P<interface>_<method>` or `P<class>_<function>` |
| Shared operations | `P<interface>_same`, `_weak`, `_weak_upgrade`, `_weak_close` |
| Release | `Prelease` |
| Outbox | `Poutbox_wait`, `Poutbox_next` |
| Async operations | `P<future/stream/signal>_wait`, `_state`, `_watch` |
| Typed reads | `Pfuture_take_<kind>`, `Pstream_next_<kind>`, `Psignal_get_<kind>` |
| Typed data error | `Pthrown_take`, generated only for data errors. |
| Memory accounting | `Pmemory_held`, `Pmemory_wanted` |
| HashLink primitive / resolver / abstract | `<library>_<interface>_<method>`, through `@:hlNative("<library>", "<library>_")` / `hlp_<library>_<interface>_<method>` / `<library>_<interface>` |
| Weak class / abstract | `<Interface>Weak` / `<library>_<interface>_weak` |
| Typed exception / enum serializer | `<Name>Exception` (strip trailing `Error`) / `<Name>Wire` |
| C / glue headers | `<library>_abi.h` / `<library>_glue.h`, namespace `<library>_glue` |
| hxcpp extern / support class | `Hxn<Library>Native` / `Hxn<Library><Role>` |
| Haxe method | Rust name in camel case. |

Every HashLink primitive, the library-wide ones included (`<library>_future_state`,
`<library>_outbox_wait`), starts with the library's name. A phone links every
library and the runtime into one program, where unprefixed resolvers collide.

Kinds name scalars or snake-case types. Wrappers add `option_`, `list_`,
`map_`, `set_`, `sorted_set_`, `tuple_<arity>_<element kinds>` or
`result_<value kind>_<error kind>`. Reject collisions.

Reserve `Atomic`, `Memory`, `Future`, `Stream`, `Signal`, `Outbox`, generated
exception/serializer names and `<Interface>Weak`. Shared interfaces reserve
`same` and `weak`. Keep `hxn_` on exports to avoid C library symbol collisions;
keep C header names distinct from generated Haxe headers on case-insensitive
filesystems.
