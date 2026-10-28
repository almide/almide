//! #4184: `almide install` builds the package as the root project, so its own
//! native code (`native/` Rust, `[native-deps]` crates, `@extern(rust|c)`)
//! would run with the user's rights at build time. `[permissions] native`
//! gates only dependencies, so install refuses such a package unless
//! `--allow-native` is given, and names the code. Control: a pure-Almide
//! package installs with no flag.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn project(root: &Path, dir: &str, native: bool) -> PathBuf {
    let p = root.join(dir);
    std::fs::create_dir_all(p.join("src")).unwrap();
    std::fs::write(p.join("almide.toml"), "[package]\nname = \"tool\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(p.join("src/main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").unwrap();
    if native {
        std::fs::create_dir_all(p.join("native")).unwrap();
        std::fs::write(p.join("native/helper.rs"), "pub fn helper() {}\n").unwrap();
    }
    p
}

fn install(root: &Path, proj: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("install")
        .arg(proj)
        .args(["--bin-dir", root.join("bin").to_str().unwrap(), "--target", "wasm"])
        .args(extra)
        .current_dir(root)
        .env("HOME", root.join("home"))
        .output()
        .expect("spawn almide install")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn a_package_with_native_code_needs_allow_native() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let p = project(td.path(), "with-native", true);
    let o = install(td.path(), &p, &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("compiles native code") && text(&o).contains("native/helper.rs"), "{}", text(&o));
    assert!(!td.path().join("bin/tool").exists(), "nothing is installed");
    let o = install(td.path(), &p, &["--allow-native"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(td.path().join("bin/tool").is_file());
}

#[test]
fn a_pure_package_installs_without_a_flag() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let p = project(td.path(), "pure", false);
    let o = install(td.path(), &p, &[]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(td.path().join("bin/tool").is_file());
}

#[test]
fn an_untrusted_remote_spec_is_refused_before_any_fetch() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["install", "github.com/someone-untrusted/tool", "--bin-dir", td.path().join("bin").to_str().unwrap()])
        .current_dir(td.path())
        .env("HOME", td.path().join("home"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("its owner is not trusted"), "{}", text(&o));
    assert!(!td.path().join("home/.almide/cache").exists(), "nothing was fetched");
}
