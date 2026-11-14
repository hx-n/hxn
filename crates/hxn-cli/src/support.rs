//! hxn's own files that a binding needs in addition to its generated code:
//! the Haxe support classes and the C header. They are embedded in this
//! program so that a library needs only this program.

/// The Haxe classes, each with its path relative to a class path.
pub const HAXE: &[(&str, &str)] = &[
    (
        "hxn/AtomicBool.hx",
        include_str!("../../../haxe/hxn/AtomicBool.hx"),
    ),
    (
        "hxn/AtomicInt.hx",
        include_str!("../../../haxe/hxn/AtomicInt.hx"),
    ),
    ("hxn/Bind.hx", include_str!("../../../haxe/hxn/Bind.hx")),
    (
        "hxn/Callback.hx",
        include_str!("../../../haxe/hxn/Callback.hx"),
    ),
    (
        "hxn/Computed.hx",
        include_str!("../../../haxe/hxn/Computed.hx"),
    ),
    ("hxn/Future.hx", include_str!("../../../haxe/hxn/Future.hx")),
    ("hxn/Gc.hx", include_str!("../../../haxe/hxn/Gc.hx")),
    ("hxn/Handle.hx", include_str!("../../../haxe/hxn/Handle.hx")),
    ("hxn/Inbox.hx", include_str!("../../../haxe/hxn/Inbox.hx")),
    ("hxn/Loop.hx", include_str!("../../../haxe/hxn/Loop.hx")),
    ("hxn/Native.hx", include_str!("../../../haxe/hxn/Native.hx")),
    (
        "hxn/Reactive.hx",
        include_str!("../../../haxe/hxn/Reactive.hx"),
    ),
    ("hxn/Set.hx", include_str!("../../../haxe/hxn/Set.hx")),
    ("hxn/Signal.hx", include_str!("../../../haxe/hxn/Signal.hx")),
    ("hxn/Stream.hx", include_str!("../../../haxe/hxn/Stream.hx")),
    ("hxn/Tuple.hx", include_str!("../../../haxe/hxn/Tuple.hx")),
    (
        "hxn/TupleMacro.hx",
        include_str!("../../../haxe/hxn/TupleMacro.hx"),
    ),
    ("hxn/Unit.hx", include_str!("../../../haxe/hxn/Unit.hx")),
    ("hxn/Wire.hx", include_str!("../../../haxe/hxn/Wire.hx")),
    (
        "hxn/native/HashLink.hx",
        include_str!("../../../haxe/hxn/native/HashLink.hx"),
    ),
    (
        "hxn/native/Hxcpp.hx",
        include_str!("../../../haxe/hxn/native/Hxcpp.hx"),
    ),
    (
        "hxn/reactive/Node.hx",
        include_str!("../../../haxe/hxn/reactive/Node.hx"),
    ),
];

/// `hxn.h`, which a library's own headers include.
pub const HEADER: &str = include_str!("../../../include/hxn.h");

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::HAXE;

    fn classes(dir: &Path, under: &str, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if under.is_empty() {
                name
            } else {
                format!("{under}/{name}")
            };
            if entry.file_type().unwrap().is_dir() {
                classes(&entry.path(), &path, found);
            } else if path.ends_with(".hx") {
                found.push(path);
            }
        }
    }

    // A class added to haxe/ has to be added to the list as well.
    #[test]
    fn every_haxe_class_is_carried() {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../haxe"));
        let mut found = Vec::new();
        classes(root, "", &mut found);
        found.sort();
        let carried: Vec<&str> = HAXE.iter().map(|(path, _)| *path).collect();
        assert_eq!(found, carried);
    }
}
