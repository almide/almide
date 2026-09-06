//! #1928: `almide test` with no file argument and no `almide.toml` in the
//! current directory must refuse with `almide check`'s hint instead of
//! walking the whole CWD tree (a reset shell landed it in a workspace of
//! twenty unrelated repos and it compiled every `.almd` under all of them).
//! Inside a project (an `almide.toml` present) the recursive walk stays.

use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

const GOOD: &str = "fn fine() -> Int = 1\ntest \"t2\" { assert_eq(fine(), 1) }\n";

#[test]
fn bare_test_without_manifest_refuses_with_the_check_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("b/y.almd"), GOOD).unwrap();
    let out = Command::new(almide_bin()).arg("test").current_dir(dir.path()).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "must refuse; stderr:\n{stderr}");
    assert!(stderr.contains("No file specified and no almide.toml found."), "the check hint is expected; stderr:\n{stderr}");
    assert!(!stderr.contains("y.almd"), "the tree must not be walked; stderr:\n{stderr}");
}

#[test]
fn bare_test_inside_a_project_still_walks() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"walk\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.path().join("b/y.almd"), GOOD).unwrap();
    let out = Command::new(almide_bin()).arg("test").current_dir(dir.path()).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "must walk and pass; stderr:\n{stderr}");
    assert!(stderr.contains("All 1 test file(s) passed"), "stderr:\n{stderr}");
}

#[test]
fn explicit_directory_still_walks_without_a_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(dir.path().join("b/y.almd"), GOOD).unwrap();
    let out = Command::new(almide_bin()).args(["test", "b"]).current_dir(dir.path()).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "an explicit directory is the user's choice; stderr:\n{stderr}");
}
