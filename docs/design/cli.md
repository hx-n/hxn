# CLI builds and packaging

[Design](../design.md) · [Conventions](../conventions.md)

`hxn` runs generation and builds the runtime adapters for a library defined in
a single Rust crate. It supplies the adapter crates, build scripts and
compiler arguments.

| Command or option | Behavior |
|---|---|
| `hxn new <name>` | creates a library as a copy of `examples/greeter`: a crate, and a Haxe program that uses it |
| `hxn build <hl\|cpp>` | generates the binding for a runtime and builds it |
| `hxn run <hl\|cpp>` | builds, then compiles and runs the library's Haxe program |
| `hxn package [hl] [cpp]` | builds for both runtimes, or the ones named, and collects what a Haxe program needs into one directory |
| `hxn check [hl] [cpp] [cppia]` | runs the program on each runtime and checks that the outputs match |
| `hxn doctor [--target <triple>]` | reports tools, runtime prerequisites and installed Rust standard libraries; checks a requested binding target |
| `hxn import <header.h>` | creates a Rust library crate for a C library, from its header |
| `hxn version` | prints the version, which is the tag `hxn new` depends on |
| `--wasm` on any of these | does the same for the runtime's wasm form |
| `--target <triple>` on `build cpp` or `package` | cross-compiles the hxcpp binding |
| `cppia` in place of `hl` or `cpp` | builds for hxcpp and a host executable that contains the binding, and runs the program as a script |

- A build creates the runtime's adapter crate under Cargo's target
  directory, adds a path dependency on the library and builds the adapter
  there. It writes the API manifest in the library's directory.
- The command uses Cargo metadata to locate the library's `hxn` dependency
  in a path, git repository or registry. It takes `hxn-abi` and `hxn-hl`
  from that same source to keep their handle types compatible.
- Before building, the command compares its version with the library's
  `hxn` version. A mismatch stops the build and reports the dependency tag
  or command version to install. Generated adapters require the matching
  version of the `hxn` crates.
- The first adapter build copies the library's `Cargo.lock`. Later builds
  copy it again when the library's lock file changes, preserving the
  library's resolved dependency versions.
- The command embeds hxn's Haxe support classes and C header and writes
  them alongside the generated binding. The command supplies these support
  files during each build.
- The command writes Haxe class paths and hxcpp header/library paths to a
  compiler argument file. `run` passes the same arguments directly to Haxe.
- On Windows a HashLink library is linked against the VM's own library.
  Its adapter's build script links `libhl.lib` and searches for it in
  `HXN_HL_DIR`, which the command sets to the VM's directory. A library
  linked into an hxcpp program on Windows is built with the static C runtime
  that hxcpp uses, the program links the system libraries that Rust's
  standard library needs, and hxcpp is told to build 64-bit for the host,
  which is not its default.
- Set the program directory, main class and extra `haxe-flags` under
  `[package.metadata.hxn]`. Flags can include `-lib` and `-D`. Select the
  VM and its options on the command line or through `HXN_VM`.
- A wasm build uses the same adapter, compiled for another target. For
  HashLink it is built as a static archive for `wasm32-wasip1` and linked
  into a module that exports the primitives' resolvers and imports functions
  the program provides; Ash builds the program and loads the module from the
  same directory. For hxcpp it is a static archive for
  `wasm32-unknown-emscripten` that hxcpp links, and node runs the result.
  For HashLink, Rust's standard library is rebuilt position-independent and
  unwinding with wasm exceptions, in the encoding Ash emits
  (`-wasm-use-legacy-eh=false`), which requires a nightly toolchain; the
  command reports this when the toolchain is not nightly. For hxcpp the prebuilt one for Emscripten is
  used: it unwinds with wasm exceptions, as hxcpp's Emscripten build does,
  so a panic is caught there as it is natively.

- `check` runs the library's Haxe program on the requested runtimes,
  defaulting to HashLink and hxcpp. It requires every run to succeed and all
  outputs to match. Before building, it compares the committed manifest with
  the current source and stops on a difference, preserving the API change
  for review.
- `doctor` checks Cargo, Haxe and the tools required by each target:
  a HashLink VM, hxcpp, and for wasm either a nightly toolchain with Rust's
  source and Ash, or Rust's Emscripten target, the Emscripten SDK and an
  hxcpp that builds for it with wasm exceptions. It also lists optional
  tools that can speed up builds. To find installed standard libraries,
  `doctor` looks for `std` archives in the active rustc sysroot. This works
  with custom toolchains independently of rustup. It excludes empty target
  directories and core-only installations and reports an unreadable
  inventory as unavailable.
  `doctor --target <triple>` checks cargo and that target's standard library
  for a Rust static binding, and its exit status reflects that check instead
  of whether a local HashLink or hxcpp program can run. A missing built-in
  target produces a `rustup target add` command; an unknown name produces a
  `rustc --print target-list` hint. Bindings require `std`. Dependencies of
  the library and the Haxe app may require additional platform tools and
  SDKs beyond this check.
- A CPPIA script calls the binding through an hxcpp host. The host contains
  the library's generated classes and hxn's support classes. The script uses
  class declarations because CPPIA cannot call C++ or hold native pointers. The generator
  also writes a declaration of each class, an `extern class` with the public
  methods and no implementation, and a script is compiled against those
  declarations and against the list of classes its host contains. For
  `cppia` the command builds such a host, compiles the library's Haxe program
  as a script and has the host run it; the host passes the arguments after
  the script's name to the script as its program arguments.
- `package` creates a haxelib directory containing the built library and its
  bindings:

  | Path | Contents |
  |---|---|
  | `<library>.hxn.json` | the manifest |
  | `hxn/` | hxn's own Haxe classes |
  | `binding/hl/`, `binding/cpp/` | the library's generated classes for each runtime |
  | `binding/cppia/` | declarations of the hxcpp classes, for a script |
  | `include/` | `hxn.h` and the library's headers, for hxcpp |
  | `native/<triple>/` | the built libraries for one platform, named by its Rust target triple, or for `wasm32-wasip1` or `wasm32-unknown-emscripten` |
  | `haxelib.json`, `extraParams.hxml` | the haxelib name and version, and the single line that runs `hxn.Bind.lib` |

  Only `native/` differs between machines, so packaging again on another
  machine, with `--wasm` or with `--target`, adds a directory to it.
- `--target` cross-compiles the hxcpp binding. The binding is a static
  library that is only linked when the program is, so cross-compiling needs
  the Rust target installed. The linker and SDK are needed later, when
  linking the application. A package can include libraries for iOS (`aarch64-apple-ios`), Android
  (`aarch64-linux-android`) or another desktop platform. An `.hdll` is a
  shared library, linked at build time against the system it runs on, so it
  has to be built on that system. `run` and `check` reject `--target`,
  since the result cannot be executed locally.
- `haxelib.json` is filled in from the crate's metadata. Name, version
  and description come from the crate, `url` from `repository`, `license`
  from the crate's SPDX expression mapped to one of the names haxelib
  accepts, and `tags` from `keywords`. `contributors` (haxelib account
  names) and `releasenote` have no Cargo equivalent and are set under
  `[package.metadata.hxn]`. `package` prints the fields `haxelib submit`
  still requires.
- `hxn.Bind.lib("<library>")` configures a compile to use a package. It
  is a macro that runs before typing, called from a package's
  `extraParams.hxml`, so a program's build specifies only `-lib <library>`
  and no paths. It locates the package by finding its manifest on the class
  path and reads it, rejecting a manifest of a different version. Based on
  the compile's defines it adds `binding/hl` or `binding/cpp` to the class
  path; for hxcpp it defines the header and archive paths, and the word size
  the archive was built for where hxcpp would default to a different one;
  for HashLink it copies the `.hdll`, or the `.wasm` module, next to the
  program's output once that is written, which is where the VM's loader and
  Ash look for it. The native library used is the one for the host machine,
  the wasm one when `wasm` or `emscripten` is defined, the iOS or Android one
  when hxcpp's defines for those platforms are set (`iphoneos`, `iphonesim`,
  or `android` with an architecture define), or the one given by
  `-D hxn_triple=<triple>`. A compile for `cppia` gets only `binding/cppia`;
  an hxcpp compile with `-D scriptable`, which builds a host, keeps every
  class of the binding and of hxn whether or not the host uses it.
- The generator produces a library's Haxe classes and adapters together,
  keeping their names and signatures consistent. `hxn.Bind.lib` selects
  those packaged classes during compilation. Generating them again from the
  manifest would require a second emitter whose version could differ from
  the package's adapter generator.

A library that keeps its adapter crates in its own repository, as the fixture
in `tests/counter` does, gives each one a build script that calls `hxn-gen`.

See also: [Binding generation](generation.md), [Releases and installation](../conventions/releases.md).
