#!/usr/bin/env python3
"""Run the shared logging fixture on an iOS simulator or Android device/emulator."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("ios-sim", "android"))
    parser.add_argument("runtime", choices=("hl", "cpp"))
    parser.add_argument("--device", required=True)
    parser.add_argument("--ash", default="ash")
    args = parser.parse_args()
    work = ROOT / "target/log-mobile" / args.platform / args.runtime
    work.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop("CC", None)
    env.pop("CXX", None)
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")

    def run(argv, cwd=ROOT, timeout=300):
        result = subprocess.run([str(v) for v in argv], cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
        with (work / "run.log").open("a") as log:
            log.write(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"command failed; see {work / 'run.log'}")
        return result.stdout + result.stderr

    (work / "run.log").write_text("")
    probe = uuid.uuid4().hex
    (work / "LogProbe.hx").write_text('class LogProbe { public static final id = ' + json.dumps(probe) + '; }\n')
    triple = "aarch64-linux-android" if args.platform == "android" else "aarch64-apple-ios-sim" if args.runtime == "hl" else "x86_64-apple-ios"
    tools = None
    if args.platform == "android":
        tools = next((Path(env["ANDROID_NDK_ROOT"]) / "toolchains/llvm/prebuilt").glob("*/bin"))
        env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = str(tools / "aarch64-linux-android21-clang")
    run(["cargo", "run", "-q", "-p", "hxn-gen", "--", "--library", "counter", "--source", ROOT / "tests/counter/core/src/lib.rs",
         "--crate", "counter_core", "--out", work / "gen"])
    crate = work / "crate"
    (crate / "src").mkdir(parents=True, exist_ok=True)
    dependencies = {"counter-core": ROOT / "tests/counter/core", "hxn-abi": ROOT / "crates/hxn-abi", "hxn-future": ROOT / "crates/hxn-future"}
    if args.runtime == "hl":
        dependencies["hxn-hl"] = ROOT / "crates/hxn-hl"
    manifest = '[workspace]\n[package]\nname="counter-log-test-' + args.runtime + '"\nversion="0.0.0"\nedition="2024"\n'
    manifest += '[lib]\nname="counter_log_' + args.runtime + '"\ncrate-type=["staticlib"]\n[dependencies]\n'
    manifest += ''.join(name + '={path=' + json.dumps(str(path)) + '}\n' for name, path in dependencies.items())
    if args.runtime == "hl":
        spec = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["dependencies"]["hl_abi"]
        manifest += 'hl_abi={git=' + json.dumps(spec["git"]) + '}\n'
    (crate / "Cargo.toml").write_text(manifest)
    (crate / "Cargo.lock").write_bytes((ROOT / "Cargo.lock").read_bytes())
    (crate / "src/lib.rs").write_text('include!(' + json.dumps(str(work / "gen" / ("hl" if args.runtime == "hl" else "c") / "adapter.rs")) + ');\n')
    run(["cargo", "build", "--manifest-path", crate / "Cargo.toml", "--target", triple], timeout=600)
    # Ash chooses its own API-qualified NDK driver for the final executable.
    env.pop("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", None)
    archive = ROOT / "target" / triple / "debug" / ("libcounter_log_" + args.runtime + ".a")
    compiler = ["haxe", "-cp", ROOT / "haxe", "-cp", ROOT / "tests/counter/haxe", "-cp", work / "gen/haxe", "-main", "LogOnly",
                "-dce", "full", "-D", "hxn_log_console", "-D", "hxn_log_probe", "-cp", work]
    if args.runtime == "hl":
        run([*compiler, "-D", "hxn_triple=" + triple, "-hl", work / "main.hl"])
        run([args.ash, "--build", work / "program", "--target", triple, "--native-lib", archive, work / "main.hl"])
    else:
        flags = ["-D", "iphonesim", "-D", "HXCPP_M64"] if args.platform == "ios-sim" else ["-D", "android", "-D", "HXCPP_ARM64", "-D", "HXCPP_DEBUG_LINK", "-D", "ANDROID_HOST=" + tools.parent.name]
        run([*compiler, *flags, "-D", "HXN_INCLUDE=" + str(ROOT / "include"), "-D", "COUNTER_INCLUDE=" + str(work / "gen/include"),
             "-D", "COUNTER_LIB=" + str(archive), "-cpp", work / "out"], timeout=600)
        if args.platform == "ios-sim":
            run(["xcrun", "--sdk", "iphonesimulator", "clang++", "-arch", "x86_64", "-mios-simulator-version-min=12.0",
                 ROOT / "tests/mobile/main.cpp", *list((work / "out").glob("libLogOnly*.a")), "-o", work / "program"])
        else:
            run([tools / "aarch64-linux-android21-clang", ROOT / "tests/mobile/main.c", "-L" + str(work / "out"), "-lLogOnly-64", "-o", work / "program"])
    if args.platform == "ios-sim":
        output = run(["xcrun", "simctl", "spawn", args.device, work / "program"])
    else:
        destination = "/data/local/tmp/hxn-log-" + args.runtime
        run(["adb", "-s", args.device, "shell", "mkdir", "-p", destination])
        files = [work / "program"] + ([work / "out/libLogOnly-64.so"] if args.runtime == "cpp" else [])
        run(["adb", "-s", args.device, "push", *files, destination + "/"])
        output = run(["adb", "-s", args.device, "shell", "cd " + destination + " && LD_LIBRARY_PATH=. ./program"])
        if args.runtime == "cpp":
            output += run(["adb", "-s", args.device, "logcat", "-d", "-v", "raw", "-s", "trace", "hxn"])
    if "PASS: namespace logging" not in output or "[WARN] fixture: platform console " + probe not in output:
        raise RuntimeError(f"missing log/success output; see {work / 'run.log'}")
    print(f"PASS: {args.platform} {args.runtime} native/frontend capture and platform output")


if __name__ == "__main__":
    main()
