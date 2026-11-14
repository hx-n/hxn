#!/usr/bin/env bash
# Runs every example under examples/ on a mobile toolchain and compares its
# output with expected.txt.
#   run.sh ios-sim [cpp|hl] [device]   in an iOS simulator. `device` is a name
#                                      or UDID; the default is the first booted
#                                      simulator, or the first available
#                                      iPhone, booted for the run
#   run.sh android [cpp|hl] [serial]   on the Android device or emulator `adb`
#                                      reaches
#
# With `cpp`, the default, each library is packaged with
# `hxn package cpp --target`, and the example is compiled against the
# package with hxcpp's defines for the platform, the way an app's build
# would do it. hxcpp only builds a library on these platforms, so the script
# links it into an executable with main.cpp (iOS) or main.c (Android) and
# runs that. Android needs ANDROID_NDK_ROOT.
#
# hxcpp builds simulator libraries for x86-64 only, so on Apple silicon the
# simulator runs them through Rosetta.
#
# With `hl`, the library is packaged with `hxn package hl --target`, the
# example is compiled to HashLink bytecode, and Ash builds the executable
# with the library's archive as a `--native-lib`. ASH names the Ash command,
# `ash` by default; it needs its runtime for the platform (ash's docs/aot.md).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
usage="usage: run.sh ios-sim [cpp|hl] [device] | android [cpp|hl] [serial]"
kind="${1:?$usage}"
runtime=cpp
case "${2:-}" in
	cpp | hl)
		runtime="$2"
		shift
		;;
esac
work="$root/target/mobile-$kind-$runtime"
mkdir -p "$work"
ash="${ASH:-ash}"

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

case "$kind" in
	ios-sim)
		triple=x86_64-apple-ios
		defines=(-D iphonesim -D HXCPP_M64)
		# Ash builds for the simulator's own architecture.
		if [ "$runtime" = hl ] && [ "$(uname -m)" = arm64 ]; then
			triple=aarch64-apple-ios-sim
		fi
		device="${2:-}"
		if [ -z "$device" ]; then
			device="$(xcrun simctl list devices booted | sed -n 's/.*(\([0-9A-F-]\{36\}\)) (Booted).*/\1/p' | head -1)"
		fi
		if [ -z "$device" ]; then
			device="$(xcrun simctl list devices available | sed -n 's/.*iPhone.*(\([0-9A-F-]\{36\}\)) (Shutdown).*/\1/p' | head -1)"
			[ -n "$device" ] || { echo "no iOS simulator is installed" >&2; exit 1; }
			xcrun simctl boot "$device"
			trap 'xcrun simctl shutdown "$device" >/dev/null 2>&1 || true' EXIT
		fi
		xcrun simctl bootstatus "$device" >/dev/null
		link() {
			xcrun --sdk iphonesimulator clang++ -arch x86_64 -mios-simulator-version-min=12.0 \
				"$here/main.cpp" "$1"/out/libMain*.a -o "$1/program"
		}
		run() {
			xcrun simctl spawn "$device" "$1/program"
		}
		run_hl() {
			run "$1"
		}
		;;
	android)
		triple=aarch64-linux-android
		adb=(adb ${2:+-s "$2"})
		if [ "$runtime" = cpp ]; then
			: "${ANDROID_NDK_ROOT:?set ANDROID_NDK_ROOT to the NDK hxcpp is to build with}"
			tools="$(echo "$ANDROID_NDK_ROOT"/toolchains/llvm/prebuilt/*/bin)"
			# hxcpp 4.3.2 defaults ANDROID_HOST to a directory newer NDKs do
			# not have, and runs a strip tool they no longer ship;
			# HXCPP_DEBUG_LINK skips stripping.
			defines=(-D android -D HXCPP_ARM64 -D HXCPP_DEBUG_LINK
				-D "ANDROID_HOST=$(basename "$(dirname "$tools")")")
		fi
		link() {
			# hxcpp builds a shared library with the Rust library linked in.
			# The executable is C so that it does not depend on
			# libc++_shared.so.
			"$tools/aarch64-linux-android21-clang" "$here/main.c" -L"$1/out" -lMain-64 -o "$1/program"
		}
		run_hl() {
			"${adb[@]}" push "$1/program" /data/local/tmp/ >/dev/null 2>&1
			"${adb[@]}" shell "cd /data/local/tmp && ./program"
		}
		run() {
			"${adb[@]}" push "$1/program" "$1/out/libMain-64.so" /data/local/tmp/ >/dev/null 2>&1
			"${adb[@]}" logcat -c
			"${adb[@]}" shell "cd /data/local/tmp && LD_LIBRARY_PATH=. ./program"
			# On Android, hxcpp writes program output to logcat.
			"${adb[@]}" logcat -d -v raw -s trace | grep -v '^--------- '
		}
		;;
	*)
		echo "unknown kind \`$kind\`: it is ios-sim or android" >&2
		exit 1
		;;
esac

for example in "$root"/examples/*/; do
	name="$(basename "$example")"
	[ -f "$example/expected.txt" ] || continue
	echo "== $name, $kind, $runtime"
	dir="$work/$name"
	mkdir -p "$dir"
	package="$work/package-$name"
	(cd "$example" && "$hxn" package "$runtime" --target "$triple" --out "$package" >/dev/null)
	if [ "$runtime" = hl ]; then
		# hxn.Bind copies the library's archive next to main.hl.
		(cd "$dir" && haxe -cp "$example/haxe" -cp "$root/haxe" -cp "$package" "$package/extraParams.hxml" \
			-D "hxn_triple=$triple" -main Main -hl main.hl >"$dir/build.txt" 2>&1 &&
			$ash --build program --target "$triple" \
				$(for archive in lib*.a; do printf -- '--native-lib %s ' "$archive"; done) \
				main.hl >>"$dir/build.txt" 2>&1)
	else
		(cd "$dir" && haxe -cp "$example/haxe" -cp "$root/haxe" -cp "$package" "$package/extraParams.hxml" \
			-main Main -cpp out "${defines[@]}" >"$dir/build.txt" 2>&1 &&
			link "$dir")
	fi || {
		tail -30 "$dir/build.txt" >&2
		echo "FAILED: $name did not build for $kind with $runtime" >&2
		exit 1
	}
	if [ "$runtime" = hl ]; then
		run_hl "$dir"
	else
		run "$dir"
	fi | tr -d '\r' | tee "$dir/out.txt"
	expected="$example/expected.txt"
	if [ "$(tail -n "$(wc -l <"$expected")" "$dir/out.txt")" != "$(cat "$expected")" ]; then
		echo "FAILED: $name did not print what expected.txt lists" >&2
		exit 1
	fi
done
echo "PASS"
