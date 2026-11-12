fn main() {
    println!("cargo:rerun-if-changed=build.rs");
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
