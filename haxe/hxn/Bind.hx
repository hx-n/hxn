package hxn;

#if macro
import haxe.io.Path;
import haxe.macro.Compiler;
import haxe.macro.Context;
import sys.FileSystem;
import sys.io.File;

/**
	Configures a Haxe compilation to use a library packaged by `hxn package`.

	The package's `extraParams.hxml` specifies
	`--macro hxn.Bind.lib("name")`, so a program's build only needs
	`-lib name`. The package is located by its manifest, `name.hxn.json`, on
	the class path. This adds the library's classes to the class path, the
	same on every runtime; depending on the compilation target, it defines the
	header and library paths for hxcpp, and copies a HashLink library next to
	the compiled program.

	A CPPIA script is compiled against the same classes and calls those its
	host contains: an hxcpp program compiled with `-D scriptable` and this
	same macro. In the host, the macro includes the library's classes whether
	or not the host uses them.

	The native library used is the one built for the host machine, or for
	wasm when `wasm` or `emscripten` is defined. `-D hxn_triple=<triple>`
	selects another, given as a Rust target triple.
**/
class Bind {
	/** The manifest version this macro reads. **/
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
		// A CPPIA script is also compiled as hxcpp, and calls the classes its
		// host contains, so it makes no native calls itself.
		var script = Context.defined("cppia");
		// The same classes for every runtime: hxn.Native implements their
		// native class for the one compiled.
		var classes = Path.join([root, "binding"]);
		if (!FileSystem.isDirectory(classes))
			fail('the package in $root has no classes: run `hxn package` on the library');
		Compiler.addClassPath(classes);
		// A display request from an editor only needs types. Nothing is built, so no native library is looked up.
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
			var has = held.length == 0 ? "none" : held.join(", ");
			return fail('the package in $root has no ${files[0]} for $triple (it has native libraries for: $has). Install a release of the package that includes $triple, run `hxn package $runtime` where Rust builds for $triple, or name another with -D hxn_triple=<triple>');
		}
		switch runtime {
			case "hl":
				var wasm = StringTools.startsWith(triple, "wasm32");
				// A library cross-compiled for another platform is an
				// archive, which Ash links into the program it builds.
				var from = library(wasm ? ['$name.wasm'] : ['$name.hdll', 'lib$name.a', '$name.lib']);
				var file = Path.withoutDirectory(from);
				// The VM loads the library by name, and Ash loads the module
				// next to the program it builds: both search the program's
				// directory. An archive is put there too, for `--native-lib`.
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
				// What the archive links: system libraries its build scripts named.
				var link = Path.join([built, "link.xml"]);
				if (FileSystem.exists(link))
					Compiler.define('${upper}_LINK', link);
				// Unless one of these is defined, hxcpp builds 32-bit on
				// Windows and the host's 64-bit under Emscripten. Neither
				// matches the library.
				if (!Lambda.exists(["HXCPP_M64", "HXCPP_ARM64", "HXCPP_M32"], Context.defined)) {
					if (StringTools.endsWith(triple, "windows-msvc"))
						Compiler.define(StringTools.startsWith(triple, "aarch64") ? "HXCPP_ARM64" : "HXCPP_M64");
					else if (StringTools.startsWith(triple, "wasm32"))
						Compiler.define("HXCPP_M32");
				}
		}
	}

	// The Rust target triple the library was built for.
	static function triple(runtime:String):String {
		var named = Context.definedValue("hxn_triple");
		if (named != null && named != "1")
			return named;
		if (Context.defined("emscripten"))
			return "wasm32-unknown-emscripten";
		if (Context.defined("wasm"))
			return runtime == "hl" ? "wasm32-wasip1" : "wasm32-unknown-emscripten";
		// iOS and Android, detected from the defines hxcpp sets when building for them.
		if (runtime == "cpp") {
			if (Context.defined("iphoneos"))
				return "aarch64-apple-ios";
			if (Context.defined("iphonesim"))
				return Context.defined("HXCPP_ARM64") ? "aarch64-apple-ios-sim" : "x86_64-apple-ios";
			if (Context.defined("android")) {
				if (Context.defined("HXCPP_ARM64"))
					return "aarch64-linux-android";
				if (Context.defined("HXCPP_X86_64"))
					return "x86_64-linux-android";
				if (Context.defined("HXCPP_X86"))
					return "i686-linux-android";
				return "armv7-linux-androideabi";
			}
		}
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
