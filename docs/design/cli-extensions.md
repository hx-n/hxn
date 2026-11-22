# CLI extensions

[Design](../design.md) · [CLI usage](../cli.md)

An installed haxelib of the hxn family, named `hxn-*` or `hxn*` such as
`hxnme`, can provide a namespace under the hxn command and participate in
its build, run and package lifecycle. The namespace defaults to the name
after `hxn-` or `hxn`: `window` for hxn-window, `me` for hxnme. `hxn plugins`
lists the registrations in the active haxelib repository. A namespaced
command does not require a Rust crate in the current directory.

## Registration

A package declares `hxn.cli` in `haxelib.json`:

```json
{
  "name": "hxn-assets",
  "hxn": {
    "cli": {
      "version": 1,
      "namespace": "assets",
      "command": ["haxe", "-cp", "{root}/sources", "--run", "assets.Tool"],
      "hooks": ["before-build", "after-package"]
    }
  }
}
```

`hxn assets ...` starts that command. Each element is one argument;
`{root}` expands to the installed package directory. No shell interprets
it. A Haxe tool can ship in `sources/`; a native executable can use the
same protocol. Built-in command names are reserved. Two registrations for
one namespace are an error. Discovery reads metadata without running tools.

A Rust library puts the same fields under `[package.metadata.hxn.cli]`.
`hxn package` carries the registration into the haxelib and copies its
`sources/` as usual. The source library's own registration works before it
has been installed. An optional `files` array lists additional relative tool
files or directories to include in the package. These are copied as bytes
with executable permissions preserved, so native tools can be shipped too;
absolute paths and parent traversal are rejected.

## Requests and replies

The process receives one JSON request on standard input, followed by EOF.
Every request has `version: 1`, `kind`, `library` and `root`.

A command request has `kind: "command"`, `cwd`, and an `arguments` array
containing everything after the namespace. The process runs in `cwd`;
its output and exit status are the command's output and exit status.

A hook request has `kind: "hook"`, `phase`, and `context`. Context identifies
the CLI command, the source project's directory, library, haxelib and Cargo
target directory. For a [program file](../cli.md#program-files), the
project's `library` and `haxelib` are null, `hxml` is the path of the file,
and the target directory is the file's `bin/<file name>`. Build/run targets include `runtime`, `wasm`, `threads`
and `triple`. `output` names the build or package directory when available;
`status` contains the program's exit code after a run.

Hooks log to standard error and return a JSON object on standard output:

```json
{
  "version": 1,
  "haxe_args": [["-D", "assets_ready"], ["-cp", "/generated/classes"]],
  "page_files": ["/generated/assets_agent.mjs"]
}
```

A build hook may also return `ash_args`, options for Ash. Each entry is a
flag with or without its value, such as `["--preset", "game"]` or
`["--no-fma"]`. hxn passes them to every Ash command a HashLink build or run
uses: the build ahead of time, the wasm build, and a VM named `ash`. A flag
already given with `--ash` or `--vm` is left out, so the command line wins.

Only build hooks may contribute `haxe_args`, `ash_args` and `page_files`.
Haxe flags are `-D`, `-cp` and `--macro`; runtime, main class and output
selection stay with hxn. Arguments reach both the Haxe invocation and `build.hxml`; CPPIA scripts
receive the plugin defines used by their compiled host.
Relative page-file paths refer to the source project. Files are copied
beside wasm output and into the package's `page/`; conflicting contents
under the same filename are an error. Runtime defaults in a haxelib still
belong in its `init` macros, rather than transient build-hook arguments.
A hook with no contributions returns `{"version":1}`.

## Lifecycle

Hooks run only for the source library and haxelibs selected by its
`haxe-flags` (`-lib`, `-L` or `--library`), including their resolved
dependencies. For a program file, they run for the haxelibs that the file
selects.
Installing a library alone does not activate its hooks. Providers run in
haxelib-name order; the source registration takes precedence over an
installed copy of that same library.

- `before-check` prepares generated sources before the committed API manifest
  is compared with the source.
- `before-build` runs before generation and compilation. Its contributions
  are applied to the resulting build.
- `after-build` receives the completed binding directory.
- `before-run` runs after compilation, before the program starts.
- `after-run` receives the program's exit status, including a nonzero exit.
- `before-package` and `after-package` surround packaging. Each runtime
  packaged also gets its own build hooks.

`run` and each runtime in `check` use the same build and run hooks. A failed
hook stops the workflow and names the provider and phase. A later phase is
not called when the operation never reached it. Unknown metadata fields,
hooks, protocol versions or malformed replies fail explicitly.
