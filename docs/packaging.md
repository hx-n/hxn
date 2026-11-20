# Packaging a library

[CLI usage](cli.md) · [Design](design.md)

`hxn package` builds both bindings by default; specify `hl` or `cpp` to select
one. Use `--release --out <directory>` to create the distributable haxelib.
Haxe programs use it with `-lib <library>` and need no Rust source.

## Library repository layout

A library for Haxe consumers keeps its generated `binding/`, `include/`,
`haxelib.json`, `extraParams.hxml` and API manifest in its package directory.
For a single-package repository this is the repository root. Regenerate those
files with `hxn package hl cpp --release --out .` and check their drift in CI.
Keep `native/` ignored in Git; CI builds and combines the platform binaries in
the installed release ZIP. Generated files under `target/` alone are development
output and do not complete a distributable Haxelib.

Haxe consumers install the prebuilt Haxelib and compile with `-lib`. They do not
build Rust, generate adapters or fork the repository to run packaging scripts.
Release checks install the actual ZIP into an isolated Haxelib repository and
compile examples with Rust disabled. Keep source-build instructions in the
contributor guide and retain licenses and packaged skills with the library.

Produce wasm archives on the common pinned Linux/toolchain setup used by the
other hxn libraries. Rebuilt Rust standard-library identities can differ across
producer hosts, even at the same compiler version. Installed consumer checks
must link composed SDKs with their published dependencies before release.

## Package contents

| Path | Contents |
|---|---|
| `<library>.hxn.json` | API manifest. |
| `binding/` | Generated classes, the same for every runtime. |
| `sources/` | The crate's own hand-written Haxe, from its `sources/` directory, when it has one. |
| `include/` | C headers and glue. |
| `native/<triple>/` | Native library for each packaged target; a threaded Emscripten one is under `native/wasm32-unknown-emscripten-threads/`. |
| `haxelib.json`, `extraParams.hxml` | Metadata, the dependency on `hxn`, and automatic `hxn.Bind.lib` setup, then the library's `init` macros. |

hxn's own Haxe classes, `hxn.Bind` among them, are the `hxn` haxelib, which
this repository is. A package depends on it at the version of hxn that
generated it, so every hxn library a program uses shares one copy.

## Additional platforms

Run packaging again into the same directory to add platforms. `--wasm` adds
`wasm32-wasip1` or `wasm32-unknown-emscripten`, and with `--threads` the
threaded build of either. `--target` adds a static
library for that triple, including iOS and Android; install its Rust target
first. For hxcpp it is linked into the app. For HashLink on wasm, Android and
iOS it is `lib<library>.a`, which `hxn.Bind` copies next to `main.hl` and Ash
links into the program with `--native-lib`, since those programs cannot open
an `.hdll`. A host `.hdll` must be built on the system it runs on.

`hxn.Bind.lib` selects the host, wasm or mobile library from Haxe defines.
Use `-D hxn_triple=<triple>` to override the selection. Haxelib gives a
program one hxn, from the first `-lib` that depends on it; `hxn.Bind.lib`
fails when that hxn is older than the one a package names, or another minor
version. CPPIA scripts use
an hxcpp host containing the binding. See [binding generation](design/generation.md).

## Hand-written Haxe

A library can carry Haxe of its own beside the generated classes: helpers,
macros, or an hxnsl target a GPU driver registers. Put it in the crate's
`sources/` directory. `hxn build`, `run` and `check` add it to the class path,
`hxn package` copies it into the package, and `hxn.Bind.lib` adds the
package's `sources/` to the class path. `init` under
`[package.metadata.hxn]` lists macros to run after `hxn.Bind.lib`, which may
name classes in `sources/`:

```toml
[package.metadata.hxn]
init = ["hxn.gpu.metal.Shaders.register()"]
```

`hxn package` writes each into `extraParams.hxml` as `--macro <call>`, and
local builds pass them to `haxe` the same way, CPPIA scripts included. The
directory keeps its name in the package, so a repository whose root is its
haxelib has the same layout as the package.

## Page files

A library that serves a browser API to a threaded wasm program, through a
browser agent the web page starts, keeps the browser agent's shim and what it
imports in the crate's `page/` directory. Ash's page starts it when the
library calls `ash_host_agent`; under Emscripten the library starts it from
the page's thread itself. `hxn build hl --wasm` and `hxn run cpp --wasm
--threads` copy them beside the program, where Ash or Emscripten writes its
web page; `hxn package` copies them into the package's `page/`, and
`hxn.Bind.lib` copies those beside the `.hl` output of a wasm build, or beside
the page of a threaded hxcpp build.

## Metadata

Cargo supplies the name, version, description, repository URL, keywords as
tags, and the SPDX license mapped to a haxelib license. Set `contributors`
(haxelib account names) and `releasenote` under `[package.metadata.hxn]`.
The haxelib is named after the library unless `haxelib` there names it; the
native library and `hxn.Bind.lib` keep the library's name.
`hxn package` lists fields still required by `haxelib submit`.

The hxn libraries' keywords include `hxcpp`, `hashlink`, `native` and
`cross`, so a haxelib search for any of them finds every hxn library, and
`wasm` unless the library cannot run in a browser, as the Metal, Vulkan,
Direct3D 12 and GLES drivers cannot.

`link-args` passes arguments to the program's link for hxcpp, by target
triple, for what a static library cannot carry, such as an Emscripten port:

```toml
[package.metadata.hxn]
link-args = { wasm32-unknown-emscripten = ["--use-port=emdawnwebgpu"] }
```

A threaded Emscripten build takes those of `wasm32-unknown-emscripten`, then
those of `wasm32-unknown-emscripten-threads`. Its `main` runs on a pthread,
so a library that draws on the web page's canvas there adds
`-sOFFSCREENCANVAS_SUPPORT`, which transfers the element with the id
`canvas` to that thread:

```toml
[package.metadata.hxn.link-args]
wasm32-unknown-emscripten = ["-sMIN_WEBGL_VERSION=2", "-sMAX_WEBGL_VERSION=2"]
wasm32-unknown-emscripten-threads = ["-sOFFSCREENCANVAS_SUPPORT"]
```

## CLI extensions

`[package.metadata.hxn.cli]` registers a library's CLI namespace and lifecycle
hooks. Packaging preserves it as `hxn.cli` in `haxelib.json`; Haxe tools in
`sources/` travel with the library. Extra tool files/directories listed in
`cli.files` are copied as bytes and retain executable permissions. Consumers discover them with `hxn plugins`
and invoke `hxn <namespace> ...`, without repository scripts or Rust sources.
See the [versioned extension protocol](design/cli-extensions.md).
