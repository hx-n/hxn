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
//!
//! The library's own crate gains one file, its manifest.

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
}

// Leaves a file that already says this alone, so cargo and haxe rebuild
// only what changed.
fn write(path: &Path, text: &str) -> Result<(), String> {
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

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

const NOTE: &str = "# Written by hxn. Do not edit.\n";

// The manifest of an adapter crate. It names the library and hxn's crates
// from where the library gets them, and stands outside any workspace.
fn manifest(project: &Project, runtime: Runtime) -> String {
    let (kind, crates): (&str, &[&str]) = match runtime {
        Runtime::Hl => ("cdylib", &["hxn-abi", "hxn-hl"]),
        Runtime::Cpp => ("staticlib", &["hxn-abi"]),
    };
    let library = &project.library;
    let suffix = runtime.name();
    let mut out = format!(
        "{NOTE}[package]\nname = \"{library}-{suffix}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\nname = \"{library}_{suffix}\"\ncrate-type = [\"{kind}\"]\ntest = false\ndoctest = false\n\n[dependencies]\n{package} = {{ path = {path} }}\n",
        package = project.package,
        path = quoted(&text(&project.dir)),
    );
    for name in crates {
        out.push_str(&format!("{name} = {}\n", project.hxn.dependency(name)));
    }
    out.push_str("\n[workspace]\n");
    out
}

// The VM's symbols are resolved when it loads the library. Other Unix
// linkers leave them undefined in a shared object; Apple's must be told.
const HL_BUILD_SCRIPT: &str = "// Written by hxn. Do not edit.\nfn main() {\n    println!(\"cargo:rerun-if-changed=build.rs\");\n    if std::env::var(\"CARGO_CFG_TARGET_VENDOR\").as_deref() == Ok(\"apple\") {\n        println!(\"cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup\");\n    }\n}\n";

// What cargo calls the library it builds from an adapter crate.
fn artifact(library: &str, runtime: Runtime) -> String {
    match runtime {
        Runtime::Hl if cfg!(windows) => format!("{library}_hl.dll"),
        Runtime::Hl if cfg!(target_os = "macos") => format!("lib{library}_hl.dylib"),
        Runtime::Hl => format!("lib{library}_hl.so"),
        Runtime::Cpp if cfg!(windows) => format!("{library}_cpp.lib"),
        Runtime::Cpp => format!("lib{library}_cpp.a"),
    }
}

/// Reads the library's marked items, writes its binding for `runtime` and
/// builds the adapter.
pub fn build(project: &Project, runtime: Runtime, release: bool) -> Result<Built, String> {
    let source = std::fs::read_to_string(&project.source)
        .map_err(|error| format!("{}: {error}", project.source.display()))?;
    let name = &project.library;
    let library = hxn_gen::parse(name, &source)
        .map_err(|error| format!("{}: {error}", project.source.display()))?;
    if library.interfaces.is_empty() {
        return Err(format!(
            "{}: nothing is marked `#[hxn::interface]`",
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
    let dir = out.join(runtime.name());
    let krate = dir.join("crate");
    write(&krate.join("Cargo.toml"), &manifest(project, runtime))?;
    let (adapter, classes) = match runtime {
        Runtime::Hl => {
            write(&krate.join("build.rs"), HL_BUILD_SCRIPT)?;
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
    for class in &classes {
        write(&haxe.join(&class.path), &class.text)?;
    }
    // A class for an interface the library no longer has would still compile.
    for entry in std::fs::read_dir(&haxe).map_err(|error| format!("{}: {error}", haxe.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if !classes.iter().any(|class| class.path == file) {
            std::fs::remove_file(entry.path())
                .map_err(|error| format!("{}: {error}", entry.path().display()))?;
        }
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
    run(&mut cargo)?;
    let profile = if release { "release" } else { "debug" };
    let built = project.target.join(profile).join(artifact(name, runtime));

    let mut arguments = vec![("-cp", text(&support)), ("-cp", text(&haxe))];
    match runtime {
        Runtime::Hl => {
            // Under the name the Haxe classes ask the VM for.
            let hdll = dir.join(format!("{name}.hdll"));
            std::fs::copy(&built, &hdll)
                .map_err(|error| format!("{}: {error}", built.display()))?;
        }
        Runtime::Cpp => {
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
    })
}
