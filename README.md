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

## Examples

[examples/](examples) has this library and two more, each a crate and a Haxe
program that run from a checkout:

| Example | What it shows |
|---|---|
| [greeter](examples/greeter) | A struct used as a Haxe class, text in and out, and a failure thrown as an exception. |
| [picture](examples/picture) | A Haxe function called by the library, bytes returned, and one object passed to another. |
| [primes](examples/primes) | Work done on another thread: a future to wait for or be called back by, and a stream. |

```
cargo install --path crates/hxn-cli
cd examples/primes
hxn run hl
```

To work on hxn itself, see [AGENTS.md](AGENTS.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
