# CLI usage

[Design](design.md) · [Conventions](conventions.md)

Run build, run, check and package commands in the library crate. See the
[README](../README.md#quick-setup) for installation and a first example.

## Commands

| Command | Use |
|---|---|
| `hxn new <name>` | Create a library and Haxe program from the greeter template. |
| `hxn build <hl\|cpp>` | Generate and build a binding. |
| `hxn run <hl\|cpp\|cppia>` | Build and run the Haxe program. |
| `hxn check [hl] [cpp] [cppia]` | Compare program output across runtimes; defaults to HashLink and hxcpp. |
| `hxn package [hl] [cpp]` | Create a [haxelib package](packaging.md). |
| `hxn doctor [--target <triple>]` | Check runtime prerequisites or a Rust binding target. |
| `hxn import <header.h>` | [Import a C library](design/c-import.md). |
| `hxn version` | Print the command version. |

## Targets and settings

- `--wasm` builds the selected runtime's wasm form. Ash requires nightly
  Rust with `rust-src`; Emscripten requires its SDK and compatible hxcpp.
- `--target <triple>` makes `build` and `package` build the binding for
  another platform, such as `aarch64-linux-android`, as a static library.
  Install that triple's Rust standard library first with
  `rustup target add <triple>`. hxn needs no linker for this; the
  platform's linker and SDK are used when the program is linked.

  hxcpp links the `cpp` library into the app. For HashLink, Ash links
  `lib<library>.a` into the program it builds for that platform:

  ```
  ash --build <program> --target <triple> --native-lib lib<library>.a main.hl
  ```

  `run` and `check` do not accept `--target`, because they run the program
  on this machine.
- `cppia` builds an hxcpp host and runs the program as a script. Arguments
  after the script name go to the program.
- Set the Haxe directory, main class and `haxe-flags` under
  `[package.metadata.hxn]`. Choose the VM and its options on the command
  line or through `HXN_VM`.

The command and library's `hxn` versions must match. `check` also requires
the committed API manifest to match the source before building.

`doctor --target` checks Cargo and the target's `std` archive in the active
rustc sysroot. It does not check the application's SDKs or dependencies.
