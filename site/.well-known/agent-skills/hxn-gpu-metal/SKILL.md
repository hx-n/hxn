---
name: hxn-gpu-metal
description: Drive the GPU from Haxe through Apple's Metal with the hxn-gpu-metal haxelib (package `gpu.metal`) on Ash, HashLink, hxcpp and CPPIA, on macOS and iOS. Use when a Haxe program runs compute shaders, renders to a texture or a window, or uses shaders written in HXSL with hxnsl on Metal.
---

# hxn-gpu-metal

## Setup

```
haxelib install hxn-gpu-metal
haxelib install hxnsl
haxe -lib hxn-gpu-metal -lib hxnsl -main Main -hl main.hl     # Ash or HashLink
haxe -lib hxn-gpu-metal -lib hxnsl -main Main -cpp out        # hxcpp
```

Prebuilt for macOS on Apple silicon (HashLink and hxcpp), Intel Macs (hxcpp
only), iOS and the iOS simulator. A program needs no Rust.

## Use

Write the shader as an hxnsl class implementing `gpu.metal.Shader`; it gets
`MSL`, `MSL_SLOTS`, and `metallib()`, the shader compiled to a Metal library
during the build (null where the build machine has no Metal compiler). Load
it with `gpu.metal.Shaders.library(device, Shader.metallib(), Shader.MSL)`,
which compiles the MSL only when there is no metallib.

```haxe
import gpu.metal.*;

var device = Device.systemDefault();             // throws without a Metal device
var queue = device.newCommandQueue();
var library = device.newLibraryFromSource(Tripled.MSL);
var pipeline = device.newComputePipeline(library.functionNamed("main_"));
var data = device.newBufferWith(bytes);

var commands = queue.commandBuffer();
var encoder = commands.computeEncoder();
encoder.setPipeline(pipeline);
encoder.setBuffer(data, 0, slot(Tripled.MSL_SLOTS, "main", Tripled.BUFFER_data_GROUP, Tripled.BUFFER_data));
encoder.dispatchThreads(64, 1, 1, 16, 1, 1);     // threads, then threadgroup size
encoder.end();
commands.completed().then(_ -> use(data.read(0, 64 * 4)));   // before commit()
commands.commit();
// or, blocking: commands.commit(); commands.waitUntilCompleted();
```

where `slot` finds the entry point in `MSL_SLOTS` by its WGSL name and returns
`Lambda.find(entry.resources, r -> r.group == group && r.binding == binding).index`.

Render: `device.newTexture({width, height, format: PixelFormat.Rgba8Unorm, usage: TextureUsage.RenderTarget | TextureUsage.ShaderRead, storage: StorageMode.Shared})`,
`device.newRenderPipeline(vertexFn, fragmentFn, format, attributes, layouts)`,
then `commands.renderEncoder(target, LoadAction.Clear, r, g, b, a)`,
`setPipeline`, `setVertexBuffer`, `setFragmentBytes`/`setFragmentBuffer`,
`setFragmentTexture`, `setFragmentSampler`, `draw(Primitive.TriangleStrip, 0, 4, 1)`, `end()`.
`texture.read(bytesPerRow)` reads a shared texture back.

## Rules

- MSL function names: hxnsl's `vertex`, `fragment` and `main` are `vertex_`,
  `fragment_` and `main_`. `library.functionNames()` lists them.
- Bind each resource at the index `MSL_SLOTS` gives for that entry point; do
  not hard-code indices.
- A stage with a storage buffer has `sizesBuffer` in its `MSL_SLOTS` entry:
  bind the runtime-sized arrays' byte lengths there as 32-bit ints with
  `setBytes`.
- Vertex buffers share the buffer table: bind them at an index no resource
  uses (30 works), and describe them with `VertexAttribute`
  (`location: Shader.INPUT_<name>`, `format`, `offset`, `buffer`) and
  `VertexLayout` (`buffer`, `stride`, `perInstance`).
- Every object holds a Metal object until `close()`. Close encoders,
  command buffers, buffers, textures, pipelines and libraries when done.
- `end()` each encoder before the next encoder or `commit()`.
  `waitUntilCompleted()` blocks, and throws if the GPU reported an error.
- Without blocking: `var done = commands.completed();` before `commit()`, then
  `done.then(...)` or `done.await()`. `newLibraryFromSourceAsync`,
  `newComputePipelineAsync` and `newRenderPipelineAsync` return futures of the
  object. `then` callbacks run through `hxn.Loop`; a custom frame loop sets
  `hxn.Loop.manual = true` and calls `hxn.Loop.sync()` each frame.
- Uniform values: fill `Bytes.alloc(Shader.PARAMS_SIZE)` at the
  `PARAMS_<name>` byte offsets and bind with `setFragmentBytes` or
  `setVertexBytes` (up to 4 KiB), or a buffer.

## Windows

`Layer.attach(device, window.raw(0), PixelFormat.Bgra8Unorm)` with an
hxn-window `Window` on macOS (AppKit) or iOS (UIKit), on the main thread.
Each frame: `layer.nextDrawable()`, render to `drawable.texture()`,
`commands.present(drawable)`, `commit()`. `setDrawableSize` on resize.

## Reference

The generated classes are committed under `binding/gpu/metal/` in
https://github.com/hx-n/hxn-gpu-metal. `examples/check/Main.hx` is a complete
compute and render program.

## Working on this repository

Test with the hxn CLI and hxnsl: `hxn check hl --vm ash` and
`hxn check cpp cppia` run `examples/check` and compare output across runtimes.
