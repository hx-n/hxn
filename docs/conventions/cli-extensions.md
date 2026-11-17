# CLI extension rules

[Conventions](../conventions.md) · [Extension protocol](../design/cli-extensions.md)

- Put the registration in `[package.metadata.hxn.cli]` for a Rust library;
  let packaging carry it into `haxelib.json`. Ship the tool with the package,
  normally as Haxe under `sources/`, or declare native tool/assets paths in
  `files`. Consumers use `hxn <namespace>`, without
  a source checkout or repository shell helpers.
- Keep repository-only test runners, app hosts and fixtures under `tests/`,
  outside published extension commands and `files`. Run those tests against
  installed packages. A mobile test host does not define general hxn app
  packaging or deployment tooling.
- Keep namespaces library-owned. Do not replace built-in commands, load
  extensions from arbitrary PATH executables, or run unselected libraries'
  hooks. Discovery must read metadata without executing the extension.
- Use argv entries, with `{root}` for package paths. Do not pass a composed
  command string to a shell. Test paths and arguments containing spaces.
- Keep protocol handling in the CLI/tool boundary. Library Rust cores,
  runtime adapters and Haxe bindings do not depend on CLI plugins.
- Hooks write logs to stderr and one versioned JSON reply to stdout.
  Commands may print normal output. Preserve failure statuses and identify
  the failing provider and phase.
- Generate assets before the phase that consumes them. Hook page filenames
  must be namespaced so providers cannot overwrite each other's modules.
  Runtime defaults belong in haxelib init macros; build-hook flags affect
  that invocation only.
- Test installed-package discovery and command dispatch, hook ordering and
  activation, invalid versions/replies, failure propagation, packaging and
  every affected runtime. Run external processes under timeouts and repeat
  concurrent/browser work. Update the library and hxn skills with public
  commands when their workflow changes.
