//! `-o` names the output FILE on every native build route (#3349).
//!
//! With `--cdylib`, `-o` used to become the Cargo crate name, so an output
//! path failed (`invalid character '/' in crate name`) and even a bare name
//! landed in the working directory as `lib<name>.<ext>`. A binary build
//! always treated `-o` as the file. This matrix pins the one meaning across
//! routes × output spellings: the artifact is at exactly the path given
//! (parent directories created), and with no `-o` the default is unchanged —
//! `<entry>` for a binary, `lib<entry>.<ext>` in the working directory for a
//! library.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tools_available() -> bool {
    let almide = Command::new(almide_bin()).arg("--version").output().is_ok();
    let cargo = Command::new("cargo").arg("--version").output().is_ok();
    almide && cargo
}

const LIB_EXT: &str = if cfg!(target_os = "windows") {
    "dll"
} else if cfg!(target_os = "macos") {
    "dylib"
} else {
    "so"
};

fn host_lib_file(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.{LIB_EXT}")
    } else {
        format!("lib{stem}.{LIB_EXT}")
    }
}

fn host_exe_file(stem: &str) -> String {
    if cfg!(target_os = "windows") { format!("{stem}.exe") } else { stem.to_string() }
}

#[derive(Clone, Copy)]
struct Route {
    name: &'static str,
    flags: &'static [&'static str],
    library: bool,
}

const ROUTES: &[Route] = &[
    Route { name: "bin", flags: &[], library: false },
    Route { name: "repr-c", flags: &["--repr-c"], library: false },
    Route { name: "cdylib", flags: &["--cdylib"], library: true },
    Route { name: "cdylib+repr-c", flags: &["--cdylib", "--repr-c"], library: true },
];

/// The output spellings for a case run in `dir`: (label, `-o` value or `None`
/// for no `-o`, where the artifact must land).
fn outputs(route: Route, dir: &Path) -> Vec<(&'static str, Option<String>, PathBuf)> {
    let suffixed = if route.library { format!("out/{}", host_lib_file("x")) } else { format!("out/{}", host_exe_file("x")) };
    let default = if route.library { host_lib_file("mylib") } else { host_exe_file("mylib") };
    let bare = if route.library { "x".to_string() } else { host_exe_file("x") };
    let abs = dir.join("abs").join("deep").join("x");
    let abs_expected = if route.library { abs.clone() } else { PathBuf::from(host_exe_file(abs.to_str().unwrap())) };
    let rel_expected = if route.library { "out/sub/x".to_string() } else { host_exe_file("out/sub/x") };
    vec![
        ("no -o", None, dir.join(default)),
        ("bare name", Some("x".to_string()), dir.join(bare)),
        ("relative path", Some("out/sub/x".to_string()), dir.join(rel_expected)),
        ("absolute path", Some(abs.to_str().unwrap().to_string()), abs_expected),
        ("platform suffix", Some(suffixed.clone()), dir.join(suffixed)),
    ]
}

/// Library files the build left in `dir` itself (not in subdirectories).
fn stray_libraries(dir: &Path, expected: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p != expected)
                .filter(|p| p.extension().is_some_and(|x| x == LIB_EXT))
                .map(|p| p.display().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn dash_o_is_the_output_file_on_every_native_route() {
    if !tools_available() {
        eprintln!("skip: almide or cargo unavailable");
        return;
    }
    let root = std::env::temp_dir().join(format!("almide-issue3349-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut failures = Vec::new();
    for (ri, route) in ROUTES.iter().enumerate() {
        for oi in 0..5 {
            // Each case gets its own working directory, so a default-named or
            // misplaced artifact cannot be credited to another case.
            let dir = root.join(format!("{ri}-{oi}"));
            let (label, o, expected) = outputs(*route, &dir).swap_remove(oi);
            std::fs::create_dir_all(&dir).expect("mkdir");
            std::fs::write(dir.join("mylib.almd"), "effect fn main() -> Unit = println(\"hi\")\n").expect("write src");
            let mut cmd = Command::new(almide_bin());
            cmd.arg("build").arg("mylib.almd").args(route.flags).current_dir(&dir);
            if let Some(o) = &o {
                cmd.arg("-o").arg(o);
            }
            let out = cmd.output().expect("spawn almide");
            let case = format!("{} × {} ({:?})", route.name, label, o);
            if !out.status.success() {
                failures.push(format!("{case}: build failed\n{}", String::from_utf8_lossy(&out.stderr)));
                continue;
            }
            let size = std::fs::metadata(&expected).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                failures.push(format!("{case}: no artifact at {}", expected.display()));
                continue;
            }
            if route.library {
                let strays = stray_libraries(&dir, &expected);
                if !strays.is_empty() {
                    failures.push(format!("{case}: library also written to {strays:?}"));
                }
            } else {
                let run = Command::new(&expected).output().expect("run built binary");
                if String::from_utf8_lossy(&run.stdout) != "hi\n" {
                    failures.push(format!("{case}: the binary at {} did not print hi", expected.display()));
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}
