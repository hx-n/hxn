---
name: hxn-gpu-vulkan
description: Drive the GPU from Haxe through Vulkan with the hxn-gpu-vulkan haxelib (package `hxn.gpu.vulkan`) on Ash, HashLink, hxcpp and CPPIA, on Linux, Windows, Android and macOS through MoltenVK. Use when a Haxe program runs compute shaders or renders with Vulkan, records barriers and image layout transitions, or uses HXSL shaders from hxnsl as SPIR-V.
---

# hxn-gpu-vulkan

## Setup

```
haxelib install hxn-gpu-vulkan
haxelib install hxnsl
haxe -lib hxn-gpu-vulkan -lib hxnsl -main Main -hl main.hl     # Ash or HashLink
haxe -lib hxn-gpu-vulkan -lib hxnsl -main Main -cpp out        # hxcpp
```

Prebuilt for Linux x86-64 and ARM64, Windows x86-64 and Apple-silicon macOS
(HashLink and hxcpp), Intel Macs and Android (hxcpp). A program needs a
Vulkan driver and loader: on Linux `libvulkan1 mesa-vulkan-drivers`, on macOS
MoltenVK (set `DYLD_FALLBACK_LIBRARY_PATH=/usr/local/lib` if the loader is
not found).

## Use

Shaders are hxnsl classes implementing `hxn.gpu.vulkan.Shader`; they get
`spirv()` and `LAYOUT`. Entry points are `vertex_main`, `fragment_main` and
`compute_main`.

```haxe
import hxn.gpu.vulkan.*;

var instance = Instance.create("app");
var device = instance.createDevice(0);                 // instance.adapterNames() lists GPUs
var module = device.createShaderModule(Tripled.spirv());
var sets = hxn.gpu.vulkan.Shaders.setLayouts(device, Tripled.LAYOUT);  // one per bind group
var layout = device.createPipelineLayout(sets);
var pipeline = device.createComputePipeline(module, "compute_main", layout);
var data = device.createBufferWith(bytes, BufferUsage.Storage);
var set = device.createDescriptorSet(sets, Tripled.BUFFER_data_GROUP);
set.writeBuffer(Tripled.BUFFER_data, data, 0, bytes.length);

var commands = device.createCommandBuffer();          // already recording
commands.bindPipeline(BindPoint.Compute, pipeline);
commands.bindDescriptorSet(BindPoint.Compute, layout, Tripled.BUFFER_data_GROUP, set);
commands.dispatch(groups, 1, 1);
commands.bufferBarrier(data, Stage.ComputeShader, Access.ShaderWrite, Stage.Host, Access.HostRead);
device.submit(commands).then(_ -> use(data.read(0, bytes.length)));  // or submitAndWait
```

Render into an image (`createImage({width, height, format: Format.Rgba8Unorm, usage: ImageUsage.ColorAttachment | ImageUsage.TransferSrc})`):

```haxe
commands.imageBarrier(target, Stage.TopOfPipe, 0, Layout.Undefined,
	Stage.ColorAttachmentOutput, Access.ColorAttachmentWrite, Layout.ColorAttachment);
commands.beginRendering(target, LoadOp.Clear, 0, 0, 0, 1);   // sets viewport and scissor
commands.bindPipeline(BindPoint.Graphics, pipeline);         // createGraphicsPipeline(...)
commands.bindDescriptorSet(BindPoint.Graphics, layout, Shader.PARAMS_GROUP, set);
commands.bindVertexBuffer(0, vertices, 0);
commands.draw(4, 1, 0);                                      // count, instances, first
commands.endRendering();
commands.imageBarrier(target, Stage.ColorAttachmentOutput, Access.ColorAttachmentWrite, Layout.ColorAttachment,
	Stage.Transfer, Access.TransferRead, Layout.TransferSrc);
commands.copyImageToBuffer(target, readback);
commands.bufferBarrier(readback, Stage.Transfer, Access.TransferWrite, Stage.Host, Access.HostRead);
```

## Windows

`instance.createSurface(window.platform(), window.raw(0), window.raw(1), window.raw(2), window.raw(3))`
with an hxn-window window, then `device.createSwapchain(surface, w, h, Format.Bgra8Unorm, vsync)`.
Each frame: `var image = swapchain.acquire()`, render into it, barrier it to
`Layout.Present`, submit, then `if (!swapchain.present()) swapchain.resize(w, h)`.
The images take the window's size in pixels: read `swapchain.width()`.

## Rules

- Synchronization is explicit. Record a barrier between a write and any
  later read or write of the same resource, including the CPU's
  (`Stage.Host`, `Access.HostRead`). Move images between layouts with
  `imageBarrier`; rendering needs `Layout.ColorAttachment`, copying from it
  `Layout.TransferSrc`.
- A command buffer is recording from `createCommandBuffer` until `submit`;
  after that, record into a new one.
- Uniform values: fill `Bytes.alloc(Shader.PARAMS_SIZE)` at the
  `PARAMS_<name>` offsets, put them in a `BufferUsage.Uniform` buffer, and
  `writeBuffer(Shader.PARAMS_BINDING, ...)` on the set of
  `Shader.PARAMS_GROUP`.
- Every object holds a Vulkan object until `close()`; close what you made.
  Keep resources alive until the submission using them has finished.
- `submit`'s future delivers through `hxn.Loop`; a custom frame loop sets
  `hxn.Loop.manual = true` and calls `hxn.Loop.sync()` each frame.

## Reference

The generated classes are committed under `binding/gpu/vulkan/` in
https://github.com/hx-n/hxn-gpu-vulkan. `examples/check/Main.hx` is a
complete compute and render program with its barriers.

## Working on this repository

Test with the hxn CLI, hxnsl and a Vulkan driver: `hxn check hl --vm ash`
and `hxn check cpp cppia`. Run with `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`
to have Vulkan check every call.

## Redistribution

Retain the applicable [dependency licenses and notices](https://github.com/hx-n/hxn-gpu-vulkan/blob/main/third-party/NOTICES.md)
when distributing binaries. Release packages include `third-party/`.
