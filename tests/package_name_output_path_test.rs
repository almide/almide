//! #4193: `almide build` (no `-o`) and `almide compile` (project mode) name
//! their output after `[package].name`. A name that is not a plain name —
//! `../x`, an absolute path, `a/b` — is refused, so a cloned repo's manifest
//! cannot make either command write outside the project.
//!
//! Each refusal has its control: a plain name builds / compiles, and `-o`
//! still overrides the name.

use std::path::Path;
use std::process::{Command, Output};

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir.join("home"))
        .output()
        .expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// A project at `<root>/p` whose manifest declares `name`.
fn project(root: &Path, name: &str) -> std::path::PathBuf {
    let p = root.join("p");
    std::fs::create_dir_all(p.join("src")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    std::fs::create_dir_all(p.join("home")).unwrap();
    std::fs::write(p.join("almide.toml"), format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", name.escape_default())).unwrap();
    std::fs::write(p.join("app.almd"), "effect fn main() -> Unit = println(\"hi\")\n").unwrap();
    std::fs::write(p.join("src/mod.almd"), "pub fn f() -> Int = 1\n").unwrap();
    p
}

#[test]
fn build_refuses_a_package_name_that_is_a_path() {
    let td = tempfile::tempdir().unwrap();
    let victim = td.path().join("outside/victim.txt");
    for name in ["../outside/pwned".to_string(), victim.display().to_string(), "sub/tool".to_string()] {
        let _ = std::fs::remove_dir_all(td.path().join("p"));
        let p = project(td.path(), &name);
        std::fs::write(&victim, "untouched").unwrap();
        let o = almide(&p, &["build", "app.almd"]);
        assert_eq!(o.status.code(), Some(1), "{name}: {}", text(&o));
        assert!(text(&o).contains("is not a plain name"), "{name}: {}", text(&o));
        assert!(!td.path().join("outside/pwned").exists(), "{name}: nothing written outside the project");
        assert!(!p.join("sub").exists(), "{name}: no directory created for the name");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched", "{name}: an existing file is never overwritten");
    }
}

#[test]
fn build_names_a_plain_package_and_honours_o() {
    let td = tempfile::tempdir().unwrap();
    let p = project(td.path(), "tool");
    let o = almide(&p, &["build", "app.almd"]);
    assert!(o.status.success(), "{}", text(&o));
    let exe = if cfg!(windows) { "tool.exe" } else { "tool" };
    assert!(p.join(exe).is_file(), "the output is named after a plain package name");
    // `-o` overrides even a name that would be refused.
    let td = tempfile::tempdir().unwrap();
    let p = project(td.path(), "../outside/pwned");
    let o = almide(&p, &["build", "app.almd", "-o", "chosen"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(!td.path().join("outside/pwned").exists());
}

#[test]
fn compile_refuses_a_package_name_that_is_a_path() {
    let td = tempfile::tempdir().unwrap();
    let p = project(td.path(), "../../../outside/v2");
    std::fs::write(td.path().join("outside/v2.almdi"), "untouched").unwrap();
    let o = almide(&p, &["compile"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("is not a plain name"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(td.path().join("outside/v2.almdi")).unwrap(), "untouched");
    // control: a plain name compiles into target/compile
    let td = tempfile::tempdir().unwrap();
    let p = project(td.path(), "plainmod");
    let o = almide(&p, &["compile"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(p.join("target/compile/plainmod.almdi").is_file(), "{}", text(&o));
}
