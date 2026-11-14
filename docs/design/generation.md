# Binding generation

[Design](../design.md) · [Conventions](../conventions.md)

`hxn-gen` follows modules from `lib.rs` and reads items marked
`#[hxn::interface]`, `#[hxn::methods]`, `#[hxn::function]` and `#[hxn::data]`.
Names are unique across the library; adapters refer to items at the crate root.

Part of the API can be written by the library's build script, as a tool
like x-idl does from an API declaration: the source includes it with
`include!(concat!(env!("OUT_DIR"), "/name.rs"))`. `hxn` then runs
`cargo check` on the library, which runs the build script, takes `out_dir`
from cargo's messages and reads the file from there. A library without such
an include is not checked first.

The generated classes are in the top-level package unless the library
declares one with `hxn::package!("name")`, which puts every class,
including the support classes it gets, in that package's directory.

| Output | Purpose |
|---|---|
| JSON manifest | Committed ABI description; regenerate and review API changes. |
| Rust adapters | C exports and HashLink primitives. |
| C headers and hxcpp glue | Boundary declarations and C++ conversion. |
| Haxe classes | One public API across runtimes. |

Attribute macros retain the Rust items and remove marker notes. The generator
writes separate adapter crates, which macro expansion cannot populate.
Free functions become static methods of the library's class or `class = "Name"`.

HashLink classes declare private `@:hlNative` methods. hxcpp uses one extern
class for glue calls and build settings; generated classes contain no inline
C++. CPPIA gets extern declarations and calls the binding through an hxcpp
host containing the generated and support classes.

## Adapter builds

`hxn` creates adapter crates under `target/hxn/<library>`, with the manifest
in the library directory. Cargo metadata locates `hxn-abi` and `hxn-hl` from
the library's own `hxn` source; command and crate versions must match.
The adapter lockfile is refreshed when the library's `Cargo.lock` changes.
Support Haxe classes, headers and compiler arguments accompany the binding.

A static library does not carry the native libraries it needs to the
program's link. For hxcpp, `hxn` writes them into `link.xml`: what the build
scripts link, from cargo's messages, and what rustc lists for the archive
(`--print native-static-libs`), which includes what a crate links with
`#[link]`, such as a system framework. An MSVC C runtime is left out, since
hxcpp links its own.

Libraries maintaining adapters themselves call `hxn-gen` from build scripts,
as in `tests/counter`.

## Packaged bindings

`hxn.Bind.lib` finds the package manifest, validates its version and selects
the pre-generated classes. Generating classes and adapters together prevents
signature drift. The macro adds header/archive paths, the target word size
and the native library selected by Haxe's target defines or `hxn_triple`.
For HashLink it copies the `.hdll` or `.wasm` beside the program. CPPIA
scripts get declarations only; `-D scriptable` hosts retain all binding and
support classes.

See [packaging](../packaging.md) and [binding rules](../conventions/bindings.md).
