//! #1928: `almide test` with no file argument and no `almide.toml` in the
//! current directory must refuse with `almide check`'s hint instead of
//! walking the whole CWD tree (a reset shell landed it in a workspace of
//! twenty unrelated repos and it compiled every `.almd` under all of them).
//! Inside a project (an `almide.toml` present) the recursive walk covers the
//! whole project root (#4530).

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

const FAILING: &str = "fn bad() -> Int = 1\ntest \"must not run\" { assert_eq(bad(), 2) }\n";

/// #4530: inside a project, a bare `almide test` walks the WHOLE project root.
/// It used to search only `spec/` + `exercises/` and fall back to `.` only when
/// those held nothing, so a project with a `spec/` never ran its `src/` tests
/// and said nothing. The walk still skips the project-local cache (a dot
/// directory), build output, vendored dependencies and any directory that
/// holds an `.almideignore` marker — each of those holds a FAILING test here,
/// so collecting any of them turns the run red.
#[test]
fn bare_test_inside_a_project_walks_the_whole_root_and_honours_exclusions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"rootwalk\"\nversion = \"0.1.0\"\n").unwrap();
    for (path, src) in [
        ("spec/a_test.almd", GOOD),
        ("src/b.almd", GOOD),
        (".almide/cache/dep/c_test.almd", FAILING),
        ("target/d_test.almd", FAILING),
        ("node_modules/e_test.almd", FAILING),
        ("corpus/f_test.almd", FAILING),
        ("corpus/.almideignore", ""),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let out = Command::new(almide_bin()).arg("test").current_dir(root).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let all = format!("{stdout}{stderr}");
    assert_eq!(out.status.code(), Some(0), "only the two real tests may run; output:\n{all}");
    assert!(stderr.contains("All 2 test file(s) passed"), "spec/ AND src/ must both run; output:\n{all}");
    for skipped in ["c_test.almd", "d_test.almd", "e_test.almd", "f_test.almd", "must not run"] {
        assert!(!all.contains(skipped), "{skipped} must not be collected; output:\n{all}");
    }
}

/// An `.almideignore` marker takes a directory out of a sweep, never out of an
/// explicit request: `almide test corpus/` on the marked directory still runs it.
#[test]
fn an_ignore_marker_does_not_hide_a_directory_named_explicitly() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("corpus")).unwrap();
    std::fs::write(dir.path().join("corpus/.almideignore"), "").unwrap();
    std::fs::write(dir.path().join("corpus/y.almd"), GOOD).unwrap();
    let out = Command::new(almide_bin()).args(["test", "corpus"]).current_dir(dir.path()).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{stderr}");
    assert!(stderr.contains("All 1 test file(s) passed"), "stderr:\n{stderr}");
}
