//! The Android app that `hxn build` writes for a HashLink program with a window host: an
//! APK with Ash's shared library, the program's files as assets, and a debug signature.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The file in the APK's assets that lists the other assets, one path per line after a
/// first line that identifies the build. The window host copies the listed files out of
/// the APK when that first line differs from the copy it made last.
pub const ASSET_LIST: &str = "hxn-files.txt";

/// The oldest Android version that the APK installs on, as an API level. Ash links the
/// library for it, through `ASH_ANDROID_API`.
pub const MIN_API: u32 = 26;

/// Whether an archive in `archives` defines `android_main`, the entry that a window host
/// gives Android's native activity. A program with such a host is built as an app.
pub fn has_activity(archives: &[PathBuf]) -> Result<bool, String> {
    for archive in archives {
        let bytes =
            std::fs::read(archive).map_err(|error| format!("{}: {error}", archive.display()))?;
        if defined_symbols(&bytes).any(|name| name == b"android_main") {
            return Ok(true);
        }
    }
    Ok(false)
}

// The names in the symbol table of a GNU or System V `ar` archive, which lists the symbols
// that the archive's members define.
fn defined_symbols(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let table = bytes
        .strip_prefix(b"!<arch>\n")
        .filter(|rest| rest.len() >= 60 && rest.starts_with(b"/ "))
        .and_then(|rest| {
            let size: usize = std::str::from_utf8(&rest[48..58])
                .ok()?
                .trim()
                .parse()
                .ok()?;
            let body = rest.get(60..60 + size)?;
            let count = u32::from_be_bytes(body.get(..4)?.try_into().ok()?) as usize;
            body.get(4 + 4 * count..)
        })
        .unwrap_or_default();
    table
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
}

/// The Android ABI directory of `triple`, under `lib/` in an APK.
pub fn abi(triple: &str) -> Result<&'static str, String> {
    match triple.split('-').next() {
        Some("aarch64") => Ok("arm64-v8a"),
        Some("x86_64") => Ok("x86_64"),
        Some("armv7") => Ok("armeabi-v7a"),
        Some("i686") => Ok("x86"),
        _ => Err(format!("hxn does not know the Android ABI of {triple}")),
    }
}

/// `id` as an Android package name: at least two segments of letters, digits and
/// underscores, each starting with a letter. An identifier of one segment goes under
/// `dev.hxn`.
pub fn package_name(id: &str) -> String {
    let segments: Vec<String> = id
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let word: String = segment
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            if word.starts_with(|c: char| c.is_ascii_alphabetic()) {
                word
            } else {
                format!("a{word}")
            }
        })
        .collect();
    match segments.len() {
        0 => String::from("dev.hxn.app"),
        1 => format!("dev.hxn.{}", segments[0]),
        _ => segments.join("."),
    }
}

/// The Android SDK tools that pack and sign an APK.
struct Sdk {
    aapt: PathBuf,
    zipalign: PathBuf,
    apksigner: PathBuf,
    platform: PathBuf,
    level: u32,
}

impl Sdk {
    fn find() -> Result<Sdk, String> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        let candidates = [
            std::env::var_os("ANDROID_HOME").map(PathBuf::from),
            std::env::var_os("ANDROID_SDK_ROOT").map(PathBuf::from),
            home.as_ref().map(|home| home.join("Library/Android/sdk")),
            home.as_ref().map(|home| home.join("Android/Sdk")),
            std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Android/Sdk")),
        ];
        let sdk = candidates
            .into_iter()
            .flatten()
            .find(|dir| dir.join("build-tools").is_dir() && dir.join("platforms").is_dir())
            .ok_or("hxn found no Android SDK with build-tools and a platform: set ANDROID_HOME")?;
        let exe = |name: &str| {
            if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            }
        };
        let signer = if cfg!(windows) {
            "apksigner.bat"
        } else {
            "apksigner"
        };
        let tools = newest(&sdk.join("build-tools"), |dir| {
            [exe("aapt"), exe("zipalign"), signer.to_owned()]
                .iter()
                .all(|tool| dir.join(tool).is_file())
        })
        .ok_or_else(|| {
            format!(
                "hxn found no build-tools with aapt, zipalign and apksigner in {}",
                sdk.join("build-tools").display()
            )
        })?;
        let platform = newest(&sdk.join("platforms"), |dir| {
            dir.join("android.jar").is_file()
        })
        .ok_or_else(|| {
            format!(
                "hxn found no platform in {}",
                sdk.join("platforms").display()
            )
        })?;
        let level = platform
            .file_name()
            .and_then(|name| name.to_str()?.strip_prefix("android-")?.parse().ok())
            .ok_or_else(|| format!("{}: not a platform level", platform.display()))?;
        Ok(Sdk {
            aapt: tools.join(exe("aapt")),
            zipalign: tools.join(exe("zipalign")),
            apksigner: tools.join(signer),
            platform: platform.join("android.jar"),
            level,
        })
    }
}

// The directory in `dir` with the highest version in its name that `fits` accepts.
fn newest(dir: &Path, fits: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let version = |path: &Path| -> Vec<u32> {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|part| part.parse().ok())
            .collect()
    };
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| fits(path))
        .max_by_key(|path| version(path))
}

/// The APK's manifest: one native activity that loads `libmain.so`, full screen, which
/// keeps running when the screen turns or resizes, and the permissions in `permissions`.
fn manifest(package: &str, label: &str, level: u32, permissions: &[String]) -> String {
    let uses: String = permissions
        .iter()
        .map(|name| {
            format!(
                "    <uses-permission android:name=\"{}\"/>\n",
                permission(name)
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{package}" android:versionCode="1" android:versionName="1.0">
    <uses-sdk android:minSdkVersion="{MIN_API}" android:targetSdkVersion="{level}"/>
{uses}    <application android:label="{label}" android:hasCode="false" android:extractNativeLibs="false">
        <activity android:name="android.app.NativeActivity" android:exported="true"
            android:theme="@android:style/Theme.NoTitleBar.Fullscreen"
            android:launchMode="singleTask"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode|density">
            <meta-data android:name="android.app.lib_name" android:value="main"/>
            <intent-filter>
                <action android:name="android.intent.action.MAIN"/>
                <category android:name="android.intent.category.LAUNCHER"/>
            </intent-filter>
        </activity>
    </application>
</manifest>
"#,
        label = label
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;"),
    )
}

/// The full name of the permission `name`: a name without a `.` is one of Android's,
/// such as `INTERNET`.
pub fn permission(name: &str) -> String {
    if name.contains('.') {
        name.to_owned()
    } else {
        format!("android.permission.{name}")
    }
}

/// Writes `<dir>/<name>.apk` from the shared library `library` that Ash built for
/// `triple`, with the files in `files` (paths relative to `dir`) as assets, and returns
/// its path. The APK is signed with the debug key, which is created when it is missing.
pub fn apk(
    dir: &Path,
    library: &Path,
    files: &[PathBuf],
    name: &str,
    package: &str,
    triple: &str,
    permissions: &[String],
) -> Result<PathBuf, String> {
    let sdk = Sdk::find()?;
    let stage = dir.join(format!("{name}.apk.stage"));
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|error| format!("{}: {error}", stage.display()))?;
    }
    let libs = stage.join("root/lib").join(abi(triple)?);
    let assets = stage.join("assets");
    for made in [&libs, &assets] {
        std::fs::create_dir_all(made).map_err(|error| format!("{}: {error}", made.display()))?;
    }
    std::fs::copy(library, libs.join("libmain.so"))
        .map_err(|error| format!("{}: {error}", library.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .unwrap_or_default();
    let mut list = format!("{stamp}\n");
    for file in files {
        crate::copy_tree(&dir.join(file), &assets.join(file))?;
        for path in walk(&assets.join(file))? {
            let relative = path.strip_prefix(&assets).unwrap_or(&path);
            let line: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            list.push_str(&line.join("/"));
            list.push('\n');
        }
    }
    crate::make::write(&assets.join(ASSET_LIST), &list)?;
    let manifest_file = stage.join("AndroidManifest.xml");
    crate::make::write(
        &manifest_file,
        &manifest(package, name, sdk.level, permissions),
    )?;

    // aapt packs the manifest, the assets and the files under `root`, here lib/<abi>.
    // The library stays uncompressed, so that Android maps it from the APK. aapt's default
    // pattern leaves out names that start with `.` or `_`, and hxn already chose the
    // files, so the pattern given matches no file name.
    let packed = stage.join("packed.apk");
    run(Command::new(&sdk.aapt)
        .args(["package", "-f", "--ignore-assets", "!.", "-0", "so", "-M"])
        .arg(&manifest_file)
        .arg("-I")
        .arg(&sdk.platform)
        .arg("-A")
        .arg(&assets)
        .arg("-F")
        .arg(&packed)
        .arg(stage.join("root")))?;
    // Libraries are aligned to 16 KB pages, which Android 15 devices can use.
    let aligned = stage.join("aligned.apk");
    run(Command::new(&sdk.zipalign)
        .args(["-f", "-P", "16", "4"])
        .arg(&packed)
        .arg(&aligned))?;
    let key = debug_key()?;
    let out = dir.join(format!("{name}.apk"));
    run(Command::new(&sdk.apksigner)
        .args(["sign", "--ks"])
        .arg(&key)
        .args([
            "--ks-pass",
            "pass:android",
            "--key-pass",
            "pass:android",
            "--out",
        ])
        .arg(&out)
        .arg(&aligned))?;
    std::fs::remove_dir_all(&stage).map_err(|error| format!("{}: {error}", stage.display()))?;
    Ok(out)
}

// Every file under `path`, or `path` itself when it is a file.
fn walk(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.is_dir() {
        return Ok(vec![path.to_owned()]);
    }
    let mut found = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        found.extend(walk(&entry)?);
    }
    Ok(found)
}

// The debug keystore that the Android tools share, `~/.android/debug.keystore`, created
// with keytool when it is missing.
fn debug_key() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("hxn found no home directory for the Android debug key")?;
    let dir = PathBuf::from(home).join(".android");
    let key = dir.join("debug.keystore");
    if key.is_file() {
        return Ok(key);
    }
    std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    run(Command::new("keytool")
        .args(["-genkeypair", "-keystore"])
        .arg(&key)
        .args([
            "-storepass",
            "android",
            "-alias",
            "androiddebugkey",
            "-keypass",
            "android",
            "-keyalg",
            "RSA",
            "-keysize",
            "2048",
            "-validity",
            "10000",
            "-dname",
            "CN=Android Debug,O=Android,C=US",
        ]))?;
    Ok(key)
}

fn run(command: &mut Command) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // An archive with a GNU symbol table that lists `names`.
    fn archive(names: &[&str]) -> Vec<u8> {
        let mut body = (names.len() as u32).to_be_bytes().to_vec();
        for _ in names {
            body.extend(68u32.to_be_bytes());
        }
        for name in names {
            body.extend(name.as_bytes());
            body.push(0);
        }
        let mut bytes = b"!<arch>\n".to_vec();
        bytes.extend(format!("{:<16}{:<32}{:<10}`\n", "/", "0", body.len()).as_bytes());
        bytes.extend(body);
        bytes
    }

    #[test]
    fn the_symbol_table_lists_the_defined_names() {
        let bytes = archive(&["xwindow_open", "android_main"]);
        let names: Vec<&[u8]> = defined_symbols(&bytes).collect();
        assert_eq!(names, [&b"xwindow_open"[..], b"android_main"]);
        assert_eq!(defined_symbols(b"not an archive").count(), 0);
    }

    #[test]
    fn a_package_name_has_only_java_identifier_segments() {
        assert_eq!(package_name("dev.hxn.my-game"), "dev.hxn.my_game");
        assert_eq!(package_name("dev.hxn.2d"), "dev.hxn.a2d");
        assert_eq!(package_name("my-game"), "dev.hxn.my_game");
    }

    #[test]
    fn the_manifest_asks_for_each_permission() {
        let text = manifest(
            "dev.hxn.game",
            "game",
            36,
            &["INTERNET".into(), "com.example.READ".into()],
        );
        assert!(text.contains(r#"<uses-permission android:name="android.permission.INTERNET"/>"#));
        assert!(text.contains(r#"<uses-permission android:name="com.example.READ"/>"#));
        assert!(!manifest("dev.hxn.game", "game", 36, &[]).contains("uses-permission"));
    }

    #[test]
    fn each_android_triple_has_an_abi() {
        assert_eq!(abi("aarch64-linux-android"), Ok("arm64-v8a"));
        assert_eq!(abi("x86_64-linux-android"), Ok("x86_64"));
    }
}
