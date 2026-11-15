---
name: hxnsl
description: Write GPU shaders in Haxe with the hxnsl haxelib, which translates HXSL (Heaps' shader language) at compile time to WGSL, MSL, HLSL, SPIR-V and GLSL ES with their bind group layout and binding slots as class constants. Use when writing an HXSL shader class, reading its generated fields to build pipelines or bind resources, setting @const values, or extending hxnsl with a framework Extension or a driver Target.
---

# hxnsl

## A shader

```haxe
class Textured implements hxnsl.Shader {
	static var SRC = {
		@input var input : { position : Vec2, uv : Vec2 };
		var output : { position : Vec4, color : Vec4 };
		@param var image : Sampler2D;
		@param var tint : Vec4;
		var calculatedUV : Vec2;

		function vertex() {
			calculatedUV = input.uv;
			output.position = vec4(input.position, 0, 1);
		}

		function fragment() {
			output.color = image.get(calculatedUV) * tint;
		}
	};
}
```

Compile with `-lib hxnsl`. The syntax is Heaps' HXSL
(https://heaps.io/documentation/hxsl.html); a shader error is a Haxe compile
error. A compute shader has `function main()`, and `setLayout(x)`,
`setLayout(x, y)` or `setLayout(x, y, z)` for its workgroup size, and `RWBuffer<T>` for a read-write storage buffer.

hxnsl links each shader with what it imports or extends at compile time only.
It does not combine shaders at run time as Heaps does.

## Fields the class gets

- `WGSL`, `MSL`, `HLSL`, `spirv()` (bytes from a resource), `GLSL_VERTEX`
  and `GLSL_FRAGMENT`. `GLSL_MISSING` replaces the GLSL fields when WebGL2
  cannot run the shader (compute, storage buffers).
- `LAYOUT`: each bind group's entries (`binding`, `visibility` bits 1 vertex,
  2 fragment, 4 compute, `type`, ...), empty for a skipped group.
  `ENTRY_POINTS`: name, stage, workgroup. Entry points are `vertex_main`,
  `fragment_main` and `compute_main` in every language.
- Slots per language: `MSL_SLOTS` (per entry point, `kind` buffer, texture
  or sampler, and `index`), `HLSL_SLOTS` (register class, index, space, or a
  sampler index), `GLSL_BINDINGS` (uniform names). SPIR-V's descriptor set
  is the bind group and its binding the WGSL binding.
- Uniforms: `PARAMS_SIZE`, `PARAMS_<name>` (byte offsets), `PARAMS_GROUP`,
  `PARAMS_BINDING`. Resources: `TEXTURE_<name>` (its sampler is the next
  binding) and `BUFFER_<name>`, each with a `_GROUP`. `INPUT_<name>` vertex
  locations, `TARGET_<name>` color targets, `CONST_<name>` pipeline constant
  keys.

The types are in `hxnsl.Layout`. Use these constants to fill buffers and
bind resources; do not hard-code slot numbers.

## Pipeline constants

A `@const` is a WGSL `override` with no default. WebGPU sets it at pipeline
creation by the `CONST_<name>` key; the other languages are written with the
value the class gives, and compilation fails without one:

```haxe
@:constants({fog: true})
class Fogged implements hxnsl.Shader { ... @const var fog : Bool; ... }
```

## Defines

- `-D hxnsl_targets=msl,spirv` keeps only those languages' fields, for
  shaders no driver registered targets for.
- `-D hxnsl_wgsl_only` writes WGSL and the `PARAMS_`/`TEXTURE_`/... constants
  only, without the tool; no `LAYOUT` or `ENTRY_POINTS`.

## Extending

- A framework: an interface extending `hxnsl.Shader` for its shaders, and a
  subclass of `hxnsl.Extension` registered from its haxelib's
  `extraParams.hxml` init macro with
  `hxnsl.Extensions.register(ext, "FrameworkShader")`. Hooks: `prelude`
  (globals), `functions` (extern functions and their WGSL), `block` and
  `group` (where globals go), `transform`, `output`.
- A driver: a subclass of `hxnsl.Target` whose `build(s:hxnsl.Compiled)`
  returns fields, registered with `hxnsl.Targets.register(target, family)`.
  `s.step(key, inputs, run, outputs)` is a cached build step for an external
  compiler; `s.resource` adds a resource; `s.field` makes a field. Shaders of
  that family get only the registered targets; register
  `new hxnsl.targets.Msl()` (or `Hlsl`, `Spirv`, `Glsl`) as well to keep those
  fields. Two targets adding one field fail compilation.

## The tool

At compile time hxnsl runs the `hxnsl` tool (russl's compiler), shipped in the haxelib
for macOS arm64 and x86-64, Linux x86-64 and arm64, and Windows x86-64.
`HXNSL_TOOL` overrides its path; `HXNSL_CACHE` sets the cache directory.

## Working on this repository

`cargo test` tests the tool. The Haxe tests:

```
cargo build
haxelib dev hxnsl .
haxelib dev hxnsl-framework test/framework
haxe -lib hxnsl-framework -cp test --interp -main Main
```

`Main` throws at the first failed check and prints nothing on success. CI
also runs the define, clash and missing-constant cases in
`.github/workflows/ci.yml`, and compiles the output with spirv-val,
glslangValidator, DXC and the Metal compiler. Open issues are in git-bug.
