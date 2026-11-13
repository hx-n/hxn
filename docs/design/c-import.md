# Importing C libraries

[Design](../design.md) · [Conventions](../conventions.md)

`hxn import` requires clang and creates a Rust crate usable from Rust or Haxe.

1. Import a header with `--source` for C files or `--link` for a built library.
2. Review the generated WebIDL, especially its `Check:` notes and declarations
   listed as not imported. Commit the reviewed file.
3. Build: `src/bindings.rs` is regenerated from the WebIDL on each build.

## Header mapping

| C | Imported API |
|---|---|
| Opaque struct and functions taking its pointer first | Object and methods; `tally_add` becomes `add`. |
| Object-returning `new`/`create`; `free`/`destroy` | Constructor; destructor. |
| Struct containing only numbers/bools; enum | Record; C-like enum. |
| `const char *`; byte pointer plus length | Text; bytes. Non-`const` bytes use writable Haxe storage. |
| Numeric pointer plus count | Array; non-`const` arrays return what C fills. |
| Trailing output pointer | Return value; C's `int` becomes a status. Later outputs receive `NULL`. |
| Object returned by another object's method | Borrowed object. |
| Function pointer plus user data | Callback, lent or retained according to registration naming. |

Only declarations in the named header are imported; included headers resolve
typedefs. Buffer/count names help infer lifetimes: review those guesses.
Numeric arrays require fixed-width elements. A returned `char *` uses the
library's sole matching free function, when present.

## WebIDL review

| Attribute | Contract |
|---|---|
| `[HxnStatus]` | Non-zero return means failure. |
| `[HxnFree=f]`, `[HxnBorrowed]` | How returned memory is freed, or remains owned by C. |
| `[HxnOut]`, `[HxnNull]` | Return an output value, or pass null for an unused output. |
| `[HxnWritten]`, `[HxnLength=int]` | Writable bytes; a count type other than `size_t`. |
| `[HxnFrees]`, `[HxnPass=n]` | Destructor for copied text/bytes; a fixed pointer value. |
| `[HxnThreadSafe]` | Allows shared calls; otherwise methods take `&mut self` and objects are only `Send`. |
| `[HxnUser=first]` / `[HxnUser=last]` | Callback user-data position. |
| `[HxnKept]`, `[HxnStop=n]` | Retained callback; callback return on failure or every retained call. |

Owned objects free their C pointers in `Drop`, including output objects on
failed calls. Wrappers return `Result<_, String>` for bad text, numeric range,
unknown enum values, null objects and error statuses.
The [IDL reference](../../crates/hxn-c/src/idl.rs) lists all attributes.

## Callbacks

Lent Haxe functions take at most four values; integer arguments must be `int`.
Trampolines keep panics/failures until C returns, then propagate them before
the C result. Exceptions never unwind through C.

Retained callbacks return a stream of copied arguments, using
`<Callback>Event` records for multiple values. The trampoline returns
`[HxnStop]` on every call. An owned object frees the sender after its C
free function; free functions and borrowed objects retain it indefinitely.
Re-registration replaces C's callback; the earlier stream ends at object
free. An `int` registration result is a status; returned pointers are ignored.

## Linking and limits

`build.rs` compiles `--source` files with `cc` or links `--link` libraries.
`nm` filters declarations missing from a resolved library. hxcpp receives
system dependencies through `link.xml`, packaged beside the archive; build
XML tracks glue headers for rebuilds. `cpp --wasm` uses `emcc`;
C imports do not support `hl --wasm`; C compilation needs the WASI SDK.

Variadics, `va_list`, callback object arguments and ambiguous pointers are
unsupported. Callback user data must be first or last. Write custom Rust
wrappers for retained buffers or untyped pointers. C++ needs an author-provided
C API. See [import tests](../conventions/testing.md).
