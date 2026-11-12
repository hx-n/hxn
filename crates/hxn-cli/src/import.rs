//! `hxn import`: makes a Rust library crate for a C library, from its header.

use std::path::{Path, PathBuf};

use crate::new::dependency;
use crate::project::{is_library_name, quoted};

const IGNORE: &str = "/target\n";

fn library_file(name: &str) -> String {
    format!(
        "//! Rust bindings for the C library `{name}`, made by `hxn import`.\n//!\n//! Its API is described in `{name}.webidl`. Edit that file, not\n//! `src/bindings.rs`: `hxn build` generates the bindings from it again.\n\nmod bindings;\npub use bindings::*;\n"
    )
}

fn program() -> &'static str {
    "class Main {\n\tstatic function main() {\n\t\t// The library's Haxe classes are generated from its WebIDL file.\n\t\thxn.Loop.exit(0);\n\t}\n}\n"
}

// What the crate's build script does: compile the C sources it was given,
// and link the libraries it was told to.
fn build_script(name: &str, sources: &[String], links: &[String]) -> String {
    let upper = name.to_uppercase();
    let mut out = String::from("// Written by `hxn import`.\nfn main() {\n");
    if !sources.is_empty() {
        out.push_str("    println!(\"cargo:rerun-if-changed=c\");\n    cc::Build::new()\n        .include(\"c\")\n");
        for source in sources {
            out.push_str(&format!("        .file(\"c/{source}\")\n"));
        }
        out.push_str(&format!("        .compile(\"{name}_c\");\n"));
    }
    for link in links {
        out.push_str(&format!("    println!(\"cargo:rustc-link-lib={link}\");\n"));
    }
    if !links.is_empty() {
        out.push_str(&format!(
            "    // Where the library is, when the linker would not find it.\n    println!(\"cargo:rerun-if-env-changed={upper}_LIB_DIR\");\n    if let Ok(dir) = std::env::var(\"{upper}_LIB_DIR\") {{\n        println!(\"cargo:rustc-link-search=native={{dir}}\");\n    }}\n"
        ));
    }
    out.push_str("}\n");
    out
}

fn manifest(name: &str, hxn: &str, compiles: bool) -> String {
    let mut out = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nhxn = {hxn}\n"
    );
    if compiles {
        out.push_str("\n[build-dependencies]\ncc = \"1\"\n");
    }
    out.push_str(&format!(
        "\n[package.metadata.hxn]\n# What src/bindings.rs is generated from.\nwebidl = {}\n",
        quoted(&format!("{name}.webidl"))
    ));
    out
}

/// What `hxn import` is given.
pub struct Request<'a> {
    pub header: &'a Path,
    pub name: &'a str,
    /// C files compiled into the crate.
    pub sources: &'a [PathBuf],
    /// Libraries the crate links.
    pub links: &'a [String],
    pub out: &'a Path,
    pub checkout: Option<&'a Path>,
    /// Passed to clang when it reads the header.
    pub clang: &'a [String],
}

fn copy(from: &Path, into: &Path) -> Result<String, String> {
    let file = from
        .file_name()
        .ok_or_else(|| format!("{} is not a file", from.display()))?
        .to_string_lossy()
        .into_owned();
    std::fs::copy(from, into.join(&file))
        .map_err(|error| format!("{}: {error}", from.display()))?;
    Ok(file)
}

/// Makes the crate, and returns what the importer could not map.
pub fn import(request: &Request) -> Result<Vec<String>, String> {
    let name = request.name;
    if !is_library_name(name) {
        return Err(format!(
            "`{name}` cannot name a library: use lowercase letters, digits and `_`, starting with a letter. Name it with --name"
        ));
    }
    let dir = request.out;
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let hxn = dependency(request.checkout)?;
    let c = dir.join("c");
    std::fs::create_dir_all(&c).map_err(|error| format!("{}: {error}", c.display()))?;
    // The header and sources are copied in, so the crate builds on its own.
    let header = copy(request.header, &c)?;
    let mut sources = Vec::new();
    for source in request.sources {
        sources.push(copy(source, &c)?);
    }
    let mut clang = vec![format!("-I{}", c.display())];
    clang.extend(request.clang.iter().cloned());
    let api = hxn_c::import(&c.join(&header), name, &clang)?;
    let webidl = format!("{name}.webidl");
    let files = [
        ("Cargo.toml", manifest(name, &hxn, !sources.is_empty())),
        (".gitignore", String::from(IGNORE)),
        ("build.rs", build_script(name, &sources, request.links)),
        (webidl.as_str(), hxn_c::write(&api)),
        ("src/lib.rs", library_file(name)),
        ("src/bindings.rs", hxn_c::bindings(&api, &webidl)),
        ("haxe/Main.hx", String::from(program())),
    ];
    for (path, text) in files {
        let path = dir.join(path);
        let parent = path.parent().expect("a file in the new directory");
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(api.skipped)
}

/// Generates `bindings` from the WebIDL file `webidl` again, if it changed.
pub fn sync(library: &str, webidl: &Path, bindings: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(webidl)
        .map_err(|error| format!("{}: {error}", webidl.display()))?;
    let api =
        hxn_c::parse(library, &text).map_err(|error| format!("{}: {error}", webidl.display()))?;
    let source = webidl
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let generated = hxn_c::bindings(&api, &source);
    if std::fs::read_to_string(bindings).ok().as_deref() != Some(generated.as_str()) {
        std::fs::write(bindings, generated)
            .map_err(|error| format!("{}: {error}", bindings.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_script_compiles_and_links_what_it_is_given() {
        let script = build_script("tally", &[String::from("tally.c")], &[String::from("m")]);
        assert!(script.contains(".file(\"c/tally.c\")"));
        assert!(script.contains(".compile(\"tally_c\");"));
        assert!(script.contains("cargo:rustc-link-lib=m"));
        assert!(script.contains("TALLY_LIB_DIR"));
        let bare = build_script("tally", &[], &[]);
        assert!(!bare.contains("cc::Build"));
    }
}
