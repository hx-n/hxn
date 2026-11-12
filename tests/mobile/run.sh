#!/usr/bin/env bash
# Runs every example under examples/ on a mobile toolchain and compares its
# output with expected.txt.
#   run.sh ios-sim [device]   in an iOS simulator. `device` is a name or UDID;
#                             the default is the first booted simulator, or
#                             the first available iPhone, booted for the run
#   run.sh android [serial]   on the Android device or emulator `adb` reaches;
#                             needs ANDROID_NDK_ROOT
#
# Each library is packaged with `hxn package cpp --target`, and the example
# is compiled against the package with hxcpp's defines for the platform, the
# way an app's build would do it. hxcpp only builds a library on these
# platforms, so the script links it into an executable with main.cpp (iOS)
# or main.c (Android) and runs that.
#
# hxcpp builds simulator libraries for x86-64 only, so on Apple silicon the
# simulator runs them through Rosetta.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
kind="${1:?usage: run.sh ios-sim [device] | android [serial]}"
work="$root/target/mobile-$kind"
mkdir -p "$work"

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
hxn="$root/target/debug/hxn"

case "$kind" in
	ios-sim)
		triple=x86_64-apple-ios
		defines=(-D iphonesim -D HXCPP_M64)
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
		;;
	android)
		triple=aarch64-linux-android
		: "${ANDROID_NDK_ROOT:?set ANDROID_NDK_ROOT to the NDK hxcpp is to build with}"
		tools="$(echo "$ANDROID_NDK_ROOT"/toolchains/llvm/prebuilt/*/bin)"
		# hxcpp 4.3.2 defaults ANDROID_HOST to a directory newer NDKs do not
		# have, and runs a strip tool they no longer ship; HXCPP_DEBUG_LINK
		# skips stripping.
		defines=(-D android -D HXCPP_ARM64 -D HXCPP_DEBUG_LINK
			-D "ANDROID_HOST=$(basename "$(dirname "$tools")")")
		adb=(adb ${2:+-s "$2"})
		link() {
			# hxcpp builds a shared library with the Rust library linked in.
			# The executable is C so that it does not depend on
			# libc++_shared.so.
			"$tools/aarch64-linux-android21-clang" "$here/main.c" -L"$1/out" -lMain-64 -o "$1/program"
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
	echo "== $name, $kind"
	dir="$work/$name"
	mkdir -p "$dir"
	(cd "$example" && "$hxn" package cpp --target "$triple" --out "$work/package-$name" >/dev/null)
	(cd "$dir" && haxe -cp "$example/haxe" -cp "$work/package-$name" "$work/package-$name/extraParams.hxml" \
		-main Main -cpp out "${defines[@]}" >"$dir/build.txt" 2>&1) || {
		tail -30 "$dir/build.txt" >&2
		echo "FAILED: $name did not compile for $kind" >&2
		exit 1
	}
	link "$dir" "$name"
	run "$dir" | tr -d '\r' | tee "$dir/out.txt"
	expected="$example/expected.txt"
	if [ "$(tail -n "$(wc -l <"$expected")" "$dir/out.txt")" != "$(cat "$expected")" ]; then
		echo "FAILED: $name did not print what expected.txt lists" >&2
		exit 1
	fi
done
echo "PASS"
