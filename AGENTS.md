# Working on hxn

A Rust library is written once and used from Haxe on HashLink, Ash and hxcpp,
natively and on wasm. Read these before changing anything:

- [docs/design.md](docs/design.md): how each part works and why.
- [docs/conventions.md](docs/conventions.md): the rules the code, the
  bindings and the tests follow.

## Before calling a change done

- **Run every runtime.** `cargo test --workspace` covers the pure crates only.
  The rest is `tests/counter/run.sh` in each of its modes: `hl`, `cpp`, `wasm`,
  `emcc`, `gen-hl`, `gen-cpp`, `gen-wasm`, `gen-emcc`. Run Ash in its hybrid,
  JIT and interpreter modes (`ash --mode jit`, `ash --mode interp`); they
  differ in how threads and native calls behave. `tests/cli/run.sh` checks
  the `hxn` command the way the readme uses it, and `tests/examples/run.sh`
  runs every library under `examples/`; both take `hl`, `cpp`, `wasm` and
  `emcc`, and a VM with its options, as in `hl "ash --mode jit"`.
- **Run runtime tests under a timeout.** A wrong wait hangs; it does not fail.
- **Repeat anything concurrent**, and on more than one machine if you can.
  A change to `hxn-abi` or `hxn-future` is also run under Miri
  (`cargo miri test -p hxn-abi`, `-p hxn-future`).
- **Windows is run by CI** (`.github/workflows/ci.yml`), with HashLink and
  hxcpp on Linux beside it. A change is not known to work there until that
  has run.
- **Say what was not run.** If a runtime was unavailable, report that, not a
  pass.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets` are clean.

## Rules that are easy to break

- `hxn-abi` gains no dependency and no exported symbol.
- `hxn-future` gains no `unsafe`.
- A library's core is plain Rust with no runtime in it, and never selects a
  runtime with a cargo feature.
- Native code does not call a host function to report a result, does not keep
  a host function alive, and does not start threads or block where the build
  has none.
- Nothing unwinds across the boundary. A runtime's exception is thrown by the
  adapter after the library has returned.

## Documents, comments, commits and issues

- **Keep the two documents current.** When an approach or a rule changes,
  change `docs/design.md` or `docs/conventions.md` in the same commit. They
  describe how hxn is, not its history.
- **No planning, backlog or status documents.** History, measurements,
  alternatives weighed and open questions go in `git-bug` issues. Look at
  `git-bug bug` before starting on an area: its issue says what is known and
  what is still open.
- **Comments** say what the code does, and why it is shaped that way when
  that is not obvious. No account of what was tried, and no measurements.
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
| `crates/hxn` | What a library depends on: the marker attributes. |
| `crates/hxn-abi` | The runtime-neutral core: what crosses the boundary, and who owns it. |
| `crates/hxn-hl` | The HashLink side: finalized handles, text, deferred throws. |
| `crates/hxn-future` | Futures, streams, signals, and running work with or without threads. |
| `crates/hxn-gen` | Reads a library's marked source and writes its adapters, header and Haxe classes. |
| `crates/hxn-cli` | The `hxn` command: makes a library, builds its binding for a runtime, runs its Haxe program. |
| `include/hxn.h` | The same layouts for C, and what hxcpp glue needs around them. |
| `haxe/hxn` | The Haxe side: `Future`, `Stream`, `Signal`, `Computed`, `Reactive`, `Inbox`, `Loop`, `Callback`, `AtomicInt`, `AtomicBool`, `Gc`. |
| `examples` | Small libraries to read and run: each a crate, a Haxe program and what it prints. |
| `tests/counter` | The fixture: a small library bound for every runtime, and the Haxe test they all run. |
| `tests/cli` | Makes a library with `hxn new` and runs it, as the readme says to. |
| `tests/examples` | Runs each example and compares what it prints. |

`internal/` is not part of the repository.
