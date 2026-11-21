//! A Haxe program that an `.hxml` file describes. The program uses installed
//! hxn libraries and has no crate of its own, so hxn generates and builds no
//! Rust. haxe compiles the file for the runtime, and `hxn.Bind.lib` copies
//! the native file of each library next to the program.
//!
//! The file specifies the class paths, the main class, the libraries and the
//! defines. It has no output line, because hxn adds one for each runtime.
//! haxe runs in the directory of the file, so relative paths in the file
//! resolve there. The output goes to `bin/<file name>/<target>` in that
//! directory.

use std::path::{Path, PathBuf};

use crate::make::Target;

/// A program read from its `.hxml` file.
pub struct Program {
    /// The directory of the file, where haxe runs.
    pub dir: PathBuf,
    /// The file, as an absolute path.
    pub file: PathBuf,
    /// The name of the file without `.hxml`. The output directory has the
    /// same name.
    pub name: String,
    /// The main class, with its package.
    pub main: String,
    /// The flags of the file, with each value after its flag.
    pub flags: Vec<String>,
}

// Flags that select an output or start another compilation. hxn selects the
// output for each runtime.
const OUTPUTS: &[&str] = &[
    "-js",
    "--js",
    "-lua",
    "--lua",
    "-swf",
    "--swf",
    "-neko",
    "--neko",
    "-php",
    "--php",
    "-cpp",
    "--cpp",
    "-cppia",
    "--cppia",
    "-cs",
    "--cs",
    "-java",
    "--java",
    "-jvm",
    "--jvm",
    "-python",
    "--python",
    "-hl",
    "--hl",
    "-x",
    "--interp",
    "--run",
    "--no-output",
];

impl Program {
    /// Reads the program from `path`. The file must specify a main class and
    /// must not specify an output.
    pub fn read(path: &Path) -> Result<Self, String> {
        let file =
            std::path::absolute(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let flags = parse(&text);
        let mut main = None;
        let mut words = flags.iter();
        while let Some(flag) = words.next() {
            if OUTPUTS.contains(&flag.as_str()) {
                return Err(format!(
                    "{} has a `{flag}` line: remove it, because hxn adds the output for each runtime",
                    file.display()
                ));
            }
            if ["--next", "--each"].contains(&flag.as_str()) {
                return Err(format!(
                    "{} has a `{flag}` line: give hxn one program per file",
                    file.display()
                ));
            }
            if ["-main", "-m", "--main"].contains(&flag.as_str()) {
                main = words.next().cloned();
            }
        }
        let main = main.ok_or(format!(
            "{} specifies no main class: add `-main <class>`",
            file.display()
        ))?;
        Ok(Program {
            dir: file.parent().ok_or("the file has no directory")?.to_owned(),
            name: file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            file,
            main,
            flags,
        })
    }

    /// The directory that holds the output for every target.
    pub fn output(&self) -> PathBuf {
        self.dir.join("bin").join(&self.name)
    }

    /// The output directory for `target`, or for its CPPIA script.
    pub fn output_for(&self, target: Target, script: bool) -> PathBuf {
        self.output().join(if script {
            String::from("cppia")
        } else {
            target.name()
        })
    }

    /// The name of the executable that hxcpp makes: the main class without
    /// its package.
    pub fn executable(&self) -> &str {
        self.main.rsplit('.').next().unwrap_or(&self.main)
    }
}

/// The libraries `flags` select with `-lib`, `-L` or `--library`.
pub fn libraries(flags: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    let mut words = flags.iter();
    while let Some(flag) = words.next() {
        if ["-lib", "-L", "--library"].contains(&flag.as_str())
            && let Some(library) = words.next()
        {
            found.push(library.clone());
        }
    }
    found
}

// The words of an hxml file. Each line is a flag and its value, or one
// argument. A line that starts with `#` is a comment.
fn parse(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_once(char::is_whitespace) {
            Some((flag, value)) if flag.starts_with('-') => {
                words.push(flag.to_owned());
                words.push(value.trim().trim_matches('"').to_owned());
            }
            _ => words.push(line.to_owned()),
        }
    }
    words
}

/// Whether a command-line argument names a program file.
pub fn is_file(argument: &str) -> bool {
    argument.ends_with(".hxml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(text: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "hxn hxml {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("checks.hxml");
        std::fs::write(&file, text).unwrap();
        file
    }

    #[test]
    fn a_file_gives_its_main_class_and_libraries() {
        let file = written(
            "# the checks\n-L hxn\n--library hxnme:git\n-cp src\n-D ash_simd=1\n-main app.Checks\n",
        );
        let program = Program::read(&file).unwrap();
        assert_eq!(program.main, "app.Checks");
        assert_eq!(program.executable(), "Checks");
        assert_eq!(program.name, "checks");
        assert_eq!(libraries(&program.flags), ["hxn", "hxnme:git"]);
        assert_eq!(program.dir, file.parent().unwrap());
        assert!(program.output().ends_with("bin/checks"));
        std::fs::remove_dir_all(program.dir).unwrap();
    }

    #[test]
    fn a_file_with_an_output_or_without_a_main_class_is_refused() {
        for text in [
            "-main Main\n-hl out.hl\n",
            "-main Main\n--interp\n",
            "-main Main\n--next\n-main Other\n",
            "-cp src\n",
        ] {
            let file = written(text);
            assert!(Program::read(&file).is_err(), "{text}");
            std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
        }
    }

    #[test]
    fn only_a_file_ending_in_hxml_is_a_program() {
        assert!(is_file("tests/checks.hxml"));
        assert!(!is_file("hl"));
        assert!(!is_file("cppia"));
    }
}
