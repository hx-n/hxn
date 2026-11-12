#!/usr/bin/env bash
# Imports the C library in this directory with `hxn import`, applies the
# edited WebIDL, and runs Main.hx against the result.
#   run.sh hl [vm]     on HashLink, or the VM named with its options
#   run.sh cpp
#   run.sh cppia
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
#
# It checks that the importer writes imported.webidl exactly, and that the
# program prints expected.txt. Where sqlite3 is installed, it also imports
# sqlite3.h unedited and runs sqlite/Main.hx against it.
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
# And a function written by hand beside the bindings.
cat "$here/extra.rs" >> "$crate/src/lib.rs"

# Runs the program in `$1` and compares what it prints with `$2`.
check() {
	out="$work/out.txt"
	(cd "$1" && "$hxn" run "${arguments[@]}") | tr -d '\r' | tee "$out"
	if [ "$(tail -n "$(wc -l <"$2")" "$out")" != "$(cat "$2")" ]; then
		echo "FAILED: $1 did not print what $2 lists" >&2
		exit 1
	fi
}
check "$crate" "$here/expected.txt"

# A real library, imported from its installed header with no edits.
# Where clang finds sqlite3.h, if it does. Read whole, so that neither a
# missing header nor an early end of the pipe stops the script.
preprocessed="$(printf '#include <sqlite3.h>\n' | clang -E -x c - 2>/dev/null || true)"
sqlite="$(printf '%s\n' "$preprocessed" | sed -n 's/^# [0-9]* "\(.*sqlite3\.h\)".*/\1/p' | sed -n 1p)"
if [ -n "$sqlite" ] && [ "$runtime" != emcc ]; then
	"$hxn" import "$sqlite" --link sqlite3 --hxn "$root" --out "$work/sqlite3" >/dev/null
	cp "$here/sqlite/Main.hx" "$work/sqlite3/haxe/Main.hx"
	check "$work/sqlite3" "$here/sqlite/expected.txt"
else
	echo "sqlite3: skipped, no sqlite3.h for this runtime"
fi
echo "PASS"
