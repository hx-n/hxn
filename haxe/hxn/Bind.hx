package hxn;

#if macro
import haxe.io.Path;
import haxe.macro.Compiler;
import haxe.macro.Context;
import sys.FileSystem;
import sys.io.File;

/**
	Points a compile at a library that `hxn package` packaged.

	`--macro hxn.Bind.lib("name")`, which the package's `extraParams.hxml`
	says, so that `-lib name` is all a program's build names. The package is
	found by its manifest, `name.hxn.json`, on the class path. From what the
	compile is for, this adds the library's classes for HashLink or hxcpp,
	tells hxcpp where the headers and the built library are, and puts a
	HashLink library beside the program.

	A CPPIA script gets declarations of the hxcpp classes, which its host
	holds: an hxcpp program compiled with `-D scriptable` and this same macro,
	which then keeps the library's classes whether or not it uses them.

	The library taken is the one built for this machine, or for wasm when
	`wasm` or `emscripten` is defined. `-D hxn_triple=<triple>` names another,
	as Rust names what it builds for.
**/
class Bind {
	/** The manifest version this reads. **/
	static inline final VERSION = 1;

	public static function lib(name:String):Void {
		var manifest = try Context.resolvePath('$name.hxn.json') catch (_:Dynamic) null;
		if (manifest == null)
			fail('no $name.hxn.json on the class path: name the package `hxn package` made with -lib or -cp');
		var root = Path.directory(manifest);
		var read:{?hxn:Int, ?library:String} = try haxe.Json.parse(File.getContent(manifest)) catch (_:Dynamic) null;
		if (read == null || read.library != name)
			fail('$manifest is not the manifest of a library named $name');
		if (read.hxn != VERSION)
			fail('$manifest is version ${read.hxn} of the manifest and this hxn reads version $VERSION: package the library and compile with the same hxn');

		var runtime = if (Context.defined("hl")) "hl" else if (Context.defined("cpp")) "cpp" else null;
		if (runtime == null)
			fail('$name is bound for HashLink and hxcpp, and this compile is for neither');
		// A script is compiled for hxcpp too, and calls nothing native itself.
		var script = Context.defined("cppia");
		var classes = Path.join([root, "binding", script ? "cppia" : runtime]);
		if (!FileSystem.isDirectory(classes))
			fail('the package in $root was not built for $runtime: run `hxn package $runtime` on the library');
		Compiler.addClassPath(classes);
		// An editor asks for types, and builds nothing to find a library for.
		if (script || Context.defined("display"))
			return;
		if (runtime == "cpp" && Context.defined("scriptable")) {
			Compiler.include("hxn");
			Compiler.include("", true, null, [classes]);
		}

		var triple = triple(runtime);
		var built = Path.join([root, "native", triple]);
		function library(files:Array<String>):String {
			for (file in files) {
				var path = Path.join([built, file]);
				if (FileSystem.exists(path))
					return path;
			}
			var held = FileSystem.isDirectory(Path.join([root, "native"])) ? FileSystem.readDirectory(Path.join([root, "native"])) : [];
			return fail('the package in $root has no ${files[0]} for $triple; it has libraries for: ${held.join(", ")}. Run `hxn package $runtime` where Rust builds for $triple, or name another with -D hxn_triple=<triple>');
		}
		switch runtime {
			case "hl":
				var wasm = StringTools.startsWith(triple, "wasm32");
				var file = name + (wasm ? ".wasm" : ".hdll");
				var from = library([file]);
				// The VM asks for the library by name, and Ash for the module
				// beside the program it builds: both look where the program is.
				Context.onAfterGenerate(() -> {
					var output = Compiler.getOutput();
					if (Path.extension(output) == "hl")
						File.copy(from, Path.join([Path.directory(FileSystem.absolutePath(output)), file]));
				});
			case _:
				var upper = name.toUpperCase();
				var include = Path.join([root, "include"]);
				Compiler.define("HXN_INCLUDE", include);
				Compiler.define('${upper}_INCLUDE', include);
				Compiler.define('${upper}_LIB', library(['lib${name}_cpp.a', '${name}_cpp.lib']));
				// hxcpp builds for 32 bits on Windows and for the host's 64
				// under Emscripten unless told, and the library is neither.
				if (!Lambda.exists(["HXCPP_M64", "HXCPP_ARM64", "HXCPP_M32"], Context.defined)) {
					if (StringTools.endsWith(triple, "windows-msvc"))
						Compiler.define(StringTools.startsWith(triple, "aarch64") ? "HXCPP_ARM64" : "HXCPP_M64");
					else if (StringTools.startsWith(triple, "wasm32"))
						Compiler.define("HXCPP_M32");
				}
		}
	}

	// What the library was built for, as Rust names it.
	static function triple(runtime:String):String {
		var named = Context.definedValue("hxn_triple");
		if (named != null && named != "1")
			return named;
		if (Context.defined("emscripten"))
			return "wasm32-unknown-emscripten";
		if (Context.defined("wasm"))
			return runtime == "hl" ? "wasm32-wasip1" : "wasm32-unknown-emscripten";
		var system = Sys.systemName();
		var machine = if (runtime == "cpp" && Context.defined("HXCPP_ARM64")) "arm64" else if (runtime == "cpp" && Context.defined("HXCPP_M64")) "x86_64" else if (system == "Windows") Sys.getEnv("PROCESSOR_ARCHITECTURE") else StringTools.trim(new sys.io.Process("uname", ["-m"]).stdout.readAll().toString());
		var arch = switch machine == null ? "" : machine.toLowerCase() {
			case "arm64" | "aarch64": "aarch64";
			case "x86_64" | "amd64": "x86_64";
			case other: fail('hxn does not know what Rust calls a machine of kind "$other": name what to build for with -D hxn_triple=<triple>');
		}
		return switch system {
			case "Windows": '$arch-pc-windows-msvc';
			case "Mac": '$arch-apple-darwin';
			case "Linux": '$arch-unknown-linux-gnu';
			case other: fail('hxn does not know what Rust calls $other: name what to build for with -D hxn_triple=<triple>');
		}
	}

	static function fail(message:String):Dynamic {
		return Context.fatalError('hxn: $message', Context.currentPos());
	}
}
#end
