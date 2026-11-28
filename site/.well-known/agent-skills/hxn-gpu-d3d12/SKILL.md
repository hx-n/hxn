---
name: hxn-gpu-d3d12
description: Drive the GPU from Haxe through Direct3D 12 with the hxn-gpu-d3d12 haxelib (package `hxn.gpu.d3d12`) on Ash, HashLink, hxcpp and CPPIA, on Windows. Use when a Haxe program runs compute shaders, renders, samples textures or presents to a window with Direct3D 12, records resource transitions or enhanced barriers, or uses HXSL shaders from hxnsl as HLSL or DXIL.
---

# hxn-gpu-d3d12

## Setup

```
haxelib install hxn-gpu-d3d12
haxelib install hxnsl
haxe -lib hxn-gpu-d3d12 -lib hxnsl -main Main -hl main.hl     # Ash or HashLink
haxe -lib hxn-gpu-d3d12 -lib hxnsl -main Main -cpp out        # hxcpp
```

Windows x86-64; a program needs no Rust. With DXC 1.8+ on `PATH` (or
`HXN_GPU_D3D12_DXC` set to it) each entry point compiles to
DXIL at compile time; without it compilation warns "hxn-gpu-d3d12: no DXC" and the HLSL is
compiled at load time.

## Use

Shaders are hxnsl classes implementing `hxn.gpu.d3d12.Shader`; they get `HLSL`,
`HLSL_SLOTS`, `LAYOUT` and `dxil(entry)`. Entry points are `vertex_main`,
`fragment_main` and `compute_main`; stages are `"vertex"`, `"fragment"` and
`"compute"`.

```haxe
import hxn.gpu.d3d12.*;

var factory = Factory.create();
var device = factory.createDevice(0);            // isSoftware(i) tells WARP apart
var cs = hxn.gpu.d3d12.Shaders.bytecode(device, Tripled.dxil("compute_main"), Tripled.HLSL, "compute_main", "compute");
var layout = new RootLayout(Tripled.HLSL_SLOTS, Tripled.LAYOUT);
var signature = device.createRootSignature(layout.parameters);
var pipeline = device.createComputePipeline(cs, signature);
var upload = device.createBufferWith(bytes);                         // upload heap
var data = device.createBuffer(n, Heap.Default, State.Common, true); // allows UAV
var readback = device.createBuffer(n, Heap.Readback, 0, false);

var commands = device.createCommandList();       // open for recording
commands.transitionBuffer(data, State.Common, State.CopyDest);
commands.copyBuffer(data, upload);
commands.transitionBuffer(data, State.CopyDest, State.UnorderedAccess);
commands.setComputeRootSignature(signature);
commands.setPipelineState(pipeline);
commands.setComputeRootBuffer(layout.buffer(group, binding), ParameterKind.UnorderedAccess, data);
commands.dispatch(groups, 1, 1);
commands.transitionBuffer(data, State.UnorderedAccess, State.CopySource);
commands.copyBuffer(readback, data);
device.execute(commands).then(_ -> use(readback.read(0, n)));   // or executeAndWait
```

Render: `createTexture({width, height, format: Format.Rgba8Unorm, usage: TextureUsage.RenderTarget})`,
`transitionTexture(t, State.Common, State.RenderTarget)`, `setRenderTarget(t, false, r, g, b, a)`
(clears, or with `true` keeps what the target holds, and sets viewport and scissor),
`setGraphicsRootSignature`, `setPipelineState`,
`setGraphicsRootBuffer(index, ParameterKind.ConstantBuffer, uniforms)`, `setTopology`,
`setVertexBuffer(0, vertices, stride, offset)`, `draw(count, instances, first)`; then
`transitionTexture(t, State.RenderTarget, State.CopySource)` and
`copyTextureToBuffer(t, readback)`, whose rows are `t.copyPitch()` bytes apart. `setViewport(x, y, width, height)` and `setScissor(x, y, width, height)` take
pixels from the target's top-left corner. Each `VertexAttribute` with `perInstance` reads its
slot once per instance. Several targets and depth: `device.createGraphicsPipelineWith(vertex, pixel, signature,
targets, hasDepth, depthFormat, compare, write, topology, attributes)` takes `ColorTarget`
records and compare codes 0 `Never` to 7 `Always`; a `Format.D32Float` texture with
`TextureUsage.DepthStencil` has a depth view; `RenderTargets.create()` with
`color(texture, keep, r, g, b, a)` and `depth(texture, keep, clear)` feeds
`commands.setRenderTargets(targets)`, with depth textures in `State.DepthWrite`.

Textures and samplers:

```haxe
var image = device.createTexture({width: w, height: h, format: Format.Rgba8Unorm, usage: 0});
// staging: an upload buffer with rows image.copyPitch() bytes apart
commands.transitionTexture(image, State.Common, State.CopyDest);
commands.copyBufferToTexture(image, staging);
commands.transitionTexture(image, State.CopyDest, State.PixelShaderResource);
// part of a texture: rows of `width` pixels, each rounded up to 256 bytes
commands.copyBufferToTextureRegion(image, staging, x, y, width, height);

var resources = device.createDescriptorHeap(HeapKind.Resources, layout.textureCount(group));
resources.writeTexture(layout.textureOffset(group, Shader.TEXTURE_image), image);
var samplers = device.createDescriptorHeap(HeapKind.Samplers, 1);
samplers.writeSampler(0, Filter.Linear, AddressMode.Clamp);
// Maps each sampler binding to its slot in the sampler heap.
var indices = device.createBufferWith(RootLayout.samplerIndexArray([samplerBinding => 0]));

commands.setDescriptorHeaps(resources, samplers);
commands.setGraphicsRootTable(layout.textureTable(group), resources, 0);
commands.setGraphicsRootTable(layout.samplerHeap, samplers, 0);
commands.setGraphicsRootTable(layout.comparisonSamplerHeap, samplers, 0);
commands.setGraphicsRootBuffer(layout.samplerIndices(group), ParameterKind.ShaderResource, indices);
```

Present: `factory.createSwapchain(device, window.raw(0), w, h, Format.Bgra8Unorm, 2)`
with an hxn-window window. Each frame: `var t = chain.buffer(chain.currentIndex())`,
`transitionTexture(t, State.Common, State.RenderTarget)`, render,
`transitionTexture(t, State.RenderTarget, State.Common)`, execute, then
`chain.present(1)` (vertical blanks to wait; 0 presents at once).

## Blending

`device.createGraphicsPipeline` takes a `Blend` record last: `enabled`, then
the operation and the source and destination factors of the color and of the
alpha. The factors are hxn-gpu's shared codes: 0 `Zero`, 1 `One`, 2 `Src`,
3 `OneMinusSrc`, 4 `SrcAlpha`, 5 `OneMinusSrcAlpha`, 6 `Dst`, 7 `OneMinusDst`,
8 `DstAlpha`, 9 `OneMinusDstAlpha` and 10 `SrcAlphaSaturated`. The operations
are 0 `Add`, 1 `Subtract`, 2 `ReverseSubtract`, 3 `Min` and 4 `Max`. Straight
alpha is `{enabled: true, colorOperation: 0, colorSource: 4, colorDestination: 5,
alphaOperation: 0, alphaSource: 1, alphaDestination: 5}`, and `enabled: false`
writes the color as it is. For the alpha, a color factor such as `Src` means the
matching alpha factor, because Direct3D 12 takes no color factor there.

## Rules

- Synchronization is explicit: transition a resource into the state each
  use needs before that use. `uavBarrier` between unordered-access writes and
  later unordered access of the same buffer.
- Where `device.supportsEnhancedBarriers()`, `bufferBarrier(b, syncBefore,
  accessBefore, syncAfter, accessAfter)` and `textureBarrier(t, syncBefore,
  accessBefore, layoutBefore, syncAfter, accessAfter, layoutAfter)` replace
  transitions; a resource's first barrier starts from `BarrierSync.None`,
  `Access.NoAccess`.
- Buffers bind as root descriptors: uniform blocks as `ConstantBuffer`,
  storage buffers as `UnorderedAccess`, read-only storage as
  `ShaderResource`. Textures bind through a descriptor table per group;
  samplers through the sampler heap tables plus the group's index array.
- `setDescriptorHeaps` before any `set*RootTable`; at most one heap of each kind.
- Upload buffers are written, readback buffers read; default-heap buffers
  only through copies.
- A command list is recording until `execute`; after that, make a new one.
- Every object holds a Direct3D 12 object until `close()`.
- `execute`'s future delivers through `hxn.Loop`; a custom frame loop sets
  `hxn.Loop.manual = true` and calls `hxn.Loop.sync()` each frame.

## Reference

The generated classes are committed under `binding/gpu/d3d12/` in
https://github.com/hx-n/hxn-gpu-d3d12. `examples/check/Main.hx` is a
complete compute, render, texture-sampling and enhanced-barrier program;
`examples/present/Main.hx` presents to a window.

## Working on this repository

Windows only. Test with the hxn CLI, hxnsl and DXC: `hxn check hl --vm ash`
and `hxn check cpp cppia`. On another platform, `cargo check --target
x86_64-pc-windows-msvc` and `hxn build hl --target x86_64-pc-windows-msvc`
compile it.

## Redistribution

Retain the applicable [dependency licenses and notices](https://github.com/hx-n/hxn-gpu-d3d12/blob/main/third-party/NOTICES.md)
when distributing binaries. Release packages include `third-party/`.

## Logging

Use core `hxn.Log` / `hxn::log`, with stable target `hxn.gpu.d3d12`.
The application owns levels and the shared sink; libraries never replace it.
Generated adapters deliver native records after calls return. Custom frame loops
call `hxn.Loop.sync()`; otherwise idle background diagnostics need `hxn.Log.flush()`.
Keep real-time callbacks free of logging and leave returned errors as exceptions.
Read the [core logging contract](https://github.com/hx-n/hxn/blob/main/docs/design/logging.md).
