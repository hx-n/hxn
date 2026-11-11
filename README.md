# hxn

Write a native library once in Rust and use it from Haxe on HashLink, Ash and
hxcpp, natively and on wasm. The library stays plain Rust: you mark what it
exposes, and hxn writes the bindings for each runtime.

How it works is in [docs/design.md](docs/design.md), and the rules it follows
in [docs/conventions.md](docs/conventions.md).

## Quick setup

You need a recent Rust toolchain, Haxe, and the runtime you want to run on.

```
cargo test --workspace
tests/counter/run.sh hl <path to hl or ash>
tests/counter/run.sh cpp
```

`tests/counter/run.sh` lists its other modes, for wasm and for generated
bindings. The wasm mode needs a nightly toolchain with `rust-src`.

## License

Apache-2.0. See [LICENSE](LICENSE).
