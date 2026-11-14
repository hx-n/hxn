# Working on hxn

A Rust library is written once and used from Haxe on HashLink, Ash and hxcpp,
natively and on wasm. Start with these indexes, then read the linked pages
for the area you are changing:

- [docs/design.md](docs/design.md): how each part works and why.
- [docs/conventions.md](docs/conventions.md): the rules the code, the
  bindings and the tests follow.

## Before calling a change done

- **Run every runtime.** `cargo test --workspace` covers the pure crates only.
  The rest is `tests/counter/run.sh` in each of its modes: `hl`, `cpp`, `wasm`,
  `emcc`, `gen-hl`, `gen-cpp`, `gen-cppia`, `gen-wasm`, `gen-emcc`. Run Ash
  in its hybrid, JIT and interpreter modes (`ash --mode jit`,
  `ash --mode interp`); they differ in how threads and native calls behave. `tests/cli/run.sh` checks
  the `hxn` command the way the readme uses it, and `tests/examples/run.sh`
  runs every library under `examples/`; both take `hl`, `cpp`, `cppia`,
  `wasm` and `emcc`, and a VM with its options, as in `hl "ash --mode jit"`.
  `tests/mobile/run.sh ios-sim|android [cpp|hl]` runs the examples in an
  iOS simulator and on an Android device or emulator, with hxcpp or with
  HashLink through Ash. Run it after changing how an hxcpp program or a
  cross-compiled library is compiled or linked. `tests/cimport/run.sh` imports a
  C library with `hxn import` and runs a program against it; it takes `hl`,
  `cpp`, `cppia` and `emcc`, and needs clang.
- **Run runtime tests under a timeout.** A wrong wait hangs instead of failing.
- **Repeat anything concurrent**, and on more than one machine if you can.
  A change to `hxn-abi` or `hxn-future` is also run under Miri
  (`cargo miri test -p hxn-abi`, `-p hxn-future`).
- **Windows is run by CI** (`.github/workflows/ci.yml`), along with HashLink
  and hxcpp on Linux. A change is not known to work on Windows until CI has
  run it.
- **Say what was not run.** If a runtime was unavailable, report that, not a
  pass.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets` are clean.

## Rules that are easy to break

- `hxn-abi` gains no dependency and no exported symbol.
- `hxn-future` gains no `unsafe`.
- A library's core is plain Rust with no runtime in it, and never selects a
  runtime with a cargo feature.
- Native code does not call a host function to report a result, does not keep
  a host function alive, and does not start threads or block in a build
  without threads.
- Nothing unwinds across the boundary. A runtime's exception is thrown by the
  adapter after the library has returned.

## Documents, comments, commits and issues

- **Keep the documentation current.** When an approach or a rule changes,
  update the relevant guide under `docs/` in the same commit. Keep the
  indexes current when adding or moving pages.
  These pages describe the current design, not its history.
- **No planning, backlog or status documents.** History, measurements,
  alternatives weighed and open questions go in `git-bug` issues. Look at
  `git-bug bug` before starting on an area: the area's issue records what is
  known and what is still open.
- **Comments** describe what the code does, and why it is shaped that way
  when that is not obvious. No account of what was tried, and no measurements.
- **Commit messages** are short: what changed, and a few sentences of why
  when that is not self-evident. Cite the issue in a trailer with its full id:

      git-bug: <64-character id>

  Do not write `Closes` or `Fixes`. Close the issue in `git-bug`.
- **No AI attribution** in commits, pull requests, issues or changelogs: no
  `Co-Authored-By` lines naming an assistant, no session links, no
  "generated with" footers.
- Commit or push only when asked. `git push` does not carry issues;
  `git-bug push` does, and also only when asked.

## Layout

| Path | What it is |
|---|---|
| `crates/hxn` | The crate a library depends on: the marker attributes. |
| `crates/hxn-abi` | The runtime-neutral core: the types passed across the boundary, and their ownership. |
| `crates/hxn-hl` | HashLink support: handles with finalizers, string conversion, exceptions thrown after the Rust call returns. |
| `crates/hxn-future` | Futures, streams, signals, and running work with or without threads. |
| `crates/hxn-gen` | Parses a library's annotated source and generates its adapters, C header and Haxe classes. |
| `crates/hxn-c` | Imports a C header as WebIDL, and generates a Rust crate from the WebIDL. |
| `crates/hxn-cli` | The `hxn` command: creates a library, builds its binding for a runtime, and runs its Haxe program. |
| `include/hxn.h` | The `hxn-abi` layouts as C declarations, and the helpers the hxcpp glue uses. |
| `haxe/hxn` | The Haxe side: `Future`, `Stream`, `Signal`, `Computed`, `Reactive`, `Inbox`, `Loop`, `Callback`, `AtomicInt`, `AtomicBool`, `Gc`, `Unit`, `Wire`, and the macro `Bind`. |
| `examples` | Small example libraries: each a crate, a Haxe program and its expected output. |
| `tests/counter` | The test fixture: a small library with bindings for every runtime, and the Haxe test that runs on all of them. |
| `tests/cli` | Creates a library with `hxn new`, runs it as the readme describes, and packages it. |
| `tests/examples` | Runs each example and compares its output with the expected output. |
| `tests/mobile` | Runs each example in an iOS simulator and on Android. |
| `tests/cimport` | A small C library imported with `hxn import`, and the Haxe program run against it. |

`internal/` is not part of the repository.
