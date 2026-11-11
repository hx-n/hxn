#!/usr/bin/env bash
# Makes a library with `hxn new` and runs it with `hxn run`, as someone
# following the readme would, against this checkout.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`,
#                      with any options for it: "ash --mode jit"
#   run.sh cpp
#   run.sh wasm [ash]  built and run by Ash
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | wasm [ash] | emcc}"

case "$runtime" in
	hl) arguments=(run hl --vm "${2:-hl}") ;;
	cpp) arguments=(run cpp) ;;
	wasm) arguments=(run hl --wasm --vm "${2:-ash}") ;;
	emcc) arguments=(run cpp --wasm) ;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

# Outside the checkout: a crate under it would be taken for a stray member
# of its workspace.
work="$(mktemp -d)"
trap 'rm -r "$work"' EXIT
cd "$work"
"$hxn" new greeter --hxn "$root"
cd greeter

# Less the carriage returns a Windows program ends its lines with.
"$hxn" "${arguments[@]}" | tr -d '\r' | tee "$work/out.txt"
# The program's own lines, whatever the VM prints around them.
expected=$'Hello, Haxe!\nHello, Rust!\n2 greeted\nrefused: nobody to greet'
if [ "$(grep -E '^(Hello, |2 greeted|refused: )' "$work/out.txt")" != "$expected" ]; then
	echo "FAILED: the program did not print what it should" >&2
	exit 1
fi
test -f greeter.hxn.json || { echo "FAILED: no manifest was written" >&2; exit 1; }

# What the crate's settings and the command line pass on: another main
# class, a flag for haxe, and arguments for the program.
cat >haxe/Other.hx <<'EOF'
class Other {
	static function main() {
		var word = haxe.macro.Compiler.getDefine("word");
		Sys.println('$word, ${Sys.args().join(" and ")}');
	}
}
EOF
cat >>Cargo.toml <<'EOF'

[package.metadata.hxn]
main = "Other"
haxe-flags = ["-D", "word=Howdy"]
EOF
"$hxn" "${arguments[@]}" -- one two | tr -d '\r' | tee "$work/out.txt"
if [ "$(grep '^Howdy' "$work/out.txt")" != "Howdy, one and two" ]; then
	echo "FAILED: the settings or the arguments did not reach the program" >&2
	exit 1
fi
echo "PASS"
