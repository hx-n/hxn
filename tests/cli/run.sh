#!/usr/bin/env bash
# Creates a library with `hxn new` and runs it with `hxn run` against this
# checkout, following the readme's steps.
#   run.sh hl [vm]     vm is a HashLink-compatible executable, default `hl`,
#                      with any options for it: "ash --mode jit"
#   run.sh cpp
#   run.sh cppia       as a CPPIA script, in an hxcpp host that contains the library
#   run.sh wasm [ash]  built and run by Ash
#   run.sh emcc        hxcpp compiled to wasm, run by node; needs emcc on PATH
# With hl, ASH names the Ash command, with any options, that `hxn build`
# uses to build the program file ahead of time; without it, that step is
# skipped.
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

# hxn.cli, the library of the tools that haxelibs register, in Haxe's interpreter.
(cd "$root" && haxe -cp haxe -cp tests/cli/parse --run Main) </dev/null

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
# Windows line endings, as git may check the manifest out, are the same
# manifest.
sed 's/$/\r/' "$work/manifest" >greeter.hxn.json
"$hxn" check "${arguments[@]}" >/dev/null || { echo "FAILED: check refused a manifest with Windows line endings" >&2; exit 1; }
cp "$work/manifest" greeter.hxn.json
"$hxn" doctor --vm "${2:-hl}" | tr -d '\r' | tee "$work/found.txt"
grep -q ' ready$' "$work/found.txt" || { echo "FAILED: doctor found nothing ready" >&2; exit 1; }
host="$(rustc -vV | sed -n 's/^host: //p')"
[ -n "$host" ] || { echo "FAILED: rustc did not report its host target" >&2; exit 1; }
grep -Fqx "  $host" "$work/found.txt" || { echo "FAILED: doctor did not list the host's Rust standard library" >&2; exit 1; }
"$hxn" doctor --target "$host" --vm "${2:-hl}" | tr -d '\r' >"$work/binding-target.txt"
grep -Fq "cpp --target $host ready" "$work/binding-target.txt" || { echo "FAILED: doctor did not check the requested binding target" >&2; exit 1; }
if "$hxn" doctor --target hxn-not-a-rust-target >"$work/unknown-target.txt" 2>&1; then
	echo "FAILED: doctor accepted an unknown Rust target" >&2
	exit 1
fi
grep -q 'is not a target listed by rustc' "$work/unknown-target.txt" || { cat "$work/unknown-target.txt" >&2; exit 1; }
if "$hxn" doctor --target "$host" --wasm >"$work/doctor-wasm.txt" 2>&1; then
	echo "FAILED: doctor silently ignored --wasm" >&2
	exit 1
fi
grep -q 'do not pass --wasm' "$work/doctor-wasm.txt" || { cat "$work/doctor-wasm.txt" >&2; exit 1; }

# As a package: a program's build refers to the package and its macro, and
# to none of the paths of its classes, headers or library.
"$hxn" package "${packaged[@]}" --out "$work/package"
# hxn's classes, which a program gets from the `hxn` haxelib the package
# depends on, supply the tuple type and its constructor macro.
mkdir "$work/tuple"
cat >"$work/tuple/TupleProbe.hx" <<'EOF'
import hxn.Tuple;
class TupleProbe {
	static function main() {
		var pair:Tuple<String, Int> = Tuple.of("name", 42);
		var copied = pair.copy();
		copied.set(1, 7);
		var values = pair.toArray();
		values[0] = "changed";
		if (pair.at(0) != "name" || pair[1] != 42 || copied.at(1) != 7 || pair.length != 2)
			throw "tuple value changed";
		var nested = Tuple.of(pair, Tuple.of(true), 3, "four", 5.5);
		if (nested.length != 5 || !nested.at(1).at(0) || nested[4] != 5.5)
			throw "tuple positions changed";
	}
}
EOF
haxe -cp "$root/haxe" -cp "$work/package" -cp "$work/tuple" -main TupleProbe --interp
for invalid in \
	'var pair:hxn.Tuple<String, Int> = hxn.Tuple.of("name", "wrong");' \
	'var pair:hxn.Tuple<String, Int> = hxn.Tuple.of("name");' \
	'var pair = hxn.Tuple.of("name", 42); pair._0 = 7;' \
	'var pair = hxn.Tuple.of();' \
	'var pair = hxn.Tuple.of("name", 42); var number:Int = pair.at(0);' \
	'var pair = hxn.Tuple.of("name", 42); pair.set(1, "wrong");' \
	'var pair = hxn.Tuple.of("name", 42); pair.at(2);' \
	'var pair = hxn.Tuple.of("name", 42); pair.set(-1, 0);' \
	'var pair = hxn.Tuple.of("name", 42); var index = 1; pair.at(index);'; do
	printf '%s\n' 'class BadTuple { static function main() {' "$invalid" '} }' >"$work/tuple/BadTuple.hx"
	if haxe -cp "$root/haxe" -cp "$work/package" -cp "$work/tuple" -main BadTuple --no-output >"$work/tuple/error.txt" 2>&1; then
		echo "FAILED: the tuple type accepted $invalid" >&2
		exit 1
	fi
done
mkdir "$work/program"
(
	cd "$work/program"
	compile=(haxe -cp "$work/greeter/haxe" -cp "$root/haxe" -cp "$work/package" "$work/package/extraParams.hxml" -main Main)
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
			haxe -cp host -cp "$root/haxe" -cp "$work/package" "$work/package/extraParams.hxml" -main Host -cpp host/out \
				-D scriptable -dce no -D dll_export=host.classes
			"${compile[@]}" -cppia main.cppia -D dll_import=host.classes
			./host/out/Host main.cppia
			;;
		wasm)
			"${compile[@]}" -D wasm -hl main.hl
			ASH_WASM_FIBERS=1 "${2:-ash}" --build main.wasm --target wasm32-wasip1 --native-lib libgreeter.a main.hl
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

# A Haxe program with no crate of its own: a program file names the
# installed package with -lib, and `hxn run` and `hxn check` compile and run
# it. The package and hxn are installed in a haxelib repository of the
# program's directory. The package depends on this version of hxn, which a
# dev link does not satisfy, so hxn is installed with the layout that
# `haxelib install` gives.
mkdir "$work/file"
# The repository hides the global one, which holds hxcpp.
hxcpp="$(haxelib libpath hxcpp 2>/dev/null || true)"
(
	cd "$work/file"
	haxelib newrepo >/dev/null
	if [ -n "$hxcpp" ]; then
		haxelib dev hxcpp "$hxcpp" >/dev/null
	fi
	version="$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$root/haxelib.json")"
	installed=".haxelib/hxn/${version//./,}"
	mkdir -p "$installed"
	cp -R "$root/haxe" "$root/haxelib.json" "$installed/"
	printf '%s' "$version" >.haxelib/hxn/.current
	haxelib dev greeter "$work/package" >/dev/null
	# Git Bash converts paths in the arguments of a Windows program, not in a
	# file it reads, so the class path is written in Windows form there.
	classes="$work/greeter/haxe"
	if command -v cygpath >/dev/null; then
		classes="$(cygpath -m "$classes")"
	fi
	printf '%s\n' "-lib greeter" "-cp $classes" "-main Main" >greet.hxml
	"$hxn" run "${arguments[@]}" greet.hxml
) | tr -d '\r' | tee "$work/out.txt"
greeted "the program file"
(cd "$work/file" && "$hxn" check "${arguments[@]}" greet.hxml) | tr -d '\r' | tee "$work/out.txt"
greeted "the program file under check"
grep -q '^PASS: greet ran on ' "$work/out.txt" || { echo "FAILED: check did not pass the program file" >&2; exit 1; }
printf '%s\n' "-lib greeter" "-main Main" "-hl out.hl" >"$work/file/output.hxml"
if (cd "$work/file" && "$hxn" run hl output.hxml) >/dev/null 2>"$work/refused.txt"; then
	echo "FAILED: run accepted a program file with an output line" >&2
	exit 1
fi
grep -q 'hxn adds the output' "$work/refused.txt" || { cat "$work/refused.txt" >&2; exit 1; }

# `hxn build` builds the program file for distribution, without running it,
# and prints the command that runs it: for hl, the executable Ash builds
# ahead of time; for cpp, hxcpp's executable. `hxn package` puts it in a
# directory of its own.
built=()
case "$runtime" in
	hl) if [ -n "${ASH:-}" ]; then built=(hl --ash "$ASH"); fi ;;
	cpp) built=(cpp) ;;
esac
if [ ${#built[@]} -gt 0 ]; then
	(cd "$work/file" && "$hxn" build "${built[@]}" greet.hxml) | tr -d '\r' | tee "$work/built.txt"
	read -ra start <<<"$(sed -n '/To run it:$/{n;s/^  //;p;}' "$work/built.txt")"
	[ ${#start[@]} -gt 0 ] || { echo "FAILED: build did not print how to run the program" >&2; exit 1; }
	(cd "$work/file" && "${start[@]}") | tr -d '\r' | tee "$work/out.txt"
	greeted "the program file that build built"
	# `hxn package` copies the built program and the files it loads into one
	# directory, which runs after it is moved.
	(cd "$work/file" && "$hxn" package "${built[@]}" greet.hxml --out "$work/app") | tr -d '\r'
	mv "$work/app" "$work/moved-app"
	(cd "$work" && ./moved-app/Main) | tr -d '\r' | tee "$work/out.txt"
	greeted "the packaged program file"
fi

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
# Part of the API can come from the build script: hxn runs it before reading
# the source, and reads the file the source includes from OUT_DIR.
cat >build.rs <<'EOF'
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(
        out.join("written.rs"),
        "#[hxn::function(class = \"Written\")]\npub fn answer() -> i32 {\n    42\n}\n",
    )
    .unwrap();
}
EOF
echo 'include!(concat!(env!("OUT_DIR"), "/written.rs"));' >>src/lib.rs
cat >haxe/Other.hx <<'EOF'
class Other {
	static function main() {
		Sys.println('written: ${Written.answer()}');
	}
}
EOF
"$hxn" run "${arguments[@]}" | tr -d '\r' | tee "$work/out.txt"
if [ "$(grep '^written' "$work/out.txt")" != "written: 42" ]; then
	echo "FAILED: the function the build script wrote is not in the binding" >&2
	exit 1
fi
# haxelib.json fields the crate provides go into the package, and the ones
# it does not provide are reported.
cat >>Cargo.toml <<'EOF'
contributors = ["someone"]
haxelib = "greeter-haxe"
EOF
"$hxn" package "${packaged[@]}" --out "$work/listed" | tr -d '\r' >"$work/listed.txt"
if ! grep -q '"contributors": \["someone"\]' "$work/listed/haxelib.json" || ! grep -q '"name": "greeter-haxe"' "$work/listed/haxelib.json" || ! grep -q 'releasenote' "$work/listed.txt"; then
	echo "FAILED: the package's haxelib.json does not list what the crate gives" >&2
	exit 1
fi
# `hxn run` and `hxn check` refuse a cross-compilation target, since they
# run the program on this machine.
for refused in "run cpp" "run hl" "check"; do
	if "$hxn" $refused --target aarch64-apple-ios >/dev/null 2>&1; then
		echo "FAILED: \`hxn $refused --target\` was not refused" >&2
		exit 1
	fi
done
echo "PASS"

# Source hooks and the same extension from a packaged haxelib.
sed 's/haxelib = "greeter-haxe"/haxelib = "hxn-cli-fixture"/' Cargo.toml >Cargo.toml.plugin
mv Cargo.toml.plugin Cargo.toml
mkdir -p sources cli-assets
printf '\000\377plugin\n' >cli-assets/fixture.bin
cp "$here/plugin/Tool.hx" "$here/plugin/plugin-agent.mjs" sources/
cat >>Cargo.toml <<'PLUGIN'

[package.metadata.hxn.cli]
version = 1
namespace = "fixture"
files = ["cli-assets"]
command = ["haxe", "-cp", "{root}/sources", "--run", "Tool"]
hooks = ["before-check", "before-build", "after-build", "before-run", "after-run", "before-package", "after-package"]
PLUGIN
cat >haxe/Other.hx <<'PLUGIN'
class Other {
    static function main() {
        Sys.println('plugin: ${haxe.macro.Compiler.getDefine("plugin_fixture")}');
    }
}
PLUGIN
export HXN_PLUGIN_LOG="$work/plugin.log"
"$hxn" run "${arguments[@]}" >"$work/plugin-out.txt"
grep -q '^plugin: ready' "$work/plugin-out.txt" || { cat "$work/plugin-out.txt" >&2; exit 1; }
expected_hooks=$'before-build\nafter-build\nbefore-run\nafter-run'
[ "$(tr -d '\r' <"$HXN_PLUGIN_LOG")" = "$expected_hooks" ] || { cat "$HXN_PLUGIN_LOG" >&2; exit 1; }
if HXN_PLUGIN_FAIL=before-run "$hxn" run "${arguments[@]}" >"$work/plugin-failed.txt" 2>&1; then
    echo 'FAILED: hook failure did not stop the run' >&2; exit 1
fi
grep -q 'hxn-cli-fixture: before-run hook failed' "$work/plugin-failed.txt" || { cat "$work/plugin-failed.txt" >&2; exit 1; }
"$hxn" package "${packaged[@]}" --out "$work/plugin-package"
grep -q '"namespace":"fixture"' "$work/plugin-package/haxelib.json" || { echo 'FAILED: plugin metadata was not packaged' >&2; exit 1; }
test -f "$work/plugin-package/sources/Tool.hx"
test -f "$work/plugin-package/page/plugin-agent.mjs"
cmp cli-assets/fixture.bin "$work/plugin-package/cli-assets/fixture.bin"
mkdir "$work/plugin-client"
(
    cd "$work/plugin-client"
    haxelib newrepo
    haxelib dev hxn-cli-fixture "$work/plugin-package"
    "$hxn" plugins >plugins.txt
    grep -q '^fixture[[:space:]]hxn-cli-fixture' plugins.txt || { cat plugins.txt >&2; exit 1; }
    "$hxn" fixture echo 'two words' '$(literal)' | tr -d '\r' >command.txt
    grep -Fqx 'echo|two words|$(literal)' command.txt || { cat command.txt >&2; exit 1; }
    set +e
    "$hxn" fixture fail >/dev/null
    status=$?
    set -e
    [ "$status" = 7 ] || { echo "FAILED: plugin exit status was $status" >&2; exit 1; }
    haxelib dev hxn "$root"
    "$hxn" new consumer --hxn "$root"
    cd consumer
    : >"$HXN_PLUGIN_LOG"
    if "$hxn" check hl --target not-a-target >/dev/null 2>&1; then exit 1; fi
    [ ! -s "$HXN_PLUGIN_LOG" ] || { echo 'FAILED: an unselected installed plugin ran' >&2; exit 1; }
    cat >>Cargo.toml <<'PLUGIN'

[package.metadata.hxn]
haxe-flags = ["-lib", "hxn", "-lib", "hxn-cli-fixture"]
PLUGIN
    if "$hxn" check hl --target not-a-target >/dev/null 2>&1; then exit 1; fi
    [ "$(tr -d '\r' <"$HXN_PLUGIN_LOG")" = before-check ] || { echo 'FAILED: the selected installed plugin did not run' >&2; cat "$HXN_PLUGIN_LOG" >&2; exit 1; }
)
