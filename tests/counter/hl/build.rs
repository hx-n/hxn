fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The VM's symbols are resolved when it loads the library. Other Unix
    // linkers leave them undefined in a shared object; Apple's must be told.
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
