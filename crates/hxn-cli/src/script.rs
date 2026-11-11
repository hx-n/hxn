//! A library for a CPPIA script.
//!
//! A script cannot call C++, so it reaches a library through its host: an
//! hxcpp program built to run scripts, which holds the library's classes and
//! hxn's own. Under `hxn/<library>/cppia`:
//!
//! | Path | What it is |
//! |---|---|
//! | `haxe/` | declarations of the library's classes, a class path for a script |
//! | `host/` | the host program's one class, and what hxcpp builds from it |
//! | `host.classes` | what the host holds, which a script's compile leaves out |
//! | `build.hxml` | what to pass `haxe` to compile a script for the host |

use std::path::PathBuf;
use std::process::Command;

use crate::make::{self, Built, text, write};
use crate::project::Project;

// hxcpp's own zlib, which a host links, takes `fdopen` for missing on a Mac
// unless it is already a macro. The host drops its own first argument, the
// script's name, from what hxcpp hands out as the program's, so a script
// reads its arguments as any program does.
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

/// A program that runs scripts against a library.
pub struct Host {
    /// Where its files are, and where a compiled script goes.
    pub dir: PathBuf,
    /// The program, which takes a script's file and then its arguments.
    pub program: PathBuf,
    /// Flags for `haxe` with their values, to compile a script for it.
    pub arguments: Vec<(&'static str, String)>,
    /// The same, as a file `haxe` takes.
    pub hxml: PathBuf,
}

/// Builds the host for a library `built` for hxcpp. It is compiled with
/// what the program's own compile takes, a `-lib` it uses: a script can only
/// call what its host holds.
pub fn host(project: &Project, built: &Built) -> Result<Host, String> {
    let dir = built
        .declarations
        .parent()
        .ok_or("the declarations have no directory")?
        .to_owned();
    let source = dir.join("host");
    write(&source.join("Host.hx"), HOST)?;
    let held = dir.join("host.classes");
    let out = source.join("out");

    let mut haxe = Command::new("haxe");
    for (flag, value) in &built.arguments {
        haxe.arg(flag).arg(value);
    }
    // In a string of Haxe's, where a Windows path's separators would escape.
    let classes = text(&built.classes).replace('\\', "/");
    haxe.arg("-cp")
        .arg(&source)
        .args(["-main", "Host", "-cpp"])
        .arg(&out)
        // Nothing is dropped for being unused here: a script may use it.
        .args(["-D", "scriptable", "-dce", "no", "-D"])
        .arg(format!("dll_export={}", text(&held)))
        .args(["--macro", "include('hxn')", "--macro"])
        .arg(format!("include('', true, null, ['{classes}'])"))
        .args(&project.flags);
    make::run(&mut haxe)?;

    // The support classes are the host's; a script is only typed against them.
    let mut arguments: Vec<(&'static str, String)> = built
        .arguments
        .iter()
        .filter(|(flag, value)| *flag == "-cp" && *value != text(&built.classes))
        .cloned()
        .collect();
    arguments.push(("-cp", text(&built.declarations)));
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
