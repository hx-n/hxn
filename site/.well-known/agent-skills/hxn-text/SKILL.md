---
name: hxn-text
description: Use or change hxn-text fonts, HarfRust-shaped glyph runs, glyph rasterization and revisioned atlas data from Haxe on native, mobile and browser runtimes. Use for text measurements, font ownership, font assets and integration with an application renderer.
---

# hxn-text

Use `-lib hxn-text` and pass bytes from hxn-file, an annotated hxn-assets field
or another application loader to `new hxn.text.Font(bytes, {})`. The prebuilt
Haxelib contains generated bindings, headers and native binaries.
Haxe applications do not build Rust. Keep binding/, include/, haxelib.json and
extraParams.hxml in the package root and refresh them with hxn package when
changing the Rust API. CI checks generated drift and ships native/<triple>
inside release ZIPs; native binaries remain ignored in the source repository.
Installed-ZIP checks disable Rust and use no producer source tree.
Keep wasm producers on the common pinned Linux/toolchain setup used by the
other hxn SDKs, and test composed installed libraries before release.
Font fetching, CSS,
UI/container layout, scene composition and drawing stay with the consumer.

For font assets add `-lib hxn-assets`. Declare a typed Font field with
`@:asset`, `@:dynamicAsset` and `@:assetDecode(hxn.text.FontAssets.decode)`.
Known source bytes can opt in to `@:contentAddressed`; native font handles are
created at runtime. Read the field after the generated `load(loader)` succeeds.
`FontAssets.load(loader, address, options)` returns a new caller-owned font and
snapshots face/weight selection before asynchronous delivery. Keep transport,
content verification and byte caching in AssetLoader. Generated `unload()` does
not close native fonts: close them before clearing their fields. Loader cache
eviction does not invalidate a decoded font. The
[font asset example](https://github.com/hx-n/hxn-text/tree/main/examples/assets)
uses the normal installed packages with FileSource and asynchronous loading.

The engine is shared `blinc_text`, not a copy of blinc_abi. Shared shaping and
rasterization fixes and their regressions belong in Blinc. HarfRust is the
intended shaping backend; keep its collection index and variable axes consistent
with the rasterizer. Pin the public engine revision before publication.

- `Font.shape` returns glyph IDs and pixel advances/offsets for one segment.
  Clusters are UTF-8 byte offsets, not Haxe string indexes. Do not assume
  they increase for right-to-left text or identify one character each.
- `Font.rasterize` returns an owned coverage or straight-RGBA glyph bitmap.
  `TextEngine.prepare` returns logical geometry and atlas pixel rectangles.
  Applications own the textures and shader. Device scale changes raster size.
- Pass each texture consumer's own revision to `engine.atlas`. Apply the
  packed rows at the given offset. Zero requests a complete snapshot, an
  unchanged revision returns no bytes, and old revisions can require a full
  upload. Normalize pixel coordinates with the current texture dimensions.
- Compare `engine.generation` before reusing prepared rectangles after `clear`
  or `clearFonts`. Clearing is explicit; atlas exhaustion does not silently
  invalidate the application's batches. Engines retain prepared fonts until
  release or clearFonts, even after a caller closes its font handle.
- Browser programs provide bundled or fetched fonts. Native font discovery,
  Apple outline/color handling and fallback need actual platform validation.
  Whole-paragraph bidi and arbitrary script fallback are not established by
  a single-segment shaping test. Keep their acceptance separate and explicit.
- Use the shared `hxn.Log` sink for diagnostics. Libraries do not change
  severity or sink configuration. Keep test hosts and runners under tests.

Read the [API contract](https://github.com/hx-n/hxn-text/blob/main/docs/design.md),
[conventions](https://github.com/hx-n/hxn-text/blob/main/docs/conventions.md) and
[testing guide](https://github.com/hx-n/hxn-text/blob/main/tests/README.md).
Native packages retain applicable dependency notices and font licenses.
