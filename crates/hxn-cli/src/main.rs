//! `hxn`: creates, builds and runs a library written for hxn.
//!
//! A library is a single crate of plain Rust. This program parses its marked
//! items, generates the adapter crate and the Haxe classes for a runtime
//! under cargo's target directory, builds the adapter, and can compile and
//! run a Haxe program against the result.

mod doctor;
mod hxml;
mod import;
mod make;
mod new;
mod package;
mod plugins;
mod project;
mod script;
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use make::{Runtime, Target};
use project::Project;

const USAGE: &str = "usage: hxn <command>

  hxn new <name> [--hxn <checkout>]
      Creates a library in a new directory: a Rust crate and a Haxe program
      that uses it. The crate depends on hxn's git repository at the tag
      for this version, or with --hxn on a local checkout of it.

  hxn import <header.h> [--name <name>] [--source <file.c>]... [--link <library>]... [--out <directory>] [--hxn <checkout>] [-- <clang arguments>]
      Creates a Rust library crate for a C library. clang reads the header,
      and the API goes into <name>.webidl, which you review and edit:
      which function frees a returned pointer, which int is a status code.
      Rust bindings are generated from the WebIDL into src/bindings.rs on
      every build. --source compiles C files into the crate; --link links an
      installed library, found through <NAME>_LIB_DIR if needed.

  hxn build <hl|cpp|cppia> [<file.hxml>] [--wasm] [--threads] [--target <triple>] [--release] [--ash <command>]
      Generates the library's binding for HashLink or hxcpp and builds it.
      Prints the haxe arguments for compiling a program against it. With a
      program file, builds that program for distribution without running
      it, and prints how to run it: for `hl`, Ash builds it ahead of time
      into a native executable; for `cpp`, hxcpp's executable. --ash, or the
      HXN_ASH environment variable, sets the Ash command and its options:
      --ash \"ash --no-fma\".

  hxn run <hl|cpp|cppia> [<file.hxml>] [--wasm] [--threads] [--browser] [--release] [--ash <command>] [--vm <command>] [-- <arguments>]
      Builds the binding, then compiles the Haxe program in haxe/ and runs
      it. With a program file, compiles and runs that program and builds no
      binding. --vm, or the HXN_VM environment variable, sets the
      HashLink-compatible VM to run it with, including any options:
      --vm ash for Ash's hybrid mode, --vm \"ash --mode jit\".

`cppia` uses the hxcpp binding from a CPPIA script. It also builds a host,
an hxcpp executable that contains the library's classes. The Haxe program
is compiled as a script and run by that host.

  hxn check [hl] [cpp] [cppia] [<file.hxml>] [--wasm] [--threads] [--browser] [--release] [--ash <command>] [--vm <command>]
      Runs the Haxe program, or the program file, on each runtime listed.
      With none listed, it runs on whichever of HashLink and hxcpp are
      installed. Fails unless every run succeeds and all runs print the same
      output. For a library, it also fails before running if the committed
      manifest does not match the source.

  hxn doctor [--ash <command>] [--vm <command>] [--target <triple>]
      Reports which tools are installed, which targets are ready, and what
      each other target still needs. Lists installed Rust standard libraries.
      --target checks prerequisites for building that Rust static binding.

  hxn version
      Prints this program's version, the tag `hxn new` depends on.

  hxn plugins
      Lists CLI extensions provided by installed hxn-* haxelibs.
      Run an extension with hxn <namespace> [arguments...].

  hxn package [hl] [cpp] [--wasm] [--target <triple>] [--release] [--out <directory>]
      Builds the library for both runtimes, or only the ones listed, and
      copies everything a Haxe program needs to use it into one directory.
      The directory works as a haxelib (`-lib`), or as a class path
      together with --macro 'hxn.Bind.lib(\"<name>\")'. Packaging again on
      another machine, or with --wasm or --target, adds to the same
      directory.

  hxn package <hl|cpp|cppia> <file.hxml> [--target <triple>] [--out <directory>] [--ash <command>]
      Builds the program file for distribution, as build does, and copies
      it into one directory with every file it loads: an app package. For
      `hl`, Ash builds it ahead of time; for `cpp`, it is hxcpp's
      executable. --target builds it for another desktop triple: Ash builds
      for each triple it supports, and hxcpp for another machine of this
      system's kind. The libraries must include that triple. --out names
      the directory, by default bin/<file name>/<target>-package.

On build or package, --target cross-compiles the binding for a target
triple, such as `aarch64-apple-ios` or `aarch64-linux-android`, as a static
library. hxcpp links the `cpp` one into the app; Ash links the `hl` one
into the program it builds for that platform (`ash --build --target
<triple> --native-lib lib<name>.a`). It needs the target's standard
library (`rustup target add <triple>`). The library's dependencies and the
Haxe app may need platform tools and SDKs. On doctor, --target checks the
Rust binding's prerequisites without building.

With --wasm the binding and the program are built for wasm. `hl --wasm` is
built and run by Ash; set --ash or HXN_ASH if its command is not `ash`.
`cpp --wasm` is built by hxcpp with Emscripten and run by node, or by the
command given with --vm. With --threads either is threaded: the library and
the program over one shared memory, each Haxe thread and each of the
library's workers a thread of its own (wasm32-wasip1-threads for Ash,
Emscripten's pthreads for hxcpp). With --browser either is a web page in headless
Chrome: emrun serves hxcpp's (--vm then replaces the emrun command), and
`ash serve` serves Ash's, which node drives over the DevTools protocol.
HXN_BROWSER names the browser, HXN_BROWSER_ARGS adds its flags. `hl --wasm` rebuilds Rust's standard library,
which requires a nightly toolchain.

build accepts --timings, which makes cargo report how long each crate took
to build.

A program file is an .hxml file for a Haxe program that uses installed hxn
libraries and has no crate of its own. The file specifies the class paths,
the main class, the libraries and the defines, and has no output line. hxn
compiles it in the directory of the file and writes the output for each
runtime to bin/<file name>/ there. For `cppia`, the host contains the
classes of the libraries that the program uses, and the script contains
the program's own classes.

build, and run and check without a program file, operate on the crate in
the current directory. Under
[package.metadata.hxn] in its Cargo.toml, `library`, `source`, `haxe` and
`main` override the defaults (the crate's name, src/lib.rs, haxe and Main),
and `haxe-flags` lists extra arguments for haxe: [\"-lib\", \"format\"].";

/// A command's parsed arguments: the positional ones and the value of each
/// flag.
struct Arguments {
    positional: Vec<String>,
    release: bool,
    timings: bool,
    wasm: bool,
    // With `--wasm`: the program is a web page, run in a headless browser.
    browser: bool,
    // With `--wasm`: a threaded program and library over shared memory.
    threads: bool,
    // The target triple given with `--target`, for a cross-compiled binding.
    cross: Option<&'static str>,
    vm: Option<String>,
    // The Ash command, with its build options, that builds a program ahead
    // of time.
    ash: Option<String>,
    // Set by `build`: the program is built for distribution and not run. A
    // HashLink program on the desktop is then built ahead of time by Ash.
    building: bool,
    hxn: Option<PathBuf>,
    out: Option<PathBuf>,
    // For `import`: the library's name, its C sources and what it links.
    name: Option<String>,
    sources: Vec<PathBuf>,
    links: Vec<String>,
    rest: Vec<String>,
}

fn arguments(mut given: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut out = Arguments {
        positional: Vec::new(),
        release: false,
        timings: false,
        wasm: false,
        browser: false,
        threads: false,
        cross: None,
        vm: None,
        ash: None,
        building: false,
        hxn: None,
        out: None,
        name: None,
        sources: Vec::new(),
        links: Vec::new(),
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
            "--browser" => out.browser = true,
            "--threads" => out.threads = true,
            // Leaked on purpose: it is parsed once and used until the process
            // exits.
            "--target" => out.cross = Some(Box::leak(value("--target")?.into_boxed_str())),
            "--vm" => out.vm = Some(value("--vm")?),
            "--ash" => out.ash = Some(value("--ash")?),
            "--hxn" => out.hxn = Some(PathBuf::from(value("--hxn")?)),
            "--out" => out.out = Some(PathBuf::from(value("--out")?)),
            "--name" => out.name = Some(value("--name")?),
            "--source" => out.sources.push(PathBuf::from(value("--source")?)),
            "--link" => out.links.push(value("--link")?),
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            _ => out.positional.push(argument),
        }
    }
    Ok(out)
}

// Whether the program is built as a CPPIA script, which uses the hxcpp
// binding through a host program.
fn script(arguments: &Arguments) -> bool {
    arguments.positional == ["cppia"]
}

fn target(arguments: &Arguments) -> Result<Target, String> {
    let runtime = match arguments.positional.as_slice() {
        [_] if script(arguments) && arguments.wasm => {
            return Err(String::from(
                "`cppia` does not support --wasm: the script host is not built for wasm",
            ));
        }
        [_] if script(arguments) => Runtime::Cpp,
        [name] => Runtime::parse(name).ok_or(format!(
            "unknown runtime `{name}`: expected `hl`, `cpp` or `cppia`"
        ))?,
        _ => {
            return Err(String::from(
                "expected exactly one runtime: `hl`, `cpp` or `cppia`",
            ));
        }
    };
    if arguments.browser && !arguments.wasm {
        return Err(String::from(
            "--browser runs a wasm program in a web page: give `cpp --wasm` or `hl --wasm`",
        ));
    }
    if arguments.threads && !arguments.wasm {
        return Err(String::from(
            "--threads builds a threaded wasm program and library: give `hl --wasm` or `cpp --wasm`",
        ));
    }
    Ok(Target {
        runtime,
        wasm: arguments.wasm,
        threads: arguments.threads,
        cross: cross(arguments)?,
    })
}

// The `--target` triple, if given and valid with the other flags.
fn cross(arguments: &Arguments) -> Result<Option<&'static str>, String> {
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
    Ok(Some(triple))
}

// Splits a command string into the program and its options.
fn words(command: &str) -> Result<(&str, Vec<&str>), String> {
    let mut words = command.split_whitespace();
    let program = words
        .next()
        .ok_or("--vm is empty: it must name a program")?;
    Ok((program, words.collect()))
}

// The directory that contains `program`, searched for on PATH if it is a
// bare name.
fn directory_of(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return std::path::absolute(path).ok()?.parent().map(Path::to_owned);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .find(|dir| dir.join(program).is_file() || dir.join(format!("{program}.exe")).is_file())
}

// The command that runs a HashLink program: the value of `--vm` or `HXN_VM`,
// including its options, otherwise `default`.
// emrun serving `page` to a headless browser: Chrome unless HXN_BROWSER names
// another, with HXN_BROWSER_ARGS added to its flags. It prints the program's
// output and exits with its status once the program exits. `--vm` replaces
// the `emrun` command.
fn browser(arguments: &Arguments, dir: &Path, page: &Path) -> Result<Program, String> {
    let (emrun, options) = words(arguments.vm.as_deref().unwrap_or("emrun"))?;
    // emrun prints its own messages on stdout, so the page's output goes to
    // files, which it appends to.
    let stdout = dir.join("page-stdout.txt");
    let stderr = dir.join("page-stderr.txt");
    for log in [&stdout, &stderr] {
        let _ = std::fs::remove_file(log);
    }
    let name = std::env::var("HXN_BROWSER").unwrap_or_else(|_| String::from("chrome"));
    // A fresh profile each run, outside the target directory: a profile that a
    // build cache restored carries another machine's lock, and Chrome exits.
    let profile = std::env::temp_dir().join(format!("hxn-browser-profile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&profile);
    let mut flags = format!(
        "--headless=new --no-first-run --no-default-browser-check --user-data-dir={}",
        profile.display()
    );
    if let Ok(more) = std::env::var("HXN_BROWSER_ARGS") {
        flags.push(' ');
        flags.push_str(&more);
    }
    let mut command = Command::new(emrun);
    command
        .args(options)
        .arg("--browser")
        .arg(name)
        .arg(format!("--browser_args={flags}"))
        .args(["--kill_start", "--kill_exit"])
        // A web page that never exits fails the run after HXN_PAGE_TIMEOUT
        // seconds rather than holding it.
        .arg("--timeout")
        .arg(std::env::var("HXN_PAGE_TIMEOUT").unwrap_or_else(|_| String::from("300")))
        .args(["--timeout-returncode", "1"])
        .arg("--log-stdout")
        .arg(&stdout)
        .arg("--log-stderr")
        .arg(&stderr)
        .arg(page)
        .args(&arguments.rest)
        .stdout(std::process::Stdio::null());
    Ok(Program {
        command,
        page: Some((stdout, stderr)),
        profile: Some(profile),
    })
}

// node running the page Ash wrote beside the program, in headless Chrome:
// HXN_BROWSER names Chrome's executable when it is not where Chrome installs,
// and HXN_BROWSER_ARGS adds flags. The script prints what the program printed
// and exits with its status.
fn ash_page(ash: &str, dir: &Path) -> Result<Program, String> {
    let script = dir.join("ash_page.mjs");
    make::write(&script, include_str!("ash_page.mjs"))?;
    let chrome = std::env::var("HXN_BROWSER")
        .ok()
        .filter(|named| named.contains('/') || named.contains('\\'))
        .unwrap_or_else(|| {
            String::from(if cfg!(target_os = "macos") {
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            } else if cfg!(windows) {
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
            } else {
                "google-chrome"
            })
        });
    let mut command = Command::new("node");
    command.arg(&script).arg(ash).arg(dir).arg(chrome);
    if let Ok(flags) = std::env::var("HXN_BROWSER_ARGS") {
        command.args(flags.split_whitespace());
    }
    Ok(command.into())
}

/// A program to run, and for a web page the files its stdout and stderr go to.
struct Program {
    command: Command,
    page: Option<(PathBuf, PathBuf)>,
    /// The browser profile to remove once the program has run.
    profile: Option<PathBuf>,
}

impl From<Command> for Program {
    fn from(command: Command) -> Self {
        Program {
            command,
            page: None,
            profile: None,
        }
    }
}

impl Program {
    /// What a web page printed on stdout, after printing what it printed on
    /// stderr to stderr; None for a program that prints for itself.
    fn page_output(&self) -> Option<String> {
        if let Some(profile) = &self.profile {
            let _ = std::fs::remove_dir_all(profile);
        }
        let (stdout, stderr) = self.page.as_ref()?;
        if let Ok(text) = std::fs::read_to_string(stderr) {
            eprint!("{text}");
        }
        Some(std::fs::read_to_string(stdout).unwrap_or_default())
    }
}

fn vm_command(arguments: &Arguments, default: &str) -> String {
    let named = arguments.vm.clone();
    named
        .or_else(|| std::env::var("HXN_VM").ok())
        .unwrap_or_else(|| default.to_owned())
}

// The Ash command that builds a program, with its options: the value of
// `--ash` or `HXN_ASH`, otherwise `ash`.
fn ash_command(arguments: &Arguments) -> String {
    arguments
        .ash
        .clone()
        .or_else(|| std::env::var("HXN_ASH").ok())
        .unwrap_or_else(|| String::from("ash"))
}

// The HashLink VM's directory, for a native build of the HashLink binding.
// On Windows the binding is linked against the VM's library, which is in
// that directory.
fn hashlink(arguments: &Arguments, target: Target) -> Option<PathBuf> {
    if target.runtime != Runtime::Hl || target.wasm {
        return None;
    }
    let vm = vm_command(arguments, "hl");
    directory_of(words(&vm).ok()?.0)
}

// The build options given on the command line.
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
    // A process killed by a signal has no exit code, so 1 is returned.
    Ok(status
        .code()
        .map_or(1, |code| u8::try_from(code).unwrap_or(1)))
}

/// What `run` and `check` build and run: the library crate in the current
/// directory with its Haxe program, or a program file that uses installed
/// libraries.
enum Source<'a> {
    Library(&'a Project),
    File(&'a hxml::Program),
}

impl Source<'_> {
    /// The library's name, or the name of the program file.
    fn name(&self) -> &str {
        match self {
            Source::Library(project) => &project.library,
            Source::File(file) => &file.name,
        }
    }

    /// The directory that hooks run in.
    fn dir(&self) -> &Path {
        match self {
            Source::Library(project) => &project.dir,
            Source::File(file) => &file.dir,
        }
    }

    fn plugins(&self) -> Result<Vec<plugins::Plugin>, String> {
        match self {
            Source::Library(project) => plugins::for_project(project),
            Source::File(file) => plugins::for_program(file),
        }
    }

    fn context(&self, target: Option<Target>, status: Option<u8>) -> serde_json::Value {
        match self {
            Source::Library(project) => plugins::context(project, target, None, status),
            Source::File(file) => plugins::program_context(file, target, None, status),
        }
    }

    /// Builds the program for `target` and returns the command that runs it.
    fn program(
        &self,
        target: Target,
        script: bool,
        arguments: &Arguments,
    ) -> Result<Program, String> {
        match self {
            Source::Library(project) => program(project, target, script, arguments),
            Source::File(file) => file_program(file, target, script, arguments),
        }
    }
}

// Removes the program file from the positional arguments, and reads it.
fn program_file(arguments: &mut Arguments) -> Result<Option<hxml::Program>, String> {
    let files: Vec<String> = arguments
        .positional
        .iter()
        .filter(|argument| hxml::is_file(argument))
        .cloned()
        .collect();
    arguments
        .positional
        .retain(|argument| !hxml::is_file(argument));
    match files.as_slice() {
        [] => Ok(None),
        [file] => hxml::Program::read(Path::new(file)).map(Some),
        _ => Err(String::from("give one program file")),
    }
}

fn run(source: &Source, target: Target, arguments: Arguments) -> Result<u8, String> {
    if let Some(triple) = target.cross {
        return Err(format!(
            "cannot run a binding built for {triple} on this machine: use `hxn build {} --target {triple}` or `hxn package --target {triple}`",
            target.runtime.name()
        ));
    }
    let script = script(&arguments);
    let mut program = source.program(target, script, &arguments)?;
    let plugins = source.plugins()?;
    plugins::hooks(
        &plugins,
        "before-run",
        &source.context(Some(target), None),
        source.dir(),
    )?;
    let code = exit(&mut program.command)?;
    if let Some(text) = program.page_output() {
        print!("{text}");
    }
    plugins::hooks(
        &plugins,
        "after-run",
        &source.context(Some(target), Some(code)),
        source.dir(),
    )?;
    Ok(code)
}

// Builds the binding and the Haxe program for `target`, as a CPPIA script
// if `script` is set, and returns the command that runs it.
fn program(
    project: &Project,
    target: Target,
    script: bool,
    arguments: &Arguments,
) -> Result<Program, String> {
    let built = make::build(
        project,
        target,
        options(arguments, hashlink(arguments, target).as_deref()),
    )?;
    if !project.haxe.is_dir() {
        return Err(format!(
            "no Haxe program in {}: create one there, or set `haxe` under `[package.metadata.hxn]` to its directory",
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
        return Ok(command.into());
    }
    for (flag, value) in &built.arguments {
        haxe.arg(flag).arg(value);
    }
    haxe.arg("-cp")
        .arg(&project.haxe)
        .arg("-main")
        .arg(&project.main)
        .args(&project.flags);
    match target.runtime {
        Runtime::Hl => {
            let program = built.dir.join("main.hl");
            haxe.arg("-hl").arg(&program);
            make::run(&mut haxe)?;
            if target.wasm {
                return ash_wasm(
                    arguments,
                    target,
                    &program,
                    std::slice::from_ref(&built.library),
                    &built.ash_args,
                    &built.dir,
                );
            }
            let name = project.main.rsplit('.').next().unwrap_or(&project.main);
            let archives: Vec<PathBuf> = built.archive.iter().cloned().collect();
            desktop(
                arguments,
                target,
                &program,
                &archives,
                &built.ash_args,
                &Executable {
                    dir: &built.dir,
                    name,
                    bundle: &format!("dev.hxn.{}", bundle_word(&project.haxelib)),
                },
            )
        }
        Runtime::Cpp => {
            let out = built.dir.join("out");
            hxcpp_flags(&mut haxe, target, arguments);
            haxe.arg("-cpp").arg(&out);
            make::run(&mut haxe)?;
            // A threaded library may start a browser agent, as under Ash: its
            // page files go beside the page Emscripten writes.
            if target.wasm
                && target.threads
                && let Some(page) = &project.page
            {
                make::copy_page(page, &out)?;
            }
            if target.wasm {
                for page in &built.plugin_page {
                    let into = out.join(page.file_name().ok_or("plugin page file has no name")?);
                    std::fs::copy(page, &into)
                        .map_err(|error| format!("{}: {error}", into.display()))?;
                }
            }
            // hxcpp names the executable after the main class, without its
            // package.
            let name = project.main.rsplit('.').next().unwrap_or(&project.main);
            hxcpp_program(arguments, target, &out, name, &built.dir)
        }
    }
}

// Compiles a program file for `target`, as a CPPIA script if `script` is
// set, and returns the command that runs it. hxn builds no binding:
// `hxn.Bind.lib` copies the native file of each installed library next to
// the program.
fn file_program(
    file: &hxml::Program,
    target: Target,
    script: bool,
    arguments: &Arguments,
) -> Result<Program, String> {
    if target.wasm && target.runtime == Runtime::Cpp && !make::on_path("emcc") {
        return Err(String::from(
            "`emcc` was not found: hxcpp builds for wasm with the Emscripten SDK, which has to be installed and on PATH",
        ));
    }
    let out = file.output_for(target, script);
    std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    // The native files of the libraries are copied again on each compile, so
    // a library that the file no longer selects is not linked.
    for old in natives(&out)? {
        let _ = std::fs::remove_file(old);
    }
    // The program has no binding to build, so both build hooks run before
    // the compile, and their Haxe arguments apply to it.
    let plugins = plugins::for_program(file)?;
    let mut flags = make::defines(target);
    // `hxn.Bind.lib` selects each library's native file for the triple.
    if let Some(triple) = target.cross {
        flags.push(("-D", format!("hxn_triple={triple}")));
        if target.runtime == Runtime::Cpp {
            flags.push(("-D", hxcpp_machine(triple)?.to_owned()));
        }
    }
    let mut ash = Vec::new();
    let mut pages = Vec::new();
    for phase in ["before-build", "after-build"] {
        let context = plugins::program_context(file, Some(target), Some(&out), None);
        plugins::hooks(&plugins, phase, &context, &file.dir)?
            .apply_to(&mut flags, &mut ash, &out, &mut pages, &file.dir)?;
    }
    let mut haxe = Command::new("haxe");
    haxe.current_dir(&file.dir).arg(&file.file);
    for (flag, value) in &flags {
        haxe.arg(flag).arg(value);
    }
    if script {
        let host = script::program_host(file, &flags, &out)?;
        let program = out.join("main.cppia");
        for (flag, value) in &host.arguments {
            haxe.arg(flag).arg(value);
        }
        haxe.arg("-cppia").arg(&program);
        make::run(&mut haxe)?;
        let mut command = Command::new(host.program);
        command.arg(program).args(&arguments.rest);
        return Ok(command.into());
    }
    match target.runtime {
        Runtime::Hl => {
            let program = out.join("main.hl");
            haxe.arg("-hl").arg(&program);
            make::run(&mut haxe)?;
            if target.wasm {
                let archives: Vec<PathBuf> = natives(&out)?
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|extension| extension == "a"))
                    .collect();
                return ash_wasm(arguments, target, &program, &archives, &ash, &out);
            }
            let archives: Vec<PathBuf> = natives(&out)?
                .into_iter()
                .filter(|path| {
                    !path
                        .extension()
                        .is_some_and(|extension| extension == "hdll")
                })
                .collect();
            // An app for iOS gets its bundle identifier from `-D hxn_bundle_id`, which
            // a build hook or the program file can give.
            let bundle = flags
                .iter()
                .find_map(|(flag, value)| {
                    (*flag == "-D")
                        .then(|| value.strip_prefix("hxn_bundle_id="))
                        .flatten()
                })
                .map(str::to_owned)
                .unwrap_or_else(|| format!("dev.hxn.{}", bundle_word(&file.name)));
            desktop(
                arguments,
                target,
                &program,
                &archives,
                &ash,
                &Executable {
                    dir: &out,
                    name: file.executable(),
                    bundle: &bundle,
                },
            )
        }
        Runtime::Cpp => {
            hxcpp_flags(&mut haxe, target, arguments);
            haxe.arg("-cpp").arg(&out);
            make::run(&mut haxe)?;
            hxcpp_program(arguments, target, &out, file.executable(), &out)
        }
    }
}

// The native files of libraries in `dir`, which `hxn.Bind.lib` copied
// there: `.hdll` files and `lib*.a` archives, in name order.
fn natives(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let path = entry
            .map_err(|error| format!("{}: {error}", dir.display()))?
            .path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_file()
            && (name.ends_with(".hdll")
                || (name.starts_with("lib") && name.ends_with(".a"))
                || name.ends_with(".lib"))
        {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

// Builds HashLink bytecode into a wasm module with Ash and links each archive
// in `natives` into it. Returns the command that runs the module, or its web
// page with `--browser`. The page goes in `dir`.
fn ash_wasm(
    arguments: &Arguments,
    target: Target,
    program: &Path,
    natives: &[PathBuf],
    contributed: &[Vec<String>],
    dir: &Path,
) -> Result<Program, String> {
    let vm = arguments
        .ash
        .clone()
        .or_else(|| std::env::var("HXN_ASH").ok())
        .unwrap_or_else(|| vm_command(arguments, "ash"));
    let (ash, options) = words(&vm)?;
    let module = program.with_extension("wasm");
    // Ash links the archives into the program. Without threads,
    // ASH_WASM_FIBERS makes a waiting Haxe thread suspend so that the others
    // can run. With threads, each Haxe thread is a thread. The `--vm` options
    // go to the build step only.
    let triple = target.triple().unwrap_or("wasm32-wasip1");
    let mut build = Command::new(ash);
    build
        .env("ASH_WASM_FIBERS", if target.threads { "0" } else { "1" })
        .args(&options);
    ash_options(&mut build, &options, contributed);
    build.arg("--build").arg(&module).args(["--target", triple]);
    for native in natives {
        build.arg("--native-lib").arg(native);
    }
    make::run(build.arg(program))?;
    if arguments.browser {
        return ash_page(ash, dir);
    }
    let mut command = Command::new(ash);
    command.arg("run").arg(&module).args(&arguments.rest);
    Ok(command.into())
}

// The command that runs a HashLink program on the desktop. `run` and
// `check` run the bytecode in the VM. `build` has Ash build the program
// ahead of time into the executable that `output` describes, with each
// archive in `archives` linked into it, and returns the command that runs
// it. For iOS, it also writes the app bundle.
fn desktop(
    arguments: &Arguments,
    target: Target,
    program: &Path,
    archives: &[PathBuf],
    contributed: &[Vec<String>],
    output: &Executable,
) -> Result<Program, String> {
    let (dir, name, bundle) = (output.dir, output.name, output.bundle);
    if !arguments.building {
        return hashlink_vm(arguments, program, contributed, dir);
    }
    let ash = ash_command(arguments);
    let (ash, options) = words(&ash)?;
    let executable = dir.join(if target.windows() {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    let mut build = Command::new(ash);
    build.args(&options);
    ash_options(&mut build, &options, contributed);
    build.arg("--build").arg(&executable);
    if let Some(triple) = target.cross {
        build.args(["--target", triple]);
    }
    for archive in archives {
        build.arg("--native-lib").arg(archive);
    }
    // Ash loads the libraries on Windows instead of linking them.
    let linked = if target.windows() {
        Vec::new()
    } else {
        archive_links(archives)?
    };
    if !linked.is_empty() {
        let given = std::env::var("ASH_LINK_ARGS").unwrap_or_default();
        let quoted: Vec<String> = linked.iter().map(|arg| shell_quote(arg)).collect();
        build.env(
            "ASH_LINK_ARGS",
            format!("{given} {}", quoted.join(" ")).trim(),
        );
    }
    make::run(build.arg(program))?;
    if target
        .cross
        .is_some_and(|triple| triple.contains("apple-ios"))
    {
        let app = app_bundle(&executable, bundle)?;
        println!(
            "The app for iOS is {}. To run it in the booted simulator:\n  xcrun simctl install booted {}\n  xcrun simctl launch --console-pty booted {bundle}",
            app.display(),
            app.display()
        );
    }
    let mut command = Command::new(executable);
    command.args(&arguments.rest);
    Ok(command.into())
}

// Writes `<executable>.app`, the bundle that iOS installs and starts: the executable, an
// Info.plist with the identifier `id`, and the files that the program reads next to it.
// The bundle is signed without an identity, which the simulator accepts.
fn app_bundle(executable: &Path, id: &str) -> Result<PathBuf, String> {
    let dir = executable.parent().unwrap_or(Path::new("."));
    let name = executable
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let app = dir.join(format!("{name}.app"));
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|error| format!("{}: {error}", app.display()))?;
    }
    std::fs::create_dir_all(&app).map_err(|error| format!("{}: {error}", app.display()))?;
    for entry in std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let path = entry
            .map_err(|error| format!("{}: {error}", dir.display()))?
            .path();
        let file = path
            .file_name()
            .map(|file| file.to_string_lossy().into_owned())
            .unwrap_or_default();
        // The build's inputs and objects stay out of the app.
        let input = [".a", ".hl", ".xml", ".o", ".app", ".txt"]
            .iter()
            .any(|ending| file.ends_with(ending));
        if path == app || input {
            continue;
        }
        copy_tree(&path, &app.join(&file))?;
    }
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>{id}</string>
	<key>CFBundleExecutable</key>
	<string>{name}</string>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSRequiresIPhoneOS</key>
	<true/>
	<key>UILaunchScreen</key>
	<dict/>
	<key>MinimumOSVersion</key>
	<string>14.0</string>
</dict>
</plist>
"#
    );
    make::write(&app.join("Info.plist"), &plist)?;
    // The linker already signed the executable, and codesign reports that it replaces that signature.
    let signed = Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&app)
        .output()
        .map_err(|error| format!("codesign: {error}"))?;
    if !signed.status.success() {
        return Err(format!(
            "codesign failed on {}: {}",
            app.display(),
            String::from_utf8_lossy(&signed.stderr)
        ));
    }
    Ok(app)
}

// Copies the file or directory `from` to `to`.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if from.is_dir() {
        std::fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
        for entry in
            std::fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))?
        {
            let path = entry
                .map_err(|error| format!("{}: {error}", from.display()))?
                .path();
            copy_tree(&path, &to.join(path.file_name().unwrap_or_default()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|error| format!("{}: {error}", from.display()))
    }
}

// `name` with only the characters that a bundle identifier allows: letters, digits,
// hyphens and periods.
fn bundle_word(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

// Where `desktop` writes the executable that Ash builds: the directory, the file name,
// and the bundle identifier of an app for iOS.
struct Executable<'a> {
    dir: &'a Path,
    name: &'a str,
    bundle: &'a str,
}

// The libraries and flags that the archives link, from the `lib<name>.link.xml` that
// `hxn.Bind.lib` copied next to each one, in order and each once. Ash links the
// archives into the program, so the program's link needs them too.
fn archive_links(archives: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut linked: Vec<String> = Vec::new();
    for archive in archives {
        let Some(stem) = archive.file_stem() else {
            continue;
        };
        let xml = archive.with_file_name(format!("{}.link.xml", stem.to_string_lossy()));
        if !xml.is_file() {
            continue;
        }
        let text =
            std::fs::read_to_string(&xml).map_err(|error| format!("{}: {error}", xml.display()))?;
        let mut framework = false;
        for value in link_values(&text) {
            // `-framework` and its name are two flags, and the pair is kept once.
            if framework {
                framework = false;
                if !linked
                    .windows(2)
                    .any(|pair| pair[0] == "-framework" && pair[1] == value)
                {
                    linked.push("-framework".to_owned());
                    linked.push(value);
                }
            } else if value == "-framework" {
                framework = true;
            } else if !linked.contains(&value) {
                linked.push(value);
            }
        }
    }
    Ok(linked)
}

// The `name` of each `<lib>` and the `value` of each `<flag>` in hxn's link XML, in order.
fn link_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for element in text.split('<').skip(1) {
        let attribute = if element.starts_with("lib ") {
            "name=\""
        } else if element.starts_with("flag ") {
            "value=\""
        } else {
            continue;
        };
        let Some(start) = element.find(attribute) else {
            continue;
        };
        let rest = &element[start + attribute.len()..];
        let Some(end) = rest.find('"') else { continue };
        values.push(
            rest[..end]
                .replace("&lt;", "<")
                .replace("&quot;", "\"")
                .replace("&amp;", "&"),
        );
    }
    values
}

// `text` as one word for a shell, quoted only when it needs it.
fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:+,@".contains(c))
    {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

// Adds the Ash options that build hooks contributed after `given`, the
// options from the command line. A flag given there is left out, because
// Ash refuses a repeated flag and the command line's value wins.
fn ash_options(command: &mut Command, given: &[&str], contributed: &[Vec<String>]) {
    let mut seen: Vec<&str> = given
        .iter()
        .filter(|word| word.starts_with("--"))
        .map(|word| word.split('=').next().unwrap_or(word))
        .collect();
    for entry in contributed {
        if seen.contains(&entry[0].as_str()) {
            continue;
        }
        seen.push(&entry[0]);
        command.args(entry);
    }
}

// The command that runs HashLink bytecode with the VM. The VM loads a
// library by name through the system's dynamic loader, and its own shared
// library from the VM's directory. `libraries` and the VM's directory go on
// the loader's search path.
fn hashlink_vm(
    arguments: &Arguments,
    program: &Path,
    contributed: &[Vec<String>],
    libraries: &Path,
) -> Result<Program, String> {
    let vm = vm_command(arguments, "hl");
    let (vm, options) = words(&vm)?;
    let mut places = vec![libraries.to_owned()];
    places.extend(directory_of(vm));
    let mut command = Command::new(vm);
    command.args(&options);
    // The Ash options that build hooks contributed go only to Ash.
    if Path::new(vm).file_stem().is_some_and(|stem| stem == "ash") {
        ash_options(&mut command, &options, contributed);
    }
    command.arg(program).args(&arguments.rest);
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
    Ok(command.into())
}

// The flags an hxcpp compile for `target` needs beyond the defines.
fn hxcpp_flags(haxe: &mut Command, target: Target, arguments: &Arguments) {
    if target.wasm {
        // hxcpp would use a compiler set in CC or CXX instead of emcc.
        haxe.env_remove("CC").env_remove("CXX");
    }
    if arguments.browser {
        // hxcpp also writes a web page that loads the program, and links it
        // with `--emrun`, which sends its output to the emrun server.
        haxe.args([
            "-D",
            "HXCPP_LINK_EMSCRIPTEN_EXT=.html",
            "-D",
            "HXCPP_LINK_EMRUN",
        ]);
    }
}

// The command that runs the program hxcpp wrote to `out` as `name`: the
// executable, the wasm module under node, or the web page with `--browser`.
// The output of a web page goes to files in `dir`.
fn hxcpp_program(
    arguments: &Arguments,
    target: Target,
    out: &Path,
    name: &str,
    dir: &Path,
) -> Result<Program, String> {
    if arguments.browser {
        return browser(arguments, dir, &out.join(format!("{name}.html")));
    }
    if target.wasm {
        // HXN_VM is not read here: it names a HashLink VM, and this program
        // runs under node.
        let (node, options) = words(arguments.vm.as_deref().unwrap_or("node"))?;
        let mut command = Command::new(node);
        command
            .args(options)
            .arg(out.join(format!("{name}.js")))
            .args(&arguments.rest);
        return Ok(command.into());
    }
    let program = out.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    });
    let mut command = Command::new(program);
    command.args(&arguments.rest);
    Ok(command.into())
}

// Builds a program file for distribution without running it: the
// executable Ash builds ahead of time for `hl`, the module or web page for
// `hl --wasm`, and hxcpp's executable for `cpp`. Prints how to run it.
fn build_program(file: &hxml::Program, arguments: &Arguments) -> Result<u8, String> {
    let target = target(arguments)?;
    let program = file_program(file, target, script(arguments), arguments)?;
    let command = &program.command;
    let mut words = vec![command.get_program().to_string_lossy().into_owned()];
    words.extend(
        command
            .get_args()
            .map(|word| word.to_string_lossy().into_owned()),
    );
    println!(
        "Built {} for {}. To run it:\n  {}",
        file.name,
        target.name(),
        words.join(" ")
    );
    Ok(0)
}

// The hxcpp define that selects the machine of a desktop `triple`. hxcpp
// builds for another machine of this system's kind, not for another system.
fn hxcpp_machine(triple: &str) -> Result<&'static str, String> {
    let system = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    if !triple.contains(system) || triple.contains("android") {
        return Err(format!(
            "hxcpp builds a program for {triple} only on that system: build it there, or with `hl`, which Ash builds for any triple it supports"
        ));
    }
    match triple.split('-').next() {
        Some("x86_64") => Ok("HXCPP_M64"),
        Some("aarch64") => Ok("HXCPP_ARM64"),
        Some("i686") => Ok("HXCPP_M32"),
        _ => Err(format!(
            "hxn does not know hxcpp's define for the machine of {triple}"
        )),
    }
}

// Whether `path` is Ash's shared runtime, which Ash stages beside an
// executable that loads an .hdll.
fn staged_runtime(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .is_some_and(|name| {
            name.starts_with("libhl")
                && [".dylib", ".so", ".dll"]
                    .iter()
                    .any(|end| name.contains(end))
        })
}

// Builds a program file for distribution and copies it into the package
// directory: the executable, and the files it loads from its own directory.
// For Ash those are the libraries' .hdll files and Ash's runtime, when Ash
// linked the shared runtime. For `cppia`, the host and the script.
fn package_program(file: &hxml::Program, arguments: &Arguments) -> Result<u8, String> {
    let target = target(arguments)?;
    if target.wasm {
        return Err(String::from(
            "an app package is a desktop program: build a wasm program with `hxn build <runtime> --wasm <file.hxml>`",
        ));
    }
    let script = script(arguments);
    let dir = file.output_for(target, script);
    // A runtime left from an earlier build is removed, so that one found
    // after the build is one that Ash staged for it.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if staged_runtime(&entry.path()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let program = file_program(file, target, script, arguments)?;
    let mut files = vec![PathBuf::from(program.command.get_program())];
    files.extend(
        program
            .command
            .get_args()
            .map(PathBuf::from)
            .filter(|path| path.is_file()),
    );
    if target.runtime == Runtime::Hl {
        let beside: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|error| format!("{}: {error}", dir.display()))?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        if beside.iter().any(|path| staged_runtime(path)) {
            files.extend(beside.into_iter().filter(|path| {
                staged_runtime(path)
                    || path
                        .extension()
                        .is_some_and(|extension| extension == "hdll")
            }));
        }
    }
    let out = match &arguments.out {
        Some(out) => std::path::absolute(out).map_err(|error| error.to_string())?,
        None => file.output().join(format!("{}-package", target.name())),
    };
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut names = Vec::new();
    for from in &files {
        let name = from.file_name().ok_or("a packaged file has no name")?;
        let into = out.join(name);
        std::fs::copy(from, &into).map_err(|error| format!("{}: {error}", from.display()))?;
        names.push(name.to_string_lossy().into_owned());
    }
    println!(
        "Packaged {} for {} in {}:\n  {}",
        file.name,
        target.name(),
        out.display(),
        names.join("\n  ")
    );
    Ok(0)
}

// Runs the same program on each runtime. Every run must succeed, and all
// runs must print the same output.
fn check(source: &Source, arguments: &Arguments) -> Result<u8, String> {
    let plugins = source.plugins()?;
    plugins::hooks(
        &plugins,
        "before-check",
        &source.context(None, None),
        source.dir(),
    )?;
    let name = source.name();
    if let Source::Library(project) = source {
        if let Some(webidl) = &project.webidl {
            import::sync(name, webidl, &project.bindings)?;
        }
        // Checked before any build, because a build overwrites the committed
        // manifest.
        let committed = project.dir.join(format!("{name}.hxn.json"));
        if let Ok(committed) = std::fs::read_to_string(&committed) {
            let library = make::parse(project)?;
            // Compared without carriage returns: git may check the file out
            // with Windows line endings.
            if hxn_gen::manifest(&library) != committed.replace("\r\n", "\n") {
                return Err(format!(
                    "{name}.hxn.json is not what the source gives: the library's API changed. Run `hxn build` to regenerate it, then review and commit the change"
                ));
            }
        }
    }

    let found = doctor::find(&vm_command(arguments, "hl"), &ash_command(arguments));
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
            "neither HashLink nor hxcpp is ready to run a program: run `hxn doctor` to see what each needs",
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
            return Err(String::from(
                "`cppia` does not support --wasm: the script host is not built for wasm",
            ));
        }
        let target = Target {
            runtime: if script {
                Runtime::Cpp
            } else {
                Runtime::parse(runtime).ok_or(format!(
                    "unknown runtime `{runtime}`: expected `hl`, `cpp` or `cppia`"
                ))?
            },
            wasm: arguments.wasm,
            threads: arguments.threads && arguments.wasm,
            cross: None,
        };
        let mut label = runtime.clone();
        for (set, flag) in [
            (arguments.wasm, " --wasm"),
            (arguments.threads, " --threads"),
            (arguments.browser, " --browser"),
        ] {
            if set {
                label.push_str(flag);
            }
        }
        let mut program = source.program(target, script, arguments)?;
        plugins::hooks(
            &plugins,
            "before-run",
            &source.context(Some(target), None),
            source.dir(),
        )?;
        let output = program
            .command
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|error| format!("running the program on {label}: {error}"))?;
        let code = output
            .status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1);
        plugins::hooks(
            &plugins,
            "after-run",
            &source.context(Some(target), Some(code)),
            source.dir(),
        )?;
        let stdout = program
            .page_output()
            .unwrap_or_else(|| String::from_utf8_lossy(&output.stdout).into_owned());
        // Strips the carriage returns of Windows line endings.
        let text = stdout.replace('\r', "");
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
                "the program's output on {label} differs from its output on {first}, starting at line {}",
                line + 1
            ));
        }
    }
    let labels: Vec<&str> = printed.iter().map(|(label, _)| label.as_str()).collect();
    match labels.as_slice() {
        [only] => println!("PASS: {name} ran on {only}"),
        [most @ .., last] => println!(
            "PASS: {name} ran on {} and {last}, and printed the same output on each",
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
                return Err(String::from("expected one library name: `hxn new <name>`"));
            };
            let dir = new::new(name, arguments.hxn.as_deref(), &here)?;
            println!(
                "Created {}:\n  Cargo.toml\n  src/lib.rs     the library, in Rust\n  haxe/Main.hx   a Haxe program that uses it\n\nRun `hxn run hl` or `hxn run cpp` in that directory to build and run it.",
                dir.display()
            );
            Ok(0)
        }
        "import" => {
            let arguments = arguments(given)?;
            let [header] = arguments.positional.as_slice() else {
                return Err(String::from(
                    "expected one header: `hxn import <header.h> [--name <name>] [--source <file.c>] [--link <library>]`",
                ));
            };
            let header = Path::new(header);
            let name = match &arguments.name {
                Some(name) => name.clone(),
                None => header
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().replace('-', "_"))
                    .unwrap_or_default(),
            };
            let out = arguments.out.clone().unwrap_or_else(|| here.join(&name));
            let skipped = import::import(&import::Request {
                header,
                name: &name,
                sources: &arguments.sources,
                links: &arguments.links,
                out: &out,
                checkout: arguments.hxn.as_deref(),
                clang: &arguments.rest,
            })?;
            println!(
                "Created {}:
  {name}.webidl     the C API, to review and edit
  src/bindings.rs  Rust bindings generated from it
  build.rs         compiles or links the C library
  haxe/Main.hx     a Haxe program that uses it

Edit {name}.webidl to add what the header cannot express, then run `hxn run hl` or `hxn run cpp` there. Each build generates src/bindings.rs again.",
                out.display()
            );
            if !skipped.is_empty() {
                println!(
                    "
Not imported (listed at the end of {name}.webidl):"
                );
                for reason in skipped {
                    println!("  {reason}");
                }
            }
            Ok(0)
        }
        "build" => {
            let mut arguments = arguments(given)?;
            if let Some(file) = program_file(&mut arguments)? {
                arguments.building = true;
                return build_program(&file, &arguments);
            }
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
                    "Built {} and a CPPIA host for scripts that use it. To compile and run a script:\n  haxe {} -cp <your classes> -main <Main> -cppia main.cppia\n  {} main.cppia",
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
            match (target.runtime, target.wasm, target.cross) {
                (Runtime::Hl, _, Some(triple)) => println!(
                    "Ash links {} into a program for {triple}. With main.hl compiled against it:\n  ash --build main --target {triple} --native-lib {} main.hl",
                    project.library,
                    built.library.display()
                ),
                (Runtime::Hl, false, None) => println!(
                    "The VM loads {}.hdll from {}.",
                    project.library,
                    built.dir.display()
                ),
                (Runtime::Hl, true, None) => println!(
                    "Ash builds the wasm program and loads {}.wasm from the same directory. With main.hl in {}:\n  ASH_WASM_FIBERS=1 ash --build main.wasm --target wasm32-wasip1 main.hl\n  ash run main.wasm",
                    project.library,
                    built.dir.display()
                ),
                (Runtime::Cpp, _, Some(triple)) => println!(
                    "Built for {triple}. Compile the program for the same platform, with hxcpp's defines for it."
                ),
                (Runtime::Cpp, _, None) => {}
            }
            Ok(0)
        }
        "run" => {
            let mut arguments = arguments(given)?;
            let file = program_file(&mut arguments)?;
            let target = target(&arguments)?;
            match &file {
                Some(file) => run(&Source::File(file), target, arguments),
                None => run(&Source::Library(&Project::read(&here)?), target, arguments),
            }
        }
        "package" => {
            let mut arguments = arguments(given)?;
            if let Some(file) = program_file(&mut arguments)? {
                arguments.building = true;
                return package_program(&file, &arguments);
            }
            let named: &[String] = &arguments.positional;
            let both = [String::from("hl"), String::from("cpp")];
            let mut targets = Vec::new();
            for name in if named.is_empty() { &both } else { named } {
                let runtime = Runtime::parse(name)
                    .ok_or(format!("unknown runtime `{name}`: expected `hl` or `cpp`"))?;
                targets.push(Target {
                    runtime,
                    wasm: arguments.wasm,
                    threads: arguments.threads && arguments.wasm,
                    cross: cross(&arguments)?,
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
            let (name, haxelib) = (&project.library, &project.haxelib);
            println!(
                "Packaged {name} in {}, with the libraries built for {}.\nAs a haxelib:\n  haxelib dev {haxelib} {}\n  haxe -lib {haxelib} ...\nOr as a class path:\n  haxe -cp {} --macro 'hxn.Bind.lib(\"{name}\")' ...",
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
            let mut arguments = arguments(given)?;
            match program_file(&mut arguments)? {
                Some(file) => check(&Source::File(&file), &arguments),
                None => check(&Source::Library(&Project::read(&here)?), &arguments),
            }
        }
        "doctor" => {
            let arguments = arguments(given)?;
            if !arguments.positional.is_empty() || !arguments.rest.is_empty() {
                return Err(String::from(
                    "doctor takes no runtime or program arguments; use --target <triple> to check a Rust binding target",
                ));
            }
            if arguments.wasm {
                return Err(String::from(
                    "doctor reports both wasm runtimes; do not pass --wasm",
                ));
            }
            let found = doctor::find(&vm_command(&arguments, "hl"), &ash_command(&arguments));
            if let Some(triple) = arguments.cross {
                print!("{}", found.binding_report(triple));
                Ok(u8::from(!found.binding_missing(triple).is_empty()))
            } else {
                print!("{}", found.report());
                Ok(u8::from(!(found.ready("hl") || found.ready("cpp"))))
            }
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        "plugins" => {
            if given.next().is_some() {
                return Err("plugins takes no arguments".into());
            }
            for plugin in plugins::installed(&here)? {
                println!(
                    "{}\t{}\t{}",
                    plugin.namespace,
                    plugin.library,
                    plugin.root.display()
                );
            }
            Ok(0)
        }
        "version" | "--version" | "-V" => {
            println!("hxn {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        other => {
            if let Some(plugin) = plugins::installed(&here)?
                .into_iter()
                .find(|plugin| plugin.namespace == other)
            {
                plugin.command(&here, &given.collect::<Vec<_>>())
            } else {
                Err(format!("unknown command `{other}`\n\n{USAGE}"))
            }
        }
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
    fn archive_links_read_each_library_and_framework_once() {
        let dir = std::env::temp_dir().join(format!("hxn-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let xml = |libs: &str, frameworks: &[&str]| {
            let mut text = String::from("<xml>\n<target id=\"haxe\">\n");
            for lib in libs.split_whitespace() {
                text.push_str(&format!("\t<lib name=\"{lib}\"/>\n"));
            }
            for framework in frameworks {
                text.push_str(&format!(
                    "\t<flag value=\"-framework\"/>\n\t<flag value=\"{framework}\"/>\n"
                ));
            }
            text + "</target>\n</xml>\n"
        };
        std::fs::write(
            dir.join("libwindow.link.xml"),
            xml("-lobjc -liconv", &["UIKit", "Foundation"]),
        )
        .unwrap();
        std::fs::write(
            dir.join("libtext.link.xml"),
            xml("-liconv /a&amp;b/libz.a", &["Foundation", "CoreText"]),
        )
        .unwrap();
        let archives = [
            dir.join("libwindow.a"),
            dir.join("libtext.a"),
            dir.join("libnone.a"),
        ];
        let linked = super::archive_links(&archives).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            linked,
            [
                "-lobjc",
                "-liconv",
                "-framework",
                "UIKit",
                "-framework",
                "Foundation",
                "/a&b/libz.a",
                "-framework",
                "CoreText"
            ]
        );
        assert_eq!(super::shell_quote("-framework"), "-framework");
        assert_eq!(super::shell_quote("/a b/it's"), "'/a b/it'\\''s'");
    }

    #[test]
    fn a_flag_on_the_command_line_wins_over_a_hook() {
        let mut command = Command::new("ash");
        let contributed = [
            vec![String::from("--preset"), String::from("game")],
            vec![String::from("--no-fma")],
            vec![String::from("--preset"), String::from("server")],
            vec![String::from("--jit-tier"), String::from("llvm")],
        ];
        ash_options(&mut command, &["--jit-tier=cranelift"], &contributed);
        let added: Vec<_> = command.get_args().collect();
        assert_eq!(added, ["--preset", "game", "--no-fma"]);
    }

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

    #[test]
    fn either_runtime_is_cross_compiled_but_not_a_script_or_for_wasm() {
        let read = |given: &[&str]| arguments(given.iter().map(|word| word.to_string())).unwrap();
        for runtime in ["hl", "cpp"] {
            let built = target(&read(&[runtime, "--target", "aarch64-linux-android"])).unwrap();
            assert_eq!(built.cross, Some("aarch64-linux-android"));
            assert!(
                target(&read(&[
                    runtime,
                    "--wasm",
                    "--target",
                    "aarch64-linux-android"
                ]))
                .is_err()
            );
        }
        assert!(target(&read(&["cppia", "--target", "aarch64-apple-ios"])).is_err());
    }
}
