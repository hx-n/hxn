---
name: hxn-gpu
description: Drive the GPU from Haxe with one API on Metal, Vulkan, Direct3D 12, OpenGL ES, WebGPU and WebGL2, using the hxn-gpu haxelib (package `hxn.gpu`) over the hxn GPU driver haxelibs, on HashLink, Ash and hxcpp, natively and in the browser. Use when a Haxe program renders or computes on whatever GPU the platform has, checks device features and limits, falls back between backends, or uses HXSL shaders from hxnsl on several backends at once.
---

# hxn-gpu

## Setup

Add hxn-gpu, hxnsl and a driver haxelib for each backend to support. Only
those backends are compiled in.

```
haxelib install hxn-gpu
haxelib install hxn-gpu-metal      # or -vulkan, -d3d12, -gles, -webgpu, -webgl2
haxe -lib hxn-gpu -lib hxn-gpu-metal -lib hxn-gpu-vulkan -lib hxnsl -main Main -hl main.hl
```

GLES and WebGL2 can't be given together. For the browser, add hxn-gpu-webgpu
and hxn-gpu-webgl2 and compile as each driver's README shows: hxcpp with
`-D emscripten`, or Ash with `-D wasm -D hxn_threads`.

## Use

```haxe
import hxn.gpu.*;

GpuDevice.open({required: [Compute]}).then(device -> {
	var shader = device.createShader(Tripled);              // a class implementing hxn.gpu.Shader
	var pipeline = device.createComputePipeline({shader: shader});
	var data = device.createBuffer({size: 256, usage: Storage, contents: input});
	var group = device.createBindGroup({pipeline: pipeline, group: Tripled.BUFFER_data_GROUP,
		entries: [{binding: Tripled.BUFFER_data, buffer: data}]});
	var encoder = device.createCommandEncoder();
	var pass = encoder.beginComputePass();
	pass.setPipeline(pipeline);
	pass.setBindGroup(Tripled.BUFFER_data_GROUP, group);
	pass.dispatch(4);
	pass.end();
	device.submit(encoder);
	device.readBuffer(data).then(bytes -> use(bytes));
}, e -> trace(e.message));
```

Render: `createRenderPipeline({shader, format, topology, vertexBuffers})`,
`encoder.beginRenderPass(texture, clear)`, `setPipeline`, `setBindGroup`,
`setVertexBuffer`, `draw`, `end`, then `device.readTexture(texture)`.

## Rules

- `open` tries the platform's order: Metal, then Vulkan and GLES on Apple
  platforms; Direct3D 12, then Vulkan and GLES on Windows; Vulkan, then GLES
  elsewhere; WebGPU, then WebGL2 in the browser. Override it with
  `{backends: [...]}`, `HXN_GPU_BACKEND=vulkan,gles` or
  `-D hxn_gpu_backend=...`. A fallback after a failed backend prints a
  warning to standard error, or to `OpenOptions.warn`.
- Check `device.features` before an optional call. A call that lacks a feature
  throws and names the feature and the backend. GLES and WebGL2 have no
  compute; Vulkan has no `TextureWrites` yet.
- Present with a `GpuSurface`: `GpuDevice.open({surface: target})` with
  `SurfaceTargets.ofWindow(window)` (an hxn-window window) or `Canvas`, then
  `device.createSurface(target)`, `surface.configure(w, h)`, and each frame
  render into `surface.currentTexture()` and call `surface.present()`. Make
  the render pipeline with `format: surface.format`.
- A bind group is made for one pipeline's shader and used with pipelines of
  that shader; give an entry for every binding the shader's `LAYOUT` lists.
- Synchronization is implicit. Writes take effect before later submissions;
  reads return an `hxn.Future` once earlier submissions have run.
- In a browser under hxcpp, deliver futures with `then`, never `await`.
- `readTexture` returns rows top first, `width * bytesPerPixel` long; on GLES
  and WebGL2 only RGBA8, with rendered rows bottom first.

## Reference

https://github.com/hx-n/hxn-gpu. `examples/check/Main.hx` computes, renders,
samples a texture and reads a buffer back on each backend.
