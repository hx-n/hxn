#!/usr/bin/env bash
# Makes a library with `hxn new` and runs it with `hxn run`, as someone
# following the readme would, against this checkout.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`
#   run.sh cpp
#   run.sh wasm [ash]  built and run by Ash
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | wasm [ash] | emcc}"

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

# Outside the checkout: a crate under it would be taken for a stray member
# of its workspace.
work="$(mktemp -d)"
trap 'rm -r "$work"' EXIT
cd "$work"
"$hxn" new greeter --hxn "$root"
cd greeter

case "$runtime" in
	hl) "$hxn" run hl --vm "${2:-hl}" | tee "$work/out.txt" ;;
	cpp) "$hxn" run cpp | tee "$work/out.txt" ;;
	wasm) "$hxn" run hl --wasm --vm "${2:-ash}" | tee "$work/out.txt" ;;
	emcc) "$hxn" run cpp --wasm | tee "$work/out.txt" ;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

# The program's own lines, whatever the VM prints around them.
expected=$'Hello, Haxe!\nHello, Rust!\n2 greeted\nrefused: nobody to greet'
if [ "$(grep -E '^(Hello, |2 greeted|refused: )' "$work/out.txt")" != "$expected" ]; then
	echo "FAILED: the program did not print what it should" >&2
	exit 1
fi
test -f greeter.hxn.json || { echo "FAILED: no manifest was written" >&2; exit 1; }
echo "PASS"
