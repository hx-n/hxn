//! The library in the current directory, read from `cargo metadata`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// A library's crate and the settings `hxn` needs to generate its binding.
pub struct Project {
    /// The crate's directory.
    pub dir: PathBuf,
    /// The package name, which the adapter crates use to depend on it.
    pub package: String,
    /// The library target's name, which is the crate name in Rust paths to
    /// its items.
    pub krate: String,
    /// The name used in its binaries' file names and in its exported symbols.
    pub library: String,
    /// The package's version and description, copied into its haxelib.json.
    pub version: String,
    pub description: String,
    /// The other fields of its haxelib.json.
    pub listing: Listing,
    /// The Rust source file that contains its marked items.
    pub source: PathBuf,
    /// The directory of its Haxe program, and that program's main class.
    pub haxe: PathBuf,
    pub main: String,
    /// For a library made by `hxn import`: the WebIDL file its bindings are
    /// generated from, and the Rust file they are written to.
    pub webidl: Option<PathBuf>,
    pub bindings: PathBuf,
    /// Extra `haxe` arguments for compiling the program, added to hxn's own.
    pub flags: Vec<String>,
    /// Cargo's target directory for it.
    pub target: PathBuf,
    /// The source of its `hxn` dependency, which the adapters also use.
    pub hxn: Source,
    /// The version of that `hxn`.
    pub hxn_version: String,
}

/// The extra `haxelib.json` fields that `haxelib submit` requires.
#[derive(Default)]
pub struct Listing {
    /// The crate's `repository`, or its `homepage`.
    pub url: Option<String>,
    /// The crate's `license`, as cargo reports it.
    pub license: Option<String>,
    /// The crate's `keywords`.
    pub tags: Vec<String>,
    /// `contributors` under `[package.metadata.hxn]`: haxelib accounts.
    pub contributors: Vec<String>,
    /// `releasenote` under `[package.metadata.hxn]`.
    pub releasenote: Option<String>,
}

impl Listing {
    /// The license mapped to one of the names haxelib accepts. For an SPDX
    /// `OR` expression, the first license that maps is used.
    pub fn haxelib_license(&self) -> Option<&'static str> {
        let license = self.license.as_deref()?;
        license
            .split(|letter: char| !letter.is_ascii_alphanumeric() && !"-.+".contains(letter))
            .find_map(|part| {
                let part = part.to_ascii_uppercase();
                [
                    ("MIT", "MIT"),
                    ("APACHE", "Apache"),
                    ("LGPL", "LGPL"),
                    ("GPL", "GPL"),
                    ("BSD", "BSD"),
                    ("UNLICENSE", "Public"),
                    ("CC0", "Public"),
                ]
                .into_iter()
                .find(|(prefix, _)| part.starts_with(prefix))
                .map(|(_, name)| name)
            })
    }

    /// The required fields that are not set, each with where to set it.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.url.is_none() {
            missing.push("`repository` in Cargo.toml");
        }
        if self.haxelib_license().is_none() {
            missing.push("a `license` in Cargo.toml that maps to one haxelib accepts (GPL, LGPL, BSD, Public, MIT, Apache)");
        }
        if self.contributors.is_empty() {
            missing.push("`contributors` (haxelib account names) under `[package.metadata.hxn]`");
        }
        if self.releasenote.is_none() {
            missing.push("`releasenote` under `[package.metadata.hxn]`");
        }
        missing
    }
}

// A list of strings from cargo's metadata for a package, or an empty list
// if it is absent.
fn texts(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The source of hxn's crates.
pub enum Source {
    /// A local checkout: the directory that contains its crates.
    Path(PathBuf),
    /// A git repository, and the requested branch, tag or revision, if any.
    Git {
        url: String,
        reference: Option<(String, String)>,
    },
    /// A registry, at this version.
    Registry(String),
}

/// `text` as a quoted TOML string.
pub fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for letter in text.chars() {
        match letter {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

impl Source {
    /// The manifest value for a dependency on `name`, one of hxn's crates,
    /// from this source. All of hxn's crates must come from the same source:
    /// two copies of `hxn-abi` would each define their own handle type.
    pub fn dependency(&self, name: &str) -> String {
        match self {
            Self::Path(crates) => format!(
                "{{ path = {} }}",
                quoted(&crates.join(name).to_string_lossy())
            ),
            Self::Git {
                url,
                reference: None,
            } => format!("{{ git = {} }}", quoted(url)),
            Self::Git {
                url,
                reference: Some((kind, value)),
            } => format!("{{ git = {}, {kind} = {} }}", quoted(url), quoted(value)),
            Self::Registry(version) => quoted(&format!("={version}")),
        }
    }
}

// Parses `git+https://host/repo?rev=abc#0123…` into the URL and the
// requested reference.
fn git_source(source: &str) -> Source {
    let rest = source.trim_start_matches("git+");
    let rest = rest.split('#').next().unwrap_or(rest);
    let (url, query) = match rest.split_once('?') {
        Some((url, query)) => (url, Some(query)),
        None => (rest, None),
    };
    let reference = query.and_then(|query| {
        query.split('&').find_map(|pair| {
            let (kind, value) = pair.split_once('=')?;
            ["branch", "tag", "rev"]
                .contains(&kind)
                .then(|| (kind.to_owned(), value.to_owned()))
        })
    });
    Source::Git {
        url: url.to_owned(),
        reference,
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// The `haxe-flags` setting: a list of arguments, each a string.
fn flags(settings: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = settings.and_then(|settings| settings.get("haxe-flags")) else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .and_then(|flags| {
            flags
                .iter()
                .map(|flag| flag.as_str().map(str::to_owned))
                .collect()
        })
        .ok_or_else(|| {
            String::from(
                "`haxe-flags` under `[package.metadata.hxn]` must be a list of strings, such as [\"-lib\", \"format\"]",
            )
        })
}

/// Whether `name` can be used in both C symbol names and Haxe class names.
pub fn is_library_name(name: &str) -> bool {
    let mut letters = name.chars();
    letters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && letters.all(|l| l.is_ascii_lowercase() || l.is_ascii_digit() || l == '_')
}

impl Project {
    /// Reads the crate whose `Cargo.toml` is in `dir`.
    pub fn read(dir: &Path) -> Result<Self, String> {
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            return Err(format!(
                "no Cargo.toml in {}: run this in a library's directory, or create one with `hxn new`",
                dir.display()
            ));
        }
        let output = Command::new("cargo")
            .args(["metadata", "--format-version", "1"])
            .current_dir(dir)
            .output()
            .map_err(|error| format!("running cargo: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        let metadata: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("reading cargo's output: {error}"))?;
        let packages = metadata
            .get("packages")
            .and_then(Value::as_array)
            .ok_or("`cargo metadata` listed no packages")?;

        let package = packages
            .iter()
            .find(|package| {
                text(package, "manifest_path")
                    .is_some_and(|path| same_file(Path::new(path), &manifest))
            })
            .ok_or("`cargo metadata` does not list the package in this directory")?;
        let name = text(package, "name").ok_or("the package has no name")?;
        let target = package
            .get("targets")
            .and_then(Value::as_array)
            .and_then(|targets| {
                targets.iter().find(|target| {
                    target
                        .get("kind")
                        .and_then(Value::as_array)
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind == "lib"))
                })
            })
            .ok_or_else(|| format!("`{name}` has no library target"))?;
        let krate = text(target, "name")
            .ok_or("the library target has no name")?
            .replace('-', "_");

        // `[package.metadata.hxn]` overrides the defaults taken from the
        // crate.
        let settings = package.get("metadata").and_then(|m| m.get("hxn"));
        let setting = |key: &str| settings.and_then(|settings| text(settings, key));
        let library = setting("library").unwrap_or(&krate).to_owned();
        if !is_library_name(&library) {
            return Err(format!(
                "`{library}` cannot name a library: use lowercase letters, digits and `_`, or set `library` under `[package.metadata.hxn]`"
            ));
        }
        let source = match setting("source") {
            Some(source) => dir.join(source),
            None => PathBuf::from(
                text(target, "src_path").ok_or("the library target has no source path")?,
            ),
        };

        let hxn = packages
            .iter()
            .find(|package| text(package, "name") == Some("hxn"))
            .ok_or_else(|| format!("`{name}` does not depend on `hxn`"))?;
        let hxn_version = text(hxn, "version").unwrap_or_default().to_owned();
        let hxn = match text(hxn, "source") {
            None => {
                let crates = text(hxn, "manifest_path")
                    .map(Path::new)
                    .and_then(|manifest| manifest.parent()?.parent())
                    .ok_or("cannot find the directory of the `hxn` crate")?;
                Source::Path(crates.to_owned())
            }
            Some(source) if source.starts_with("git+") => git_source(source),
            Some(_) => Source::Registry(
                text(hxn, "version")
                    .ok_or("the `hxn` crate has no version")?
                    .to_owned(),
            ),
        };

        Ok(Self {
            dir: dir.to_owned(),
            package: name.to_owned(),
            krate,
            library,
            version: text(package, "version").unwrap_or("0.0.0").to_owned(),
            description: text(package, "description").unwrap_or_default().to_owned(),
            listing: Listing {
                url: text(package, "repository")
                    .or_else(|| text(package, "homepage"))
                    .map(str::to_owned),
                license: text(package, "license").map(str::to_owned),
                tags: texts(package.get("keywords")),
                contributors: texts(settings.and_then(|settings| settings.get("contributors"))),
                releasenote: setting("releasenote").map(str::to_owned),
            },
            source,
            haxe: dir.join(setting("haxe").unwrap_or("haxe")),
            main: setting("main").unwrap_or("Main").to_owned(),
            webidl: setting("webidl").map(|file| dir.join(file)),
            bindings: dir.join(setting("bindings").unwrap_or("src/bindings.rs")),
            flags: flags(settings)?,
            target: PathBuf::from(
                text(&metadata, "target_directory")
                    .ok_or("`cargo metadata` reported no target directory")?,
            ),
            hxn,
            hxn_version,
        })
    }
}

#[cfg(test)]
mod listing {
    use super::Listing;

    #[test]
    fn a_license_is_listed_as_haxelib_names_it() {
        let named = |license: &str| {
            Listing {
                license: Some(license.to_owned()),
                ..Listing::default()
            }
            .haxelib_license()
        };
        assert_eq!(named("MIT"), Some("MIT"));
        assert_eq!(named("Apache-2.0"), Some("Apache"));
        assert_eq!(named("MIT OR Apache-2.0"), Some("MIT"));
        assert_eq!(named("LGPL-3.0-only"), Some("LGPL"));
        assert_eq!(named("GPL-2.0-or-later"), Some("GPL"));
        assert_eq!(named("BSD-3-Clause"), Some("BSD"));
        assert_eq!(named("(Zlib OR Unlicense)"), Some("Public"));
        assert_eq!(named("Zlib"), None);
        assert_eq!(Listing::default().haxelib_license(), None);
    }

    #[test]
    fn what_haxelib_asks_for_is_named_where_it_is_set() {
        assert_eq!(Listing::default().missing().len(), 4);
        let whole = Listing {
            url: Some(String::from("https://example.org")),
            license: Some(String::from("MIT")),
            tags: Vec::new(),
            contributors: vec![String::from("someone")],
            releasenote: Some(String::from("First release.")),
        };
        assert!(whole.missing().is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_git_source_keeps_what_was_asked_for_and_drops_what_was_resolved() {
        let plain = git_source("git+https://example.org/hxn#0123abcd");
        assert_eq!(
            plain.dependency("hxn-abi"),
            "{ git = \"https://example.org/hxn\" }"
        );
        let pinned = git_source("git+https://example.org/hxn?rev=abc#abc0123");
        assert_eq!(
            pinned.dependency("hxn-hl"),
            "{ git = \"https://example.org/hxn\", rev = \"abc\" }"
        );
        let branch = git_source("git+https://example.org/hxn?branch=next#0123");
        assert_eq!(
            branch.dependency("hxn-abi"),
            "{ git = \"https://example.org/hxn\", branch = \"next\" }"
        );
    }

    #[test]
    fn a_checkout_gives_each_crate_its_directory() {
        let crates = PathBuf::from("/work/hxn/crates");
        // Joined with the platform's path separator and quoted for TOML.
        let expected = quoted(&crates.join("hxn-abi").to_string_lossy());
        assert!(expected.contains("hxn-abi") && expected.starts_with("\"/work/hxn/crates"));
        assert_eq!(
            Source::Path(crates).dependency("hxn-abi"),
            format!("{{ path = {expected} }}")
        );
        assert_eq!(
            Source::Registry(String::from("0.3.1")).dependency("hxn-hl"),
            "\"=0.3.1\""
        );
    }

    #[test]
    fn a_library_name_is_one_c_and_haxe_can_both_use() {
        assert!(is_library_name("greeter") && is_library_name("audio_2"));
        for bad in ["", "Greeter", "2d", "my-lib", "a b"] {
            assert!(!is_library_name(bad), "{bad}");
        }
    }

    #[test]
    fn haxe_flags_are_a_list_of_strings() {
        let settings: Value =
            serde_json::from_str(r#"{ "haxe-flags": ["-lib", "format", "-D", "fast"] }"#).unwrap();
        assert_eq!(
            flags(Some(&settings)).unwrap(),
            ["-lib", "format", "-D", "fast"]
        );
        assert!(flags(None).unwrap().is_empty());
        assert!(flags(Some(&Value::Null)).unwrap().is_empty());
        for bad in [
            r#"{ "haxe-flags": "-lib format" }"#,
            r#"{ "haxe-flags": ["-D", 1] }"#,
        ] {
            let settings: Value = serde_json::from_str(bad).unwrap();
            assert!(
                flags(Some(&settings))
                    .unwrap_err()
                    .contains("list of strings")
            );
        }
    }

    #[test]
    fn toml_strings_escape_what_a_path_can_hold() {
        assert_eq!(quoted(r#"C:\work\"x""#), r#""C:\\work\\\"x\"""#);
    }
}
