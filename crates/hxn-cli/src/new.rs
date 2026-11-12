//! Makes a library to start from: a crate, an example of the marked API,
//! and a Haxe program that uses it.

use std::path::{Path, PathBuf};

use crate::project::{is_library_name, quoted};

// What it starts from is the repository's first example.
const LIBRARY: &str = include_str!("../../../examples/greeter/src/lib.rs");
const PROGRAM: &str = include_str!("../../../examples/greeter/haxe/Main.hx");
const IGNORE: &str = "/target\n";
// The tag a release of this version is published under.
const RELEASE: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// The new crate's manifest. `hxn` is how it depends on hxn.
fn manifest(name: &str, hxn: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nhxn = {hxn}\n"
    )
}

/// Makes the library `name` in a directory of that name under `at`. With
/// `checkout` it depends on that copy of hxn's repository; otherwise on the
/// repository at the release tag of this program's version.
pub fn new(name: &str, checkout: Option<&Path>, at: &Path) -> Result<PathBuf, String> {
    if !is_library_name(&name.replace('-', "_")) {
        return Err(format!(
            "`{name}` cannot name a library: use lowercase letters, digits, `-` and `_`, starting with a letter"
        ));
    }
    let dir = at.join(name);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let hxn = match checkout {
        Some(checkout) => {
            // Not canonical: on Windows that is a path cargo takes and
            // other tools do not.
            let krate = std::path::absolute(checkout)
                .map_err(|error| format!("{}: {error}", checkout.display()))?
                .join("crates/hxn");
            if !krate.join("Cargo.toml").is_file() {
                return Err(format!(
                    "{} is not a copy of hxn's repository",
                    checkout.display()
                ));
            }
            format!("{{ path = {} }}", quoted(&krate.to_string_lossy()))
        }
        None => format!(
            "{{ git = {}, tag = {} }}",
            quoted(env!("CARGO_PKG_REPOSITORY")),
            quoted(RELEASE)
        ),
    };
    let files = [
        ("Cargo.toml", manifest(name, &hxn)),
        (".gitignore", String::from(IGNORE)),
        ("src/lib.rs", String::from(LIBRARY)),
        ("haxe/Main.hx", String::from(PROGRAM)),
    ];
    for (path, text) in files {
        let path = dir.join(path);
        let parent = path.parent().expect("a file in the new directory");
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The example is what a new user reads first, and nothing else here
    // compiles it.
    #[test]
    fn the_example_is_one_the_generator_takes() {
        let library = hxn_gen::parse("greeter", LIBRARY).unwrap();
        let names: Vec<_> = library.interfaces[0]
            .methods
            .iter()
            .map(|method| method.name.as_str())
            .collect();
        assert_eq!(library.interfaces[0].name, "Greeter");
        assert_eq!(names, ["new", "greet", "greeted"]);
        for name in names.iter().skip(1) {
            assert!(PROGRAM.contains(&format!("greeter.{name}(")), "{name}");
        }
    }

    // The readme shows a new library's two files as they are made.
    #[test]
    fn the_readme_shows_the_example_as_it_is() {
        let readme = include_str!("../../../README.md");
        let (_, library) = LIBRARY.split_once("\n\n").unwrap();
        assert!(readme.contains(library), "src/lib.rs");
        assert!(readme.contains(PROGRAM), "haxe/Main.hx");
    }

    #[test]
    fn a_new_library_depends_on_hxn_from_where_it_is_told() {
        let checkout = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let at = std::env::temp_dir().join(format!("hxn-new-{}", std::process::id()));
        std::fs::create_dir_all(&at).unwrap();
        let dir = new("my-lib", Some(checkout), &at).unwrap();
        let written = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(written.contains("name = \"my-lib\""));
        assert!(written.contains("crates/hxn\" }"));
        assert!(dir.join("src/lib.rs").is_file() && dir.join("haxe/Main.hx").is_file());
        assert!(
            new("my-lib", Some(checkout), &at)
                .unwrap_err()
                .contains("already exists")
        );
        assert!(
            new("My Lib", None, &at)
                .unwrap_err()
                .contains("cannot name a library")
        );
        assert!(manifest("x", "{ git = \"u\" }").contains("hxn = { git = \"u\" }"));
        std::fs::remove_dir_all(&at).unwrap();
    }
}
