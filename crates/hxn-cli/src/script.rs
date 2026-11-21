//! Builds what a CPPIA script needs to use a library.
//!
//! A script cannot call C++, so it uses a library through its host: an hxcpp
//! executable built to run scripts, with the library's classes and hxn's
//! support classes compiled in. Under `hxn/<library>/cppia`:
//!
//! | Path | What it is |
//! |---|---|
//! | `host/` | the host program's single class, and hxcpp's build output for it |
//! | `host.classes` | the list of classes compiled into the host, which a script's compile excludes |
//! | `build.hxml` | the `haxe` arguments for compiling a script for the host |

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hxml::Program;
use crate::make::{self, Built, text, write};
use crate::project::Project;

// The host links hxcpp's bundled zlib, which on macOS treats `fdopen` as
// missing unless it is already defined as a macro. The host removes its
// first argument, the script's file name, from the argument list hxcpp
// exposes, so a script reads only its own arguments, as any program does.
const HOST: &str = "// Written by hxn. Do not edit.
@:buildXml('
<compiler id=\"darwin\" exe=\"clang++\" if=\"macos\">
\t<flag value=\"-Dfdopen=fdopen\"/>
</compiler>
')
@:cppFileCode('extern int _hxcpp_argc; extern char **_hxcpp_argv;')
class Host {
\tstatic function main() {
\t\tvar script = Sys.args()[0];
\t\tif (script == null) {
\t\t\tSys.println(\"usage: Host <script> [arguments]\");
\t\t\tSys.exit(2);
\t\t}
\t\tuntyped __cpp__(\"if (_hxcpp_argc > 1) { _hxcpp_argc -= 1; _hxcpp_argv += 1; }\");
\t\tcpp.cppia.Host.runFile(script);
\t}
}
";

/// A host program that runs CPPIA scripts that use a library.
pub struct Host {
    /// The directory that holds its files, and where a compiled script is
    /// written.
    pub dir: PathBuf,
    /// The executable. Its arguments are the script file, then the script's
    /// arguments.
    pub program: PathBuf,
    /// `haxe` flags and their values for compiling a script for it.
    pub arguments: Vec<(&'static str, String)>,
    /// The same arguments as an hxml file.
    pub hxml: PathBuf,
}

/// Builds the host for a library whose hxcpp binding is `built`. The host is
/// compiled with the project's extra `haxe` arguments, such as a `-lib` the
/// program uses, because a script can only call code compiled into its host.
pub fn host(project: &Project, built: &Built) -> Result<Host, String> {
    let dir = built
        .dir
        .parent()
        .ok_or("the binding has no directory")?
        .join("cppia");
    let source = dir.join("host");
    write(&source.join("Host.hx"), HOST)?;
    let held = dir.join("host.classes");
    let out = source.join("out");

    let mut haxe = Command::new("haxe");
    for (flag, value) in &built.arguments {
        haxe.arg(flag).arg(value);
    }
    // The path goes inside a Haxe string literal, where a Windows path's
    // backslashes would be read as escapes.
    let classes = text(&built.classes).replace('\\', "/");
    haxe.arg("-cp")
        .arg(&source)
        .args(["-main", "Host", "-cpp"])
        .arg(&out)
        // Dead code elimination is off: a script may use code the host
        // itself does not.
        .args(["-D", "scriptable", "-dce", "no", "-D"])
        .arg(format!("dll_export={}", text(&held)))
        .args(["--macro", "include('hxn')", "--macro"])
        .arg(format!("include('', true, null, ['{classes}'])"))
        .args(&project.flags);
    make::run(&mut haxe)?;

    // The library's and hxn's classes are compiled into the host. A script is
    // compiled against them and calls the host's, and runs the library's init macros too.
    let mut arguments: Vec<(&'static str, String)> = built
        .arguments
        .iter()
        .filter(|(flag, _)| *flag == "-cp" || *flag == "--macro")
        .cloned()
        .collect();
    arguments.extend(
        built
            .plugin_haxe
            .iter()
            .filter(|(flag, _)| *flag == "-D")
            .cloned(),
    );
    arguments.push(("-D", format!("dll_import={}", text(&held))));
    let hxml = dir.join("build.hxml");
    let lines: Vec<String> = arguments
        .iter()
        .map(|(flag, value)| format!("{flag} {value}\n"))
        .collect();
    write(&hxml, &lines.concat())?;

    Ok(Host {
        program: out.join(if cfg!(windows) { "Host.exe" } else { "Host" }),
        dir,
        arguments,
        hxml,
    })
}

/// Builds the host for a program file in `dir`. The host contains the
/// classes of the libraries that the program uses, and the script contains
/// the classes of the program's own class paths. A first compile of the
/// script lists every module it uses and the file of each, so the host gets
/// only the modules under a library's class path.
pub fn program_host(
    program: &Program,
    defines: &[(&'static str, String)],
    dir: &Path,
) -> Result<Host, String> {
    let source = dir.join("host");
    write(&source.join("Host.hx"), HOST)?;
    write(&source.join("HostModules.hx"), MODULES)?;
    let held = dir.join("host.classes");
    let out = source.join("out");

    let listed = dir.join("modules.txt");
    let mut probe = Command::new("haxe");
    probe.current_dir(&program.dir).arg(&program.file);
    for (flag, value) in defines {
        probe.arg(flag).arg(value);
    }
    probe
        .arg("-cp")
        .arg(&source)
        .arg("--macro")
        .arg(format!(
            "HostModules.list('{}')",
            text(&listed).replace('\\', "/")
        ))
        .arg("-cppia")
        .arg(dir.join("probe.cppia"))
        .arg("--no-output");
    make::run(&mut probe)?;
    let libraries = canonical(crate::plugins::class_paths(&program.flags, &program.dir)?);
    let own = canonical(own_class_paths(program));
    let mut modules = Vec::new();
    for line in std::fs::read_to_string(&listed)
        .map_err(|error| format!("{}: {error}", listed.display()))?
        .lines()
    {
        let Some((module, file)) = line.split_once('\t') else {
            continue;
        };
        let Ok(file) = std::fs::canonicalize(file) else {
            continue;
        };
        if libraries.iter().any(|root| file.starts_with(root))
            && !own.iter().any(|root| file.starts_with(root))
        {
            modules.push(module.to_owned());
        }
    }

    let mut haxe = Command::new("haxe");
    haxe.current_dir(&program.dir);
    // The libraries and defines of the file, without its class paths, main
    // class or macros, which can name the program's own classes.
    let mut words = program.flags.iter();
    while let Some(flag) = words.next() {
        if ["-lib", "-L", "--library", "-D", "--define"].contains(&flag.as_str())
            && let Some(value) = words.next()
        {
            haxe.arg(flag).arg(value);
        }
    }
    for (flag, value) in defines {
        haxe.arg(flag).arg(value);
    }
    haxe.args(&modules)
        .arg("-cp")
        .arg(&source)
        .args(["-main", "Host", "-cpp"])
        .arg(&out)
        // Dead code elimination is off: a script may use code the host
        // itself does not.
        .args(["-D", "scriptable", "-dce", "no", "-D"])
        .arg(format!("dll_export={}", text(&held)));
    make::run(&mut haxe)?;

    let arguments = vec![("-D", format!("dll_import={}", text(&held)))];
    let hxml = dir.join("build.hxml");
    write(&hxml, &format!("-D dll_import={}\n", text(&held)))?;
    Ok(Host {
        program: out.join(if cfg!(windows) { "Host.exe" } else { "Host" }),
        dir: dir.to_owned(),
        arguments,
        hxml,
    })
}

// A macro that writes each module that a compile generates, with the file
// of the module, to `path`. Each line holds a module, a tab and the file.
const MODULES: &str = "// Written by hxn. Do not edit.
class HostModules {
\tpublic static function list(path:String) {
\t\thaxe.macro.Context.onGenerate(function(types) {
\t\t\tvar lines = new Map<String, String>();
\t\t\tfor (type in types) {
\t\t\t\tvar base:haxe.macro.Type.BaseType = switch type {
\t\t\t\t\tcase TInst(t, _): t.get();
\t\t\t\t\tcase TEnum(t, _): t.get();
\t\t\t\t\tcase TAbstract(t, _): t.get();
\t\t\t\t\tcase TType(t, _): t.get();
\t\t\t\t\tcase _: null;
\t\t\t\t}
\t\t\t\tif (base == null)
\t\t\t\t\tcontinue;
\t\t\t\tvar file = haxe.macro.Context.getPosInfos(base.pos).file;
\t\t\t\tif (sys.FileSystem.exists(file))
\t\t\t\t\tlines.set(base.module, base.module + '\\t' + sys.FileSystem.fullPath(file));
\t\t\t}
\t\t\tsys.io.File.saveContent(path, [for (line in lines) line].join('\\n'));
\t\t});
\t}
}
";

// The class paths that the file specifies with `-cp`, `-p` or `--class-path`.
fn own_class_paths(program: &Program) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut words = program.flags.iter();
    while let Some(flag) = words.next() {
        if ["-cp", "-p", "--class-path"].contains(&flag.as_str())
            && let Some(path) = words.next()
        {
            found.push(program.dir.join(path));
        }
    }
    found
}

fn canonical(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .collect()
}
