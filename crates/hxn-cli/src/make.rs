//! Writes and builds a library's binding for one runtime.
//!
//! Everything goes under cargo's target directory, in `hxn/<library>`:
//!
//! | Path | What it is |
//! |---|---|
//! | `support/` | hxn's own Haxe classes, a class path |
//! | `include/` | `hxn.h` and the library's headers |
//! | `<runtime>/crate/` | the adapter, a crate that depends on the library |
//! | `<runtime>/haxe/` | the library's Haxe classes, a class path |
//! | `<runtime>/build.hxml` | what to pass `haxe` to compile against them |
//! | `hl/<library>.hdll` | the built library, for HashLink |
//! | `cppia/` | what a CPPIA script needs, which [`crate::script`] writes |
//! | `hl-wasm/<library>.wasm` | the same as a module Ash loads into a wasm program |
//!
//! `<runtime>` is `hl` or `cpp`, and `hl-wasm` or `cpp-wasm` for a wasm
//! build. The library's own crate gains one file, its manifest. `package/`
//! beside these is what [`crate::package`] gathers from them.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::project::{Project, quoted};
use crate::support;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Runtime {
    Hl,
    Cpp,
}

impl Runtime {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "hl" => Some(Self::Hl),
            "cpp" => Some(Self::Cpp),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hl => "hl",
            Self::Cpp => "cpp",
        }
    }
}

/// What a binding is built for: a runtime, natively or as wasm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Target {
    pub runtime: Runtime,
    pub wasm: bool,
}

impl Target {
    /// The directory its output goes in: `hl`, or `cpp-wasm`.
    pub fn name(self) -> String {
        let runtime = self.runtime.name();
        if self.wasm {
            format!("{runtime}-wasm")
        } else {
            runtime.to_owned()
        }
    }

    // What rustc calls it, where that is not the host. HashLink bytecode
    // becomes wasm through Ash, and hxcpp's C++ through Emscripten.
    pub fn triple(self) -> Option<&'static str> {
        match (self.wasm, self.runtime) {
            (false, _) => None,
            (true, Runtime::Hl) => Some("wasm32-wasip1"),
            (true, Runtime::Cpp) => Some("wasm32-unknown-emscripten"),
        }
    }

    // A side module is position-independent. Emscripten's exceptions are
    // JavaScript's, which a library that unwinds cannot link against.
    fn rustflags(self) -> Option<&'static str> {
        match (self.wasm, self.runtime) {
            // hxcpp links the static C runtime on Windows, and a library
            // linked into its program has to use the same one.
            (false, Runtime::Cpp) if cfg!(windows) => Some("-C target-feature=+crt-static"),
            (false, _) => None,
            (true, Runtime::Hl) => {
                Some("-C relocation-model=pic -C target-feature=+mutable-globals")
            }
            (true, Runtime::Cpp) => Some("-C panic=abort"),
        }
    }
}

/// What a build leaves for a Haxe compile.
pub struct Built {
    /// The runtime's directory: where the built library is, and where a
    /// compiled program goes.
    pub dir: PathBuf,
    /// Flags for `haxe` with their values: the class paths, and for hxcpp
    /// where the headers and the library are.
    pub arguments: Vec<(&'static str, String)>,
    /// The same, as a file `haxe` takes.
    pub hxml: PathBuf,
    /// The library's Haxe classes for the runtime, a class path.
    pub classes: PathBuf,
    /// `hxn.h` and the library's headers, which hxcpp compiles against.
    pub include: PathBuf,
    /// What was built: the library a VM loads, or the archive hxcpp links.
    pub library: PathBuf,
    /// Declarations of the hxcpp classes for a CPPIA script, a class path.
    /// Written with a native hxcpp build and no other.
    pub declarations: PathBuf,
}

// Leaves a file that already holds this text alone, so cargo and haxe
// rebuild only what changed.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == text) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Runs a program the user sees the output of, and fails if it does.
pub fn run(command: &mut Command) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("`{program}` was not found: it has to be installed and on PATH")
        } else {
            format!("running {program}: {error}")
        }
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed"))
    }
}

pub fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

const NOTE: &str = "# Written by hxn. Do not edit.\n";

// The manifest of an adapter crate. It names the library and hxn's crates
// from where the library gets them, and stands outside any workspace.
fn manifest(project: &Project, target: Target, later: bool) -> String {
    let (kind, crates): (&str, &[&str]) = match target.runtime {
        // Nothing here can link a wasm library whose imports are the
        // program's, so that one is linked from an archive afterwards.
        Runtime::Hl if target.wasm => ("staticlib", &["hxn-abi", "hxn-hl"]),
        Runtime::Hl => ("cdylib", &["hxn-abi", "hxn-hl"]),
        Runtime::Cpp => ("staticlib", &["hxn-abi"]),
    };
    let library = &project.library;
    let suffix = target.runtime.name();
    let mut out = format!(
        "{NOTE}[package]\nname = \"{library}-{suffix}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\nname = \"{library}_{suffix}\"\ncrate-type = [\"{kind}\"]\ntest = false\ndoctest = false\n\n[dependencies]\n{package} = {{ path = {path} }}\n",
        package = project.package,
        path = quoted(&text(&project.dir)),
    );
    for name in crates {
        out.push_str(&format!("{name} = {}\n", project.hxn.dependency(name)));
    }
    // Named by the adapter of a library with results over time, and by no other.
    if later {
        out.push_str(&format!(
            "hxn-future = {}\n",
            project.hxn.dependency("hxn-future")
        ));
    }
    out.push_str("\n[workspace]\n");
    out
}

// The VM's symbols are resolved when it loads the library. Other Unix
// linkers leave them undefined in a shared object; Apple's must be told,
// and Windows resolves them at link time against the VM's `libhl.lib`,
// found in HXN_HL_DIR.
const HL_BUILD_SCRIPT: &str = "// Written by hxn. Do not edit.\nfn main() {\n    println!(\"cargo:rerun-if-changed=build.rs\");\n    println!(\"cargo:rerun-if-env-changed=HXN_HL_DIR\");\n    if std::env::var(\"CARGO_CFG_TARGET_VENDOR\").as_deref() == Ok(\"apple\") {\n        println!(\"cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup\");\n    }\n    if std::env::var(\"CARGO_CFG_TARGET_OS\").as_deref() == Ok(\"windows\") {\n        if let Ok(dir) = std::env::var(\"HXN_HL_DIR\") {\n            println!(\"cargo:rustc-link-search=native={dir}\");\n        }\n        println!(\"cargo:rustc-link-lib=dylib=libhl\");\n    }\n}\n";

// What cargo calls the library it builds from an adapter crate.
fn artifact(library: &str, target: Target) -> String {
    match target.runtime {
        Runtime::Hl if target.wasm => format!("lib{library}_hl.a"),
        Runtime::Hl if cfg!(windows) => format!("{library}_hl.dll"),
        Runtime::Hl if cfg!(target_os = "macos") => format!("lib{library}_hl.dylib"),
        Runtime::Hl => format!("lib{library}_hl.so"),
        Runtime::Cpp if cfg!(windows) && !target.wasm => format!("{library}_cpp.lib"),
        Runtime::Cpp => format!("lib{library}_cpp.a"),
    }
}

// What a command prints, when it ran and printed something.
pub fn printed(command: &mut Command) -> Option<String> {
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

// The line of `rustc -vV` that starts with `key`.
pub fn rustc_reports(key: &str) -> Result<String, String> {
    printed(Command::new("rustc").arg("-vV"))
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix(key).map(|value| value.trim().to_owned()))
        })
        .ok_or_else(|| String::from("`rustc -vV` did not report what it is"))
}

/// Whether `program` is in a directory on PATH.
pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .any(|dir| dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file())
    })
}

// The name of each primitive's resolver in a HashLink adapter, which is
// what a VM looks a primitive up by.
fn resolvers(adapter: &str) -> Vec<&str> {
    adapter
        .split("define_prim!(")
        .skip(1)
        .filter_map(|rest| rest.split(',').next())
        .map(str::trim)
        .collect()
}

// Links the adapter's archive into a module a wasm program loads. What it
// leaves undefined, the VM's own functions and the allocator, becomes
// imports the program answers.
fn side_module(archive: &Path, adapter: &str, module: &Path) -> Result<(), String> {
    let sysroot = printed(Command::new("rustc").args(["--print", "sysroot"]))
        .ok_or("`rustc --print sysroot` did not print where it is")?;
    let linker = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(rustc_reports("host:")?)
        .join("bin/rust-lld");
    let mut link = Command::new(linker);
    link.args(["-flavor", "wasm", "--experimental-pic", "-shared"])
        .args(["--no-entry", "--gc-sections", "--no-export-dynamic"])
        .arg("--unresolved-symbols=import-dynamic");
    for resolver in resolvers(adapter) {
        link.arg(format!("--export={resolver}"));
    }
    link.arg("--whole-archive")
        .arg(archive)
        .arg("--no-whole-archive")
        .arg("-o")
        .arg(module);
    run(&mut link)
}

// Makes `dir` hold these classes and no others: one for an interface the
// library no longer has would still compile.
fn classes_in(dir: &Path, classes: &[hxn_gen::File]) -> Result<(), String> {
    for class in classes {
        write(&dir.join(&class.path), &class.text)?;
    }
    for entry in std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if !classes.iter().any(|class| class.path == file) {
            std::fs::remove_file(entry.path())
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// How a build is made.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    pub release: bool,
    /// Has cargo report which crates the build spent its time on.
    pub timings: bool,
    /// The directory the HashLink VM is in.
    pub hashlink: Option<&'a Path>,
}

/// Reads the library's marked items, writes its binding for `target` and
/// builds the adapter.
pub fn build(project: &Project, target: Target, options: Options) -> Result<Built, String> {
    let Options {
        release,
        timings,
        hashlink,
    } = options;
    let name = &project.library;
    let runtime = target.runtime;
    if target.wasm {
        // std is rebuilt for a wasm target, to be position-independent or
        // to abort, and only a nightly cargo will do that.
        let release = rustc_reports("release:")?;
        if !release.contains("nightly") && !release.contains("dev") {
            return Err(format!(
                "a wasm build rebuilds Rust's standard library, which takes a nightly toolchain, and this one is {release}: install one with `rustup toolchain install nightly --component rust-src` and set RUSTUP_TOOLCHAIN=nightly"
            ));
        }
        if runtime == Runtime::Cpp && !on_path("emcc") {
            return Err(String::from(
                "`emcc` was not found: hxcpp builds for wasm with the Emscripten SDK, which has to be installed and on PATH",
            ));
        }
    }
    let (library, _) =
        hxn_gen::parse_crate(name, &project.source).map_err(|error| error.to_string())?;
    if library.interfaces.is_empty() {
        return Err(format!(
            "{}: nothing is marked `#[hxn::interface]` or `#[hxn::function]`",
            project.source.display()
        ));
    }

    // Beside the crate, to be committed: a change to it is a change to the
    // library's ABI.
    write(
        &project.dir.join(format!("{name}.hxn.json")),
        &hxn_gen::manifest(&library),
    )?;

    let out = project.target.join("hxn").join(name);
    let support = out.join("support");
    for (path, text) in support::HAXE {
        write(&support.join(path), text)?;
    }
    let dir = out.join(target.name());
    let krate = dir.join("crate");
    write(
        &krate.join("Cargo.toml"),
        &manifest(project, target, library.returns_later()),
    )?;
    let (adapter, classes) = match runtime {
        Runtime::Hl => {
            if !target.wasm {
                write(&krate.join("build.rs"), HL_BUILD_SCRIPT)?;
            }
            (
                hxn_gen::hl_adapter(&library, &project.krate),
                hxn_gen::haxe_hl(&library),
            )
        }
        Runtime::Cpp => {
            let include = out.join("include");
            write(&include.join("hxn.h"), support::HEADER)?;
            write(
                &include.join(format!("{name}_abi.h")),
                &hxn_gen::c_header(&library),
            )?;
            write(
                &include.join(format!("{name}_glue.h")),
                &hxn_gen::cpp_glue(&library),
            )?;
            (
                hxn_gen::c_adapter(&library, &project.krate),
                hxn_gen::haxe_cpp(&library),
            )
        }
    };
    // Every export's contract is the ABI's: its arguments are what the
    // runtime passes.
    write(
        &krate.join("src/lib.rs"),
        &format!("#![allow(clippy::missing_safety_doc)]\n\n{adapter}"),
    )?;

    let haxe = dir.join("haxe");
    classes_in(&haxe, &classes)?;
    let declarations = out.join("cppia").join("haxe");
    if runtime == Runtime::Cpp && !target.wasm {
        classes_in(&declarations, &hxn_gen::haxe_cppia(&library))?;
    }

    // Built with what the library's own lock file resolved, the first time.
    let lock = krate.join("Cargo.lock");
    if !lock.exists() {
        for above in project.dir.ancestors() {
            if std::fs::copy(above.join("Cargo.lock"), &lock).is_ok() {
                break;
            }
        }
    }
    let mut cargo = Command::new("cargo");
    cargo
        .arg("build")
        .arg("--manifest-path")
        .arg(krate.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&project.target);
    if release {
        cargo.arg("--release");
    }
    if timings {
        cargo.arg("--timings");
    }
    let mut built = project.target.clone();
    if let Some(triple) = target.triple() {
        cargo
            .args(["--target", triple])
            .arg("-Zbuild-std=std,panic_abort");
        built.push(triple);
    }
    // Where the adapter's build script finds the VM's library, unless the
    // environment already specifies it.
    if let Some(dir) = hashlink
        && std::env::var_os("HXN_HL_DIR").is_none()
    {
        cargo.env("HXN_HL_DIR", dir);
    }
    // With the flags the environment already asks for, which these would
    // otherwise replace.
    if let Some(flags) = target.rustflags() {
        let given = std::env::var("RUSTFLAGS").unwrap_or_default();
        cargo.env("RUSTFLAGS", format!("{given} {flags}"));
    }
    run(&mut cargo)?;
    built.push(if release { "release" } else { "debug" });
    built.push(artifact(name, target));

    let mut arguments = vec![("-cp", text(&support)), ("-cp", text(&haxe))];
    if target.wasm {
        arguments.push(("-D", String::from("wasm")));
    }
    match runtime {
        Runtime::Hl if target.wasm => {
            let module = dir.join(format!("{name}.wasm"));
            side_module(&built, &adapter, &module)?;
            built = module;
        }
        Runtime::Hl => {
            // Under the name the Haxe classes ask the VM for.
            let hdll = dir.join(format!("{name}.hdll"));
            std::fs::copy(&built, &hdll)
                .map_err(|error| format!("{}: {error}", built.display()))?;
            built = hdll;
        }
        Runtime::Cpp => {
            if target.wasm {
                // hxcpp assumes the host's 64 bits, and its UTF-16 strings
                // reach for a Windows-only call under Emscripten.
                for define in ["emscripten", "HXCPP_M32", "disable_unicode_strings"] {
                    arguments.push(("-D", String::from(define)));
                }
            } else if cfg!(windows) {
                // hxcpp builds for 32 bits there unless told, and the
                // library is built for the host.
                let bits = if cfg!(target_arch = "aarch64") {
                    "HXCPP_ARM64"
                } else {
                    "HXCPP_M64"
                };
                arguments.push(("-D", String::from(bits)));
            }
            let include = text(&out.join("include"));
            let upper = name.to_uppercase();
            arguments.push(("-D", format!("HXN_INCLUDE={include}")));
            arguments.push(("-D", format!("{upper}_INCLUDE={include}")));
            arguments.push(("-D", format!("{upper}_LIB={}", text(&built))));
        }
    }
    let hxml = dir.join("build.hxml");
    let lines: Vec<String> = arguments
        .iter()
        .map(|(flag, value)| format!("{flag} {value}\n"))
        .collect();
    write(&hxml, &lines.concat())?;

    Ok(Built {
        dir,
        arguments,
        hxml,
        classes: haxe,
        include: out.join("include"),
        library: built,
        declarations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wasm_build_has_its_own_directory_and_target() {
        let native = Target {
            runtime: Runtime::Hl,
            wasm: false,
        };
        assert_eq!(native.name(), "hl");
        assert_eq!((native.triple(), native.rustflags()), (None, None));
        let ash = Target {
            runtime: Runtime::Hl,
            wasm: true,
        };
        assert_eq!(ash.name(), "hl-wasm");
        assert_eq!(ash.triple(), Some("wasm32-wasip1"));
        assert_eq!(artifact("greeter", ash), "libgreeter_hl.a");
        let emscripten = Target {
            runtime: Runtime::Cpp,
            wasm: true,
        };
        assert_eq!(emscripten.name(), "cpp-wasm");
        assert_eq!(emscripten.triple(), Some("wasm32-unknown-emscripten"));
        assert_eq!(artifact("greeter", emscripten), "libgreeter_cpp.a");
    }

    #[test]
    fn every_primitive_of_an_adapter_is_exported_from_its_module() {
        let source = "#[hxn::interface]\npub struct Greeter;\n#[hxn::methods]\nimpl Greeter {\n    pub fn new() -> Self { Self }\n    pub fn greeted(&self) -> i32 { 0 }\n}\n";
        let library = hxn_gen::parse("greeter", source).unwrap();
        let adapter = hxn_gen::hl_adapter(&library, "greeter");
        let found = resolvers(&adapter);
        assert_eq!(found.len(), adapter.matches("define_prim!").count());
        assert!(found.iter().all(|name| name.starts_with("hlp_")));
        assert!(found.contains(&"hlp_greeter_new") && found.contains(&"hlp_greeter_greeted"));
    }
}
