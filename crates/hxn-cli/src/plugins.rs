//! Installed haxelibs extend the CLI through argv-based, versioned processes.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::hxml::Program;
use crate::make::{self, Built, Target};
use crate::project::Project;

const VERSION: u64 = 1;
const HOOKS: &[&str] = &[
    "before-check",
    "before-build",
    "after-build",
    "before-run",
    "after-run",
    "before-package",
    "after-package",
];
const RESERVED: &[&str] = &[
    "new", "import", "build", "run", "check", "package", "doctor", "plugins", "help", "version",
];

#[derive(Clone, Debug)]
pub struct Plugin {
    pub library: String,
    pub namespace: String,
    pub root: PathBuf,
    command: Vec<String>,
    hooks: BTreeSet<String>,
    files: Vec<PathBuf>,
}

// The namespace a haxelib of the hxn family gets by default: its name after
// `hxn-` or `hxn`, such as `window` for hxn-window and `me` for hxnme. None
// for a haxelib outside the family, and for hxn itself.
fn family(library: &str) -> Option<&str> {
    let rest = library.strip_prefix("hxn")?;
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    (!rest.is_empty()).then_some(rest)
}

impl Plugin {
    pub fn parse(library: &str, root: &Path, value: &Value) -> Result<Self, String> {
        let error = |why: &str| format!("{library}: invalid hxn CLI plugin: {why}");
        let Some(default) = family(library) else {
            return Err(error(
                "the haxelib name must start with hxn, as hxn-window and hxnme do",
            ));
        };
        let object = value
            .as_object()
            .ok_or_else(|| error("expected an object"))?;
        for key in object.keys() {
            if !["version", "namespace", "command", "hooks", "files"].contains(&key.as_str()) {
                return Err(error(&format!("unknown field `{key}`")));
            }
        }
        if value["version"].as_u64() != Some(VERSION) {
            return Err(error("version must be 1"));
        }
        let namespace = match value.get("namespace") {
            Some(value) => value
                .as_str()
                .ok_or_else(|| error("namespace must be a string"))?,
            None => default,
        };
        if namespace.is_empty()
            || !namespace.as_bytes()[0].is_ascii_lowercase()
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || RESERVED.contains(&namespace)
        {
            return Err(error(
                "namespace must be a lowercase command name and cannot replace a built-in command",
            ));
        }
        let strings = |key: &str| -> Result<Vec<String>, String> {
            value[key]
                .as_array()
                .and_then(|values| {
                    values
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .ok_or_else(|| error(&format!("{key} must be an array of strings")))
        };
        let command = strings("command")?;
        if command.is_empty() || command[0].is_empty() || command.iter().any(|v| v.contains('\0')) {
            return Err(error(
                "command must contain an executable and valid argv strings",
            ));
        }
        let hooks: BTreeSet<String> = if value.get("hooks").is_some() {
            strings("hooks")?.into_iter().collect()
        } else {
            BTreeSet::new()
        };
        if hooks.iter().any(|hook| !HOOKS.contains(&hook.as_str())) {
            return Err(error("unrecognized lifecycle hook"));
        }
        let mut files = Vec::new();
        if value.get("files").is_some() {
            for name in strings("files")? {
                let path = PathBuf::from(&name);
                if name.is_empty()
                    || name.contains('\\')
                    || !path
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_)))
                {
                    return Err(error(
                        "files must contain relative package paths without traversal",
                    ));
                }
                files.push(path);
            }
        }
        Ok(Self {
            library: library.into(),
            namespace: namespace.into(),
            root: root.into(),
            command,
            hooks,
            files,
        })
    }

    fn process(&self, cwd: &Path) -> Command {
        let root = self.root.to_string_lossy();
        let argv: Vec<String> = self
            .command
            .iter()
            .map(|arg| arg.replace("{root}", &root))
            .collect();
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .current_dir(cwd)
            .stdin(Stdio::piped());
        command
    }

    fn start(
        &self,
        request: &Value,
        cwd: &Path,
        hook: bool,
    ) -> Result<std::process::Child, String> {
        let mut command = self.process(cwd);
        command
            .stdout(if hook {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .stderr(Stdio::inherit());
        let mut child = command
            .spawn()
            .map_err(|error| format!("{}: starting CLI plugin: {error}", self.library))?;
        let sent = child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{request}\n").as_bytes());
        if let Err(error) = sent {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{}: sending plugin request: {error}", self.library));
        }
        Ok(child)
    }

    pub fn command(&self, cwd: &Path, args: &[String]) -> Result<u8, String> {
        let request = json!({"version": VERSION, "kind": "command", "library": self.library,
            "root": self.root, "cwd": cwd, "arguments": args});
        let status = self
            .start(&request, cwd, false)?
            .wait()
            .map_err(|error| format!("{}: waiting for plugin: {error}", self.library))?;
        Ok(status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1))
    }

    fn hook(&self, phase: &str, context: &Value, cwd: &Path) -> Result<Reply, String> {
        if !self.hooks.contains(phase) {
            return Ok(Reply::default());
        }
        let request = json!({"version": VERSION, "kind": "hook", "phase": phase,
            "library": self.library, "root": self.root, "context": context});
        let output = self
            .start(&request, cwd, true)?
            .wait_with_output()
            .map_err(|error| format!("{}: {phase}: {error}", self.library))?;
        if !output.status.success() {
            return Err(format!(
                "{}: {phase} hook failed ({})",
                self.library, output.status
            ));
        }
        let mut reply = Reply::parse(&output.stdout, phase)
            .map_err(|why| format!("{}: {phase}: {why}", self.library))?;
        for (_, origin) in &mut reply.page {
            *origin = format!("{}: {phase}", self.library);
        }
        Ok(reply)
    }
}

/// Copies declared tool assets as bytes, preserving executable permissions.
pub fn package_files(project: &Project, out: &Path) -> Result<(), String> {
    let Some(config) = &project.cli else {
        return Ok(());
    };
    let plugin = Plugin::parse(&project.haxelib, &project.dir, config)?;
    copy_files(plugin, out)
}

fn copy_files(plugin: Plugin, out: &Path) -> Result<(), String> {
    for relative in plugin.files {
        let from = plugin.root.join(&relative);
        let files = if from.is_dir() {
            make::files_under(&from)?
                .into_iter()
                .map(|file| relative.join(file))
                .collect()
        } else {
            vec![relative]
        };
        for relative in files {
            let from = plugin.root.join(&relative);
            let into = out.join(&relative);
            if std::fs::canonicalize(&from).ok() == std::fs::canonicalize(&into).ok()
                && from.exists()
            {
                continue;
            }
            std::fs::create_dir_all(into.parent().unwrap()).map_err(|error| error.to_string())?;
            std::fs::copy(&from, &into).map_err(|error| {
                format!(
                    "{}: packaging CLI asset {}: {error}",
                    plugin.library,
                    from.display()
                )
            })?;
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct Reply {
    haxe: Vec<(&'static str, String)>,
    page: Vec<(PathBuf, String)>,
    ash: Vec<Vec<String>>,
}
impl Reply {
    fn parse(bytes: &[u8], phase: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|e| format!("expected a JSON hook reply: {e}"))?;
        let object = value.as_object().ok_or("hook reply must be an object")?;
        if value["version"].as_u64() != Some(VERSION) {
            return Err("hook reply version must be 1".into());
        }
        for key in object.keys() {
            if !["version", "haxe_args", "page_files", "ash_args"].contains(&key.as_str()) {
                return Err(format!("unknown hook reply field `{key}`"));
            }
        }
        let mut reply = Self::default();
        if let Some(args) = value.get("haxe_args") {
            for pair in args
                .as_array()
                .ok_or("haxe_args must be an array of [flag, value] pairs")?
            {
                let pair = pair
                    .as_array()
                    .filter(|p| p.len() == 2)
                    .ok_or("haxe_args must contain [flag, value] pairs")?;
                let flag = match pair[0].as_str() {
                    Some("-D") => "-D",
                    Some("-cp") => "-cp",
                    Some("--macro") => "--macro",
                    _ => return Err("hook Haxe flags are -D, -cp and --macro".into()),
                };
                reply.haxe.push((
                    flag,
                    pair[1]
                        .as_str()
                        .ok_or("Haxe argument values must be strings")?
                        .into(),
                ));
            }
        }
        if let Some(files) = value.get("page_files") {
            for file in files.as_array().ok_or("page_files must be an array")? {
                reply.page.push((
                    PathBuf::from(file.as_str().ok_or("page_files must contain paths")?),
                    "plugin".into(),
                ));
            }
        }
        if let Some(args) = value.get("ash_args") {
            for entry in args
                .as_array()
                .ok_or("ash_args must be an array of [flag] or [flag, value] entries")?
            {
                let words = entry
                    .as_array()
                    .filter(|words| matches!(words.len(), 1 | 2))
                    .and_then(|words| {
                        words
                            .iter()
                            .map(|word| word.as_str().map(String::from))
                            .collect::<Option<Vec<String>>>()
                    })
                    .filter(|words| words[0].starts_with("--"))
                    .ok_or(
                        "ash_args must contain [flag] or [flag, value] entries of strings, each flag starting with --",
                    )?;
                reply.ash.push(words);
            }
        }
        if !["before-build", "after-build"].contains(&phase)
            && (!reply.haxe.is_empty() || !reply.page.is_empty() || !reply.ash.is_empty())
        {
            return Err(
                "only build hooks may contribute Haxe arguments, Ash arguments or page files"
                    .into(),
            );
        }
        Ok(reply)
    }

    pub fn apply(self, built: &mut Built, cwd: &Path) -> Result<(), String> {
        built.plugin_haxe.extend(self.haxe.iter().cloned());
        let Self { haxe, page, ash } = self;
        built.arguments.extend(haxe);
        built.ash_args.extend(ash);
        copy_pages(page, &built.dir, &mut built.plugin_page, cwd)?;
        let lines: Vec<String> = built
            .arguments
            .iter()
            .map(|(flag, value)| format!("{flag} {value}\n"))
            .collect();
        make::write(&built.hxml, &lines.concat())
    }

    /// Adds the Haxe arguments of the reply to `arguments` and its Ash
    /// arguments to `ash`. Copies its page files into `into` and adds each
    /// copy to `copied`.
    pub fn apply_to(
        self,
        arguments: &mut Vec<(&'static str, String)>,
        ash: &mut Vec<Vec<String>>,
        into: &Path,
        copied: &mut Vec<PathBuf>,
        cwd: &Path,
    ) -> Result<(), String> {
        arguments.extend(self.haxe);
        ash.extend(self.ash);
        copy_pages(self.page, into, copied, cwd)
    }
}

// Copies page files into `dir`. Two files with the same name and different
// contents are an error.
fn copy_pages(
    page: Vec<(PathBuf, String)>,
    dir: &Path,
    copied: &mut Vec<PathBuf>,
    cwd: &Path,
) -> Result<(), String> {
    for (path, origin) in page {
        let from = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        let name = from
            .file_name()
            .ok_or_else(|| format!("{origin}: page file has no name"))?;
        let into = dir.join(name);
        let bytes = std::fs::read(&from)
            .map_err(|e| format!("{origin}: page file {}: {e}", from.display()))?;
        if copied.contains(&into)
            && into.is_file()
            && std::fs::read(&into).ok().as_deref() != Some(bytes.as_slice())
        {
            return Err(format!(
                "{origin}: page file {} conflicts with {}",
                from.display(),
                into.display()
            ));
        }
        std::fs::write(&into, bytes).map_err(|e| format!("{}: {e}", into.display()))?;
        if !copied.contains(&into) {
            copied.push(into);
        }
    }
    Ok(())
}

fn from_root(root: &Path) -> Result<Option<Plugin>, String> {
    let path = root.join("haxelib.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(config) = value.get("hxn").and_then(|v| v.get("cli")) else {
        return Ok(None);
    };
    let name = value["name"]
        .as_str()
        .ok_or("plugin haxelib.json has no name")?;
    Plugin::parse(name, root, config).map(Some)
}

fn haxelib(args: &[String], cwd: &Path) -> Result<String, String> {
    let output = Command::new("haxelib")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("running haxelib for CLI plugins: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "haxelib {}: {}",
            args.join(" "),
            String::from_utf8_lossy(if output.stderr.is_empty() {
                &output.stdout
            } else {
                &output.stderr
            })
            .trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("haxelib output: {e}"))
}

fn unique(mut plugins: Vec<Plugin>) -> Result<Vec<Plugin>, String> {
    plugins.sort_by(|a, b| a.library.cmp(&b.library));
    let mut namespaces = BTreeMap::new();
    for plugin in &plugins {
        if let Some(other) = namespaces.insert(&plugin.namespace, &plugin.library) {
            return Err(format!(
                "CLI namespace `{}` is registered by {other} and {}",
                plugin.namespace, plugin.library
            ));
        }
    }
    Ok(plugins)
}

pub fn installed(cwd: &Path) -> Result<Vec<Plugin>, String> {
    let list = haxelib(&["list".into()], cwd)?;
    let mut plugins = Vec::new();
    for line in list.lines() {
        let Some((name, _)) = line.split_once(':') else {
            continue;
        };
        if family(name).is_none() {
            continue;
        }
        let root = haxelib(&["libpath".into(), name.into()], cwd)?;
        if let Some(plugin) = from_root(Path::new(root.trim()))? {
            plugins.push(plugin);
        }
    }
    unique(plugins)
}

pub fn for_project(project: &Project) -> Result<Vec<Plugin>, String> {
    let mut plugins = selected(&project.flags, &project.dir)?;
    if let Some(config) = &project.cli {
        let plugin = Plugin::parse(&project.haxelib, &project.dir, config)?;
        plugins.insert(plugin.library.clone(), plugin);
    }
    unique(plugins.into_values().collect())
}

/// The plugins of the libraries a program file selects.
pub fn for_program(program: &Program) -> Result<Vec<Plugin>, String> {
    unique(
        selected(&program.flags, &program.dir)?
            .into_values()
            .collect(),
    )
}

// The plugins of the libraries `flags` select with `-lib`, `-L` or
// `--library`, and of the libraries they depend on.
fn selected(flags: &[String], cwd: &Path) -> Result<BTreeMap<String, Plugin>, String> {
    let mut plugins = BTreeMap::new();
    for path in class_paths(flags, cwd)? {
        let mut path = path.as_path();
        while path.is_dir() {
            if path.join("haxelib.json").is_file() {
                if let Some(plugin) = from_root(path)? {
                    plugins.insert(plugin.library.clone(), plugin);
                }
                break;
            }
            let Some(parent) = path.parent() else {
                break;
            };
            path = parent;
        }
    }
    Ok(plugins)
}

/// The class paths of the libraries `flags` select, and of the libraries
/// they depend on, as haxelib resolves them in `cwd`.
pub fn class_paths(flags: &[String], cwd: &Path) -> Result<Vec<PathBuf>, String> {
    let libraries = crate::hxml::libraries(flags);
    if libraries.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec![String::from("path")];
    args.extend(libraries);
    Ok(haxelib(&args, cwd)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('-'))
        .map(PathBuf::from)
        .collect())
}

pub fn context(
    project: &Project,
    target: Option<Target>,
    output: Option<&Path>,
    status: Option<u8>,
) -> Value {
    with(
        json!({"directory": project.dir, "library": project.library, "haxelib": project.haxelib, "target_directory": project.target}),
        target,
        output,
        status,
    )
}

/// The hook context for a program file. The program has no library of its
/// own, so `library` and `haxelib` are null. `hxml` is the path of the file.
pub fn program_context(
    program: &Program,
    target: Option<Target>,
    output: Option<&Path>,
    status: Option<u8>,
) -> Value {
    with(
        json!({"directory": program.dir, "library": null, "haxelib": null, "target_directory": program.output(), "hxml": program.file}),
        target,
        output,
        status,
    )
}

fn with(
    project: Value,
    target: Option<Target>,
    output: Option<&Path>,
    status: Option<u8>,
) -> Value {
    json!({"command": std::env::args().nth(1), "arguments": std::env::args().skip(2).collect::<Vec<_>>(),
        "project": project,
        "target": target.map(|t| json!({"runtime": t.runtime.name(), "wasm": t.wasm, "threads": t.threads, "triple": t.triple()})),
        "output": output, "status": status})
}

pub fn hooks(
    plugins: &[Plugin],
    phase: &str,
    context: &Value,
    cwd: &Path,
) -> Result<Reply, String> {
    let mut reply = Reply::default();
    for plugin in plugins {
        let next = plugin.hook(phase, context, cwd)?;
        reply.haxe.extend(next.haxe);
        reply.page.extend(next.page);
        reply.ash.extend(next.ash);
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn root() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hxn plugin {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn config() -> Value {
        json!({"version": 1, "command": ["haxe", "-cp", "{root}/sources", "--run", "Tool"], "hooks": ["before-build"]})
    }

    #[test]
    fn namespaces_and_protocol_versions_are_checked_without_running_code() {
        let root = root();
        assert_eq!(
            Plugin::parse("hxn-window", &root, &config())
                .unwrap()
                .namespace,
            "window"
        );
        for namespace in ["run", "help", "plugins", "has spaces", "../window", ""] {
            let mut config = config();
            config["namespace"] = json!(namespace);
            assert!(Plugin::parse("hxn-window", &root, &config).is_err());
        }
        let mut bad = config();
        bad["version"] = json!(2);
        assert!(Plugin::parse("hxn-window", &root, &bad).is_err());
        let mut bad = config();
        bad["hooks"] = json!(["after-deploy"]);
        assert!(Plugin::parse("hxn-window", &root, &bad).is_err());
        let mut bad = config();
        bad["command"] = json!([]);
        assert!(Plugin::parse("hxn-window", &root, &bad).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extra_tool_files_preserve_binary_bytes_and_permissions() {
        let root = root();
        let out = root.join("package");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let source = root.join("bin/tool");
        std::fs::write(&source, [0, 255, 128, 1]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut config = config();
        config["files"] = json!(["bin"]);
        copy_files(Plugin::parse("hxn-assets", &root, &config).unwrap(), &out).unwrap();
        assert_eq!(
            std::fs::read(out.join("bin/tool")).unwrap(),
            [0, 255, 128, 1]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(out.join("bin/tool"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
        for path in ["../secret", "/absolute", "bin/../../secret"] {
            config["files"] = json!([path]);
            assert!(Plugin::parse("hxn-assets", &root, &config).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installed_manifests_use_the_packaged_metadata_and_namespace_conflicts_fail() {
        let root = root();
        std::fs::write(
            root.join("haxelib.json"),
            json!({"name":"hxn-window", "hxn":{"cli":config()}}).to_string(),
        )
        .unwrap();
        let one = from_root(&root).unwrap().unwrap();
        let mut two = one.clone();
        two.library = "hxn-other".into();
        let error = unique(vec![one, two]).unwrap_err();
        assert!(
            error.contains("hxn-window") && error.contains("hxn-other") && error.contains("window")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hooks_cannot_change_the_compilation_target_and_replies_are_versioned() {
        assert!(
            Reply::parse(
                br#"{"version":1,"haxe_args":[["-main","Injected"]]}"#,
                "after-build"
            )
            .is_err()
        );
        assert!(
            Reply::parse(
                br#"{"version":1,"haxe_args":[["-D","ready"]]}"#,
                "after-run"
            )
            .is_err()
        );
        assert!(Reply::parse(br#"{"version":2}"#, "after-build").is_err());
        assert!(
            Reply::parse(
                br#"log line
{"version":1}"#,
                "after-build"
            )
            .is_err()
        );
        assert!(
            Reply::parse(
                br#"{"version":1,"haxe_args":[["-D","ready"],["-cp","path with spaces"]]}"#,
                "after-build"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_plugin_comes_from_the_hxn_family_and_is_named_after_it() {
        assert_eq!(family("hxn-window"), Some("window"));
        assert_eq!(family("hxnme"), Some("me"));
        assert_eq!(family("hxn"), None);
        assert_eq!(family("hxn-"), None);
        assert_eq!(family("format"), None);
        let root = root();
        assert_eq!(
            Plugin::parse("hxnme", &root, &config()).unwrap().namespace,
            "me"
        );
        assert!(Plugin::parse("format", &root, &config()).is_err());
    }

    #[test]
    fn build_hooks_give_ash_flags_with_or_without_a_value() {
        let reply = Reply::parse(
            br#"{"version":1,"ash_args":[["--preset","game"],["--no-fma"]]}"#,
            "before-build",
        )
        .unwrap();
        assert_eq!(reply.ash, [vec!["--preset", "game"], vec!["--no-fma"]]);
        for bad in [
            r#"{"version":1,"ash_args":[["preset","game"]]}"#,
            r#"{"version":1,"ash_args":[["--preset","game","x"]]}"#,
            r#"{"version":1,"ash_args":[[]]}"#,
            r#"{"version":1,"ash_args":[["--preset",1]]}"#,
            r#"{"version":1,"ash_args":"--preset game"}"#,
        ] {
            assert!(
                Reply::parse(bad.as_bytes(), "before-build").is_err(),
                "{bad}"
            );
        }
        assert!(Reply::parse(br#"{"version":1,"ash_args":[["--no-fma"]]}"#, "before-run").is_err());
    }

    #[test]
    fn argv_and_json_cross_the_process_boundary_without_a_shell_and_failures_keep_the_provider() {
        let root = root();
        let source = root.join("fixture.rs");
        let executable = root.join(if cfg!(windows) {
            "plugin.exe"
        } else {
            "plugin"
        });
        std::fs::write(
            &source,
            r#"
use std::io::{self, Read};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut request = String::new(); io::stdin().read_to_string(&mut request).unwrap();
    std::fs::write(&args[1], request).unwrap();
    if args[2] == "fail" { std::process::exit(7); }
    println!("{}", args[2]);
}"#,
        )
        .unwrap();
        assert!(
            Command::new("rustc")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        let recorded = root.join("request with spaces.json");
        let value = json!({"version":1,"command":[executable,recorded,"{\"version\":1}"],"hooks":["before-build"]});
        let mut plugin = Plugin::parse("hxn-fixture", &root, &value).unwrap();
        plugin
            .hook("before-build", &json!({"marker":"kept"}), &root)
            .unwrap();
        let request: Value =
            serde_json::from_str(&std::fs::read_to_string(&recorded).unwrap()).unwrap();
        assert_eq!(request["version"], 1);
        assert_eq!(request["phase"], "before-build");
        assert_eq!(request["context"]["marker"], "kept");
        plugin.command[2] = "fail".into();
        assert_eq!(
            plugin
                .command(&root, &["a b".into(), "$(literal)".into()])
                .unwrap(),
            7
        );
        let request: Value =
            serde_json::from_str(&std::fs::read_to_string(&recorded).unwrap()).unwrap();
        assert_eq!(request["arguments"], json!(["a b", "$(literal)"]));
        let error = plugin
            .hook("before-build", &json!({}), &root)
            .err()
            .unwrap();
        assert!(error.contains("hxn-fixture") && error.contains("before-build"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contributed_assets_and_arguments_reach_the_build_and_conflicting_files_fail() {
        let root = root();
        let out = root.join("output");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(root.join("agent.mjs"), "export {};").unwrap();
        let mut built = Built {
            plugin_haxe: Vec::new(),
            plugin_page: Vec::new(),
            dir: out.clone(),
            arguments: vec![],
            hxml: out.join("build.hxml"),
            classes: out.clone(),
            include: out.clone(),
            library: out.join("lib.a"),
            archive: None,
            ash_args: Vec::new(),
            linked: make::Linked::default(),
        };
        Reply::parse(
            br#"{"version":1,"haxe_args":[["-D","ready"]],"page_files":["agent.mjs"]}"#,
            "after-build",
        )
        .unwrap()
        .apply(&mut built, &root)
        .unwrap();
        assert_eq!(std::fs::read_to_string(&built.hxml).unwrap(), "-D ready\n");
        assert_eq!(
            std::fs::read_to_string(out.join("agent.mjs")).unwrap(),
            "export {};"
        );
        assert_eq!(built.plugin_page, [out.join("agent.mjs")]);
        std::fs::write(root.join("agent.mjs"), "different").unwrap();
        assert!(
            Reply::parse(
                br#"{"version":1,"page_files":["agent.mjs"]}"#,
                "after-build"
            )
            .unwrap()
            .apply(&mut built, &root)
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
