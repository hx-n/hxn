<p align="center">
  <img src="docs/logo.png" alt="hxn" width="280">
</p>

# hxn

Write a native library once in Rust and use it from Haxe on HashLink, Ash and
hxcpp, natively and on wasm. The library stays plain Rust: you annotate the
types and methods to expose, and hxn generates the Haxe classes and the native
glue for each target.

How it works is in [docs/design.md](docs/design.md), and the rules it follows
in [docs/conventions.md](docs/conventions.md).

## Quick setup

You need Rust, Haxe, and HashLink or hxcpp. Install the `hxn` command and make
a library with it:

```
cargo install --git https://github.com/hx-n/hxn hxn-cli
hxn new greeter
cd greeter
```

The library lives in `src/lib.rs`. It is ordinary Rust: `#[hxn::interface]`
exposes a struct to Haxe as a class, and `#[hxn::methods]` exposes its public
functions as that class's public Haxe methods. A function outside an `impl`
is marked `#[hxn::function]` and becomes a static method.

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

`haxe/Main.hx` uses the generated class like any other:

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
through Ash, and `hxn run cpp --wasm` through hxcpp and Emscripten. Both need
a nightly Rust toolchain.

`hxn run cppia` runs the same program as a CPPIA script: it builds the hxcpp
binding into a host program and has that run the script. A script calls the
library through the host, which is the only thing that needs rebuilding when
the library changes.

`hxn check` runs the program on HashLink and hxcpp, or on the runtimes you
name, and passes only if they all print the same. `hxn doctor` lists which
targets your machine is ready for and what the others need.

`hxn package` builds the library for both runtimes and gathers the classes,
the headers and the native libraries into one directory, for a Haxe project
that does not have the Rust source. That directory is a haxelib:

```
hxn package --release --out ../greeter-haxelib
haxelib dev greeter ../greeter-haxelib
haxe -lib greeter -cp src -main Main -hl main.hl
```

`-lib greeter` runs the macro `hxn.Bind.lib("greeter")`, which picks the
classes for the target being compiled, tells hxcpp where the headers and the
library are, and for HashLink puts `greeter.hdll` beside `main.hl`. The
package holds native libraries for the machine it was made on; run
`hxn package` into the same directory on another machine, or with `--wasm`,
to add more.

## Examples

[examples/](examples) has this library and six more, each a crate and a Haxe
program that run from a checkout:

| Example | What it shows |
|---|---|
| [greeter](examples/greeter) | A struct used as a Haxe class, text in and out, and a failure thrown as an exception. |
| [picture](examples/picture) | A Haxe function called by the library, bytes returned, and one object passed to another. |
| [primes](examples/primes) | Work done on another thread: a future to wait for or be called back by, and a stream. |
| [downloads](examples/downloads) | An `async fn`, a value shared between the library and Haxe, and a counter Haxe reads without a call. |
| [thermostat](examples/thermostat) | A value the library keeps current, followed from Haxe as a signal. |
| [pantry](examples/pantry) | Records and enums passed by value, an optional field, a list of records returned, and a typed error. |
| [document](examples/document) | Wraps an existing crate (`serde_json`): JSON values as a recursive enum, objects as a map, and parse errors as a typed exception with line and column. |

```
cargo install --path crates/hxn-cli
cd examples/primes
hxn run hl
```

To work on hxn itself, see [AGENTS.md](AGENTS.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
