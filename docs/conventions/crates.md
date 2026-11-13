# Crate boundaries

[Design](../design.md) · [Conventions](../conventions.md)

- `hxn-abi` has no dependencies or exported symbols. Libraries re-export
  host-callable functions under their own names.
- `hxn-future` keeps `#![forbid(unsafe_code)]`.
- Only `hxn-hl` imports runtime symbols. Its Ash wasm build supplies the two
  missing libunwind functions. Fallible primitives use `extern "C-unwind"`.
- Library cores depend on `hxn`, optionally `hxn-future`, and no runtime.
  Runtime selection belongs in adapters, never core features. Adapter crates
  contain generated code only.
- Build output goes under `target/hxn/<library>`; only the manifest goes in
  the library directory.
- Add new Haxe support classes/headers to the CLI's embedded-file list.

See [architecture](../design/architecture.md) and [generation](../design/generation.md).
