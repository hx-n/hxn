//! Checks which tools each target requires and whether they are installed.

use std::path::Path;
use std::process::Command;

use crate::make::{on_path, printed};

/// The tools found on this machine. A tool is recorded as the first line of
/// its version output, or `None` if it is not installed.
#[derive(Default)]
pub struct Found {
    pub cargo: Option<String>,
    /// Whether that cargo is from a nightly toolchain, which an Ash wasm
    /// build requires.
    pub nightly: bool,
    /// Whether the source of Rust's standard library is installed. An Ash
    /// wasm build rebuilds it.
    pub source: bool,
    /// Targets with a standard-library archive in the active rustc sysroot.
    /// `None` means the installation could not be inspected.
    pub rust_targets: Option<Vec<String>>,
    /// Built-in target names reported by the active rustc.
    pub supported_targets: Option<Vec<String>>,
    pub haxe: Option<String>,
    /// The HashLink VM, and the command name used to look for it.
    pub vm: Option<String>,
    pub vm_command: String,
    pub hxcpp: Option<String>,
    /// The installed hxcpp predates HaxeFoundation/hxcpp#1298, so a program
    /// can crash on Windows when it exits right after a collection.
    pub hxcpp_old: bool,
    /// The installed hxcpp builds for Emscripten with wasm exceptions, which
    /// Rust's standard library for Emscripten unwinds with.
    pub hxcpp_wasm_exceptions: bool,
    pub ash: bool,
    pub emcc: bool,
    pub node: bool,
    pub sccache: bool,
    /// A linker faster than the system's, if the platform has one and it is
    /// installed.
    pub linker: Option<&'static str>,
}

// Whether the hxcpp at `path` predates HaxeFoundation/hxcpp#1298, which made
// the collector's thread-pool lock a `std::mutex`. Read from its source,
// which a release and a git checkout both have.
fn predates_1298(path: &str) -> bool {
    std::fs::read_to_string(Path::new(path).join("src/hx/gc/Immix.cpp"))
        .is_ok_and(|source| source.contains("ThreadPoolLock sThreadPoolLock"))
}

// Whether the hxcpp at `path` compiles for Emscripten with wasm exceptions
// rather than JavaScript ones.
fn wasm_exceptions(path: &str) -> bool {
    std::fs::read_to_string(Path::new(path).join("toolchain/emscripten-toolchain.xml"))
        .is_ok_and(|toolchain| toolchain.contains("-fwasm-exceptions"))
}

fn first_line(command: &mut Command) -> Option<String> {
    printed(command).map(|text| text.lines().next().unwrap_or_default().trim().to_owned())
}

// A static binding needs std, not just an empty target directory or core.
fn standard_libraries(root: &Path) -> std::io::Result<Vec<String>> {
    let mut targets = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let libraries = match std::fs::read_dir(entry.path().join("lib")) {
            Ok(libraries) => libraries,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        for library in libraries {
            let library = library?;
            let name = library.file_name();
            let name = name.to_string_lossy();
            if (name == "libstd.rlib"
                || name == "std.rlib"
                || ((name.starts_with("libstd-") || name.starts_with("std-"))
                    && name.ends_with(".rlib")))
                && library.path().is_file()
            {
                targets.push(entry.file_name().to_string_lossy().into_owned());
                break;
            }
        }
    }
    targets.sort();
    Ok(targets)
}

/// Detects every tool. `vm` is the HashLink VM command given with `--vm` or
/// `HXN_VM`.
pub fn find(vm: &str) -> Found {
    let release = first_line(Command::new("rustc").arg("--version")).unwrap_or_default();
    let sysroot = printed(Command::new("rustc").args(["--print", "sysroot"]))
        .map(|sysroot| Path::new(sysroot.trim()).join("lib/rustlib"));
    let installed = |part: &str| {
        sysroot
            .as_ref()
            .is_some_and(|root| root.join(part).is_dir())
    };
    let source = installed("src/rust/library");
    let rust_targets = sysroot
        .as_ref()
        .and_then(|root| standard_libraries(root).ok());
    let supported_targets = printed(Command::new("rustc").args(["--print", "target-list"]))
        .map(|targets| targets.lines().map(str::to_owned).collect());
    let program = vm.split_whitespace().next().unwrap_or("hl");
    let hxcpp = first_line(Command::new("haxelib").args(["libpath", "hxcpp"]))
        .filter(|path| Path::new(path).is_dir());
    Found {
        cargo: first_line(Command::new("cargo").arg("--version")),
        nightly: release.contains("nightly") || release.contains("dev"),
        source,
        rust_targets,
        supported_targets,
        haxe: first_line(Command::new("haxe").arg("--version"))
            .map(|version| format!("haxe {version}")),
        vm: on_path(program).then(|| {
            first_line(Command::new(program).arg("--version"))
                .filter(|line| !line.is_empty())
                .map_or_else(
                    || program.to_owned(),
                    |version| format!("{program} {version}"),
                )
        }),
        vm_command: program.to_owned(),
        hxcpp: hxcpp.clone().map(|path| format!("hxcpp at {path}")),
        hxcpp_old: hxcpp.as_deref().is_some_and(predates_1298),
        hxcpp_wasm_exceptions: hxcpp.as_deref().is_some_and(wasm_exceptions),
        ash: on_path("ash"),
        emcc: on_path("emcc"),
        node: on_path("node"),
        sccache: on_path("sccache"),
        linker: if cfg!(target_os = "linux") {
            on_path("mold").then_some("mold")
        } else if cfg!(windows) {
            on_path("lld-link").then_some("lld-link")
        } else {
            None
        },
    }
}

impl Found {
    fn standard_library_missing(&self, triple: &str) -> Option<String> {
        match &self.rust_targets {
            Some(targets) if targets.iter().any(|target| target == triple) => None,
            None => Some(String::from(
                "could not inspect Rust's standard libraries (`rustc --print sysroot`)",
            )),
            Some(_) => match &self.supported_targets {
                Some(targets) if targets.iter().any(|target| target == triple) => Some(format!(
                    "Rust's standard library for {triple} (`rustup target add {triple}` if supplied by the toolchain; hxn requires std, not only core)"
                )),
                Some(_) => Some(format!(
                    "`{triple}` is not a target listed by rustc (`rustc --print target-list`)"
                )),
                None => Some(String::from(
                    "could not inspect Rust's supported targets (`rustc --print target-list`)",
                )),
            },
        }
    }

    /// The prerequisites for building the Rust static binding for `triple`.
    /// The Haxe application and the library's own dependencies are separate.
    pub fn binding_missing(&self, triple: &str) -> Vec<String> {
        let mut missing = Vec::new();
        if self.cargo.is_none() {
            missing.push(String::from("cargo (https://rustup.rs)"));
        }
        missing.extend(self.standard_library_missing(triple));
        missing
    }

    pub fn binding_report(&self, triple: &str) -> String {
        let missing = self.binding_missing(triple);
        let mut out = format!("Rust binding target:\n  cpp --target {triple}");
        if missing.is_empty() {
            out.push_str(" ready\n");
        } else {
            out.push_str(" needs:\n");
            for item in missing {
                out.push_str(&format!("    - {item}\n"));
            }
        }
        out.push_str("  This checks the Rust binding. The library's own dependencies and the Haxe app may need additional platform tools and SDKs.\n");
        out
    }

    // The Rust components an Ash wasm build requires that are missing.
    fn wasm_missing(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.nightly {
            missing.push(String::from(
                "a nightly Rust toolchain (`rustup toolchain install nightly`, then RUSTUP_TOOLCHAIN=nightly)",
            ));
        }
        if !self.source {
            missing.push(String::from(
                "the source of Rust's standard library (`rustup component add rust-src`)",
            ));
        }
        missing
    }

    /// Each target `hxn` builds for, with the tools it is missing. An empty
    /// list means the target is ready.
    pub fn targets(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut base = Vec::new();
        if self.cargo.is_none() {
            base.push(String::from("cargo (https://rustup.rs)"));
        }
        if self.haxe.is_none() {
            base.push(String::from("haxe (https://haxe.org/download)"));
        }
        let with = |more: Vec<String>| {
            let mut all = base.clone();
            all.extend(more);
            all
        };
        let vm = if self.vm.is_some() {
            Vec::new()
        } else {
            vec![format!(
                "`{}`, a HashLink VM, on PATH (or set one with --vm or HXN_VM)",
                self.vm_command
            )]
        };
        let hxcpp = if self.hxcpp.is_some() {
            Vec::new()
        } else {
            vec![String::from(
                "hxcpp from git: `haxelib git hxcpp https://github.com/HaxeFoundation/hxcpp`, then `haxe compile.hxml` in its tools/hxcpp",
            )]
        };
        let mut hl_wasm = self.wasm_missing();
        if !self.ash {
            hl_wasm.push(String::from(
                "`ash`, which builds and runs the wasm program",
            ));
        }
        let mut cpp_wasm = hxcpp.clone();
        if self.hxcpp.is_some() && !self.hxcpp_wasm_exceptions {
            cpp_wasm.push(String::from(
                "hxcpp from git, whose Emscripten build uses wasm exceptions as Rust's does: `haxelib git hxcpp https://github.com/HaxeFoundation/hxcpp`",
            ));
        }
        if let Some(missing) = self.standard_library_missing("wasm32-unknown-emscripten") {
            cpp_wasm.push(missing);
        }
        if !self.emcc {
            cpp_wasm.push(String::from("`emcc`, from the Emscripten SDK, on PATH"));
        }
        if !self.node {
            cpp_wasm.push(String::from("`node`, which runs the built program"));
        }
        vec![
            ("hl", with(vm)),
            ("cpp", with(hxcpp.clone())),
            ("cppia", with(hxcpp)),
            ("hl --wasm", with(hl_wasm)),
            ("cpp --wasm", with(cpp_wasm)),
        ]
    }

    /// Whether everything `target` requires is installed.
    pub fn ready(&self, target: &str) -> bool {
        self.targets()
            .iter()
            .any(|(name, missing)| *name == target && missing.is_empty())
    }

    /// The text `hxn doctor` prints.
    pub fn report(&self) -> String {
        let mut out = String::from("Found:\n");
        let found = [
            self.cargo.clone().map(|cargo| {
                if self.nightly {
                    cargo
                } else {
                    format!("{cargo} (not nightly)")
                }
            }),
            self.haxe.clone(),
            self.vm.clone(),
            self.hxcpp.clone(),
            self.ash.then(|| String::from("ash")),
            self.emcc.then(|| String::from("emcc")),
            self.node.then(|| String::from("node")),
        ];
        for tool in found.into_iter().flatten() {
            out.push_str(&format!("  {tool}\n"));
        }
        out.push_str("\nRust standard libraries (active toolchain):\n");
        match &self.rust_targets {
            Some(targets) if targets.is_empty() => out.push_str("  none found\n"),
            Some(targets) => {
                for target in targets {
                    out.push_str(&format!("  {target}\n"));
                }
            }
            None => out.push_str("  unavailable: could not inspect rustc's sysroot\n"),
        }
        out.push_str("\nTargets:\n");
        for (name, missing) in self.targets() {
            if missing.is_empty() {
                out.push_str(&format!("  {name:<12}ready\n"));
            } else {
                out.push_str(&format!("  {name:<12}needs:\n"));
                for tool in missing {
                    out.push_str(&format!("    - {tool}\n"));
                }
            }
        }
        if self.hxcpp_old {
            out.push_str("\nThis hxcpp predates HaxeFoundation/hxcpp#1298: on Windows a program can crash when it exits right after a collection. Use hxcpp from git, or end programs with `hxn.Loop.exit`.\n");
        }
        let mut faster = Vec::new();
        if !self.sccache {
            faster.push(
                "sccache caches compiled dependencies across checkouts and in CI: `cargo install sccache`, then set RUSTC_WRAPPER=sccache. It disables incremental compilation, so leave it off for everyday work.",
            );
        }
        if self.linker.is_none() && cfg!(target_os = "linux") {
            faster.push("mold links faster than the system linker, and linking takes most of the time in an incremental build.");
        }
        if self.linker.is_none() && cfg!(windows) {
            faster.push("lld-link links faster than link.exe, and linking takes most of the time in an incremental build.");
        }
        if !faster.is_empty() {
            out.push_str("\nOptional, for faster builds:\n");
            for note in faster {
                out.push_str(&format!("  - {note}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_targets_have_std_archives_not_only_directories_or_core() {
        let root = std::env::temp_dir().join(format!("hxn-doctor-targets-{}", std::process::id()));
        for (target, archive) in [
            ("z-host", "libstd-123.rlib"),
            ("a-cross", "std-456.rlib"),
            ("core-only", "libcore-123.rlib"),
            ("metadata-only", "libstd-123.rmeta"),
            ("dynamic-only", "libstd-123.so"),
        ] {
            let lib = root.join(target).join("lib");
            std::fs::create_dir_all(&lib).unwrap();
            std::fs::write(lib.join(archive), []).unwrap();
        }
        std::fs::create_dir_all(root.join("empty/lib")).unwrap();
        std::fs::create_dir_all(root.join("src/rust/library")).unwrap();
        assert_eq!(standard_libraries(&root).unwrap(), ["a-cross", "z-host"]);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(standard_libraries(&root).is_err());
    }

    #[test]
    fn a_binding_target_needs_cargo_and_its_std_without_a_host_vm_or_haxe() {
        let found = Found {
            cargo: Some(String::from("cargo nightly")),
            rust_targets: Some(vec![String::from("aarch64-apple-ios")]),
            supported_targets: Some(vec![
                String::from("aarch64-apple-ios"),
                String::from("aarch64-linux-android"),
            ]),
            ..Found::default()
        };
        assert!(found.binding_missing("aarch64-apple-ios").is_empty());
        assert!(!found.ready("cpp") && !found.ready("hl"));
        assert!(
            found
                .binding_report("aarch64-apple-ios")
                .contains("cpp --target aarch64-apple-ios ready")
        );
        let missing = found.binding_report("aarch64-linux-android");
        assert!(missing.contains("rustup target add aarch64-linux-android"));
        assert!(missing.contains("hxn requires std, not only core"));
        let unknown = found.binding_report("not-a-target");
        assert!(unknown.contains("is not a target listed by rustc"));
        assert!(!unknown.contains("rustup target add"));
        let no_cargo = Found {
            cargo: None,
            ..found
        };
        assert_eq!(
            no_cargo.binding_missing("aarch64-apple-ios"),
            ["cargo (https://rustup.rs)"]
        );
    }

    #[test]
    fn an_unavailable_inventory_is_not_reported_as_a_missing_target() {
        let found = Found::default();
        assert!(
            found
                .report()
                .contains("unavailable: could not inspect rustc's sysroot")
        );
        let report = found.binding_report("aarch64-apple-ios");
        assert!(report.contains("could not inspect Rust's standard libraries"));
        assert!(!report.contains("rustup target add"));
        let empty = Found {
            rust_targets: Some(Vec::new()),
            ..Found::default()
        };
        assert!(empty.report().contains("  none found"));
        assert!(
            empty
                .binding_report("unknown")
                .contains("could not inspect Rust's supported targets")
        );
    }

    #[test]
    fn an_hxcpp_is_old_when_its_collector_has_the_old_lock() {
        let dir = std::env::temp_dir().join(format!("hxn-hxcpp-{}", std::process::id()));
        let gc = dir.join("src/hx/gc");
        std::fs::create_dir_all(&gc).unwrap();
        let path = dir.to_string_lossy().into_owned();
        std::fs::write(
            gc.join("Immix.cpp"),
            "static ThreadPoolLock sThreadPoolLock;",
        )
        .unwrap();
        assert!(predates_1298(&path));
        std::fs::write(gc.join("Immix.cpp"), "static std::mutex sThreadPoolLock;").unwrap();
        assert!(!predates_1298(&path));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!predates_1298(&path));
    }

    #[test]
    fn a_target_is_ready_when_nothing_it_needs_is_missing() {
        let bare = Found {
            vm_command: String::from("hl"),
            ..Found::default()
        };
        assert!(!bare.ready("hl") && !bare.ready("cpp"));
        let report = bare.report();
        assert!(report.contains("cargo (https://rustup.rs)"));
        assert!(report.contains("`hl`, a HashLink VM, on PATH"));

        let native = Found {
            cargo: Some(String::from("cargo 1.90.0")),
            haxe: Some(String::from("haxe 4.3.6")),
            vm: Some(String::from("hl 1.15.0")),
            vm_command: String::from("hl"),
            hxcpp: Some(String::from("hxcpp at /lib/hxcpp")),
            hxcpp_old: false,
            ..Found::default()
        };
        assert!(native.ready("hl") && native.ready("cpp") && native.ready("cppia"));
        assert!(!native.ready("hl --wasm") && !native.ready("cpp --wasm"));
        let report = native.report();
        assert!(report.contains("cargo 1.90.0 (not nightly)"));
        assert!(report.contains("  hl          ready"));
        assert!(report.contains("a nightly Rust toolchain"));
        assert!(report.contains("`emcc`, from the Emscripten SDK"));

        let all = Found {
            nightly: true,
            source: true,
            rust_targets: Some(vec![String::from("wasm32-unknown-emscripten")]),
            hxcpp_wasm_exceptions: true,
            ash: true,
            emcc: true,
            node: true,
            ..native
        };
        assert!(all.ready("hl --wasm") && all.ready("cpp --wasm"));
    }
}
