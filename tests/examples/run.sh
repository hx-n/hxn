#!/usr/bin/env bash
# Runs each library under examples/ with `hxn run`, as its readme describes,
# and compares what it prints with its expected.txt.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`,
#                      with any options for it: "ash --mode jit"
#   run.sh cpp
#   run.sh cppia       as a script, in an hxcpp program that holds the library
#   run.sh wasm [ash]  built and run by Ash
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | cppia | wasm [ash] | emcc}"

case "$runtime" in
	hl) arguments=(run hl --vm "${2:-hl}") ;;
	cpp) arguments=(run cpp) ;;
	cppia) arguments=(run cppia) ;;
	wasm) arguments=(run hl --wasm --vm "${2:-ash}") ;;
	emcc) arguments=(run cpp --wasm) ;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

out="$(mktemp)"
trap 'rm "$out"' EXIT
for example in "$root"/examples/*/; do
	name="$(basename "$example")"
	echo "== $name"
	# Less the carriage returns a Windows program ends its lines with.
	(cd "$example" && "$hxn" "${arguments[@]}") | tr -d '\r' | tee "$out"
	# The program's lines are the last ones, after whatever the build printed.
	expected="$example/expected.txt"
	if [ "$(tail -n "$(wc -l <"$expected")" "$out")" != "$(cat "$expected")" ]; then
		echo "FAILED: $name did not print what expected.txt lists" >&2
		exit 1
	fi
done
echo "PASS"
