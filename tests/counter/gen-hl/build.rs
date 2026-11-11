use std::path::PathBuf;

fn main() {
    let source = "../core/src/lib.rs";
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed=build.rs");
    let text = std::fs::read_to_string(source).expect("reading the library's source");
    let library =
        hxn_gen::parse("counter", &text).unwrap_or_else(|error| panic!("{source}: {error}"));
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
