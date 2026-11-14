# Packaging a library

[CLI usage](cli.md) · [Design](design.md)

`hxn package` builds both bindings by default; specify `hl` or `cpp` to select
one. Use `--release --out <directory>` to create the distributable haxelib.
Haxe programs use it with `-lib <library>` and need no Rust source.

## Package contents

| Path | Contents |
|---|---|
| `<library>.hxn.json` | API manifest. |
| `binding/` | Generated classes, the same for every runtime. |
| `include/` | C headers and glue. |
| `native/<triple>/` | Native library for each packaged target. |
| `haxelib.json`, `extraParams.hxml` | Metadata, the dependency on `hxn`, and automatic `hxn.Bind.lib` setup. |

hxn's own Haxe classes, `hxn.Bind` among them, are the `hxn` haxelib, which
this repository is. A package depends on it at the version of hxn that
generated it, so every hxn library a program uses shares one copy.

## Additional platforms

Run packaging again into the same directory to add platforms. `--wasm` adds
`wasm32-wasip1` or `wasm32-unknown-emscripten`. `--target` adds a static
library for that triple, including iOS and Android; install its Rust target
first. For hxcpp it is linked into the app. For HashLink it is
`lib<library>.a`, which `hxn.Bind` copies next to `main.hl` and Ash links
into the program as a `--native-lib`, since an Android or iOS program cannot
open an `.hdll`. A host `.hdll` must be built on the system it runs on.

`hxn.Bind.lib` selects the host, wasm or mobile library from Haxe defines.
Use `-D hxn_triple=<triple>` to override the selection. CPPIA scripts use
an hxcpp host containing the binding. See [binding generation](design/generation.md).

## Metadata

Cargo supplies the name, version, description, repository URL, keywords as
tags, and the SPDX license mapped to a haxelib license. Set `contributors`
(haxelib account names) and `releasenote` under `[package.metadata.hxn]`.
The haxelib is named after the library unless `haxelib` there names it; the
native library and `hxn.Bind.lib` keep the library's name.
`hxn package` lists fields still required by `haxelib submit`.
