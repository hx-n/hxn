# Packaging a library

[CLI usage](cli.md) · [Design](design.md)

`hxn package` builds both bindings by default; specify `hl` or `cpp` to select
one. Use `--release --out <directory>` to create the distributable haxelib.
Haxe programs use it with `-lib <library>` and need no Rust source.

## Package contents

| Path | Contents |
|---|---|
| `<library>.hxn.json` | API manifest. |
| `hxn/` | Haxe support classes. |
| `binding/hl/`, `binding/cpp/` | Generated classes. |
| `binding/cppia/` | Script declarations. |
| `include/` | C headers and glue. |
| `native/<triple>/` | Native library for each packaged target. |
| `haxelib.json`, `extraParams.hxml` | Metadata and automatic `hxn.Bind.lib` setup. |

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
`hxn package` lists fields still required by `haxelib submit`.
