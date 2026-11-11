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
	hl) arguments=(hl --vm "${2:-hl}") ;;
	cpp) arguments=(cpp) ;;
	wasm) arguments=(hl --wasm --vm "${2:-ash}") ;;
	emcc) arguments=(cpp --wasm) ;;
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

# The program's own lines, whatever the VM prints around them.
expected=$'Hello, Haxe!\nHello, Rust!\n2 greeted\nrefused: nobody to greet'
greeted() {
	if [ "$(grep -E '^(Hello, |2 greeted|refused: )' "$work/out.txt")" != "$expected" ]; then
		echo "FAILED: $1 did not print what it should" >&2
		exit 1
	fi
}

# Less the carriage returns a Windows program ends its lines with.
"$hxn" run "${arguments[@]}" | tr -d '\r' | tee "$work/out.txt"
greeted "the program"
test -f greeter.hxn.json || { echo "FAILED: no manifest was written" >&2; exit 1; }

# As a package: a program's build names it and the macro it carries, and
# nothing of where its classes, headers or library are.
"$hxn" package "${arguments[@]}" --out "$work/package"
mkdir "$work/program"
(
	cd "$work/program"
	compile=(haxe -cp "$work/greeter/haxe" -cp "$work/package" "$work/package/extraParams.hxml" -main Main)
	case "$runtime" in
		hl)
			"${compile[@]}" -hl main.hl
			# The library is beside the program, where the VM's loader looks.
			read -ra vm <<<"${2:-hl}"
			"${vm[@]}" main.hl
			;;
		cpp)
			"${compile[@]}" -cpp out
			./out/Main
			;;
		wasm)
			"${compile[@]}" -D wasm -hl main.hl
			ASH_WASM_FIBERS=1 "${2:-ash}" --build main.wasm --target wasm32-wasip1 main.hl
			"${2:-ash}" run main.wasm
			;;
		emcc)
			# hxcpp takes CXX from the environment in place of emcc, and its
			# UTF-16 strings reach for a Windows-only call under Emscripten.
			unset CC CXX
			"${compile[@]}" -D wasm -D emscripten -D disable_unicode_strings -cpp out
			node out/Main.js
			;;
	esac
) | tr -d '\r' | tee "$work/out.txt"
greeted "the program built against the package"

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
"$hxn" run "${arguments[@]}" -- one two | tr -d '\r' | tee "$work/out.txt"
if [ "$(grep '^Howdy' "$work/out.txt")" != "Howdy, one and two" ]; then
	echo "FAILED: the settings or the arguments did not reach the program" >&2
	exit 1
fi
echo "PASS"
