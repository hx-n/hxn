//! `hxn`: makes, builds and runs a library written for hxn.
//!
//! A library is one crate of plain Rust. This program reads its marked
//! items, writes the adapter crate and the Haxe classes for a runtime beside
//! cargo's other output, builds the adapter, and can compile and run a Haxe
//! program against the result.

mod make;
mod new;
mod project;
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use make::{Runtime, Target};
use project::Project;

const USAGE: &str = "usage: hxn <command>

  hxn new <name> [--hxn <checkout>]
      Makes a library in a new directory: a crate, and a Haxe program that
      uses it. With --hxn it depends on that copy of hxn's repository.

  hxn build <hl|cpp> [--wasm] [--release]
      Writes the library's binding for HashLink or hxcpp and builds it.
      Prints what to pass haxe to compile against it.

  hxn run <hl|cpp> [--wasm] [--release] [--vm <command>] [-- <arguments>]
      Builds, then compiles the Haxe program in haxe/ and runs it. --vm, or
      HXN_VM, names the HashLink-compatible program to run it with, and
      any options for it: --vm \"ash --mode jit\".

With --wasm the binding and the program are built for wasm. `hl --wasm` is
built and run by Ash, which --vm or HXN_VM names if it is not `ash`.
`cpp --wasm` is built by hxcpp through Emscripten and run by node, or by the
program --vm names. Both rebuild Rust's standard library, which takes a
nightly toolchain.

build and run work on the crate in the current directory. Under
[package.metadata.hxn] in its Cargo.toml, `library`, `source`, `haxe` and
`main` replace the crate's name, src/lib.rs, haxe and Main, and `haxe-flags`
lists what else to pass haxe: [\"-lib\", \"format\"].";

/// A command's arguments: what is positional, and what each flag was given.
struct Arguments {
    positional: Vec<String>,
    release: bool,
    wasm: bool,
    vm: Option<String>,
    hxn: Option<PathBuf>,
    rest: Vec<String>,
}

fn arguments(mut given: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut out = Arguments {
        positional: Vec::new(),
        release: false,
        wasm: false,
        vm: None,
        hxn: None,
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
            "--wasm" => out.wasm = true,
            "--vm" => out.vm = Some(value("--vm")?),
            "--hxn" => out.hxn = Some(PathBuf::from(value("--hxn")?)),
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            _ => out.positional.push(argument),
        }
    }
    Ok(out)
}

fn target(arguments: &Arguments) -> Result<Target, String> {
    let runtime = match arguments.positional.as_slice() {
        [name] => {
            Runtime::parse(name).ok_or(format!("unknown runtime `{name}`: it is `hl` or `cpp`"))?
        }
        _ => return Err(String::from("name one runtime: `hl` or `cpp`")),
    };
    Ok(Target {
        runtime,
        wasm: arguments.wasm,
    })
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
    let built = make::build(
        project,
        target,
        arguments.release,
        hashlink(&arguments, target).as_deref(),
    )?;
    if !project.haxe.is_dir() {
        return Err(format!(
            "no Haxe program in {}: put one there, or name its directory as `haxe` under `[package.metadata.hxn]`",
            project.haxe.display()
        ));
    }
    let mut haxe = Command::new("haxe");
    for (flag, value) in &built.arguments {
        haxe.arg(flag).arg(value);
    }
    haxe.arg("-cp")
        .arg(&project.haxe)
        .arg("-main")
        .arg(&project.main)
        .args(&project.flags);
    let vm = |default: &str| vm_command(&arguments, default);
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
            exit(
                Command::new(ash)
                    .arg("run")
                    .arg(&module)
                    .args(&arguments.rest),
            )
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
            exit(&mut command)
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
                return exit(
                    Command::new(node)
                        .args(options)
                        .arg(out.join(format!("{name}.js")))
                        .args(&arguments.rest),
                );
            }
            let program = out.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            });
            exit(Command::new(program).args(&arguments.rest))
        }
    }
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
                arguments.release,
                hashlink(&arguments, target).as_deref(),
            )?;
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
            Ok(0)
        }
        "run" => {
            let arguments = arguments(given)?;
            let target = target(&arguments)?;
            run(&Project::read(&here)?, target, arguments)
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
    }
}
