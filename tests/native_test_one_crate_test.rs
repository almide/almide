//! #4441: `almide test --target native` builds ONE crate per run, not one per
//! test file, and keeps every per-file semantic it had.
//!
//! The fixture is a package with one shared module and four test files that
//! each reach it. Two files define a top-level fn of the same name with
//! different bodies; one file has a test that panics (it fails alone); one
//! uses `testing.assert_aborts` (its child re-executes the binary by its test
//! path, which now sits in the file's module); one ends its whole process
//! with an out-of-bounds index (that file fails, the others do not).
//!
//! The gate is the crate count: the per-file driver leaves one crate dir per
//! test file in the native build cache, the one-crate driver one in all. Run
//! with `ALMIDE_BIN` pointing at a per-file `almide` (v0.69.0-rc2 or older)
//! and `a_run_builds_one_crate_not_one_per_test_file` fails: that is its
//! negative control.

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

const SHARED: &str = "fn double(x: Int) -> Int = x * 2\n";

const A_TEST: &str = r#"import self.shared

fn helper() -> Int = 1

test "a doubles" {
  assert_eq(shared.double(2), 4)
}

test "a panics alone" {
  panic("boom from a")
}

test "a helper" {
  assert_eq(helper(), 1)
}
"#;

const B_TEST: &str = r#"import self.shared

fn helper() -> Int = 2

test "b doubles" {
  assert_eq(shared.double(3), 6)
}

test "b helper" {
  assert_eq(helper(), 2)
}
"#;

const C_TEST: &str = r#"import testing
import self.shared

test "c aborts in a child" {
  testing.assert_aborts("boom c", () => panic("boom c"))
  assert_eq(shared.double(1), 2)
}
"#;

const D_TEST: &str = r#"import self.shared

test "d index aborts the process" {
  let xs = [shared.double(1)]
  assert_eq(xs[5], 0)
}
"#;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    tmp: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::Builder::new().prefix("one_crate").tempdir().expect("tempdir");
    let root = dir.path().join("pkg");
    let tmp = dir.path().join("tmp");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"onecrate\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(root.join("src/shared.almd"), SHARED).unwrap();
    for (name, src) in [("a_test", A_TEST), ("b_test", B_TEST), ("c_test", C_TEST), ("d_test", D_TEST)] {
        std::fs::write(root.join("tests").join(format!("{name}.almd")), src).unwrap();
    }
    Fixture { _dir: dir, root, tmp }
}

/// `almide test --target native [args]` in the package, with the build cache
/// under the fixture's own temp dir; (exit code, stdout + stderr).
fn run(f: &Fixture, args: &[&str]) -> (i32, String) {
    let out = Command::new(almide_bin())
        .args(["test", "--target", "native"])
        .args(args)
        .current_dir(&f.root)
        .env("TMPDIR", &f.tmp)
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The crate dirs the native test build cache holds: each one is a rustc run.
fn crate_dirs(tmp: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for base in std::fs::read_dir(tmp).unwrap().flatten() {
        let native = base.path().join("test").join("native");
        let Ok(entries) = std::fs::read_dir(&native) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.join("src/main.rs").exists() || p.join("almide_test_main.rs").exists() {
                found.push(p);
            }
        }
    }
    found
}

#[test]
fn a_run_builds_one_crate_not_one_per_test_file() {
    let f = fixture();
    let (code, out) = run(&f, &[]);
    assert_eq!(code, 1, "two files fail\n{out}");
    let crates = crate_dirs(&f.tmp);
    assert_eq!(crates.len(), 1, "a run of 4 test files built {} crates — one per file again?\n{crates:?}\n{out}", crates.len());
}

#[test]
fn a_failing_test_fails_alone_and_the_verdicts_are_per_file() {
    let f = fixture();
    let (code, out) = run(&f, &[]);
    assert_eq!(code, 1, "{out}");
    // The panicking test fails its file; the file's other tests still ran.
    assert!(out.contains("FAILED: tests/a_test.almd"), "{out}");
    assert!(out.contains("a panics alone"), "{out}");
    assert!(out.contains("boom from a"), "{out}");
    // The process-ending abort fails its own file only.
    assert!(out.contains("tests/d_test.almd"), "{out}");
    assert!(out.contains("index out of bounds"), "{out}");
    for ok in ["b_test", "c_test"] {
        assert!(!out.contains(&format!("FAILED: tests/{ok}.almd")), "{ok} must pass\n{out}");
    }
    assert!(out.contains("2/4 test file(s) failed"), "{out}");
    // a: 3 tests, b: 2, c: 1, d: 1 — no file reports another file's tests.
    assert!(out.contains("7 tests in 4 files"), "{out}");
    // The merged module's path never reaches the report.
    assert!(!out.contains("almide_test_file_"), "{out}");
}

#[test]
fn run_selects_by_test_name_across_the_files() {
    let f = fixture();
    let (code, out) = run(&f, &["--run", "b doubles"]);
    assert_eq!(code, 0, "{out}");
    // Filtered out, per file as before: a 3, b 1, c 1 + its hidden abort-site
    // entry, d 1.
    assert!(out.contains("1 test in 4 files (7 filtered out)"), "{out}");
    let (code, out) = run(&f, &["--run", "no test is called this"]);
    assert_eq!(code, 5, "a pattern that selects nothing is the no-tests verdict\n{out}");
}

#[test]
fn an_edit_to_the_shared_module_rebuilds_the_run() {
    let f = fixture();
    let (_, out) = run(&f, &["--run", "b doubles"]);
    assert!(out.contains("All 4 test file(s) passed"), "{out}");
    std::fs::write(f.root.join("src/shared.almd"), "fn double(x: Int) -> Int = x * 3\n").unwrap();
    let (code, out) = run(&f, &["--run", "b doubles"]);
    assert_eq!(code, 1, "the edited module must be what the tests run against\n{out}");
    assert!(out.contains("FAILED: tests/b_test.almd"), "{out}");
    assert!(out.contains("9"), "{out}");
    // The rebuild reuses the run's crate dir instead of adding one per file.
    assert_eq!(crate_dirs(&f.tmp).len(), 1);
}
