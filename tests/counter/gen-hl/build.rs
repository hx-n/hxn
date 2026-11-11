use std::path::{Path, PathBuf};

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
        hxn_gen::hl_adapter(&library, "counter_core"),
    )
    .expect("writing the adapter");

    // The VM's symbols are resolved when it loads the library. Other Unix
    // linkers leave them undefined in a shared object; Apple's must be told.
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
