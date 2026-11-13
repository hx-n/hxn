# Testing

[Design](../design.md) · [Conventions](../conventions.md)

Run every runtime under a timeout. `cargo test --workspace` covers pure crates;
the counter fixture exercises native calls and generated bindings:

  ```
  tests/counter/run.sh hl <vm>      # stock HashLink, and Ash
  tests/counter/run.sh cpp
  tests/counter/run.sh wasm <ash>
  tests/counter/run.sh emcc
  tests/counter/run.sh gen-hl <vm>
  tests/counter/run.sh gen-cpp
  tests/counter/run.sh gen-cppia
  tests/counter/run.sh gen-wasm <ash>
  tests/counter/run.sh gen-emcc
  ```

Use stock HashLink and Ash's hybrid, JIT and interpreter modes. Repeat
concurrent tests on more than one machine. Report unavailable runtimes;
Windows is known to pass only after CI runs.

## Other suites

| Suite | Coverage / modes |
|---|---|
| `tests/cli/run.sh` | New, run, check and package; `hl <vm>`, `cpp`, `cppia`, `wasm <ash>`, `emcc`. |
| `tests/examples/run.sh` | Each example against `expected.txt`; same modes as CLI. |
| `tests/cimport/run.sh` | Import tally and installed sqlite3; `hl <vm>`, `cpp`, `cppia`, `emcc`; needs clang. |
| `tests/mobile/run.sh ios-sim` / `android` | Package, compile, link and run examples on local mobile tools. Run after hxcpp compilation/linking changes. |
| `tests/install/run.sh` | Offline release archives, platforms and installer failures. |

VM strings include options, for example `hl "ash --mode jit"`.
CI runs unit tests on Linux/macOS/Windows, native HashLink/hxcpp and CPPIA,
Emscripten and iOS simulator tests. Run Ash and Android locally.

## Checks and fixtures

- Run `cargo fmt --all` and `cargo clippy --workspace --all-targets`.
- ABI/future changes also run `cargo miri test -p hxn-abi` and
  `cargo miri test -p hxn-future`. Use multiple seeds for concurrency; the future suite needs `-Zmiri-ignore-leaks` for its pool.
  Queue tests use bounded rounds, not throughput-dependent timing.
- Keep the README example identical to `examples/greeter`. Commit current
  manifests, example output and imported-header expectations with API changes.
- For C importer changes, `HXN_BLESS=1 cargo test -p hxn-c` updates
  `imported.webidl`; update the reviewed `tally.webidl` with it. `extra.rs`
  remains the hand-written wrapper fixture.
- Examples demonstrate realistic usage; assertions and regression checks belong
  in `tests/counter`. Runtime-dependent skips state their reason.
- Force GC or assert bounds independent of GC timing. End programs with
  `hxn.Loop.exit`. CI pins hxcpp through `HXN_HXCPP`.

See [CI configuration](../../.github/workflows/ci.yml) for the platform matrix.
