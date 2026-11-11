#!/usr/bin/env bash
# Builds the counter binding for one runtime and runs haxe/Main.hx on it.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`
#   run.sh cpp
#   run.sh wasm [ash]  as a side module of an Ash wasm program, run by `ash run`
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | wasm [ash] | emcc}"
work="$root/target/counter-$runtime"
mkdir -p "$work"

case "$(uname -s)" in
	Darwin) dylib=dylib ;;
	*) dylib=so ;;
esac

case "$runtime" in
	hl)
		vm="$(command -v "${2:-hl}")"
		cargo build --manifest-path "$root/Cargo.toml" -p counter-hl
		cp "$root/target/debug/libcounter_hl.$dylib" "$work/counter.hdll"
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/hl" -main Main -hl "$work/main.hl"
		cd "$work"
		# The VM resolves a bare library name through the OS loader.
		vm_dir="$(dirname "$vm")"
		DYLD_LIBRARY_PATH="$work:$vm_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
		LD_LIBRARY_PATH="$work:$vm_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
			"$vm" main.hl | tee "$work/out.txt"
		;;
	cpp)
		cargo build --manifest-path "$root/Cargo.toml" -p counter-c
		haxe -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/cpp" -main Main -cpp "$work" \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$here/c" \
			-D COUNTER_LIB="$root/target/debug/libcounter.a"
		"$work/Main" | tee "$work/out.txt"
		;;
	wasm)
		ash="$(command -v "${2:-ash}")"
		# A side module is position-independent, which the shipped std is not,
		# so std is rebuilt; only the archive is built, since nothing here can
		# link a wasm library whose imports are the program's.
		RUSTFLAGS="-C relocation-model=pic -C target-feature=+mutable-globals" \
			cargo rustc --manifest-path "$root/Cargo.toml" -p counter-hl --crate-type staticlib \
			--target wasm32-wasip1 -Z build-std=std,panic_abort
		lld="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
		exports=()
		while read -r resolver; do
			exports+=("--export=$resolver")
		done < <(tr '\n' ' ' < "$here/hl/src/lib.rs" | grep -o 'define_prim!( *hlp_[a-z_]*' | sed 's/define_prim!( *//')
		# Undefined symbols become imports the program answers when it loads this.
		"$lld" -flavor wasm --experimental-pic -shared --no-entry --gc-sections --no-export-dynamic \
			--unresolved-symbols=import-dynamic "${exports[@]}" \
			--whole-archive "$root/target/wasm32-wasip1/debug/libcounter_hl.a" --no-whole-archive \
			-o "$work/counter.wasm"
		haxe -D wasm -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/hl" -main Main -hl "$work/main.hl"
		cd "$work"
		# Ash finds the library beside the output and exports what it imports.
		# With fibers a Haxe thread that waits suspends and lets the others run.
		ASH_WASM_FIBERS=1 "$ash" --build main.wasm --target wasm32-wasip1 main.hl
		"$ash" run main.wasm | tee "$work/out.txt"
		;;
	emcc)
		command -v emcc >/dev/null || { echo "emcc is not on PATH; source emsdk_env.sh first" >&2; exit 2; }
		# hxcpp takes CXX from the environment in place of emcc.
		unset CC CXX
		# Aborting panics, and a std rebuilt to match: the shipped one unwinds
		# with wasm exceptions, which hxcpp's JavaScript-based ones cannot link.
		RUSTFLAGS="-C panic=abort" \
			cargo build --manifest-path "$root/Cargo.toml" -p counter-c \
			--target wasm32-unknown-emscripten -Z build-std=std,panic_abort
		# hxcpp assumes the host's 64 bits, and its UTF-16 strings reach for a
		# Windows-only call under Emscripten.
		haxe -D wasm -cp "$root/haxe" -cp "$here/haxe" -cp "$here/haxe/cpp" -main Main -cpp "$work" \
			-D emscripten -D HXCPP_M32 -D disable_unicode_strings \
			-D HXN_INCLUDE="$root/include" \
			-D COUNTER_INCLUDE="$here/c" \
			-D COUNTER_LIB="$root/target/wasm32-unknown-emscripten/debug/libcounter.a"
		node "$work/Main.js" | tee "$work/out.txt"
		;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

# The last checks run from the event loop; a program that ends before them
# exits cleanly without having passed.
grep -q '^PASS' "$work/out.txt"
