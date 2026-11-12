# Examples

Each directory here is a complete library: a Rust crate, and a Haxe program
that uses it.

| Example | What it shows |
|---|---|
| [greeter](greeter) | A struct exposed as a Haxe class, string parameters and return values, and an error thrown as an exception. |
| [picture](picture) | A Haxe callback called by the library, a byte array return value, and an object passed as a parameter to another object's method. |
| [primes](primes) | Work done on another thread: a future that can be awaited or given a completion callback, and a stream. |
| [downloads](downloads) | An `async fn`, a value shared between the library and Haxe, and an atomic counter that Haxe reads without a native call. |
| [thermostat](thermostat) | A value the library updates, observed from Haxe as a signal. |
| [pantry](pantry) | Records and enums passed by value, an optional field, a list of records returned, and a typed error. |
| [document](document) | Wraps an existing crate (`serde_json`): JSON values as a recursive enum, objects as a map, and parse errors as a typed exception with line and column. |

## Running one

You need Rust, Haxe, and HashLink or hxcpp. From a checkout of this
repository, install the `hxn` command and run an example on either target:

```
cargo install --path crates/hxn-cli
cd examples/greeter
hxn run hl
hxn run cpp
```

The expected output is in the example's `expected.txt`.

`hxn run hl` runs the program with `hl`. `--vm` selects another
HashLink-compatible VM, with any options for it: `hxn run hl --vm ash`, or
`hxn run hl --vm "ash --mode jit"`.

`--wasm` runs the same program as wasm. `hxn run hl --wasm` builds and runs
it with Ash, and `hxn run cpp --wasm` with hxcpp, the Emscripten SDK and
node. Both need a nightly Rust toolchain with `rust-src`.

`hxn run cppia` runs it as a CPPIA script, inside an hxcpp host executable
that the command builds with the library linked in. The first build of the
host takes a few minutes.

To run an example without installing the command, use
`cargo run -p hxn-cli -- run hl` in the example's directory. It runs the
`hxn` command built from the checkout.

## What is in one

| File | What it is |
|---|---|
| `Cargo.toml` | An ordinary crate that depends on `hxn`. |
| `src/lib.rs` | The library. `#[hxn::interface]` and `#[hxn::methods]` expose a struct and its functions to Haxe. |
| `haxe/Main.hx` | The Haxe program. |
| `<name>.hxn.json` | The manifest: the library's API as parsed by hxn. A build generates it. It is committed so that an API change is visible in review. |
| `expected.txt` | The program's expected output. |

All generated files go under `target/hxn/<name>/` at the root of the
checkout. That includes the Haxe classes the program uses, for example
`target/hxn/greeter/hl/haxe/Greeter.hx`.

## Starting your own

`hxn new <name>` creates a copy of `greeter` with the name you give. The
examples use a path dependency on hxn because they are in its repository. A
library outside it uses a git dependency on the repository, as the comment
in each `Cargo.toml` shows.
