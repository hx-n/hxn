//! Writes everything hxn-gen makes for a library into a directory.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: hxn-gen --library NAME --source FILE --crate PATH --out DIR

  --library  the name the library's binaries and symbols carry
  --source   its lib.rs; the files of the modules declared from there are read too
  --crate    the path its items are found under in the adapters, such as counter_core
  --out      where to write: the manifest, c/ and hl/ adapters, include/ headers and haxe/ classes";

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    println!("{}", path.display());
    Ok(())
}

fn run() -> Result<(), String> {
    let mut library = None;
    let mut source = None;
    let mut core = None;
    let mut out = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let slot = match flag.as_str() {
            "--library" => &mut library,
            "--source" => &mut source,
            "--crate" => &mut core,
            "--out" => &mut out,
            _ => return Err(format!("unknown argument `{flag}`\n\n{USAGE}")),
        };
        *slot = Some(
            arguments
                .next()
                .ok_or_else(|| format!("{flag} needs a value\n\n{USAGE}"))?,
        );
    }
    let (Some(name), Some(source), Some(core), Some(out)) = (library, source, core, out) else {
        return Err(String::from(USAGE));
    };

    let (library, _) =
        hxn_gen::parse_crate(&name, Path::new(&source)).map_err(|error| error.to_string())?;
    let out = PathBuf::from(out);

    write(
        &out.join(format!("{name}.hxn.json")),
        &hxn_gen::manifest(&library),
    )?;
    write(
        &out.join(format!("{name}.hxn.webidl")),
        &hxn_gen::webidl(&library),
    )?;
    write(
        &out.join("c/adapter.rs"),
        &hxn_gen::c_adapter(&library, &core),
    )?;
    write(
        &out.join("hl/adapter.rs"),
        &hxn_gen::hl_adapter(&library, &core),
    )?;
    write(
        &out.join(format!("include/{name}_abi.h")),
        &hxn_gen::c_header(&library),
    )?;
    write(
        &out.join(format!("include/{name}_glue.h")),
        &hxn_gen::cpp_glue(&library),
    )?;
    for file in hxn_gen::haxe_hl(&library) {
        write(&out.join("haxe/hl").join(&file.path), &file.text)?;
    }
    for file in hxn_gen::haxe_cpp(&library) {
        write(&out.join("haxe/cpp").join(&file.path), &file.text)?;
    }
    for file in hxn_gen::haxe_cppia(&library) {
        write(&out.join("haxe/cppia").join(&file.path), &file.text)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
