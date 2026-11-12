#!/usr/bin/env bash
# Imports the C library in this directory with `hxn import`, applies the
# edited WebIDL, and runs Main.hx against the result.
#   run.sh hl [vm]     on HashLink, or the VM named with its options
#   run.sh cpp
#   run.sh cppia
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
#
# It checks that the importer writes imported.webidl exactly, and that the
# program prints expected.txt.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | cppia | emcc}"
case "$runtime" in
	hl) arguments=(hl --vm "${2:-hl}") ;;
	cpp | cppia) arguments=("$runtime") ;;
	emcc) arguments=(cpp --wasm) ;;
	*)
		echo "unknown runtime \`$runtime\`" >&2
		exit 1
		;;
esac
case "$(uname -s)" in
	MINGW* | MSYS* | CYGWIN*)
		here="$(cygpath -m "$here")"
		root="$(cygpath -m "$root")"
		;;
esac

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"
# Outside the checkout, where cargo would take the crate for an unlisted
# member of hxn's workspace. Fresh each run: `hxn import` refuses a
# directory that exists.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
crate="$work/tally"

"$hxn" import "$here/tally.h" --source "$here/tally.c" --hxn "$root" --out "$crate"
if ! diff -u "$here/imported.webidl" "$crate/tally.webidl"; then
	echo "FAILED: the importer did not write imported.webidl" >&2
	exit 1
fi
# The edit a user makes: `tally_add` returns a status code.
cp "$here/tally.webidl" "$crate/tally.webidl"
cp "$here/Main.hx" "$crate/haxe/Main.hx"

out="$work/out.txt"
(cd "$crate" && "$hxn" run "${arguments[@]}") | tr -d '\r' | tee "$out"
expected="$here/expected.txt"
if [ "$(tail -n "$(wc -l <"$expected")" "$out")" != "$(cat "$expected")" ]; then
	echo "FAILED: the program did not print what expected.txt lists" >&2
	exit 1
fi
echo "PASS"
