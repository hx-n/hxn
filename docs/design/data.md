# Data and serialization

[Design](../design.md) · [Conventions](../conventions.md)

An interface crosses as a handle: one value, held by reference, with an owner.
Plain data crosses by value. A library marks it with `#[hxn::data]`:

| Rust | Haxe |
|---|---|
| a struct of public fields | a class with the same fields |
| a C-like enum (no variant has fields) | an `enum abstract` over `Int` with the same values |
| a tagged enum (some variants have fields) | an `enum` with the same constructors |
| `Option<T>`, `Vec<T>` | `Null<T>`, `Array<T>` |
| a nonempty tuple `(A, B, ...)` | `hxn.Tuple<A, B, ...>`, with typed `_0`, `_1`, ... fields |
| `HashMap<K, V>`, `BTreeMap<K, V>` | `Map<K, V>`; keys are `String`, `Int` or a C-like enum |
| `HashSet<T>`, `BTreeSet<T>` | `hxn.Set<T>`; a `BTreeSet` returns Rust's `Ord` order |
| `Box<T>` | `T` |

- The adapter converts a C-like enum between variant names and declared
  integer values with a `match` in each direction. An unknown integer fails
  the call. This conversion is independent of Rust's enum layout.
- The generated serializers write records, tagged enums, `Option`, `Vec`
  and maps to bytes in declaration order: a
  tagged enum as its variant index followed by that variant's fields, an
  `Option` as a presence byte followed by the value, a list or a map as a
  count followed by the entries. A set uses a list's count and elements. An
  empty record occupies one zero byte, so the count remains bounded by the
  bytes available. The Rust and Haxe serializers use the same declaration;
  the encoding contains field values and structural markers, with no type
  information. Each runtime's byte-buffer support transports composite and
  nested data.
- `Vec` and `Box` support recursive data such as trees. Generated readers
  track nesting depth and fail with `Status::DEPTH` beyond
  `wire::Reader::DEPTH`.
- A nonempty Rust tuple becomes `hxn.Tuple<A, B, ...>`, a Haxe abstract over
  an array. Construct one with `hxn.Tuple.of("name", 42)` or
  `new hxn.Tuple<String, Int>("name", 42)`; numbered record literals are also
  accepted. `pair[index]` reads and writes with runtime bounds checks. Since
  positions can have different types, indexing returns `Dynamic`. For a
  statically typed position, `pair.at(1)` and `pair.set(1, value)` require an
  integer literal and check its bounds and type during compilation. Typed
  `_0`, `_1`, ... properties remain available. Positions are mutable, but
  their declared types and the tuple's `length` are fixed. `copy` makes a
  shallow copy; `toArray` returns a separate `Array<Dynamic>` for inspection.
  A generic-build macro defines the typed properties for each arity. The
  common indexing abstract and each arity's abstract erase to the same array,
  including in a CPPIA host and its scripts, with no wrapper allocation.
  Elements use the data encoding in position order, with no count or tag.
  The reader reads each element before constructing the tuple, so evaluation
  order cannot reorder the input. Tuples can nest anywhere data is accepted;
  `()` denotes a return value with no data; tuple data requires at least one
  element.
- For `Map<Mode, T>` with a C-like enum key, Haxe retains the enum type and
  uses integer key storage. The wire contains each
  key's declared discriminant, checked by Rust's enum reader, followed by
  the value. Tagged enums are not map keys.
- `hxn.Set<T>` provides `add`, `has`, `remove`,
  `clear`, `length`, iteration, `copy` and `toArray`. Array literals convert
  to sets, and construction and `toArray` copy the array. Its array storage
  preserves insertion order and membership checks are linear. Membership
  uses Haxe `==` by default, including object identity; an optional equality
  function compares data values differently. Rust applies its own `Eq + Hash`
  or `Ord` when reading elements, so it may remove more duplicates. A
  `BTreeSet` writes elements in Rust's `Ord` order, and a `HashSet` promises
  no order. The wrapper preserves that order, but later additions do not
  sort it. Sets, including a set of bytes, use the data encoding rather
  than a byte buffer.
- A reader rejects truncated data, counts that exceed the available bytes,
  invalid UTF-8 and trailing bytes. Each fails the call with the same status
  as malformed text.
- Adapters copy data in both directions. A Rust function takes the
  deserialized value or a reference to that copy; returned data is serialized
  and then dropped. Haxe records therefore need no `close()`. Editing a Haxe
  record after a call leaves the library's copy unchanged.
- Generated data classes use the same Haxe file for HashLink, hxcpp and
  CPPIA. Construct a record with named fields, `{label: "x", enabled: true}`,
  and a field that is an `Option` can be left out. A null where a value is
  needed is refused in Haxe, before anything is written. A Haxe enum
  cannot have methods, so a tagged enum's serializer is a separate
  generated class: `EventWire` for `Event`.
- On a record field, `#[hxn(default)]` calls
  `Default::default()` when Haxe omits the field; `#[hxn(default =
  "Item::default_unit")]` calls that public zero-argument function, whose
  path is relative to the library root. The generated Haxe field is
  nullable and starts as null. Null requests the default on every crossing
  into Rust; false, zero and empty collections are explicit values. Defaults
  are constructed lazily by the adapter, without changing the Haxe input.
  Rust always writes the actual value back, so returned fields are filled.
  Each defaulted field adds a presence byte before its usual encoding;
  bindings and adapters must be regenerated together after changing a
  default annotation. The manifest records `default: true` or the function
  path. Defaults apply to non-nullable record fields. An `Option` field
  already uses `None` when omitted; tagged enum fields remain required.
- Data can be passed as function or callback parameters and return values,
  returned from async functions, and stored in futures, streams and signals. The
  encoding is the same in every position: the adapter serializes or
  deserializes on the Rust side and the binding on the Haxe side.
- Serializing data copies and visits every field on each call. Use records
  for descriptors, results and short lists; use byte buffers for large data
  transfers and interfaces for values that require identity.

See also: [Rust API](../conventions/rust-api.md).
