---
name: hxn-gpu-webgpu
description: Drive the GPU from Haxe in the browser through WebGPU with the hxn-gpu-webgpu haxelib (package `hxn.gpu.webgpu`), for hxcpp programs compiled to wasm with `-D emscripten`, threaded or not, and threaded Ash programs (`-D wasm -D hxn_threads`); HashLink and CPPIA do not run in a browser. Use when a Haxe program in a web page runs compute shaders or renders with WebGPU, reads buffers back, or uses HXSL shaders from hxnsl as WGSL.
---

# hxn-gpu-webgpu

## Setup

```
haxelib install hxn-gpu-webgpu
haxelib install hxnsl
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -cpp out -D emscripten -D disable_unicode_strings -D HXCPP_LINK_EMSCRIPTEN_EXT=.html
```

Needs the Emscripten SDK on `PATH` and hxcpp from git (its Emscripten
toolchain uses wasm exceptions). Serve `out/` and open `Main.html` in a
browser with WebGPU. Add `-D hxn_threads` for threads: `main` then runs on a
pthread that holds the canvas, and the web page needs cross-origin isolation
headers (COOP/COEP), which `emrun out/Main.html` sends.

Ash (0.1.20 or later), threaded:

```
haxe -lib hxn-gpu-webgpu -lib hxnsl -main Main -hl out/main.hl -D wasm -D hxn_threads
ash --build out/main.wasm --target wasm32-wasip1-threads --native-lib out/libgpu_webgpu.a out/main.hl
ash serve out          # sends the cross-origin isolation headers shared memory needs
```

The compile copies the library, `libgpu_webgpu.a`, and the browser agent's
`.mjs` files beside `main.hl`. The haxelib carries both builds prebuilt; no
Rust needed.

## Use

Shaders are hxnsl classes implementing `hxn.gpu.webgpu.Shader`; they get `WGSL`
and `LAYOUT` only. Entry points are `vertex_main`, `fragment_main` and
`compute_main`. A texture's sampler is at `TEXTURE_<name> + 1`.

```haxe
import hxn.gpu.webgpu.*;

Instance.create().requestAdapter().then(adapter -> adapter.requestDevice().then(device -> {
	var module = device.createShaderModule(Tinted.WGSL);
	var layouts = hxn.gpu.webgpu.Shaders.bindGroupLayouts(device, Tinted.LAYOUT);
	var layout = device.createPipelineLayout(layouts);
	var pipeline = device.createRenderPipeline(module, "vertex_main", "fragment_main", layout,
		Format.Rgba8Unorm, Topology.TriangleStrip, [{stride: 8, perInstance: false}],
		[{location: Tinted.INPUT_position, format: VertexFormat.Float32x2, offset: 0, buffer: 0}], [], blend);
	var target = device.createTexture(w, h, Format.Rgba8Unorm, TextureUsage.RenderAttachment | TextureUsage.CopySrc);
	var uniforms = device.createBufferWith(params, BufferUsage.Uniform);   // padded to 4 bytes
	var entries = BindGroupEntries.create();
	entries.buffer(Tinted.PARAMS_BINDING, uniforms, 0, -1);                // -1: the rest
	var group = device.createBindGroup(layouts, Tinted.PARAMS_GROUP, entries);

	var encoder = device.createCommandEncoder();
	var pass = encoder.beginRenderPass(target, false, 0, 0, 0, 1);        // cleared first
	pass.setPipeline(pipeline);
	pass.setBindGroup(Tinted.PARAMS_GROUP, group);
	pass.setVertexBuffer(0, vertices, 0);                                   // BufferUsage.Vertex
	pass.draw(4, 1, 0, 0);
	pass.end();
	encoder.copyTextureToBuffer(target, readback, 256);                     // rows 256 bytes apart
	device.submit(encoder);
	readback.read(0, 256 * h).then(pixels -> use(pixels));                  // BufferUsage.MapRead
}));
```

Canvas: claim it, `instance.claimCanvas().then(...)`, which fails when another
browser agent holds it, then `var surface = instance.createCanvasSurface()` and
`surface.configure(device, surface.preferredFormat(adapter), TextureUsage.RenderAttachment, w, h)`;
each frame, render into `surface.currentTexture()` and call `surface.present()`.
Without threads the browser also shows the frame when the program returns to its
event loop, so read a frame back before then.

Compute: `createComputePipeline(module, "compute_main", layout, constants)`,
`encoder.beginComputePass()`, `setPipeline`, `setBindGroup`,
`dispatch(x, y, z)`, `end()`. Textures: `device.writeTexture(texture, bytes,
bytesPerRow)`, or `device.writeTextureRegion(texture, x, y, width, height, bytes,
bytesPerRow)` for part of one, `device.createSampler(Filter.Linear, AddressMode.ClampToEdge)`,
`entries.texture(binding, t)`, `entries.sampler(binding + 1, s)`.

Vertex buffers: `createRenderPipeline` takes one `VertexBuffer` record per buffer,
`{stride, perInstance}`, and each attribute's `buffer` is the index of the buffer it
reads. `pass.setVertexBuffer(slot, buffer, offset)` starts `offset` bytes in, so one
buffer can hold several batches of instances. `beginRenderPass(target, true, ...)`
keeps what the target holds (`loadOp: "load"`) and ignores the color.
`pass.setViewport(x, y, width, height)` and `pass.setScissorRect(x, y, width,
height)` take pixels from the target's top-left corner. Several targets and depth: `createRenderPipelineWith(module, vertexEntry, fragmentEntry,
layout, targets, hasDepth, depthFormat, compare, write, topology, buffers, attributes,
constants)` takes `ColorTarget` records and compare codes 0 `"never"` to 7 `"always"`;
`RenderTargets.create()` with `color(texture, keep, r, g, b, a)` and
`depth(texture, keep, clear)` feeds `encoder.beginRenderPassWith(targets)`.

## Blending

`device.createRenderPipeline` takes a `Blend` record last, which becomes the
color target's `GPUBlendState`: `enabled`, then the operation and the source
and destination factors of the color and of the alpha. The factors are hxn-gpu's shared codes: 0 `Zero`, 1 `One`, 2 `Src`,
3 `OneMinusSrc`, 4 `SrcAlpha`, 5 `OneMinusSrcAlpha`, 6 `Dst`, 7 `OneMinusDst`,
8 `DstAlpha`, 9 `OneMinusDstAlpha` and 10 `SrcAlphaSaturated`. The operations
are 0 `Add`, 1 `Subtract`, 2 `ReverseSubtract`, 3 `Min` and 4 `Max`. Straight
alpha is `{enabled: true, colorOperation: 0, colorSource: 4, colorDestination: 5,
alphaOperation: 0, alphaSource: 1, alphaDestination: 5}`, and `enabled: false`
writes the color as it is.

## Rules

- In a threaded build `await` works (under Ash, from 0.1.21): WebGPU runs in
  a browser agent, a Worker of its own. Without threads, deliver a future with
  `then`, never `await`: the browser resolves WebGPU's promises on the
  program's thread, and only while that thread is not blocked.
- Synchronization is WebGPU's: no barriers. `device.completed()` settles when
  submitted work has finished.
- `Device.submit(encoder)` finishes the encoder; record a new one after.
  End each pass before the next command on its encoder.
- Copies from textures need rows a multiple of 256 bytes apart; buffer sizes
  and `writeBuffer` lengths a multiple of 4.
- Unthreaded hxcpp routes uncaptured validation/device-loss diagnostics through
  `hxn.Log`. Threaded agent diagnostics remain browser-owned. Failures making
  objects are thrown.
- Every object holds its WebGPU object until `close()`.

## Reference

The generated classes are committed under `binding/gpu/webgpu/` in
https://github.com/hx-n/hxn-gpu-webgpu. `examples/check/Main.hx` computes,
renders and samples a texture, reading each result back.

## Working on this repository

Test with the hxn CLI, hxnsl, the Emscripten SDK, Ash and Chrome, with
`HXN_BROWSER_ARGS=--enable-unsafe-webgpu`: `hxn check cpp --wasm --browser`,
`hxn check cpp --wasm --threads --browser` and
`hxn check hl --wasm --threads --browser`.
`src/ffi.rs` is generated from emdawnwebgpu's webgpu.h by bindgen.

## Redistribution

Retain the applicable [dependency licenses and notices](https://github.com/hx-n/hxn-gpu-webgpu/blob/main/third-party/NOTICES.md)
when distributing binaries. Release packages include `third-party/`.

## Logging

Use core `hxn.Log` / `hxn::log`, with stable target `hxn.gpu.webgpu`.
The application owns levels and the shared sink; libraries never replace it.
Generated adapters deliver native records after calls return. Custom frame loops
call `hxn.Loop.sync()`; otherwise idle background diagnostics need `hxn.Log.flush()`.
Keep real-time callbacks free of logging and leave returned errors as exceptions.
Read the [core logging contract](https://github.com/hx-n/hxn/blob/main/docs/design/logging.md).
