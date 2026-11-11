//! Gathers what a Haxe program needs of a library into one directory, which
//! is a haxelib:
//!
//! | Path | What it is |
//! |---|---|
//! | `haxelib.json` | the library's name and version |
//! | `extraParams.hxml` | runs `hxn.Bind.lib`, which does the rest at compile time |
//! | `<library>.hxn.json` | the manifest, which `hxn.Bind` finds the package by |
//! | `hxn/` | hxn's own Haxe classes |
//! | `binding/<runtime>/` | the library's Haxe classes for `hl` or `cpp` |
//! | `binding/cppia/` | declarations of the `cpp` ones, for a CPPIA script |
//! | `include/` | `hxn.h` and the library's headers, for hxcpp |
//! | `native/<triple>/` | the built libraries for one kind of machine, or for wasm |
//!
//! Only `native/` differs between machines, so packaging on another one into
//! the same directory, or copying its `native/<triple>/` in, adds to it.

use std::path::{Path, PathBuf};

use crate::make::{self, Built, Runtime, Target, write};
use crate::project::Project;
use crate::support;

// Makes `to` hold the files of `from` and no others.
fn mirror(from: &Path, to: &Path) -> Result<(), String> {
    let said = |path: &Path, error: std::io::Error| format!("{}: {error}", path.display());
    std::fs::create_dir_all(to).map_err(|error| said(to, error))?;
    let mut kept = Vec::new();
    for entry in std::fs::read_dir(from).map_err(|error| said(from, error))? {
        let entry = entry.map_err(|error| said(from, error))?;
        let text = std::fs::read_to_string(entry.path()).map_err(|e| said(&entry.path(), e))?;
        write(&to.join(entry.file_name()), &text)?;
        kept.push(entry.file_name());
    }
    for entry in std::fs::read_dir(to).map_err(|error| said(to, error))? {
        let entry = entry.map_err(|error| said(to, error))?;
        if !kept.contains(&entry.file_name()) {
            std::fs::remove_file(entry.path()).map_err(|error| said(&entry.path(), error))?;
        }
    }
    Ok(())
}

// What a built library is called in a package, where `hxn.Bind` looks for it.
fn packaged(library: &str, target: Target, built: &Built) -> String {
    match target.runtime {
        Runtime::Hl if target.wasm => format!("{library}.wasm"),
        Runtime::Hl => format!("{library}.hdll"),
        Runtime::Cpp => built
            .library
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn haxelib(project: &Project) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    format!(
        "{{\n  \"name\": {},\n  \"version\": {},\n  \"description\": {}\n}}\n",
        quoted(&project.library),
        quoted(&project.version),
        quoted(&project.description)
    )
}

/// Builds the library for each of `targets` and gathers the results in
/// `out`. Returns what the libraries added were built for.
pub fn package(
    project: &Project,
    targets: &[Target],
    release: bool,
    hashlink: Option<&Path>,
    out: &Path,
) -> Result<Vec<String>, String> {
    let name = &project.library;
    let host = make::rustc_says("host:")?;
    let mut triples = Vec::new();
    for &target in targets {
        let built = make::build(project, target, release, hashlink)?;
        mirror(
            &built.classes,
            &out.join("binding").join(target.runtime.name()),
        )?;
        if target.runtime == Runtime::Cpp {
            mirror(&built.include, &out.join("include"))?;
        }
        if target.runtime == Runtime::Cpp && !target.wasm {
            mirror(&built.declarations, &out.join("binding").join("cppia"))?;
        }
        let triple = target.triple().unwrap_or(&host).to_owned();
        let native = out.join("native").join(&triple);
        std::fs::create_dir_all(&native)
            .map_err(|error| format!("{}: {error}", native.display()))?;
        std::fs::copy(&built.library, native.join(packaged(name, target, &built)))
            .map_err(|error| format!("{}: {error}", built.library.display()))?;
        if !triples.contains(&triple) {
            triples.push(triple);
        }
    }

    let manifest = format!("{name}.hxn.json");
    let text = std::fs::read_to_string(project.dir.join(&manifest))
        .map_err(|error| format!("{manifest}: {error}"))?;
    write(&out.join(&manifest), &text)?;
    for (path, text) in support::HAXE {
        write(&out.join(path), text)?;
    }
    write(
        &out.join("extraParams.hxml"),
        &format!("--macro hxn.Bind.lib(\"{name}\")\n"),
    )?;
    write(&out.join("haxelib.json"), &haxelib(project))?;
    Ok(triples)
}

/// Where a library is packaged unless the command line says.
pub fn default_directory(project: &Project) -> PathBuf {
    project
        .target
        .join("hxn")
        .join(&project.library)
        .join("package")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mirrored_directory_loses_what_its_source_no_longer_has() {
        let dir = std::env::temp_dir().join(format!("hxn-mirror-{}", std::process::id()));
        let (from, to) = (dir.join("from"), dir.join("to"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("Kept.hx"), "class Kept {}").unwrap();
        std::fs::write(from.join("Gone.hx"), "class Gone {}").unwrap();
        mirror(&from, &to).unwrap();
        std::fs::remove_file(from.join("Gone.hx")).unwrap();
        std::fs::write(from.join("Kept.hx"), "class Kept { var one:Int; }").unwrap();
        mirror(&from, &to).unwrap();
        let held: Vec<_> = std::fs::read_dir(&to)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(held, ["Kept.hx"]);
        assert_eq!(
            std::fs::read_to_string(to.join("Kept.hx")).unwrap(),
            "class Kept { var one:Int; }"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
