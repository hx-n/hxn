# Examples

Each directory here is a complete library: a Rust crate, and a Haxe program
that uses it.

| Example | What it shows |
|---|---|
| [greeter](greeter) | A struct used as a Haxe class, text in and out, and a failure thrown as an exception. |
| [picture](picture) | A Haxe function called by the library, bytes returned, and one object passed to another. |
| [primes](primes) | Work done on another thread: a future to wait for or be called back by, and a stream. |

## Running one

You need Rust, Haxe, and HashLink or hxcpp. From a checkout of this
repository, install the `hxn` command and run an example on either target:

```
cargo install --path crates/hxn-cli
cd examples/greeter
hxn run hl
hxn run cpp
```

What it prints is in the example's `expected.txt`.

`hxn run hl` runs the program with `hl`. `--vm` names another
HashLink-compatible VM: `hxn run hl --vm ash`.

Without installing anything, `cargo run -p hxn-cli -- run hl` in an example's
directory does the same with the command as the checkout has it.

## What is in one

| File | What it is |
|---|---|
| `Cargo.toml` | An ordinary crate that depends on `hxn`. |
| `src/lib.rs` | The library. `#[hxn::interface]` and `#[hxn::methods]` expose a struct and its functions to Haxe. |
| `haxe/Main.hx` | The Haxe program. |
| `<name>.hxn.json` | The library's API as hxn read it. A build writes it; it is committed so that a change to the API shows in review. |
| `expected.txt` | What the program prints. |

Everything generated goes under `target/hxn/<name>/` at the top of the
checkout. The Haxe classes the program uses are there to read, in
`target/hxn/greeter/hl/haxe/Greeter.hx` for one.

## Starting your own

`hxn new <name>` makes a copy of `greeter` under the name you give. The
examples depend on hxn by path, because they live in its repository; a
library anywhere else names the repository, as the comment in each
`Cargo.toml` shows.
