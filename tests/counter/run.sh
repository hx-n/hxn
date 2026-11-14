#!/usr/bin/env bash
# Builds the counter binding for one runtime and runs haxe/Main.hx on it.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`
#   run.sh cpp
#   run.sh wasm [ash]  as a side module of an Ash wasm program, run by `ash run`
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
#
# The same for the binding hxn-gen generates from the library's source,
# tested by haxe/Generated.hx:
#   run.sh gen-hl [vm]
#   run.sh gen-cpp
#   run.sh gen-cppia   as a CPPIA script, in an hxcpp host that contains the classes
#   run.sh gen-wasm [ash]
#   run.sh gen-emcc
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | wasm [ash] | emcc | gen-hl [vm] | gen-cpp | gen-cppia | gen-wasm [ash] | gen-emcc}"

# The file name prefixes and extensions cargo and hxcpp use on this system.
prefix=lib static=a exe=
case "$(uname -s)" in
	Darwin) dylib=dylib ;;
	MINGW* | MSYS* | CYGWIN*)
		prefix='' dylib=dll static=lib exe=.exe
		# Convert paths to the form native Windows programs accept.
		here="$(cygpath -m "$here")"
		root="$(cygpath -m "$root")"
		;;
	*) dylib=so ;;
esac
work="$root/target/counter-$runtime"
mkdir -p "$work"

# Builds one of the HashLink libraries against the VM in $vm and puts it
# where the VM will look for `counter`.
hdll() {
	vm_dir="$(dirname "$vm")"
	if [ -n "$exe" ]; then
		# On Windows the library links against the VM's import library, and
		# the loader searches PATH.
		export HXN_HL_DIR="${HXN_HL_DIR:-$(cygpath -m "$vm_dir")}"
		PATH="$(cygpath -u "$work"):$PATH"
	fi
	cargo build --manifest-path "$root/Cargo.toml" -p "$1"
	cp "$root/target/debug/$prefix${1//-/_}.$dylib" "$work/counter.hdll"
}

# Builds one of the static libraries hxcpp links, and sets the extra flags
# hxcpp needs to link it.
linked=()
staticlib() {
	if [ -n "$exe" ]; then
		# On Windows hxcpp links the static C runtime and builds 32-bit unless
		# HXCPP_M64 is set. HXCPP_DEBUG_LINK keeps symbols so a debugger can
		# show where a crash happened.
		RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static" \
			cargo build --manifest-path "$root/Cargo.toml" -p "$1"
		linked=(-D HXCPP_M64 -D HXCPP_DEBUG_LINK)
	else
		cargo build --manifest-path "$root/Cargo.toml" -p "$1"
	fi
}

# Generates the header, the glue and the Haxe classes from the same source
# the adapter crates generate their adapters from.
generate() {
	rm -rf "$work/gen"
	cargo run --quiet --manifest-path "$root/Cargo.toml" -p hxn-gen -- \
		--library counter --source "$here/core/src/lib.rs" --crate counter_core --out "$work/gen" >/dev/null
}

case "$runtime" in
	gen-hl)
		vm="$(command -v "${2:-hl}")"
		hdll counter-gen-hl
		generate
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$work/gen/haxe" -main Generated -hl "$work/main.hl"
		cd "$work"
		DYLD_LIBRARY_PATH="$work:$vm_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
		LD_LIBRARY_PATH="$work:$vm_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
			"$vm" main.hl | tee "$work/out.txt"
		;;
	gen-cpp)
		staticlib counter-gen-c
		generate
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$work/gen/haxe" -main Generated -cpp "$work/out" \
			${linked[@]+"${linked[@]}"} \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$work/gen/include" \
			-D COUNTER_LIB="$root/target/debug/${prefix}counter_gen.$static"
		"$work/out/Generated$exe" | tee "$work/out.txt"
		;;
	gen-cppia)
		staticlib counter-gen-c
		generate
		# The host is an hxcpp program that contains every class of the
		# binding and of hxn, and lists them in a file for the script's
		# compilation. Dead-code elimination is off because the script may use
		# classes the host does not.
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/cppia" -cp "$work/gen/haxe" -main Host -cpp "$work/host" \
			-D scriptable -dce no -D dll_export="$work/host.classes" \
			--macro "include('hxn')" --macro "include('', true, null, ['$work/gen/haxe'])" \
			${linked[@]+"${linked[@]}"} \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$work/gen/include" \
			-D COUNTER_LIB="$root/target/debug/${prefix}counter_gen.$static"
		# The script is compiled against the binding's classes, and omits
		# every class the host already contains.
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$work/gen/haxe" -main Generated -cppia "$work/generated.cppia" \
			-D dll_import="$work/host.classes"
		"$work/host/Host$exe" "$work/generated.cppia" | tee "$work/out.txt"
		;;
	hl)
		vm="$(command -v "${2:-hl}")"
		hdll counter-hl
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/hl" -main Main -hl "$work/main.hl"
		cd "$work"
		# The VM resolves a bare library name through the OS loader.
		DYLD_LIBRARY_PATH="$work:$vm_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
		LD_LIBRARY_PATH="$work:$vm_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
			"$vm" main.hl | tee "$work/out.txt"
		;;
	cpp)
		staticlib counter-c
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/cpp" -main Main -cpp "$work" \
			${linked[@]+"${linked[@]}"} \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$here/c" \
			-D COUNTER_LIB="$root/target/debug/${prefix}counter.$static"
		"$work/Main$exe" | tee "$work/out.txt"
		;;
	wasm | gen-wasm)
		ash="$(command -v "${2:-ash}")"
		if [ "$runtime" = gen-wasm ]; then
			generate
			package=counter-gen-hl archive=libcounter_gen_hl.a main=Generated
			adapter="$work/gen/hl/adapter.rs" classes="$work/gen/haxe"
		else
			package=counter-hl archive=libcounter_hl.a main=Main
			adapter="$here/hl/src/lib.rs" classes="$here/haxe/hl"
		fi
		# A side module must be position-independent and the prebuilt std is
		# not, so std is rebuilt. It unwinds with wasm exceptions in the
		# encoding Ash emits, so a panic is caught. Only the static library is
		# built, because cargo cannot link a wasm shared library whose imports
		# come from the main program.
		RUSTFLAGS="-C relocation-model=pic -C target-feature=+mutable-globals,+exception-handling -C panic=unwind -C llvm-args=-wasm-use-legacy-eh=false" \
			cargo rustc --manifest-path "$root/Cargo.toml" -p "$package" --crate-type staticlib \
			--target wasm32-wasip1 -Z build-std=std,panic_unwind
		lld="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
		exports=()
		while read -r resolver; do
			exports+=("--export=$resolver")
		done < <(tr '\n' ' ' < "$adapter" | grep -o 'define_prim!( *hlp_[a-z0-9_]*' | sed 's/define_prim!( *//')
		# Undefined symbols become imports that the main program resolves when
		# it loads the module.
		"$lld" -flavor wasm --experimental-pic -shared --no-entry --gc-sections --no-export-dynamic \
			--unresolved-symbols=import-dynamic "${exports[@]}" \
			--whole-archive "$root/target/wasm32-wasip1/debug/$archive" --no-whole-archive \
			-o "$work/counter.wasm"
		haxe -D wasm -cp "$root/haxe" -cp "$here/haxe" -cp "$classes" -main "$main" -hl "$work/main.hl"
		cd "$work"
		# Ash finds the library next to the output and exports the symbols it
		# imports. With fibers, a Haxe thread that blocks is suspended and the
		# other threads run.
		ASH_WASM_FIBERS=1 "$ash" --build main.wasm --target wasm32-wasip1 main.hl
		"$ash" run main.wasm | tee "$work/out.txt"
		;;
	emcc | gen-emcc)
		if [ "$runtime" = gen-emcc ]; then
			generate
			package=counter-gen-c archive=libcounter_gen.a main=Generated
			include="$work/gen/include" classes="$work/gen/haxe"
		else
			package=counter-c archive=libcounter.a main=Main
			include="$here/c" classes="$here/haxe/cpp"
		fi
		command -v emcc >/dev/null || { echo "emcc is not on PATH; source emsdk_env.sh first" >&2; exit 2; }
		# hxcpp would use CXX from the environment instead of emcc.
		unset CC CXX
		# The prebuilt std unwinds with wasm exceptions, as hxcpp does.
		cargo build --manifest-path "$root/Cargo.toml" -p "$package" \
			--target wasm32-unknown-emscripten
		# hxcpp assumes the build machine's 64-bit word size, and its UTF-16
		# strings call a Windows-only function under Emscripten.
		haxe -D wasm -cp "$root/haxe" -cp "$here/haxe" -cp "$classes" -main "$main" -cpp "$work" \
			-D emscripten -D HXCPP_M32 -D disable_unicode_strings \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$include" \
			-D COUNTER_LIB="$root/target/wasm32-unknown-emscripten/debug/$archive"
		node "$work/$main.js" | tee "$work/out.txt"
		;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

# The last checks run from the event loop, so a program that exits before
# them exits with status 0 without passing. Require the PASS line.
grep -q '^PASS' "$work/out.txt"
