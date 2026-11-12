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

    println!("cargo:rerun-if-env-changed=HXN_HL_DIR");
    // The VM's symbols are resolved when it loads the library. Other Unix
    // linkers allow undefined symbols in a shared object. Apple's linker needs
    // a flag to allow them, and Windows resolves them at link time against
    // the VM's `libhl.lib`, found in HXN_HL_DIR.
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if let Ok(dir) = std::env::var("HXN_HL_DIR") {
            println!("cargo:rustc-link-search=native={dir}");
        }
        println!("cargo:rustc-link-lib=dylib=libhl");
    }
}
