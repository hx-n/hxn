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

use std::path::PathBuf;
use std::process::Command;

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
    // compiled against them and calls the host's.
    let mut arguments: Vec<(&'static str, String)> = built
        .arguments
        .iter()
        .filter(|(flag, _)| *flag == "-cp")
        .cloned()
        .collect();
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
