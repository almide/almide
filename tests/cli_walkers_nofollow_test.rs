//! #4140: the toolchain's directory walkers (`almide fmt <dir>`, `almide
//! test <dir>`) never follow a symlink. A link loop cannot make them repeat
//! work, and `almide fmt` cannot rewrite a file outside the directory it was
//! pointed at through a symlinked file; a symlink named explicitly is refused,
//! and a regular file is replaced whole (temp file + rename), never written
//! through a link.
//!
//! Negative control: the pre-#4140 walk (`path.is_dir()`, which follows)
//! printed `Formatted proj/sub/loop/sub/loop/…/ext.almd` at every level until
//! ELOOP and rewrote `outside/victim.almd`.
#![cfg(unix)]

use std::process::Command;

const UNFORMATTED: &str = "fn   f( )->Int=1\n";

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let t = tempfile::tempdir().expect("scratch");
    let base = std::fs::canonicalize(t.path()).expect("canonical");
    std::fs::create_dir_all(base.join("proj/sub")).unwrap();
    std::fs::create_dir_all(base.join("outside")).unwrap();
    std::fs::write(base.join("outside/victim.almd"), UNFORMATTED).unwrap();
    std::fs::write(base.join("proj/sub/a_test.almd"), "fn   g( )->Int=2\n\ntest \"g\" {\n  assert_eq(g(), 2)\n}\n").unwrap();
    std::os::unix::fs::symlink("..", base.join("proj/sub/loop")).unwrap();
    std::os::unix::fs::symlink("../outside/victim.almd", base.join("proj/ext.almd")).unwrap();
    (t, base)
}

#[test]
fn fmt_of_a_directory_formats_each_real_file_once_and_never_through_a_link() {
    let (_t, base) = tree();
    let o = Command::new(almide()).args(["fmt", "proj"]).current_dir(&base).output().expect("spawn");
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "{err}");
    let formatted: Vec<&str> = err.lines().filter(|l| l.starts_with("Formatted ")).collect();
    assert_eq!(formatted, ["Formatted proj/sub/a_test.almd"], "{err}");
    assert_eq!(std::fs::read_to_string(base.join("outside/victim.almd")).unwrap(), UNFORMATTED, "fmt rewrote a file outside proj");
    assert!(std::fs::read_to_string(base.join("proj/sub/a_test.almd")).unwrap().starts_with("fn g() -> Int = 2"));
    assert!(base.join("proj/ext.almd").is_symlink(), "the link was replaced");

    // A symlink named explicitly is refused, its target untouched.
    let o = Command::new(almide()).args(["fmt", "proj/ext.almd"]).current_dir(&base).output().expect("spawn");
    assert!(!o.status.success(), "fmt wrote through an explicit symlink");
    assert!(String::from_utf8_lossy(&o.stderr).contains("a symlink"), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(base.join("outside/victim.almd")).unwrap(), UNFORMATTED);
}

#[test]
fn test_discovery_does_not_descend_a_symlinked_directory() {
    let (_t, base) = tree();
    let o = Command::new(almide()).args(["test", "proj"]).current_dir(&base).output().expect("spawn");
    let out = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "{out}");
    assert!(!out.contains("loop/"), "discovery followed the loop:\n{out}");
    assert!(out.contains("1 test"), "expected exactly one test file:\n{out}");
}
