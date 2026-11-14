//! Generates and builds a library's binding for one runtime.
//!
//! All output goes under cargo's target directory, in `hxn/<library>`:
//!
//! | Path | What it is |
//! |---|---|
//! | `support/` | hxn's Haxe support classes (a class path) |
//! | `include/` | `hxn.h` and the library's headers |
//! | `<runtime>/crate/` | the adapter crate, which depends on the library |
//! | `<runtime>/haxe/` | the library's Haxe classes (a class path) |
//! | `<runtime>/build.hxml` | the `haxe` arguments for compiling against them |
//! | `hl/<library>.hdll` | the built library for HashLink |
//! | `hl-<triple>/lib<library>.a` | the built library for HashLink on another platform, which Ash links into the program |
//! | `cppia/` | the files a CPPIA script needs, written by [`crate::script`] |
//! | `hl-wasm/<library>.wasm` | the built library as a wasm module that Ash loads into a wasm program |
//!
//! `<runtime>` is `hl` or `cpp`, or `hl-wasm` or `cpp-wasm` for a wasm
//! build. One file is written into the library's own crate: its manifest.
//! The `package/` directory next to these is assembled from them by
//! [`crate::package`].

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

/// The build target of a binding: a runtime, built for the host, for wasm,
/// or cross-compiled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Target {
    pub runtime: Runtime,
    pub wasm: bool,
    /// The target triple of a cross-compiled build. The binding is then a
    /// static library, linked later with the program: by hxcpp, or by Ash
    /// for HashLink.
    pub cross: Option<&'static str>,
}

impl Target {
    /// The name of its output directory, such as `hl`, `cpp-wasm` or
    /// `cpp-aarch64-apple-ios`.
    pub fn name(self) -> String {
        let runtime = self.runtime.name();
        match self.cross {
            Some(triple) => format!("{runtime}-{triple}"),
            None if self.wasm => format!("{runtime}-wasm"),
            None => runtime.to_owned(),
        }
    }

    // The rustc target triple, or `None` for the host. The wasm triple
    // depends on what compiles the program: Ash compiles HashLink bytecode
    // to wasm, and Emscripten compiles hxcpp's C++.
    pub fn triple(self) -> Option<&'static str> {
        match (self.cross, self.wasm, self.runtime) {
            (Some(triple), _, _) => Some(triple),
            (None, false, _) => None,
            (None, true, Runtime::Hl) => Some("wasm32-wasip1"),
            (None, true, Runtime::Cpp) => Some("wasm32-unknown-emscripten"),
        }
    }

    // Whether the adapter is a static library, linked into the program by a
    // later step. Only a native HashLink binding is a library the VM loads.
    fn archive(self) -> bool {
        self.runtime == Runtime::Cpp || self.wasm || self.cross.is_some()
    }

    // Whether the target platform is Windows.
    fn windows(self) -> bool {
        self.cross
            .map_or(cfg!(windows), |triple| triple.contains("windows"))
    }

    // Extra rustc flags. A wasm side module must be position-independent.
    // Emscripten implements exceptions as JavaScript exceptions, which a
    // library that unwinds cannot link against, so that build aborts on
    // panic.
    fn rustflags(self) -> Option<&'static str> {
        match (self.wasm, self.runtime) {
            // hxcpp links the static C runtime on Windows, and a library
            // linked into its program has to use the same one.
            (false, Runtime::Cpp) if self.windows() => Some("-C target-feature=+crt-static"),
            (false, _) => None,
            // Position-independent for a side module, and unwinding with wasm
            // exceptions in the encoding Ash emits.
            (true, Runtime::Hl) => Some(
                "-C relocation-model=pic -C target-feature=+mutable-globals,+exception-handling -C panic=unwind -C llvm-args=-wasm-use-legacy-eh=false",
            ),
            // The prebuilt std unwinds with wasm exceptions, as hxcpp does.
            (true, Runtime::Cpp) => None,
        }
    }
}

// Whether `file` was modified after `than`, or `than` does not exist.
fn newer(file: &Path, than: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    match (modified(file), modified(than)) {
        (Some(file), Some(than)) => file > than,
        (_, None) => true,
        (None, Some(_)) => false,
    }
}

// The adapters this command generates are written against the `hxn` crates
// of its own version, which the library's `hxn` dependency selects, so the
// two have to match.
fn check_version(package: &str, hxn: &str) -> Result<(), String> {
    let own = env!("CARGO_PKG_VERSION");
    if hxn.is_empty() || hxn == own {
        return Ok(());
    }
    Err(format!(
        "this is hxn {own}, and `{}` depends on hxn {}: the bindings it generates are for its own version. Depend on `hxn = {{ git = \"{}\", tag = \"v{own}\" }}`, or install the command of the library's version with `cargo install --git {} --tag v{} hxn-cli`",
        package,
        hxn,
        env!("CARGO_PKG_REPOSITORY"),
        env!("CARGO_PKG_REPOSITORY"),
        hxn,
    ))
}

/// The outputs of a build that a Haxe compile uses.
pub struct Built {
    /// The runtime's output directory: it holds the built library, and a
    /// compiled program is written there.
    pub dir: PathBuf,
    /// `haxe` flags and their values: the class paths, and for hxcpp the
    /// locations of the headers and the library.
    pub arguments: Vec<(&'static str, String)>,
    /// The same arguments as an hxml file.
    pub hxml: PathBuf,
    /// The class path of the library's Haxe classes for the runtime.
    pub classes: PathBuf,
    /// `hxn.h` and the library's headers, which hxcpp compiles against.
    pub include: PathBuf,
    /// The build artifact: the library a VM loads, or the static library
    /// hxcpp links.
    pub library: PathBuf,
    /// The class path of the declarations of the hxcpp classes, for a CPPIA
    /// script. Written only by a native hxcpp build.
    pub declarations: PathBuf,
}

// Writes `text` to `path` unless the file already has that content, so that
// cargo and haxe rebuild only what changed.
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

/// Runs a command with its output shown to the user, and returns an error
/// if it fails.
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

/// What a library's build scripts asked to link: library names and
/// directories, as cargo reports them.
#[derive(Default, Debug, PartialEq)]
pub struct Linked {
    libraries: Vec<String>,
    frameworks: Vec<String>,
    paths: Vec<String>,
}

impl Linked {
    // Reads cargo's `build-script-executed` messages. A static library is
    // already inside the archive cargo builds, so it is left out.
    fn read(messages: &str) -> Self {
        let mut out = Self::default();
        for line in messages.lines() {
            let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if message.get("reason").and_then(|r| r.as_str()) != Some("build-script-executed") {
                continue;
            }
            let strings = |key: &str| -> Vec<String> {
                message
                    .get(key)
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            };
            for library in strings("linked_libs") {
                let (kind, name) = library.split_once('=').unwrap_or(("", &library));
                // `dylib:+verbatim=x` and the like: the kind ends at a colon.
                let kind = kind.split(':').next().unwrap_or(kind);
                let list = match kind {
                    "static" => continue,
                    "framework" => &mut out.frameworks,
                    _ => &mut out.libraries,
                };
                if !list.iter().any(|known| known == name) {
                    list.push(name.to_owned());
                }
            }
            for path in strings("linked_paths") {
                let (kind, dir) = path.split_once('=').unwrap_or(("", &path));
                if kind != "framework" && !out.paths.iter().any(|known| known == dir) {
                    out.paths.push(dir.to_owned());
                }
            }
        }
        out
    }

    /// The hxcpp build XML that links these, for a program built for
    /// Windows or not.
    pub fn xml(&self, windows: bool) -> String {
        let mut out = String::from(
            "<xml>\n<!-- Written by hxn: what the library links. -->\n<target id=\"haxe\">\n",
        );
        let escape = |text: &str| {
            text.replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
        };
        for path in &self.paths {
            if windows {
                out.push_str(&format!("\t<flag value=\"-libpath:{}\"/>\n", escape(path)));
            } else {
                out.push_str(&format!("\t<flag value=\"-L{}\"/>\n", escape(path)));
            }
        }
        for library in &self.libraries {
            if windows {
                out.push_str(&format!("\t<lib name=\"{}.lib\"/>\n", escape(library)));
            } else {
                out.push_str(&format!("\t<lib name=\"-l{}\"/>\n", escape(library)));
            }
        }
        for framework in &self.frameworks {
            out.push_str(&format!(
                "\t<flag value=\"-framework\"/>\n\t<flag value=\"{}\"/>\n",
                escape(framework)
            ));
        }
        out.push_str("</target>\n</xml>\n");
        out
    }
}

// Runs cargo with `--message-format=json-render-diagnostics` and reads what
// its build scripts link. Diagnostics still go to standard error.
fn linked_by(cargo: &mut Command) -> Result<Linked, String> {
    let output = cargo
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|error| format!("running cargo: {error}"))?;
    if !output.status.success() {
        return Err(String::from("cargo failed"));
    }
    Ok(Linked::read(&String::from_utf8_lossy(&output.stdout)))
}

// The `out_dir` of the build script of the package `id`, from cargo's
// messages.
fn out_dir_in(messages: &str, id: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "build-script-executed")
        .find(|message| message["package_id"] == id)
        .and_then(|message| message["out_dir"].as_str().map(PathBuf::from))
}

/// The library's parsed API. A file its source includes from the build
/// script's `OUT_DIR` is read after `cargo check` has run that script, for
/// the host.
pub fn parse(project: &Project) -> Result<hxn_gen::Library, String> {
    let manifest = project.dir.join("Cargo.toml");
    let mut out_dir = || -> Result<PathBuf, String> {
        let id = printed(
            Command::new("cargo")
                .arg("pkgid")
                .arg("--manifest-path")
                .arg(&manifest)
                .args(["-p", &project.package]),
        )
        .ok_or_else(|| format!("`cargo pkgid -p {}` failed", project.package))?;
        let output = Command::new("cargo")
            .arg("check")
            .arg("--manifest-path")
            .arg(&manifest)
            .args(["-p", &project.package, "--message-format=json"])
            .arg("--target-dir")
            .arg(&project.target)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|error| format!("running cargo: {error}"))?;
        if !output.status.success() {
            return Err(String::from(
                "`cargo check`, which runs the build script, failed",
            ));
        }
        out_dir_in(&String::from_utf8_lossy(&output.stdout), id.trim())
            .ok_or_else(|| format!("`{}` has no build script that cargo ran", project.package))
    };
    hxn_gen::parse_crate_with(&project.library, &project.source, &mut out_dir)
        .map(|(library, _)| library)
        .map_err(|error| error.to_string())
}

pub fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

const NOTE: &str = "# Written by hxn. Do not edit.\n";

// The Cargo manifest of an adapter crate. It depends on the library, and on
// hxn's crates from the same source the library uses. Its empty
// `[workspace]` table keeps it out of any enclosing workspace.
fn manifest(project: &Project, target: Target, later: bool) -> String {
    let (kind, crates): (&str, &[&str]) = match target.runtime {
        // cargo cannot link a wasm library whose imports are provided by
        // the program, so it is built as a static library and linked in a
        // later step.
        Runtime::Hl if target.archive() => ("staticlib", &["hxn-abi", "hxn-hl"]),
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
    // Only the adapter of a library with asynchronous results depends on
    // hxn-future.
    if later {
        out.push_str(&format!(
            "hxn-future = {}\n",
            project.hxn.dependency("hxn-future")
        ));
    }
    out.push_str("\n[workspace]\n");
    out
}

// The VM's symbols are resolved when the VM loads the library. Most Unix
// linkers allow undefined symbols in a shared object. Apple's linker needs
// `-undefined dynamic_lookup` for that. On Windows the symbols are resolved
// at link time against the VM's `libhl.lib`, found in HXN_HL_DIR.
const HL_BUILD_SCRIPT: &str = "// Written by hxn. Do not edit.\nfn main() {\n    println!(\"cargo:rerun-if-changed=build.rs\");\n    println!(\"cargo:rerun-if-env-changed=HXN_HL_DIR\");\n    if std::env::var(\"CARGO_CFG_TARGET_VENDOR\").as_deref() == Ok(\"apple\") {\n        println!(\"cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup\");\n    }\n    if std::env::var(\"CARGO_CFG_TARGET_OS\").as_deref() == Ok(\"windows\") {\n        if let Ok(dir) = std::env::var(\"HXN_HL_DIR\") {\n            println!(\"cargo:rustc-link-search=native={dir}\");\n        }\n        println!(\"cargo:rustc-link-lib=dylib=libhl\");\n    }\n}\n";

// The file name of the library cargo builds from an adapter crate.
fn artifact(library: &str, target: Target) -> String {
    match target.runtime {
        Runtime::Hl if target.archive() && target.windows() && !target.wasm => {
            format!("{library}_hl.lib")
        }
        Runtime::Hl if target.archive() => format!("lib{library}_hl.a"),
        Runtime::Hl if cfg!(windows) => format!("{library}_hl.dll"),
        Runtime::Hl if cfg!(target_os = "macos") => format!("lib{library}_hl.dylib"),
        Runtime::Hl => format!("lib{library}_hl.so"),
        Runtime::Cpp if target.windows() && !target.wasm => format!("{library}_cpp.lib"),
        Runtime::Cpp => format!("lib{library}_cpp.a"),
    }
}

// A command's standard output, or `None` if it could not be run or failed.
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
        .ok_or_else(|| String::from("`rustc -vV` did not print the expected line"))
}

/// Whether `program` is in a directory on PATH.
pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path)
            .any(|dir| dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file())
    })
}

// The name of each primitive's resolver function in a HashLink adapter. A
// VM looks up a primitive by that name.
fn resolvers(adapter: &str) -> Vec<&str> {
    adapter
        .split("define_prim!(")
        .skip(1)
        .filter_map(|rest| rest.split(',').next())
        .map(str::trim)
        .collect()
}

// Links the adapter's static library into a side module that a wasm program
// loads. Its undefined symbols, the VM's functions and the allocator, become
// imports that the program provides.
fn side_module(archive: &Path, adapter: &str, module: &Path) -> Result<(), String> {
    let sysroot = printed(Command::new("rustc").args(["--print", "sysroot"]))
        .ok_or("`rustc --print sysroot` did not print a path")?;
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

// Writes these classes to `dir` and deletes every other file there. A stale
// class for an interface the library has removed would otherwise still be
// compiled.
fn classes_in(dir: &Path, classes: &[hxn_gen::File]) -> Result<(), String> {
    for class in classes {
        write(&dir.join(&class.path), &class.text)?;
    }
    let kept: Vec<PathBuf> = classes
        .iter()
        .map(|class| PathBuf::from(&class.path))
        .collect();
    keep_only(dir, &kept)
}

/// The files under `dir`, as paths relative to it, in any order. A class in
/// a package is in a directory of its own.
pub fn files_under(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let at = dir.join(&relative);
        for entry in std::fs::read_dir(&at).map_err(|error| format!("{}: {error}", at.display()))? {
            let entry = entry.map_err(|error| format!("{}: {error}", at.display()))?;
            let path = relative.join(entry.file_name());
            if entry.path().is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    Ok(found)
}

/// Deletes every file under `dir` that is not one of `kept`, relative paths,
/// and then every directory left empty.
pub fn keep_only(dir: &Path, kept: &[PathBuf]) -> Result<(), String> {
    for file in files_under(dir)? {
        if !kept.contains(&file) {
            let path = dir.join(&file);
            std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    // Deepest first, so a directory's own subdirectories have gone.
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).map_err(|error| format!("{}: {error}", at.display()))? {
            let entry = entry.map_err(|error| format!("{}: {error}", at.display()))?;
            if entry.path().is_dir() {
                pending.push(entry.path());
                dirs.push(entry.path());
            }
        }
    }
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in dirs {
        if std::fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_none()) {
            std::fs::remove_dir(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

/// Build options.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    pub release: bool,
    /// Passes `--timings` to cargo, which reports how long each crate took
    /// to build.
    pub timings: bool,
    /// The directory that contains the HashLink VM.
    pub hashlink: Option<&'a Path>,
}

/// Parses the library's marked items, generates its binding for `target`
/// and builds the adapter.
pub fn build(project: &Project, target: Target, options: Options) -> Result<Built, String> {
    let Options {
        release,
        timings,
        hashlink,
    } = options;
    let name = &project.library;
    let runtime = target.runtime;
    check_version(&project.package, &project.hxn_version)?;
    if target.wasm && runtime == Runtime::Hl {
        // An Ash side module needs std rebuilt as position-independent code
        // that unwinds, and only a nightly cargo can rebuild std.
        let release = rustc_reports("release:")?;
        if !release.contains("nightly") && !release.contains("dev") {
            return Err(format!(
                "a wasm build rebuilds Rust's standard library, which requires a nightly toolchain, but the current toolchain is {release}: install one with `rustup toolchain install nightly --component rust-src` and set RUSTUP_TOOLCHAIN=nightly"
            ));
        }
    }
    if target.wasm && runtime == Runtime::Cpp && !on_path("emcc") {
        return Err(String::from(
            "`emcc` was not found: hxcpp builds for wasm with the Emscripten SDK, which has to be installed and on PATH",
        ));
    }
    if let Some(webidl) = &project.webidl {
        crate::import::sync(name, webidl, &project.bindings)?;
    }
    let library = parse(project)?;
    if library.interfaces.is_empty() {
        return Err(format!(
            "{}: nothing is marked `#[hxn::interface]` or `#[hxn::function]`",
            project.source.display()
        ));
    }

    // The manifest is written into the crate's directory so that it is
    // committed: a change to it is a change to the library's ABI.
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
            if !target.archive() {
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
    // The exports carry no safety docs of their own: each one's safety
    // contract is the ABI's, and its arguments are the ones the runtime
    // passes.
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

    // The adapter starts from the library's Cargo.lock, so it uses the
    // dependency versions the library resolved, and starts from it again
    // whenever the library's changes; cargo then adds the adapter's own.
    let lock = krate.join("Cargo.lock");
    let library_lock = project
        .dir
        .ancestors()
        .map(|above| above.join("Cargo.lock"))
        .find(|path| path.is_file());
    if let Some(library_lock) = library_lock
        && newer(&library_lock, &lock)
    {
        std::fs::copy(&library_lock, &lock)
            .map_err(|error| format!("{}: {error}", library_lock.display()))?;
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
        cargo.args(["--target", triple]);
        // A library's C code, compiled by the `cc` crate, is compiled for
        // Emscripten by its own compiler, which `cc` does not pick itself.
        if triple == "wasm32-unknown-emscripten" {
            for (variable, tool) in [
                ("CC_wasm32_unknown_emscripten", "emcc"),
                ("AR_wasm32_unknown_emscripten", "emar"),
            ] {
                if std::env::var_os(variable).is_none() {
                    cargo.env(variable, tool);
                }
            }
        }
        if target.wasm && runtime == Runtime::Hl {
            cargo.arg("-Zbuild-std=std,panic_unwind");
        }
        built.push(triple);
    }
    // HXN_HL_DIR tells the adapter's build script where the VM's library is.
    // A value already set in the environment is kept.
    if let Some(dir) = hashlink
        && std::env::var_os("HXN_HL_DIR").is_none()
    {
        cargo.env("HXN_HL_DIR", dir);
    }
    // Appended to the RUSTFLAGS already in the environment, which setting
    // the variable would otherwise replace.
    if let Some(flags) = target.rustflags() {
        let given = std::env::var("RUSTFLAGS").unwrap_or_default();
        cargo.env("RUSTFLAGS", format!("{given} {flags}"));
    }
    // For hxcpp, what the library's build scripts link is read from cargo's
    // messages: a static library does not carry it to the program's link.
    let linked = if runtime == Runtime::Cpp {
        cargo.arg("--message-format=json-render-diagnostics");
        linked_by(&mut cargo)?
    } else {
        run(&mut cargo)?;
        Linked::default()
    };
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
        Runtime::Hl if target.cross.is_some() => {
            // Named after the library, as an .hdll is, for Ash's
            // `--native-lib`.
            let archive = dir.join(if target.windows() {
                format!("{name}.lib")
            } else {
                format!("lib{name}.a")
            });
            std::fs::copy(&built, &archive)
                .map_err(|error| format!("{}: {error}", built.display()))?;
            built = archive;
        }
        Runtime::Hl => {
            // Copied to the file name the Haxe classes load it by.
            let hdll = dir.join(format!("{name}.hdll"));
            std::fs::copy(&built, &hdll)
                .map_err(|error| format!("{}: {error}", built.display()))?;
            built = hdll;
        }
        Runtime::Cpp => {
            if target.wasm {
                // hxcpp assumes the host's 64-bit pointers unless told
                // otherwise, and its UTF-16 strings call a Windows-only
                // function under Emscripten.
                for define in ["emscripten", "HXCPP_M32", "disable_unicode_strings"] {
                    arguments.push(("-D", String::from(define)));
                }
            } else if cfg!(windows) && target.cross.is_none() {
                // On Windows hxcpp builds a 32-bit program by default, and
                // the library is built for the host architecture.
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
            let link = dir.join("link.xml");
            write(&link, &linked.xml(target.windows()))?;
            arguments.push(("-D", format!("{upper}_LINK={}", text(&link))));
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
    fn a_lock_file_is_copied_again_once_it_changes() {
        let dir = std::env::temp_dir().join(format!("hxn-newer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (library, adapter) = (dir.join("library.lock"), dir.join("adapter.lock"));
        std::fs::write(&library, "a").unwrap();
        assert!(newer(&library, &adapter));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&adapter, "a").unwrap();
        assert!(!newer(&library, &adapter));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&library, "b").unwrap();
        assert!(newer(&library, &adapter));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_library_on_another_hxn_is_refused() {
        let own = env!("CARGO_PKG_VERSION");
        assert!(check_version("lib", own).is_ok());
        let error = check_version("lib", "0.0.1").unwrap_err();
        assert!(error.contains(&format!(
            "this is hxn {own}, and `lib` depends on hxn 0.0.1"
        )));
        assert!(error.contains(&format!("tag = \"v{own}\"")));
        assert!(error.contains("--tag v0.0.1 hxn-cli"));
    }

    #[test]
    fn what_build_scripts_link_is_read_from_cargo() {
        let messages = concat!(
            "{\"reason\":\"compiler-artifact\"}\n",
            "{\"reason\":\"build-script-executed\",\"linked_libs\":[\"sqlite3\",\"static=tally_c\",\"dylib=z\",\"framework=Security\"],\"linked_paths\":[\"native=/opt/lib\"]}\n",
            "not json\n",
        );
        let linked = Linked::read(messages);
        assert_eq!(linked.libraries, ["sqlite3", "z"]);
        assert_eq!(linked.frameworks, ["Security"]);
        assert_eq!(linked.paths, ["/opt/lib"]);
        let xml = linked.xml(false);
        assert!(
            xml.contains("<lib name=\"-lsqlite3\"/>")
                && xml.contains("<flag value=\"-L/opt/lib\"/>")
        );
        assert!(xml.contains("<flag value=\"Security\"/>"));
        assert!(linked.xml(true).contains("<lib name=\"sqlite3.lib\"/>"));
        assert!(
            Linked::default()
                .xml(false)
                .contains("<target id=\"haxe\">\n</target>")
        );
    }

    #[test]
    fn the_build_script_output_is_found_for_its_package() {
        let messages = concat!(
            "{\"reason\":\"build-script-executed\",\"package_id\":\"path+file:///w/other#0.1.0\",\"out_dir\":\"/t/other/out\"}\n",
            "{\"reason\":\"compiler-artifact\",\"package_id\":\"path+file:///w/window#0.1.0\"}\n",
            "{\"reason\":\"build-script-executed\",\"package_id\":\"path+file:///w/window#0.1.0\",\"out_dir\":\"/t/window/out\"}\n",
        );
        assert_eq!(
            out_dir_in(messages, "path+file:///w/window#0.1.0"),
            Some(PathBuf::from("/t/window/out"))
        );
        assert_eq!(out_dir_in(messages, "path+file:///w/none#0.1.0"), None);
    }

    #[test]
    fn a_wasm_build_has_its_own_directory_and_target() {
        let native = Target {
            runtime: Runtime::Hl,
            wasm: false,
            cross: None,
        };
        assert_eq!(native.name(), "hl");
        assert_eq!((native.triple(), native.rustflags()), (None, None));
        let ash = Target {
            runtime: Runtime::Hl,
            wasm: true,
            cross: None,
        };
        assert_eq!(ash.name(), "hl-wasm");
        assert_eq!(ash.triple(), Some("wasm32-wasip1"));
        assert_eq!(artifact("greeter", ash), "libgreeter_hl.a");
        let emscripten = Target {
            runtime: Runtime::Cpp,
            wasm: true,
            cross: None,
        };
        // A cross-compiled build uses the installed standard library.
        let mobile = Target {
            runtime: Runtime::Cpp,
            wasm: false,
            cross: Some("aarch64-apple-ios"),
        };
        assert_eq!(mobile.name(), "cpp-aarch64-apple-ios");
        assert_eq!(
            (mobile.triple(), mobile.rustflags()),
            (Some("aarch64-apple-ios"), None)
        );
        assert_eq!(artifact("greeter", mobile), "libgreeter_cpp.a");
        let windows = Target {
            cross: Some("x86_64-pc-windows-msvc"),
            ..mobile
        };
        assert_eq!(artifact("greeter", windows), "greeter_cpp.lib");
        // So is a HashLink one, which Ash links into a mobile program.
        let ash_mobile = Target {
            runtime: Runtime::Hl,
            ..mobile
        };
        assert_eq!(ash_mobile.name(), "hl-aarch64-apple-ios");
        assert!(ash_mobile.archive() && !native.archive() && ash.archive());
        assert_eq!(artifact("greeter", ash_mobile), "libgreeter_hl.a");
        assert!(windows.rustflags().is_some());
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
        // Named after the library, then the interface.
        assert!(
            found.contains(&"hlp_greeter_greeter_new")
                && found.contains(&"hlp_greeter_greeter_greeted")
        );
    }
}
