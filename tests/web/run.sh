#!/usr/bin/env bash
# Runs the timer library under Emscripten with `hxn run cpp --wasm`, its futures settled by
# JavaScript timers after `main` returns, and compares what it prints with expected.txt.
#   run.sh node      run by node
#   run.sh browser   a web page, served by emrun to headless Chrome
#   run.sh node-threads and browser-threads
#                    the same, threaded: Emscripten's pthreads over shared memory
# Needs emcc and emrun on PATH: source emsdk_env.sh first.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
case "${1:?usage: run.sh node | browser | node-threads | browser-threads}" in
	node) flags=(--wasm) ;;
	browser) flags=(--wasm --browser) ;;
	node-threads) flags=(--wasm --threads) ;;
	browser-threads) flags=(--wasm --threads --browser) ;;
	*)
		echo "unknown mode: $1" >&2
		exit 2
		;;
esac

cargo build --manifest-path "$root/Cargo.toml" -p hxn-cli
out="$(mktemp)"
trap 'rm "$out"' EXIT
(cd "$here" && "$root/target/debug/hxn" run cpp "${flags[@]}") | tr -d '\r' | tee "$out"
if [ "$(tail -n "$(wc -l <"$here/expected.txt")" "$out")" != "$(cat "$here/expected.txt")" ]; then
	echo "FAILED: the timer did not print what expected.txt lists" >&2
	exit 1
fi
echo "PASS"
