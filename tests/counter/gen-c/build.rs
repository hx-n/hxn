use std::path::PathBuf;

// Writes the adapter from the library's source. The header, the glue and
// the Haxe classes come from the same source through the hxn-gen command.
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
        hxn_gen::c_adapter(&library, "counter_core"),
    )
    .expect("writing the adapter");
}
