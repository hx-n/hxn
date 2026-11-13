<p align="center">
  <img src="docs/logo.png" alt="hxn" width="280">
</p>

# hxn

Use the same Rust library from Haxe on HashLink, Ash and hxcpp, natively and
on wasm. Annotate the Rust types and methods to expose; hxn generates the Haxe
classes and native adapters for each target.

[Design](docs/design.md) links to guides on the architecture, bindings and
tools. [Conventions](docs/conventions.md) covers API rules, testing and
contribution workflow.

## Quick setup

You need Rust, Haxe, and HashLink or hxcpp. For hxcpp, use its git version:
hxcpp 4.3.2 can crash on Windows when a program exits right after a
collection ([HaxeFoundation/hxcpp#1470](https://github.com/HaxeFoundation/hxcpp/issues/1470)),
which is fixed in git.

```
haxelib git hxcpp https://github.com/HaxeFoundation/hxcpp
cd "$(haxelib libpath hxcpp)/tools/hxcpp" && haxe compile.hxml
```

Install the `hxn` command from the latest release:

```sh
curl -fsSL https://raw.githubusercontent.com/hx-n/hxn/main/install.sh | bash
```

The script selects the macOS, Linux or Windows binary for x86-64 or ARM64,
verifies its SHA-256 checksum, and installs it in `~/.local/bin`. Add that
directory to your `PATH` if it is not already there. On Windows, run it in
Git Bash, or download the zip from the
[releases](https://github.com/hx-n/hxn/releases) and put `hxn.exe` on your
`PATH`. Set `HXN_VERSION` to select a release tag or `HXN_INSTALL_DIR` to
choose another directory, for example:

```sh
curl -fsSL https://raw.githubusercontent.com/hx-n/hxn/main/install.sh | HXN_VERSION=v0.2.0 HXN_INSTALL_DIR="$HOME/.local/bin" bash
```

To build the command from source, use Cargo:

```
cargo install --git https://github.com/hx-n/hxn --tag v0.2.0 hxn-cli
```

Create a library with `hxn new`:

```
hxn new greeter
cd greeter
```

Write the library in `src/lib.rs`. `#[hxn::interface]` exposes a Rust struct
to Haxe as a class, and `#[hxn::methods]` exposes its public functions as that
class's public Haxe methods. A function outside an `impl` is marked
`#[hxn::function]` and becomes a static method.

```rust
/// Greets people, and counts them.
#[hxn::interface]
pub struct Greeter {
    greeting: String,
    greeted: i32,
}

#[hxn::methods]
impl Greeter {
    pub fn new(greeting: &str) -> Self {
        Self {
            greeting: greeting.to_owned(),
            greeted: 0,
        }
    }

    /// A greeting for `name`. Fails for an empty name.
    pub fn greet(&mut self, name: &str) -> Result<String, &'static str> {
        if name.is_empty() {
            return Err("nobody to greet");
        }
        self.greeted += 1;
        Ok(format!("{}, {name}!", self.greeting))
    }

    /// How many have been greeted.
    pub fn greeted(&self) -> i32 {
        self.greeted
    }
}
```

Use the generated class from `haxe/Main.hx`:

```haxe
class Main {
	static function main() {
		var greeter = new Greeter("Hello");
		Sys.println(greeter.greet("Haxe"));
		Sys.println(greeter.greet("Rust"));
		Sys.println('${greeter.greeted()} greeted');
		try {
			greeter.greet("");
		} catch (error:haxe.Exception) {
			Sys.println('refused: ${error.message}');
		}
		greeter.close();
	}
}
```

Run it on either target:

```
hxn run hl
hxn run cpp
```

```
Hello, Haxe!
Hello, Rust!
2 greeted
refused: nobody to greet
```

`hxn build hl` or `hxn build cpp` generates the Haxe classes and builds the
native library without running anything, and prints the `haxe` arguments for
compiling your own program against them.

Add `--wasm` to build and run the same program as wasm: `hxn run hl --wasm`
through Ash, and `hxn run cpp --wasm` through hxcpp and Emscripten. The first
needs a nightly Rust toolchain; the second needs Rust's Emscripten target
(`rustup target add wasm32-unknown-emscripten`).

`hxn run cppia` runs the same program as a CPPIA script: it builds the hxcpp
binding into a host executable, which runs the script. The script calls the
library through the host, so only the host needs rebuilding when the library
changes.

`hxn check` runs the program on HashLink and hxcpp, or on the runtimes you
name, and passes only if their output is identical. `hxn doctor` lists the
targets your machine can build for and what is missing for the others.

`hxn package` builds the library for both runtimes and collects the Haxe
classes, headers and native libraries into a haxelib directory. Haxe projects
can compile against this package without the library's Rust source:

```
hxn package --release --out ../greeter-haxelib
haxelib dev greeter ../greeter-haxelib
haxe -lib greeter -cp src -main Main -hl main.hl
```

`-lib greeter` runs the macro `hxn.Bind.lib("greeter")`, which selects the
classes for the compile target, passes hxcpp the header and library paths,
and for HashLink copies `greeter.hdll` next to `main.hl`. The package
contains native libraries for the machine it was built on. To add more, run
`hxn package` with the same output directory on another machine, or with
`--wasm`. The hxcpp library can also be cross-compiled, including for iOS
and Android:

```
rustup target add aarch64-apple-ios aarch64-linux-android
hxn package cpp --target aarch64-apple-ios --out ../greeter-haxelib
hxn package cpp --target aarch64-linux-android --out ../greeter-haxelib
```

Compiling with hxcpp's defines for that platform (`-D iphoneos`, or
`-D android -D HXCPP_ARM64`) selects the matching library.

`haxelib submit` requires a few more fields. Set `repository` and `license`
in Cargo.toml, and the haxelib contributors and a release note under
`[package.metadata.hxn]`:

```toml
[package.metadata.hxn]
contributors = ["your-haxelib-name"]
releasenote = "First release."
```

## Binding a C library

`hxn import` uses clang to read a C header and generate a Rust library crate.
The hxn generator then creates Haxe bindings from that crate.

```
hxn import vendor/tally.h --source vendor/tally.c
cd tally
```

The crate's API is in `tally.webidl`, generated from the header. Review it and
add what a header cannot express: `[HxnStatus]` on a function whose `int`
return value is a status code, `[HxnFree=function]` where returned text or an
object must be freed. Each `hxn build` regenerates `src/bindings.rs` from it.
A C callback the library calls during the call is passed a Haxe function; one
it keeps to call later, such as the one `sqlite3_update_hook` registers, is
returned as a stream of its calls. Use `--link <library>` instead of
`--source` to link a library that is already built. See
[Importing C libraries](docs/design/c-import.md) for supported declarations
and import restrictions.

## Examples

[examples/](examples) contains `greeter` and six other libraries. Each example
has a Rust crate and a Haxe program that you can run from a checkout:

| Example | Features |
|---|---|
| [greeter](examples/greeter) | A struct exposed as a Haxe class, string parameters and return values, and an error thrown as an exception. |
| [picture](examples/picture) | A Haxe callback called by the library, a byte array return value, and an object passed as a parameter to another object's method. |
| [primes](examples/primes) | Work done on another thread: a future that can be awaited or given a completion callback, and a stream. |
| [downloads](examples/downloads) | An `async fn`, a value shared between the library and Haxe, and an atomic counter that Haxe reads without a native call. |
| [thermostat](examples/thermostat) | A value the library updates, observed from Haxe as a signal. |
| [pantry](examples/pantry) | Records and enums passed by value, an optional field, a list of records returned, and a typed error. |
| [document](examples/document) | Wraps an existing crate (`serde_json`): JSON values as a recursive enum, objects as a map, and parse errors as a typed exception with line and column. |

```
cargo install --path crates/hxn-cli
cd examples/primes
hxn run hl
```

To work on hxn itself, see [AGENTS.md](AGENTS.md).

## License

Licensed under Apache-2.0. See [LICENSE](LICENSE).
