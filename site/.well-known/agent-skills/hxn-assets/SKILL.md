---
name: hxn-assets
description: Use or change hxn-assets typed embedded and dynamic values, metadata catalogs, HTTP/WS streams, content identity, byte caching and the hxn assets CLI.
---

Author assets in a class with `@:build(hxn.assets.Assets.build())`.
Static fields use `@:asset("path")` or `@:asset("address", "path")` and describe
payload values: JSON Dynamic/anonymous structures/typedefs, String text, Bytes,
or portable ImageData PNG pixels. Known data is decoded and embedded at compile
time. Addresses are generated separately as `Class.addresses.Field`; `catalog()`
generates descriptors. Opt in to SHA-256 IDs with field/class `@:contentAddressed`.

Mark data unavailable or undecodable at compilation with `@:dynamicAsset`.
The declared payload type selects decoding and JSON shape validation; Dynamic
receives raw bytes for application decoding unless a format/decoder is explicit.
`@:assetFormat` selects a built-in format; `@:assetDecode(fn)` selects an
application Bytes-to-payload decoder. The generated `load(loader)` returns a
future and publishes the dynamic fields together. Read them after it completes;
`unload()` clears their bindings and supersedes pending loads. Keep loader-cache
invalidation separate when source revisions change. GPU textures need a device;
ImageData is CPU pixels, with PNG currently the built-in image codec.

Use `@:assetDependsOn(OtherField)` for dependencies and the group/label/media-type
metadata in the design guide. Consumers do not handwrite catalog records or
digests. Low-level catalog construction supports imports and validation tests.

For remote assets use dynamic HTTP(S) declarations and AssetStream<T> for WS(S).
RemoteSource also supports HTTP byte-chunk streams and a borrowed local source.
Expected stream payload types select per-message decoding. HTTP chunks have no
application record boundaries; keep them Bytes or supply explicit framing.
AssetStream.next returns a Future with separate done/value/text fields; mapped
views share one reader/connection. Close streams on unload, failed/superseded
loads or explicit cancellation. Keep queues and messages bounded, and never
perform URI fetches during compilation. Applications configure authentication
and reconnection; production TLS verification remains enabled.

Use AssetLoader for asynchronous byte loading and FileSource to borrow hxn.file.Storage. Custom
AssetSource and CatalogSource providers return hxn.Future. Loader-returned bytes
are owned copies; unloading must also invalidate stale in-flight cache writes.

The installed haxelib supplies `hxn assets catalog` and `hxn assets stage`, each
with `--source`, `--out` and optional `--content-addressed`. Compiler integration
uses `hxn_assets_dir` and stages the generated declaration catalog. Whole-directory
imports without declarations can use `hxn_assets_content_addressed`. Consumers
use hxn and installed haxelibs, without repository shell helpers.

Read the [asset API](https://github.com/hx-n/hxn-assets/blob/main/packages/hxn-assets/README.md),
[design](https://github.com/hx-n/hxn-assets/blob/main/docs/design.md) and
[conventions](https://github.com/hx-n/hxn-assets/blob/main/docs/conventions.md).
Use this repository's git-bug tracker. Graph projection annotations and the
hosted assets service remain separate issue plans; do not describe them as
implemented APIs or add graph/service requirements to basic asset loading.

For macro changes run the annotation suite under tests/assets/run.py, including
compilation-server reuse, asset edits and full DCE. Compare staged descriptors
with the generated runtime catalog. Normalize host filesystem separators before canonical containment comparisons,
including Windows compilation and staging. Preserve symlink escape rejection.
Asset files are compiler module dependencies;
retain that invalidation when changing macro generation or staging.

Retain the [codec license terms](https://github.com/hx-n/hxn-assets/blob/main/packages/hxn-assets/third-party/NOTICES.md)
when redistributing runtime decoders. Embedded-resource edits must invalidate
compiler-server caches just as descriptor/hash edits do.

Remote transport tests use local HTTP(S)/WS/WSS servers under tests/assets/remote.py.
Its temporary-CA/browser flag setup is test-only. CRDT assets (local durable edits,
causal merge, offline journals and remote sync) remain a repository-owned issue
plan; do not claim the transport itself implements a CRDT.

## Logging

Diagnostics use core `hxn.Log` with target `hxn.assets`.
The application owns `Log.level` and `Log.sink`; frame loops drain native
records at `hxn.Loop.sync()`, and idle background work uses `Log.flush()`.

## Repository CI

The [CI workflows](https://github.com/hx-n/hxn-assets/tree/main/.github/workflows)
run Rust/macro checks, native file/asset/remote fixtures on Linux, macOS and
Windows, and real Chrome pages in Ash and both hxcpp wasm modes. Reuse the
bounded runners under tests/ci; they are repository test tooling, not consumer
commands. For mobile bundle/APK reads, persistence across fresh app processes,
shared logging and real URI traffic, use the bounded
[mobile fixture](https://github.com/hx-n/hxn-assets/blob/main/tests/mobile/run.py)
with both hl (Ash AOT) and cpp on iOS simulator and Android emulator. Its app
hosts and Android context shim are test-only; keep them out of consumer tooling.
Physical-device lifecycle and network-transition checks are separate coverage.
