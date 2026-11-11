use std::path::{Path, PathBuf};

// Writes the adapter from the library's source. The header, the glue and
// the Haxe classes come from the same source through the hxn-gen command.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let (library, files) = hxn_gen::parse_crate("counter", Path::new("../core/src/lib.rs"))
        .unwrap_or_else(|error| panic!("{error}"));
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(
        out.join("adapter.rs"),
        hxn_gen::c_adapter(&library, "counter_core"),
    )
    .expect("writing the adapter");
}
