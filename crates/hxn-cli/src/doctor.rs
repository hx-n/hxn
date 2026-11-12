//! Checks which tools each target requires and whether they are installed.

use std::path::Path;
use std::process::Command;

use crate::make::{on_path, printed};

/// The tools found on this machine. A tool is recorded as the first line of
/// its version output, or `None` if it is not installed.
#[derive(Default)]
pub struct Found {
    pub cargo: Option<String>,
    /// Whether that cargo is from a nightly toolchain, which a wasm build
    /// requires.
    pub nightly: bool,
    /// Whether the source of Rust's standard library is installed. A wasm
    /// build rebuilds it.
    pub source: bool,
    pub haxe: Option<String>,
    /// The HashLink VM, and the command name used to look for it.
    pub vm: Option<String>,
    pub vm_command: String,
    pub hxcpp: Option<String>,
    pub ash: bool,
    pub emcc: bool,
    pub node: bool,
    pub sccache: bool,
    /// A linker faster than the system's, if the platform has one and it is
    /// installed.
    pub linker: Option<&'static str>,
}

fn first_line(command: &mut Command) -> Option<String> {
    printed(command).map(|text| text.lines().next().unwrap_or_default().trim().to_owned())
}

/// Detects every tool. `vm` is the HashLink VM command given with `--vm` or
/// `HXN_VM`.
pub fn find(vm: &str) -> Found {
    let release = first_line(Command::new("rustc").arg("--version")).unwrap_or_default();
    let source =
        printed(Command::new("rustc").args(["--print", "sysroot"])).is_some_and(|sysroot| {
            Path::new(sysroot.trim())
                .join("lib/rustlib/src/rust/library")
                .is_dir()
        });
    let program = vm.split_whitespace().next().unwrap_or("hl");
    Found {
        cargo: first_line(Command::new("cargo").arg("--version")),
        nightly: release.contains("nightly") || release.contains("dev"),
        source,
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
        hxcpp: first_line(Command::new("haxelib").args(["libpath", "hxcpp"]))
            .filter(|path| Path::new(path).is_dir())
            .map(|path| format!("hxcpp at {path}")),
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
    // The Rust components a wasm build requires that are missing.
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
            vec![String::from("hxcpp (`haxelib install hxcpp`)")]
        };
        let mut hl_wasm = self.wasm_missing();
        if !self.ash {
            hl_wasm.push(String::from(
                "`ash`, which builds and runs the wasm program",
            ));
        }
        let mut cpp_wasm = hxcpp.clone();
        cpp_wasm.extend(self.wasm_missing());
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
            ash: true,
            emcc: true,
            node: true,
            ..native
        };
        assert!(all.ready("hl --wasm") && all.ready("cpp --wasm"));
    }
}
