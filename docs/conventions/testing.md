# Testing

[Design](../design.md) · [Conventions](../conventions.md)

- `tests/counter` is the fixture: one small library, hand-written bindings for
  both runtimes, and one Haxe test (`haxe/Main.hx`) that every runtime runs.
  `haxe/Generated.hx` checks the classes the generator writes.
- A change is run on every runtime before it is considered done:

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

  Ash is run in its hybrid, JIT and interpreter modes, which differ in how
  threads and native calls behave.
- `tests/cli/run.sh` makes a library with `hxn new` and runs it with
  `hxn run`, as the readme describes, runs it again under `hxn check`, then
  packages it with `hxn package` and builds the same program against the
  package alone.
  `tests/examples/run.sh` runs each library under `examples/` and compares
  what it prints with its `expected.txt`. Both take `hl <vm>`, `cpp`,
  `cppia`, `wasm <ash>` and `emcc`, and a VM is named with its options, so
  `hl "ash --mode jit"` runs Ash's JIT.
- `hxn doctor` lists installed standard libraries from the active rustc
  sysroot. `doctor --target <triple>` succeeds only when cargo and that
  target's `std` archive are present; it does not require a local VM or Haxe.
  Unreadable inventories, unsupported target names, and missing standard
  libraries are distinct diagnostics. The CLI test checks the host target
  and rejects an unknown target and an ignored `--wasm` option.
- `tests/mobile/run.sh` runs each example on mobile toolchains: `ios-sim`
  in an iOS simulator, `android` on a device or emulator reachable by `adb`.
  It packages each library with `--target`, compiles the program against
  the package with hxcpp's defines for the platform, links the result into
  an executable (hxcpp only builds a library on these platforms) and runs
  it. Run it after changing how an hxcpp program is compiled or linked.
- `tests/cimport/run.sh` imports the C library in `tests/cimport` with
  `hxn import`, checks that the importer writes `imported.webidl` exactly,
  replaces it with the edited `tally.webidl`, and runs `Main.hx` against
  the result, then imports the installed `sqlite3.h` unedited, where there
  is one, and runs `sqlite/Main.hx` against it. It takes `hl <vm>`, `cpp`,
  `cppia` and `emcc`. A change to the importer that changes its output
  updates `imported.webidl` in the same commit: `HXN_BLESS=1 cargo test -p
  hxn-c` writes it. `tally.webidl` is that file with the edit a user would
  make, `[HxnStatus]` on `add`, names for the listener's values and
  `[HxnPass]` on `add_blob`, and is updated with it. `extra.rs` is appended
  to the crate's `src/lib.rs` as a function written by hand beside the
  bindings.
- Keep the README's example identical to `examples/greeter`, the template
  for `hxn new`. A test checks that they match.
- Examples demonstrate features through realistic libraries. Each contains
  a crate, a Haxe program, a committed manifest and expected output that
  matches on every run and runtime. Put assertions and regression checks in
  the fixture.
- `.github/workflows/ci.yml` runs the
  unit tests on Linux, Windows and macOS, the lints and Miri on Linux, and
  the fixture's native modes and the CLI and example tests on HashLink and
  hxcpp on Linux and Windows, and the fixture and the CLI test as CPPIA
  scripts. It also runs the generated fixture and the examples
  through Emscripten on Linux, and the examples in an iOS simulator on macOS.
  Windows is run nowhere else. Ash, wasm under Ash, and Android are run by
  hand, as above.
- Run runtime tests under a timeout so incorrect waits fail with a timeout.
- Anything concurrent is run repeatedly, on more than one machine.
- `cargo miri test -p hxn-abi` checks the unsafe core, and
  `cargo miri test -p hxn-future` finds interleavings a native run does not,
  more so with `-Zmiri-many-seeds`. Queued completion and fairness tests
  advance a bounded number of one-round waits, so they do not depend on how
  much work fits in a time slice. The budget test uses up a slice in one
  step and checks that the wait returns before another round. These checks
  also run under Miri. Its future suite uses `-Zmiri-ignore-leaks` because
  the worker pool lives as long as the process.
- A test that depends on a runtime's limitations documents this next to the
  check it skips, with the reason.
- The fixture's JSON manifest is committed, and a test fails when it is out
  of date.
- hxcpp is used from git, at the commit `HXN_HXCPP` in
  `.github/workflows/ci.yml` pins, which comes after
  HaxeFoundation/hxcpp#1298. A test program still ends with
  `hxn.Loop.exit`, not `Sys.exit`, so that it also passes with hxcpp 4.3.2,
  where a program that exits right after a collection crashes on Windows.
- A check does not depend on when the collector runs on its own: it forces a
  collection, or asserts a bound that holds either way.
- The pure crates have unit tests; `hxn-hl` and the adapters are tested only
  in a real runtime.
- The fixture's manifest and each example's are committed, and a test fails
  when one no longer matches its source.

See also: [Architecture and runtimes](../design/architecture.md), [Importing C libraries](../design/c-import.md).
