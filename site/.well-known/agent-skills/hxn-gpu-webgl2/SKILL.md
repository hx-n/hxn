---
name: hxn-gpu-webgl2
description: Drive the GPU from Haxe in the browser through WebGL2 with the hxn-gpu-webgl2 haxelib (package `gpu.webgl2`), for hxcpp programs compiled to wasm with `-D emscripten`, threaded or not, and threaded Ash programs (`-D wasm -D hxn_threads`); HashLink and CPPIA do not run in a browser. Use when a Haxe program in a web page renders with WebGL2, for browsers without WebGPU, or uses HXSL shaders from hxnsl as GLSL ES 3.00.
---

# hxn-gpu-webgl2

## Setup

```
haxelib install hxn-gpu-webgl2
haxelib install hxnsl
haxe -lib hxn-gpu-webgl2 -lib hxnsl -main Main -cpp out -D emscripten -D disable_unicode_strings -D HXCPP_LINK_EMSCRIPTEN_EXT=.html
```

Needs the Emscripten SDK on `PATH` and hxcpp from git (its Emscripten
toolchain uses wasm exceptions). Serve `out/` and open `Main.html`. The
context is made on the element with id `canvas`, which Emscripten's default
HTML page has; a page of your own needs a `<canvas id="canvas">`. Add
`-D hxn_threads` for threads: `main` then runs on a pthread that holds the
canvas, and the web page needs cross-origin isolation headers (COOP/COEP),
which `emrun out/Main.html` sends.

Ash (0.1.19 or later), threaded:

```
haxe -lib hxn-gpu-webgl2 -lib hxnsl -main Main -hl out/main.hl -D wasm -D hxn_threads
ash --build out/main.wasm --target wasm32-wasip1-threads --native-lib out/libgpu_webgl2.a out/main.hl
ash serve out          # sends the cross-origin isolation headers shared memory needs
```

The compile copies the library, `libgpu_webgl2.a`, and its `.mjs` files
beside `main.hl`. Under Ash the WebGL2 context is held by a browser agent: a
Worker the web page starts with those files. The haxelib carries the
Emscripten and Ash builds prebuilt, so a program needs no Rust toolchain.

## Use

The classes are those of hxn-gpu-gles (https://github.com/hx-n/hxn-gpu-gles)
for OpenGL ES 3.0: `Context`, `Buffer`, `Texture`, `Sampler`, `Program`,
`VertexArray`, `Framebuffer` and `Bindings`. `Context.create()` makes the
context on the web page's canvas. Shaders are hxnsl classes implementing
`gpu.webgl2.Shader`; they get `GLSL_VERTEX`, `GLSL_FRAGMENT`,
`GLSL_BINDINGS` and constants such as `INPUT_<name>`, `TEXTURE_<name>`,
`TEXTURE_<name>_GROUP`, `PARAMS_GROUP` and `PARAMS_BINDING`. A shader that
uses compute or storage buffers gets `GLSL_MISSING`, a string saying why,
and no GLSL: WebGL2 cannot run it.

```haxe
import gpu.webgl2.*;

var context = Context.create();                   // throws without WebGL2 or a canvas
context.setSize(w, h);                            // the canvas's drawing buffer
var bindings = new Bindings(Textured.GLSL_BINDINGS);
var program = Shaders.program(context, Textured.GLSL_VERTEX, Textured.GLSL_FRAGMENT, bindings);

var vertices = context.createVertexArray();
var corners = context.createBufferWith(BufferTarget.Array, bytes, Usage.StaticDraw);
vertices.attribute(Textured.INPUT_position, corners, 2, ComponentType.Float, false, 16, 0);
vertices.attribute(Textured.INPUT_uv, corners, 2, ComponentType.Float, false, 16, 8);

var image = context.createTexture(iw, ih, TextureFormat.Rgba8);
image.write(0, 0, iw, ih, pixels);                // rows tightly packed, bottom row first
var sampler = context.createSampler(Filter.Linear, Filter.Linear, Wrap.ClampToEdge, Wrap.ClampToEdge);

context.bindDefaultFramebuffer();                 // the canvas; or bindFramebuffer(createFramebuffer(texture))
context.viewport(0, 0, w, h);
context.clear(ClearBits.Color, 0, 0, 0, 1);
context.useProgram(program);
context.bindVertexArray(vertices);
context.bindTexture(bindings.textureUnit(Textured.TEXTURE_image_GROUP, Textured.TEXTURE_image), image, sampler);
context.drawArrays(Primitive.TriangleStrip, 0, 4, 1);
```

`Textured` above is an hxnsl shader whose only `@param` is a `Sampler2D`. A
shader's other `@param` values go in a uniform buffer of `PARAMS_SIZE` bytes,
each at its `PARAMS_<name>` offset:

```haxe
var params = haxe.io.Bytes.alloc(Tinted.PARAMS_SIZE);
params.setFloat(Tinted.PARAMS_tint, 0.25);
var uniforms = context.createBufferWith(BufferTarget.Uniform, params, Usage.StaticDraw);
context.bindUniformBuffer(bindings.uniformBlock(Tinted.PARAMS_GROUP, Tinted.PARAMS_BINDING), uniforms);
```

## Rules

- The browser shows what was drawn on the canvas once the code that drew it
  returns to the browser, and clears the canvas after: call `readPixels` on
  the canvas before returning.
- Draws and copies run in the order they are made. Wait only to read on the
  CPU: `completed()` (an `hxn.Future`, settled once the GPU has finished every
  command issued before the call) or `finish()`. Under Ash `await` works,
  since the browser agent holds the context. Under hxcpp, threaded or not,
  use `then`: the context's thread checks the fence only while not blocked.
- Binding points and texture units come from `Bindings`, keyed by hxnsl's
  group and binding; `Shaders.program` assigns them by name.
- `Program.uniformBlock` and `Program.sampler` assign bindings by name;
  `Shaders.program` calls them for an hxnsl shader. `Program.sampler` leaves
  that program in use: call `useProgram` again before drawing with another.
- Per-call errors are not checked; `context.error()` is `getError`.
  Compile and link failures throw with WebGL's log.
- Under Ash, a call that returns a value (`readPixels`, `Buffer.read`,
  `error`, `createProgram`'s compile and link checks) waits for the browser
  agent to run every call before it: make those calls once a frame's drawing
  is done.
- Every object holds its WebGL object until `close()`.

## Reference

The Haxe classes hxn generates from the Rust source are committed under
`binding/gpu/webgl2/` in https://github.com/hx-n/hxn-gpu-webgl2.
`examples/check/Main.hx` renders into a texture, samples one, writes and
reads a buffer, and clears the web page's canvas and reads it back.

## Working on this repository

Test with the hxn CLI and hxnsl: `hxn check cpp --wasm --browser`,
`hxn check cpp --wasm --threads --browser` and
`hxn check hl --wasm --threads --browser`, which need Chrome and node 22.
The Ash backend's calls are encoded by `src/agent/wire.rs` and made by the
browser agent's `page/gpu_webgl2_service.mjs`, both generated from the WebGL
IDL in `tools/wire/idl/`; `cargo run --manifest-path tools/wire/Cargo.toml`
regenerates them.
