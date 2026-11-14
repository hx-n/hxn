//! Copies everything a Haxe program needs to use a library into one
//! directory, laid out as a haxelib:
//!
//! | Path | What it is |
//! |---|---|
//! | `haxelib.json` | the library's name and version, and the other haxelib fields the crate's metadata provides |
//! | `extraParams.hxml` | calls the macro `hxn.Bind.lib`, which sets up the rest at compile time |
//! | `<library>.hxn.json` | the manifest, which `hxn.Bind` uses to locate the package |
//! | `hxn/` | hxn's Haxe support classes |
//! | `binding/<runtime>/` | the library's Haxe classes for `hl` or `cpp` |
//! | `binding/cppia/` | declarations of the `cpp` classes, for a CPPIA script |
//! | `include/` | `hxn.h` and the library's headers, for hxcpp |
//! | `native/<triple>/` | the built libraries for one target triple, which may be a wasm one; for hxcpp also `link.xml`, the system libraries the archive links. A HashLink library built with `--target` is the archive `lib<library>.a`, which Ash links into the program |
//!
//! Only `native/` differs between platforms. Packaging on another machine
//! into the same directory, or copying that machine's `native/<triple>/` into
//! it, adds that platform to the package.

use std::path::{Path, PathBuf};

use crate::make::{self, Built, Runtime, Target, write};
use crate::project::Project;
use crate::support;

// Copies the files of `from` into `to` and deletes every other file in `to`.
fn mirror(from: &Path, to: &Path) -> Result<(), String> {
    let failed = |path: &Path, error: std::io::Error| format!("{}: {error}", path.display());
    std::fs::create_dir_all(to).map_err(|error| failed(to, error))?;
    let mut kept = Vec::new();
    for entry in std::fs::read_dir(from).map_err(|error| failed(from, error))? {
        let entry = entry.map_err(|error| failed(from, error))?;
        let text = std::fs::read_to_string(entry.path()).map_err(|e| failed(&entry.path(), e))?;
        write(&to.join(entry.file_name()), &text)?;
        kept.push(entry.file_name());
    }
    for entry in std::fs::read_dir(to).map_err(|error| failed(to, error))? {
        let entry = entry.map_err(|error| failed(to, error))?;
        if !kept.contains(&entry.file_name()) {
            std::fs::remove_file(entry.path()).map_err(|error| failed(&entry.path(), error))?;
        }
    }
    Ok(())
}

// The file name of a built library in a package. `hxn.Bind` looks for it
// under that name.
fn packaged(library: &str, target: Target, built: &Built) -> String {
    match target.runtime {
        Runtime::Hl if target.wasm => format!("{library}.wasm"),
        Runtime::Hl if target.cross.is_none() => format!("{library}.hdll"),
        _ => built
            .library
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

// The text of haxelib.json. An optional field is written only if the
// crate's metadata provides it.
fn haxelib(project: &Project) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    let list = |items: &[String]| {
        let items: Vec<String> = items.iter().map(|item| quoted(item)).collect();
        format!("[{}]", items.join(", "))
    };
    let listing = &project.listing;
    let mut fields = vec![
        format!("\"name\": {}", quoted(&project.library)),
        format!("\"version\": {}", quoted(&project.version)),
        format!("\"description\": {}", quoted(&project.description)),
    ];
    if let Some(url) = &listing.url {
        fields.push(format!("\"url\": {}", quoted(url)));
    }
    if let Some(license) = listing.haxelib_license() {
        fields.push(format!("\"license\": {}", quoted(license)));
    }
    if !listing.tags.is_empty() {
        fields.push(format!("\"tags\": {}", list(&listing.tags)));
    }
    if !listing.contributors.is_empty() {
        fields.push(format!("\"contributors\": {}", list(&listing.contributors)));
    }
    if let Some(note) = &listing.releasenote {
        fields.push(format!("\"releasenote\": {}", quoted(note)));
    }
    format!("{{\n  {}\n}}\n", fields.join(",\n  "))
}

/// Builds the library for each of `targets` and copies the results into
/// `out`. Returns the target triples of the libraries it added.
pub fn package(
    project: &Project,
    targets: &[Target],
    options: make::Options,
    out: &Path,
) -> Result<Vec<String>, String> {
    let name = &project.library;
    let host = make::rustc_reports("host:")?;
    let mut triples = Vec::new();
    for &target in targets {
        let built = make::build(project, target, options)?;
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
        // What the hxcpp archive links, beside it.
        let link = built.dir.join("link.xml");
        if target.runtime == Runtime::Cpp && link.is_file() {
            std::fs::copy(&link, native.join("link.xml"))
                .map_err(|error| format!("{}: {error}", link.display()))?;
        }
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

/// Where a library is packaged unless the command line specifies a directory.
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
