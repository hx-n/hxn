# Data and serialization

[Design](../design.md) · [Conventions](../conventions.md)

`#[hxn::data]` values are copied across the boundary and need no `close()`.
Use records for small structured values, bytes for large transfers and
[handles](handles.md) for identity.

| Rust | Haxe |
|---|---|
| Struct with public fields | Record class, constructed with named fields. |
| C-like enum | `enum abstract` over `Int`; unknown discriminants fail. |
| Tagged enum | `enum`, with a separate `<Name>Wire` serializer. |
| `Option<T>`, `Vec<T>`, `Box<T>` | `Null<T>`, `Array<T>`, `T`. |
| Nonempty tuple | `hxn.Tuple<A, B, ...>`. |
| `HashMap<K, V>`, `BTreeMap<K, V>` | `Map<K, V>`; keys are strings, integers up to 32 bits or C-like enums. |
| `HashSet<T>`, `BTreeSet<T>` | `hxn.Set<T>`; a set of bytes is not `Bytes`. |

## Haxe collections

- Construct tuples with `Tuple.of(...)`, `new hxn.Tuple<A, B>(...)` or
  numbered record literals. `_0`, `_1`, ... are typed properties.
  `tuple[index]` returns `Dynamic` with runtime bounds checks; `at` and
  `set` require literal positions and check types at compile time.
  Every position is required; null requires `Option`. Length is fixed.
  `copy` is shallow; `toArray` returns a separate array.
- Sets accept array literals and provide `add`, `has`, `remove`, `clear`,
  `length`, iteration, `copy` and `toArray`. Construction copies the array.
  Membership is linear and uses `==` unless given an equality function.
  Rust applies `Eq + Hash` or `Ord` again. Returned `BTreeSet` values are
  sorted; later Haxe additions preserve insertion order. `HashSet` order
  is unspecified.

## Encoding

| Data | Encoding |
|---|---|
| Record or tuple | Fields in declaration order; a tuple has no count or tag. |
| Empty record | One zero byte. |
| Tagged enum | Variant index followed by fields. |
| `Option` | Presence byte, then value. |
| List, map or set | Count, then elements or key/value entries. |
| C-like enum | Declared integer discriminant. |

Both adapters use the same encoding and generated Haxe data classes, including
CPPIA. Optional record fields may be omitted; null required fields are rejected
before encoding. Readers reject truncation, invalid counts, invalid UTF-8
and trailing bytes. Recursive `Vec`/`Box` data is bounded by `wire::Reader::DEPTH`.

## Field defaults

`#[hxn(default)]` uses `Default::default()`;
`#[hxn(default = "Item::default_unit")]` calls a public zero-argument function
relative to the crate root, returning the field type. Omitted or null fields
request the default; false, zero and empty values override it. Input records stay unchanged and
returned records contain materialized values.

Defaults apply only to non-nullable record fields. `Option` already defaults
to `None`; tagged enum fields remain required. A default adds a presence byte
and a manifest entry: regenerate both sides together. See [Rust API rules](../conventions/rust-api.md).
