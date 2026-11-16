---
name: hxn-gpu-gles
description: Drive the GPU from Haxe through OpenGL ES 3.0 over EGL with the hxn-gpu-gles haxelib (package `hxn.gpu.gles`) on Ash, HashLink, hxcpp and CPPIA, on Android, Linux through Mesa, and macOS and Windows through ANGLE. Use when a Haxe program renders with OpenGL ES, samples textures, presents to a window with EGL, or uses HXSL shaders from hxnsl as GLSL ES 3.00.
---

# hxn-gpu-gles

## Setup

```
haxelib install hxn-gpu-gles
haxelib install hxnsl
haxe -lib hxn-gpu-gles -lib hxnsl -main Main -hl main.hl     # Ash or HashLink
haxe -lib hxn-gpu-gles -lib hxnsl -main Main -cpp out        # hxcpp
```

Prebuilt for Linux x86-64 and ARM64, macOS arm64, Windows x86-64, and as
static libraries for Intel macOS (hxcpp) and Android; a program needs no Rust.
EGL and OpenGL ES load at run time: Mesa's on Linux (`libegl1 libgles2
libegl-mesa0`), the system's on Android. On macOS and Windows set
`HXN_GPU_GLES_EGL` to ANGLE's `libEGL` (Electron's releases up to 43 include
it); `libGLESv2` loads from beside it, or from `HXN_GPU_GLES_GLES`.

## Use

Shaders are hxnsl classes implementing `hxn.gpu.gles.Shader`; they get
`GLSL_VERTEX`, `GLSL_FRAGMENT` and `GLSL_BINDINGS`. Compute shaders and
storage buffers have `GLSL_MISSING` instead: OpenGL ES 3.0 cannot run them.

```haxe
import hxn.gpu.gles.*;

var display = Display.headless();                 // Mesa surfaceless, else the default display
var context = display.createContext();            // no default framebuffer; current on this thread
var bindings = new Bindings(Textured.GLSL_BINDINGS);
var program = hxn.gpu.gles.Shaders.program(context, Textured.GLSL_VERTEX, Textured.GLSL_FRAGMENT, bindings);

var vertices = context.createVertexArray();
var corners = context.createBufferWith(BufferTarget.Array, bytes, Usage.StaticDraw);
vertices.attribute(Textured.INPUT_position, corners, 2, ComponentType.Float, false, 16, 0);
vertices.attribute(Textured.INPUT_uv, corners, 2, ComponentType.Float, false, 16, 8);

var image = context.createTexture(w, h, TextureFormat.Rgba8);
image.write(0, 0, w, h, pixels);                  // rows tightly packed, bottom row first
var sampler = context.createSampler(Filter.Linear, Filter.Linear, Wrap.ClampToEdge, Wrap.ClampToEdge);

var target = context.createTexture(tw, th, TextureFormat.Rgba8);
var framebuffer = context.createFramebuffer(target);
context.bindFramebuffer(framebuffer);
context.viewport(0, 0, tw, th);
context.clear(ClearBits.Color, 0, 0, 0, 1);
context.useProgram(program);
context.bindVertexArray(vertices);
context.bindTexture(bindings.textureUnit(Textured.TEXTURE_image_GROUP, Textured.TEXTURE_image), image, sampler);
context.bindUniformBuffer(bindings.uniformBlock(group, binding), uniforms);   // BufferTarget.Uniform
context.drawArrays(Primitive.TriangleStrip, 0, 4, 1);
context.completed().then(_ -> use(context.readPixels(0, 0, tw, th)));        // RGBA8, bottom row first
```

Window: `Display.native(platform, display)` and
`createWindowContext(window, visual)`, then `bindDefaultFramebuffer`, draw,
`swapBuffers`. With hxn-window on Xlib (`platform() == 3`):
`Display.native(0x31D5, window.raw(2))` and
`createWindowContext(window.raw(0), haxe.Int64.toInt(window.raw(1)))`. On
Android: `Display.native(0, 0)` and the `ANativeWindow*` with visual 0.
Wayland needs a `wl_egl_window`, which hxn-window does not make.

## Rules

- A context and its objects are used on the thread where the context is
  current; `createContext` makes it current, `makeCurrent` moves it.
- OpenGL ES orders the commands itself: no barriers. Wait only to read on the
  CPU: `completed()` (an `hxn.Future`; an EGL fence waited on another thread
  where EGL 1.5 has `EGL_KHR_fence_sync`, else `glFinish`) or `finish()`.
- Binding points and texture units come from `Bindings`, keyed by hxnsl's
  group and binding; `Shaders.program` assigns them by name. A texture
  sampled with two samplers needs `textureUnit(g, b, samplerGroup, samplerBinding)`.
- `Program.sampler` leaves that program in use.
- Per-call GL errors are not checked; `context.error()` is `glGetError`.
  Creation, compile and link failures throw with OpenGL ES's log.
- Every object holds its OpenGL ES object until `close()`.
- `completed`'s future delivers through `hxn.Loop`; a custom frame loop sets
  `hxn.Loop.manual = true` and calls `hxn.Loop.sync()` each frame.

## Reference

The generated classes are committed under `binding/gpu/gles/` in
https://github.com/hx-n/hxn-gpu-gles. `examples/check/Main.hx` renders,
samples a texture and reads a buffer back; `examples/present/Main.hx`
presents to an X11 window.

## Working on this repository

Test with the hxn CLI and hxnsl: `hxn check hl --vm ash` and
`hxn check cpp cppia`. On macOS, point `HXN_GPU_GLES_EGL` at an ANGLE
`libEGL.dylib`, such as the one in an Electron app's
`Electron Framework.framework/Versions/A/Libraries/`.
