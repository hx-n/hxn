#!/usr/bin/env bash
# Builds the counter binding for one runtime and runs haxe/Main.hx on it.
#   run.sh hl [vm]   vm is a HashLink-compatible executable, default `hl`
#   run.sh cpp
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp}"
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
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

# The last checks run from the event loop; a program that ends before them
# exits cleanly without having passed.
grep -q '^PASS' "$work/out.txt"
