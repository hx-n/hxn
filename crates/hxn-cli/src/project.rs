//! The library in the current directory, as cargo describes it.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// A library's crate, and what `hxn` needs to know to bind it.
pub struct Project {
    /// The crate's directory.
    pub dir: PathBuf,
    /// The package's name, which the adapter crates depend on it by.
    pub package: String,
    /// The library target's name, which its items are found under in Rust.
    pub krate: String,
    /// The name its binaries and symbols carry.
    pub library: String,
    /// The Rust file holding its marked items.
    pub source: PathBuf,
    /// Where its Haxe program is, and that program's main class.
    pub haxe: PathBuf,
    pub main: String,
    /// Cargo's target directory for it.
    pub target: PathBuf,
    /// Where it gets `hxn` from, so the adapters get the same.
    pub hxn: Source,
}

/// Where a crate of hxn's comes from.
pub enum Source {
    /// A checkout: the directory its crates are in.
    Path(PathBuf),
    /// A repository, and the branch, tag or revision asked for, if any.
    Git {
        url: String,
        reference: Option<(String, String)>,
    },
    /// A registry, at a version.
    Registry(String),
}

/// A string as TOML writes it.
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
    /// What a manifest says to depend on `name`, one of hxn's crates, from
    /// here. Each of them must come from one place: two copies of `hxn-abi`
    /// would not agree on a handle.
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

// `git+https://host/repo?rev=abc#0123…` as the url and what was asked for.
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

/// A name C symbols and Haxe classes can be made from.
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
                "no Cargo.toml in {}: run this in a library's directory, or make one with `hxn new`",
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
            .map_err(|error| format!("reading what cargo said: {error}"))?;
        let packages = metadata
            .get("packages")
            .and_then(Value::as_array)
            .ok_or("cargo listed no packages")?;

        let package = packages
            .iter()
            .find(|package| {
                text(package, "manifest_path")
                    .is_some_and(|path| same_file(Path::new(path), &manifest))
            })
            .ok_or("cargo does not list this directory's package")?;
        let name = text(package, "name").ok_or("a package with no name")?;
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
            .ok_or("a target with no name")?
            .replace('-', "_");

        // `[package.metadata.hxn]` overrides what is otherwise taken from
        // the crate.
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
            None => PathBuf::from(text(target, "src_path").ok_or("a target with no source")?),
        };

        let hxn = packages
            .iter()
            .find(|package| text(package, "name") == Some("hxn"))
            .ok_or_else(|| format!("`{name}` does not depend on `hxn`"))?;
        let hxn = match text(hxn, "source") {
            None => {
                let crates = text(hxn, "manifest_path")
                    .map(Path::new)
                    .and_then(|manifest| manifest.parent()?.parent())
                    .ok_or("`hxn` has no directory")?;
                Source::Path(crates.to_owned())
            }
            Some(source) if source.starts_with("git+") => git_source(source),
            Some(_) => Source::Registry(
                text(hxn, "version")
                    .ok_or("`hxn` has no version")?
                    .to_owned(),
            ),
        };

        Ok(Self {
            dir: dir.to_owned(),
            package: name.to_owned(),
            krate,
            library,
            source,
            haxe: dir.join(setting("haxe").unwrap_or("haxe")),
            main: setting("main").unwrap_or("Main").to_owned(),
            target: PathBuf::from(
                text(&metadata, "target_directory").ok_or("cargo named no target directory")?,
            ),
            hxn,
        })
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
        let source = Source::Path(PathBuf::from("/work/hxn/crates"));
        assert_eq!(
            source.dependency("hxn-abi"),
            "{ path = \"/work/hxn/crates/hxn-abi\" }"
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
    fn toml_strings_escape_what_a_path_can_hold() {
        assert_eq!(quoted(r#"C:\work\"x""#), r#""C:\\work\\\"x\"""#);
    }
}
