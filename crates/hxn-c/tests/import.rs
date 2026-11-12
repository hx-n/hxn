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
    weedle::parse(&idl).unwrap_or_else(|error| panic!("not WebIDL: {error:?}"));
    // What the importer writes, before it is edited. HXN_BLESS=1 writes it
    // instead, after a change to the importer.
    let golden = Path::new(ROOT).join("imported.webidl");
    if std::env::var_os("HXN_BLESS").is_some() {
        std::fs::write(&golden, &idl).unwrap();
    }
    assert_eq!(idl, std::fs::read_to_string(&golden).unwrap());
    // It reads back as it was written, less the lists of what was not
    // imported and what to check, which are only comments.
    let mut expected = api.clone();
    expected.skipped.clear();
    expected.review.clear();
    assert_eq!(hxn_c::parse("tally", &idl).unwrap(), expected);
}

#[test]
fn the_edited_webidl_generates_rust_marked_for_hxn() {
    let text = std::fs::read_to_string(Path::new(ROOT).join("tally.webidl")).unwrap();
    let api = hxn_c::parse("tally", &text).unwrap();
    let rust = hxn_c::bindings(&api, "tally.webidl");
    if std::env::var_os("HXN_SHOW").is_some() {
        print!("{rust}");
    }
    // The status code is a failure, not a result.
    assert!(rust.contains("pub fn add(&mut self, amount: i32) -> Result<(), String>"));
    assert!(rust.contains("return Err(format!(\"tally_add failed with status {status}\"));"));
    // The object is freed with its C function.
    assert!(rust.contains("unsafe { ffi::tally_free(self.raw) };"));
    // The text is copied, then freed with the function the WebIDL names.
    assert!(rust.contains("ffi::tally_string_free(value.cast_mut())"));
    // A callback C keeps is a stream, whose sender the object frees after
    // C frees the object.
    assert!(rust.contains("pub fn listen(&mut self) -> Stream<ListenerEvent>"));
    assert!(rust.contains("kept: std::sync::Mutex<Vec<Box<dyn Send>>>"));
    // A length other than a `size_t` is converted, and a fixed pointer
    // passed as it is.
    assert!(rust.contains("let data_length = i32::try_from(data.len())"));
    assert!(rust.contains("std::ptr::without_provenance_mut(-1_isize as usize)"));
    // Text C keeps is a copy it frees.
    assert!(rust.contains("label.into_raw(), Some(hxn_free_text)"));
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
        (
            "callback C = undefined (long a, long b, long c, long d, long e);\nnamespace Tally {\n  [HxnC=f] undefined f(C c);\n};\n",
            "line 3: `C` takes 5 values, and a Haxe function lent to C takes at most 4",
        ),
        (
            "callback C = undefined (unsigned long a);\nnamespace Tally {\n  [HxnC=f] undefined f(C c);\n};\n",
            "`a` of `C` is not one. If C keeps it, mark the argument `[HxnKept]`",
        ),
        (
            "callback C = undefined ();\nnamespace Tally {\n  [HxnC=f] long f([HxnKept] C c);\n};\n",
            "line 3: `f` keeps a callback, so it returns `undefined`",
        ),
        (
            "callback C = long ();\nnamespace Tally {\n  [HxnC=f] undefined f([HxnKept] C c);\n};\n",
            "line 3: C keeps `C`, which returns a value: give `C` `[HxnStop=n]`",
        ),
        (
            "namespace Tally {\n  [HxnC=f, HxnIgnore=int] undefined f();\n};\n",
            "`[HxnIgnore]` applies to an operation with an `[HxnKept]` argument",
        ),
        (
            "namespace Tally {\n  [HxnC=f] undefined f([HxnKept] long a);\n};\n",
            "`[HxnKept]` applies to a callback argument",
        ),
        (
            "namespace Tally {\n  [HxnC=f] undefined f([HxnLength=short] Uint8Array a);\n};\n",
            "`[HxnLength=short]` is not an integer type",
        ),
        (
            "namespace Tally {\n  [HxnC=f] undefined f(any a);\n};\n",
            "is `[HxnPass=n]`, passed a fixed value, or `[HxnFrees]`",
        ),
    ];
    for (text, expected) in errors {
        let error = hxn_c::parse("tally", text).unwrap_err();
        assert!(error.contains(expected), "{text}: {error}");
    } // C may call a callback it keeps with what a Haxe function cannot take.
    hxn_c::parse(
        "tally",
        "callback C = undefined (long a, long b, long c, long d, unsigned long long e);\nnamespace Tally {\n  [HxnC=f, HxnIgnore=pointer] undefined f([HxnKept] C c);\n};\n",
    )
    .unwrap();
}

#[test]
fn what_the_library_does_not_define_is_left_out() {
    let text = std::fs::read_to_string(Path::new(ROOT).join("tally.webidl")).unwrap();
    let mut api = hxn_c::parse("tally", &text).unwrap();
    let missing = ["tally_describe", "tally_version", "tally_free"];
    api.retain(
        |function| !missing.contains(&function),
        "not defined by libtally.so",
    );
    let tally = &api.objects[0];
    assert!(!tally.methods.iter().any(|m| m.c == "tally_describe"));
    assert!(!api.functions.iter().any(|f| f.c == "tally_version"));
    assert_eq!(tally.free, None);
    assert!(
        api.skipped
            .contains(&String::from("tally_version: not defined by libtally.so"))
    );
    assert!(
        api.skipped
            .iter()
            .any(|s| s.starts_with("tally_free: not defined by libtally.so; `Tally` is not freed"))
    );
}
