//! The importer and the generator over the C library in tests/cimport.

use std::path::Path;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cimport");

fn clang() -> bool {
    std::process::Command::new("clang")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn a_header_is_imported_as_webidl() {
    if !clang() {
        eprintln!("clang is not installed; skipped");
        return;
    }
    let api = hxn_c::import(&Path::new(ROOT).join("tally.h"), "tally", &[]).unwrap();
    let idl = hxn_c::write(&api);
    print!("{idl}");
    weedle::parse(&idl).unwrap_or_else(|error| panic!("not WebIDL: {error:?}"));
    // What the importer writes, before it is edited.
    let committed = std::fs::read_to_string(Path::new(ROOT).join("imported.webidl")).unwrap();
    assert_eq!(idl, committed);
    // It reads back as it was written, less the list of what was not
    // imported, which is only comments.
    let mut expected = api.clone();
    expected.skipped.clear();
    assert_eq!(hxn_c::parse("tally", &idl).unwrap(), expected);
}

#[test]
fn the_edited_webidl_generates_rust_marked_for_hxn() {
    let text = std::fs::read_to_string(Path::new(ROOT).join("tally.webidl")).unwrap();
    let api = hxn_c::parse("tally", &text).unwrap();
    let rust = hxn_c::bindings(&api, "tally.webidl");
    print!("{rust}");
    // The status code is a failure, not a result.
    assert!(rust.contains("pub fn add(&mut self, amount: i32) -> Result<(), String>"));
    assert!(rust.contains("return Err(format!(\"tally_add failed with status {value}\"));"));
    // The object is freed with its C function.
    assert!(rust.contains("unsafe { ffi::tally_free(self.raw) };"));
    // The text is copied, then freed with the function the WebIDL names.
    assert!(rust.contains("ffi::tally_string_free(value.cast_mut())"));
    // And it is what hxn reads.
    hxn_gen::parse("tally", &rust).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn a_mistake_in_the_webidl_is_reported_with_its_line() {
    let errors = [
        (
            "interface X {\n  long f();\n};\n",
            "line 2: an operation needs `[HxnC=<C function>]`",
        ),
        (
            "namespace Other {\n};\n",
            "line 1: the namespace of library `tally` is `Tally`",
        ),
        (
            "namespace Tally {\n  [HxnC=f] Unknown f();\n};\n",
            "line 2: `Unknown` is not a type",
        ),
        (
            "namespace Tally {\n  [HxnC=f, HxnStatus] long f();\n};\n",
            "it returns `undefined`",
        ),
        (
            "namespace Tally {\n  [HxnC=f, HxnFree=g] long f();\n};\n",
            "frees the text it returns",
        ),
        (
            "namespace Tally {\n  [HxnC=f] long? f();\n};\n",
            "only text and objects may be null",
        ),
        (
            "dictionary D {\n  required DOMString name;\n};\n",
            "a dictionary member is a boolean",
        ),
        (
            "[Odd] interface X {\n};\n",
            "`[Odd]` does not apply to an interface",
        ),
    ];
    for (text, expected) in errors {
        let error = hxn_c::parse("tally", text).unwrap_err();
        assert!(error.contains(expected), "{text}: {error}");
    }
}
