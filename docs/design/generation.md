# Binding generation

[Design](../design.md) · [Conventions](../conventions.md)

`hxn-gen` parses a library's source for the items marked `#[hxn::interface]`,
`#[hxn::methods]` and `#[hxn::function]` and generates the manifest, both Rust
adapters, the C header, the hxcpp glue and the Haxe classes. It starts at the
crate's `lib.rs` and follows every module declaration from there, so a library
can use any normal crate layout. Names are unique across the whole library.
Adapters refer to each interface at the crate's root.

The generator reads the marked items from source and writes the files for both
adapter crates. Attribute macros remove the marker notes and retain the Rust
items. Macro expansion inside the library cannot add code to the separate
adapter crates or reliably write their files.

A marked function outside an `impl` becomes a static Haxe method. By default,
the generator puts the method in the class named after the library. It uses an
existing interface class when the names match, or generates a class for the
functions. A function-only class needs no handle or `close`; the adapters call
its functions through paths from the crate's root.

A generated Haxe class is a plain class with the library's public API, the
same on both runtimes, that calls native functions. On HashLink each class
declares its own natives, in an `@:hlNative` class private to its file. On
hxcpp one extern class per library declares every function of the glue
header with its types, and carries the build's include and link settings; no
generated class contains inline C++.

The manifest is JSON, generated, committed and reviewed: a change to it is a
change to the library's ABI.

See also: [Crate boundaries](../conventions/crates.md), [Runtime bindings](../conventions/bindings.md).
