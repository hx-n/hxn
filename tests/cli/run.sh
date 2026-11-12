#!/usr/bin/env bash
# Creates a library with `hxn new` and runs it with `hxn run` against this
# checkout, following the readme's steps.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`,
#                      with any options for it: "ash --mode jit"
#   run.sh cpp
#   run.sh cppia       as a CPPIA script, in an hxcpp host that contains the library
#   run.sh wasm [ash]  built and run by Ash
#   run.sh emcc        hxcpp through Emscripten, run by node; needs emcc on PATH
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runtime="${1:?usage: run.sh hl [vm] | cpp | cppia | wasm [ash] | emcc}"

# Arguments for `run`, and for `package` where they differ: a CPPIA script
# uses the hxcpp package.
case "$runtime" in
	hl) arguments=(hl --vm "${2:-hl}") ;;
	cpp) arguments=(cpp) ;;
	cppia) arguments=(cppia) packaged=(cpp) ;;
	wasm) arguments=(hl --wasm --vm "${2:-ash}") ;;
	emcc) arguments=(cpp --wasm) ;;
	*)
		echo "unknown runtime: $runtime" >&2
		exit 2
		;;
esac

if [ -z "${packaged+set}" ]; then
	packaged=("${arguments[@]}")
fi

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

# Outside the checkout, because cargo would treat a crate inside it as an
# unlisted member of the workspace.
work="$(mktemp -d)"
trap 'rm -r "$work"' EXIT
cd "$work"
"$hxn" new greeter --hxn "$root"
cd greeter

# The program's output lines, ignoring anything else the VM prints.
expected=$'Hello, Haxe!\nHello, Rust!\n2 greeted\nrefused: nobody to greet'
greeted() {
	if [ "$(grep -E '^(Hello, |2 greeted|refused: )' "$work/out.txt")" != "$expected" ]; then
		echo "FAILED: $1 did not print what it should" >&2
		exit 1
	fi
}

# Strip the carriage returns of Windows line endings.
"$hxn" run "${arguments[@]}" | tr -d '\r' | tee "$work/out.txt"
greeted "the program"
test -f greeter.hxn.json || { echo "FAILED: no manifest was written" >&2; exit 1; }

# The same through `check`, which also verifies the committed manifest
# against the source, and `doctor`, which must report this runtime as ready.
"$hxn" check "${arguments[@]}" | tr -d '\r' | tee "$work/out.txt"
greeted "the program under check"
grep -q '^PASS: greeter ran on ' "$work/out.txt" || { echo "FAILED: check did not pass" >&2; exit 1; }
cp greeter.hxn.json "$work/manifest"
echo >>greeter.hxn.json
if "$hxn" check "${arguments[@]}" >/dev/null 2>"$work/refused.txt"; then
	echo "FAILED: check passed a manifest that is not the source's" >&2
	exit 1
fi
grep -q 'is not what the source gives' "$work/refused.txt" || { cat "$work/refused.txt" >&2; exit 1; }
cp "$work/manifest" greeter.hxn.json
"$hxn" doctor --vm "${2:-hl}" | tr -d '\r' | tee "$work/found.txt"
grep -q ' ready$' "$work/found.txt" || { echo "FAILED: doctor found nothing ready" >&2; exit 1; }

# As a package: a program's build refers to the package and its macro, and
# to none of the paths of its classes, headers or library.
"$hxn" package "${packaged[@]}" --out "$work/package"
test -f "$work/package/greeter.hxn.webidl" || { echo "FAILED: the package has no WebIDL" >&2; exit 1; }
mkdir "$work/program"
(
	cd "$work/program"
	compile=(haxe -cp "$work/greeter/haxe" -cp "$work/package" "$work/package/extraParams.hxml" -main Main)
	case "$runtime" in
		hl)
			"${compile[@]}" -hl main.hl
			# The library is next to the program, where the VM's loader looks.
			read -ra vm <<<"${2:-hl}"
			"${vm[@]}" main.hl
			;;
		cpp)
			"${compile[@]}" -cpp out
			./out/Main
			;;
		cppia)
			# The host is a separate hxcpp program that runs scripts and is
			# built against the package. The script is compiled against both
			# the package and the host.
			mkdir host
			cat >host/Host.hx <<'EOF'
// hxcpp's own zlib takes `fdopen` for missing on a Mac unless it is a macro.
@:buildXml('<compiler id="darwin" exe="clang++" if="macos"><flag value="-Dfdopen=fdopen"/></compiler>')
class Host {
	static function main() {
		cpp.cppia.Host.main();
	}
}
EOF
			haxe -cp host -cp "$work/package" "$work/package/extraParams.hxml" -main Host -cpp host/out \
				-D scriptable -dce no -D dll_export=host.classes
			"${compile[@]}" -cppia main.cppia -D dll_import=host.classes
			./host/out/Host main.cppia
			;;
		wasm)
			"${compile[@]}" -D wasm -hl main.hl
			ASH_WASM_FIBERS=1 "${2:-ash}" --build main.wasm --target wasm32-wasip1 main.hl
			"${2:-ash}" run main.wasm
			;;
		emcc)
			# hxcpp would use CXX from the environment instead of emcc, and its
			# UTF-16 strings call a Windows-only function under Emscripten.
			unset CC CXX
			"${compile[@]}" -D wasm -D emscripten -D disable_unicode_strings -cpp out
			node out/Main.js
			;;
	esac
) | tr -d '\r' | tee "$work/out.txt"
greeted "the program built against the package"

# Settings from the crate and the command line are passed through: another
# main class, a haxe flag, and program arguments.
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
# haxelib.json fields the crate provides go into the package, and the ones
# it does not provide are reported.
cat >>Cargo.toml <<'EOF'
contributors = ["someone"]
EOF
"$hxn" package "${packaged[@]}" --out "$work/listed" | tr -d '\r' >"$work/listed.txt"
if ! grep -q '"contributors": \["someone"\]' "$work/listed/haxelib.json" || ! grep -q 'releasenote' "$work/listed.txt"; then
	echo "FAILED: the package's haxelib.json does not list what the crate gives" >&2
	exit 1
fi
# `hxn run` refuses a cross-compilation target, and `hxn build hl` refuses
# to cross-compile an .hdll.
for refused in "run cpp" "build hl"; do
	if "$hxn" $refused --target aarch64-apple-ios >/dev/null 2>&1; then
		echo "FAILED: \`hxn $refused --target\` was not refused" >&2
		exit 1
	fi
done
echo "PASS"
