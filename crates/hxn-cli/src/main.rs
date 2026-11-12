//! `hxn`: makes, builds and runs a library written for hxn.
//!
//! A library is one crate of plain Rust. This program reads its marked
//! items, writes the adapter crate and the Haxe classes for a runtime beside
//! cargo's other output, builds the adapter, and can compile and run a Haxe
//! program against the result.

mod doctor;
mod make;
mod new;
mod package;
mod project;
mod script;
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use make::{Runtime, Target};
use project::Project;

const USAGE: &str = "usage: hxn <command>

  hxn new <name> [--hxn <checkout>]
      Makes a library in a new directory: a crate, and a Haxe program that
      uses it. With --hxn it depends on that copy of hxn's repository.

  hxn build <hl|cpp|cppia> [--wasm] [--target <triple>] [--release]
      Writes the library's binding for HashLink or hxcpp and builds it.
      Prints what to pass haxe to compile against it.

  hxn run <hl|cpp|cppia> [--wasm] [--release] [--vm <command>] [-- <arguments>]
      Builds, then compiles the Haxe program in haxe/ and runs it. --vm, or
      HXN_VM, names the HashLink-compatible program to run it with, and
      any options for it: --vm \"ash --mode jit\".

`cppia` is the hxcpp binding used from a script: it also builds a host, an
hxcpp program that holds the library's classes, and the Haxe program is
compiled as a script and run by that host.

  hxn check [hl] [cpp] [cppia] [--wasm] [--release] [--vm <command>]
      Runs the Haxe program on each runtime named, or on HashLink and hxcpp
      when both are installed and none is, and fails unless each run
      succeeds and they all print the same. Fails first if the committed
      manifest is not what the source gives.

  hxn doctor [--vm <command>]
      Reports what is installed, which targets are ready, and what each of
      the others still needs.

  hxn package [hl] [cpp] [--wasm] [--target <triple>] [--release] [--out <directory>]
      Builds the library for both runtimes, or the ones named, and gathers
      what a Haxe program needs of it into one directory: a haxelib that
      `-lib` names, or a class path beside --macro 'hxn.Bind.lib(\"<name>\")'.
      Packaging again on another machine, or with --wasm or --target, adds
      to the same directory.

--target cross-compiles the `cpp` binding for a Rust target triple, such as
`aarch64-apple-ios` or `aarch64-linux-android`. It needs that target
installed (`rustup target add <triple>`) and nothing else, because the
binding is a static library that is linked with the program. An .hdll is
linked when it is built, so it has to be built on the platform it runs on.

With --wasm the binding and the program are built for wasm. `hl --wasm` is
built and run by Ash, which --vm or HXN_VM names if it is not `ash`.
`cpp --wasm` is built by hxcpp through Emscripten and run by node, or by the
program --vm names. Both rebuild Rust's standard library, which takes a
nightly toolchain.

build takes --timings, which has cargo report the crates the build spent
its time on.

build and run work on the crate in the current directory. Under
[package.metadata.hxn] in its Cargo.toml, `library`, `source`, `haxe` and
`main` replace the crate's name, src/lib.rs, haxe and Main, and `haxe-flags`
lists what else to pass haxe: [\"-lib\", \"format\"].";

/// A command's arguments: what is positional, and what each flag was given.
struct Arguments {
    positional: Vec<String>,
    release: bool,
    timings: bool,
    wasm: bool,
    // The target triple given with `--target`, for a cross-compiled binding.
    cross: Option<&'static str>,
    vm: Option<String>,
    hxn: Option<PathBuf>,
    out: Option<PathBuf>,
    rest: Vec<String>,
}

fn arguments(mut given: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut out = Arguments {
        positional: Vec::new(),
        release: false,
        timings: false,
        wasm: false,
        cross: None,
        vm: None,
        hxn: None,
        out: None,
        rest: Vec::new(),
    };
    while let Some(argument) = given.next() {
        let mut value = |flag: &str| given.next().ok_or(format!("{flag} needs a value"));
        match argument.as_str() {
            "--" => {
                out.rest.extend(given);
                break;
            }
            "--release" => out.release = true,
            "--timings" => out.timings = true,
            "--wasm" => out.wasm = true,
            // Read once, and kept for as long as the command runs.
            "--target" => out.cross = Some(Box::leak(value("--target")?.into_boxed_str())),
            "--vm" => out.vm = Some(value("--vm")?),
            "--hxn" => out.hxn = Some(PathBuf::from(value("--hxn")?)),
            "--out" => out.out = Some(PathBuf::from(value("--out")?)),
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            _ => out.positional.push(argument),
        }
    }
    Ok(out)
}

// Whether the program is to be a CPPIA script, which is the hxcpp binding
// with a host around it.
fn script(arguments: &Arguments) -> bool {
    arguments.positional == ["cppia"]
}

fn target(arguments: &Arguments) -> Result<Target, String> {
    let runtime = match arguments.positional.as_slice() {
        [_] if script(arguments) && arguments.wasm => {
            return Err(String::from("a script's host is not built for wasm"));
        }
        [_] if script(arguments) => Runtime::Cpp,
        [name] => Runtime::parse(name).ok_or(format!(
            "unknown runtime `{name}`: it is `hl`, `cpp` or `cppia`"
        ))?,
        _ => return Err(String::from("name one runtime: `hl`, `cpp` or `cppia`")),
    };
    Ok(Target {
        runtime,
        wasm: arguments.wasm,
        cross: cross(arguments, runtime)?,
    })
}

// The `--target` triple, if given and valid for this runtime.
fn cross(arguments: &Arguments, runtime: Runtime) -> Result<Option<&'static str>, String> {
    let Some(triple) = arguments.cross else {
        return Ok(None);
    };
    if arguments.wasm {
        return Err(String::from("--wasm and --target cannot be combined"));
    }
    if script(arguments) {
        return Err(String::from(
            "--target is not supported for `cppia`: cross-compile the `cpp` binding instead",
        ));
    }
    if runtime == Runtime::Hl {
        return Err(format!(
            "--target is not supported for `hl`: an .hdll has to be linked on the platform it runs on ({triple}). Only the `cpp` binding, a static library, can be cross-compiled"
        ));
    }
    Ok(Some(triple))
}

// A program and its options, from how a command line names them.
fn words(command: &str) -> Result<(&str, Vec<&str>), String> {
    let mut words = command.split_whitespace();
    let program = words.next().ok_or("--vm names no program")?;
    Ok((program, words.collect()))
}

// The directory a program named on the command line is in.
fn directory_of(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return std::path::absolute(path).ok()?.parent().map(Path::to_owned);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .find(|dir| dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file())
}

// What runs a HashLink program: the VM named with its options, or `default`.
fn vm_command(arguments: &Arguments, default: &str) -> String {
    let named = arguments.vm.clone();
    named
        .or_else(|| std::env::var("HXN_VM").ok())
        .unwrap_or_else(|| default.to_owned())
}

// Where HashLink is, for a native build of its binding: on Windows a
// library is linked against the VM's own, which is beside it.
fn hashlink(arguments: &Arguments, target: Target) -> Option<PathBuf> {
    if target.runtime != Runtime::Hl || target.wasm {
        return None;
    }
    let vm = vm_command(arguments, "hl");
    directory_of(words(&vm).ok()?.0)
}

// How the command line says to build.
fn options<'a>(arguments: &Arguments, hashlink: Option<&'a Path>) -> make::Options<'a> {
    make::Options {
        release: arguments.release,
        timings: arguments.timings,
        hashlink,
    }
}

fn exit(command: &mut Command) -> Result<u8, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("`{program}` was not found: it has to be installed and on PATH")
        } else {
            format!("running {program}: {error}")
        }
    })?;
    // A program ended by a signal has no code of its own.
    Ok(status
        .code()
        .map_or(1, |code| u8::try_from(code).unwrap_or(1)))
}

fn run(project: &Project, target: Target, arguments: Arguments) -> Result<u8, String> {
    if let Some(triple) = target.cross {
        return Err(format!(
            "cannot run a binding built for {triple} on this machine: use `hxn build cpp --target {triple}` or `hxn package --target {triple}`"
        ));
    }
    let script = script(&arguments);
    exit(&mut program(project, target, script, &arguments)?)
}

// Builds the binding and the Haxe program for `target`, as a script if
// `script`, and gives the command that runs it.
fn program(
    project: &Project,
    target: Target,
    script: bool,
    arguments: &Arguments,
) -> Result<Command, String> {
    let built = make::build(
        project,
        target,
        options(arguments, hashlink(arguments, target).as_deref()),
    )?;
    if !project.haxe.is_dir() {
        return Err(format!(
            "no Haxe program in {}: put one there, or name its directory as `haxe` under `[package.metadata.hxn]`",
            project.haxe.display()
        ));
    }
    let mut haxe = Command::new("haxe");
    if script {
        let host = script::host(project, &built)?;
        for (flag, value) in &host.arguments {
            haxe.arg(flag).arg(value);
        }
        let program = host.dir.join("main.cppia");
        haxe.arg("-cp")
            .arg(&project.haxe)
            .arg("-main")
            .arg(&project.main)
            .args(&project.flags)
            .arg("-cppia")
            .arg(&program);
        make::run(&mut haxe)?;
        let mut command = Command::new(host.program);
        command.arg(program).args(&arguments.rest);
        return Ok(command);
    }
    for (flag, value) in &built.arguments {
        haxe.arg(flag).arg(value);
    }
    haxe.arg("-cp")
        .arg(&project.haxe)
        .arg("-main")
        .arg(&project.main)
        .args(&project.flags);
    let vm = |default: &str| vm_command(arguments, default);
    match target.runtime {
        Runtime::Hl if target.wasm => {
            let program = built.dir.join("main.hl");
            haxe.arg("-hl").arg(&program);
            make::run(&mut haxe)?;
            let vm = vm("ash");
            let (ash, options) = words(&vm)?;
            let module = built.dir.join("main.wasm");
            // Ash finds the library's module beside its output. With fibers
            // a Haxe thread that waits suspends and lets the others run.
            // Its options are for the build.
            make::run(
                Command::new(ash)
                    .env("ASH_WASM_FIBERS", "1")
                    .args(options)
                    .arg("--build")
                    .arg(&module)
                    .args(["--target", "wasm32-wasip1"])
                    .arg(&program),
            )?;
            let mut command = Command::new(ash);
            command.arg("run").arg(&module).args(&arguments.rest);
            Ok(command)
        }
        Runtime::Hl => {
            let program = built.dir.join("main.hl");
            haxe.arg("-hl").arg(&program);
            make::run(&mut haxe)?;
            let vm = vm("hl");
            let (vm, options) = words(&vm)?;
            // The VM asks the system's loader for the library by name, and
            // for its own beside itself.
            let mut places = vec![built.dir.clone()];
            places.extend(directory_of(vm));
            let mut command = Command::new(vm);
            command.args(options).arg(&program).args(&arguments.rest);
            for variable in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH", "PATH"] {
                if variable == "PATH" && !cfg!(windows) {
                    continue;
                }
                let mut all = places.clone();
                if let Some(existing) = std::env::var_os(variable) {
                    all.extend(std::env::split_paths(&existing));
                }
                let joined = std::env::join_paths(all).map_err(|error| error.to_string())?;
                command.env(variable, joined);
            }
            Ok(command)
        }
        Runtime::Cpp => {
            let out = built.dir.join("out");
            haxe.arg("-cpp").arg(&out);
            if target.wasm {
                // hxcpp would take a compiler named here in place of emcc.
                haxe.env_remove("CC").env_remove("CXX");
            }
            make::run(&mut haxe)?;
            // hxcpp names the program after the main class, less its package.
            let name = project.main.rsplit('.').next().unwrap_or(&project.main);
            if target.wasm {
                // HXN_VM names a HashLink VM, which this is not run by.
                let (node, options) = words(arguments.vm.as_deref().unwrap_or("node"))?;
                let mut command = Command::new(node);
                command
                    .args(options)
                    .arg(out.join(format!("{name}.js")))
                    .args(&arguments.rest);
                return Ok(command);
            }
            let program = out.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            });
            let mut command = Command::new(program);
            command.args(&arguments.rest);
            Ok(command)
        }
    }
}

// The same program on each runtime: every run has to succeed, and print
// what the others print.
fn check(project: &Project, arguments: &Arguments) -> Result<u8, String> {
    let name = &project.library;
    // Before any build, which would write the manifest over the one committed.
    let committed = project.dir.join(format!("{name}.hxn.json"));
    if let Ok(committed) = std::fs::read_to_string(&committed) {
        let (library, _) =
            hxn_gen::parse_crate(name, &project.source).map_err(|error| error.to_string())?;
        if hxn_gen::manifest(&library) != committed {
            return Err(format!(
                "{name}.hxn.json is not what the source gives: the library's API changed. `hxn build` rewrites it, and the change is one to review and commit"
            ));
        }
    }

    let found = doctor::find(&vm_command(arguments, "hl"));
    let named: Vec<String> = if arguments.positional.is_empty() {
        ["hl", "cpp"]
            .into_iter()
            .filter(|runtime| found.ready(runtime))
            .map(String::from)
            .collect()
    } else {
        arguments.positional.clone()
    };
    if named.is_empty() {
        return Err(String::from(
            "neither HashLink nor hxcpp is ready to run a program: `hxn doctor` reports what each needs",
        ));
    }
    if arguments.cross.is_some() {
        return Err(String::from(
            "`hxn check` runs the program, so it does not accept --target",
        ));
    }
    let mut printed: Vec<(String, String)> = Vec::new();
    for runtime in &named {
        let script = runtime == "cppia";
        if script && arguments.wasm {
            return Err(String::from("a script's host is not built for wasm"));
        }
        let target = Target {
            runtime: if script {
                Runtime::Cpp
            } else {
                Runtime::parse(runtime).ok_or(format!(
                    "unknown runtime `{runtime}`: it is `hl`, `cpp` or `cppia`"
                ))?
            },
            wasm: arguments.wasm,
            cross: None,
        };
        let label = if arguments.wasm {
            format!("{runtime} --wasm")
        } else {
            runtime.clone()
        };
        let mut command = program(project, target, script, arguments)?;
        let output = command
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|error| format!("running the program on {label}: {error}"))?;
        // Less the carriage returns a Windows program ends its lines with.
        let text = String::from_utf8_lossy(&output.stdout).replace('\r', "");
        println!("== {label}\n{}", text.trim_end());
        if !output.status.success() {
            return Err(format!("the program failed on {label}"));
        }
        printed.push((label, text));
    }
    let (first, expected) = &printed[0];
    for (label, text) in &printed[1..] {
        if text != expected {
            let line = expected
                .lines()
                .zip(text.lines())
                .position(|(one, other)| one != other)
                .unwrap_or_else(|| expected.lines().count().min(text.lines().count()));
            return Err(format!(
                "the program printed something else on {label} than on {first}, from line {}",
                line + 1
            ));
        }
    }
    let labels: Vec<&str> = printed.iter().map(|(label, _)| label.as_str()).collect();
    match labels.as_slice() {
        [only] => println!("PASS: {name} ran on {only}"),
        [most @ .., last] => println!(
            "PASS: {name} ran on {} and {last}, and printed the same on each",
            most.join(", ")
        ),
        [] => {}
    }
    Ok(0)
}

fn command() -> Result<u8, String> {
    let mut given = std::env::args().skip(1);
    let Some(command) = given.next() else {
        return Err(String::from(USAGE));
    };
    let here = std::env::current_dir().map_err(|error| error.to_string())?;
    match command.as_str() {
        "new" => {
            let arguments = arguments(given)?;
            let [name] = arguments.positional.as_slice() else {
                return Err(String::from("name the library: `hxn new <name>`"));
            };
            let dir = new::new(name, arguments.hxn.as_deref(), &here)?;
            println!(
                "Made {}:\n  Cargo.toml\n  src/lib.rs     the library, in Rust\n  haxe/Main.hx   a Haxe program that uses it\n\nIn that directory, `hxn run hl` or `hxn run cpp` builds and runs it.",
                dir.display()
            );
            Ok(0)
        }
        "build" => {
            let arguments = arguments(given)?;
            let target = target(&arguments)?;
            let project = Project::read(&here)?;
            let built = make::build(
                &project,
                target,
                options(&arguments, hashlink(&arguments, target).as_deref()),
            )?;
            if script(&arguments) {
                let host = script::host(&project, &built)?;
                println!(
                    "Built {} and a host for scripts that use it. To compile a script and run it:\n  haxe {} -cp <your classes> -main <Main> -cppia main.cppia\n  {} main.cppia",
                    project.library,
                    host.hxml.display(),
                    host.program.display()
                );
                return Ok(0);
            }
            let output = match target.runtime {
                Runtime::Hl => "-hl main.hl",
                Runtime::Cpp => "-cpp out",
            };
            println!(
                "Built {} for {}. To compile a program against it:\n  haxe {} -cp <your classes> -main <Main> {output}",
                project.library,
                target.name(),
                built.hxml.display()
            );
            match (target.runtime, target.wasm) {
                (Runtime::Hl, false) => println!(
                    "The VM loads {}.hdll from {}.",
                    project.library,
                    built.dir.display()
                ),
                (Runtime::Hl, true) => println!(
                    "Ash builds the wasm program, and loads {}.wasm from beside it. With main.hl in {}:\n  ASH_WASM_FIBERS=1 ash --build main.wasm --target wasm32-wasip1 main.hl\n  ash run main.wasm",
                    project.library,
                    built.dir.display()
                ),
                (Runtime::Cpp, _) => {}
            }
            if let Some(triple) = target.cross {
                println!(
                    "Built for {triple}. Compile the program for the same platform, with hxcpp's defines for it."
                );
            }
            Ok(0)
        }
        "run" => {
            let arguments = arguments(given)?;
            let target = target(&arguments)?;
            run(&Project::read(&here)?, target, arguments)
        }
        "package" => {
            let arguments = arguments(given)?;
            let named: &[String] = &arguments.positional;
            let both = [String::from("hl"), String::from("cpp")];
            let mut targets = Vec::new();
            // Only the hxcpp binding can be cross-compiled.
            let other = [String::from("cpp")];
            let all: &[String] = if arguments.cross.is_some() {
                &other
            } else {
                &both
            };
            for name in if named.is_empty() { all } else { named } {
                let runtime = Runtime::parse(name)
                    .ok_or(format!("unknown runtime `{name}`: it is `hl` or `cpp`"))?;
                targets.push(Target {
                    runtime,
                    wasm: arguments.wasm,
                    cross: cross(&arguments, runtime)?,
                });
            }
            let project = Project::read(&here)?;
            let out = match &arguments.out {
                Some(out) => std::path::absolute(out).map_err(|error| error.to_string())?,
                None => package::default_directory(&project),
            };
            let hashlink = targets
                .iter()
                .find_map(|&target| hashlink(&arguments, target));
            let triples = package::package(
                &project,
                &targets,
                options(&arguments, hashlink.as_deref()),
                &out,
            )?;
            let name = &project.library;
            println!(
                "Packaged {name} in {}, with what was built for {}.\nAs a haxelib:\n  haxelib dev {name} {}\n  haxe -lib {name} ...\nOr as it is:\n  haxe -cp {} --macro 'hxn.Bind.lib(\"{name}\")' ...",
                out.display(),
                triples.join(" and "),
                out.display(),
                out.display()
            );
            let missing = project.listing.missing();
            if !missing.is_empty() {
                println!(
                    "`haxelib submit` also requires these fields in haxelib.json:\n  {}",
                    missing.join("\n  ")
                );
            }
            Ok(0)
        }
        "check" => {
            let arguments = arguments(given)?;
            check(&Project::read(&here)?, &arguments)
        }
        "doctor" => {
            let arguments = arguments(given)?;
            let found = doctor::find(&vm_command(&arguments, "hl"));
            print!("{}", found.report());
            Ok(u8::from(!(found.ready("hl") || found.ready("cpp"))))
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    match command() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("hxn: {message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vm_is_named_with_its_options() {
        assert_eq!(words("hl").unwrap(), ("hl", vec![]));
        assert_eq!(
            words("/opt/ash  --mode jit").unwrap(),
            ("/opt/ash", vec!["--mode", "jit"])
        );
        assert!(words(" ").is_err());
    }

    #[test]
    fn flags_are_read_and_the_rest_is_the_programs() {
        let given = ["hl", "--wasm", "--vm", "ash -O 2", "--", "--release", "x"];
        let read = arguments(given.into_iter().map(String::from)).unwrap();
        assert_eq!(read.positional, ["hl"]);
        assert!(read.wasm && !read.release);
        assert_eq!(read.vm.as_deref(), Some("ash -O 2"));
        assert_eq!(read.rest, ["--release", "x"]);
        assert!(arguments(["--vm"].into_iter().map(String::from)).is_err());
        assert!(arguments(["--fast"].into_iter().map(String::from)).is_err());
        let timed = arguments(["cpp", "--timings"].into_iter().map(String::from)).unwrap();
        assert!(timed.timings && !timed.release);
    }

    #[test]
    fn a_script_is_the_hxcpp_binding_and_has_no_wasm_form() {
        let read = |given: &[&str]| arguments(given.iter().map(|word| word.to_string())).unwrap();
        let cppia = read(&["cppia"]);
        assert!(script(&cppia));
        let built = target(&cppia).unwrap();
        assert_eq!((built.runtime, built.wasm), (Runtime::Cpp, false));
        assert!(!script(&read(&["cpp"])));
        assert!(target(&read(&["cppia", "--wasm"])).is_err());
        assert!(target(&read(&["neko"])).is_err());
    }
}
