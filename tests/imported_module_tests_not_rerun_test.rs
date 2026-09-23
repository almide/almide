//! A file's test run runs exactly that file's tests (#2550).
//!
//! `app.almd` imports `helper.almd`. On the native leg the imported module's
//! `test` blocks used to be flattened into the importer's test binary as
//! `almide_rt_helper___test_*`, so a failing helper test failed `app.almd` too —
//! reported under the raw Rust fn name, at the helper's line with the app's
//! path — and one broken test was counted as two failed files. Each module's
//! tests belong to that module's own run: a directory run runs every test
//! exactly once, and every leg agrees.

use std::path::Path;
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

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The issue's repro, verbatim: `process.spawn` keeps both files on native.
const NATIVE_HELPER: &str = "import process\n\
\n\
effect fn start() -> Result[Int, String] = process.spawn(\"true\", [])\n\
\n\
test \"helper fails\" {\n\
  let pid = start()!\n\
  println(\"from helper\")\n\
  assert(pid < 0)\n\
}\n";

const NATIVE_APP: &str = "import helper\n\
\n\
effect fn main() -> Unit = println(\"app\")\n\
\n\
test \"app passes\" {\n\
  let pid = helper.start()!\n\
  assert(pid > 0)\n\
}\n";

/// A pure pair every leg (native, wasm, default lane) can run.
fn pure_helper(expected: i64) -> String {
    format!(
        "fn double(x: Int) -> Int = x * 2\n\
\n\
test \"helper check\" {{\n\
  println(\"from helper\")\n\
  assert(double(2) == {expected})\n\
}}\n"
    )
}

const PURE_APP: &str = "import helper\n\
\n\
effect fn main() -> Unit = println(\"app\")\n\
\n\
test \"app passes\" {\n\
  assert(helper.double(2) == 4)\n\
}\n";

fn run(helper: &str, app: &str, args: &[&str]) -> (i32, String) {
    let dir = tempfile::Builder::new().prefix("t2550").tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"repro\"\n").expect("manifest");
    std::fs::create_dir(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/helper.almd"), helper).expect("helper");
    std::fs::write(dir.path().join("src/app.almd"), app).expect("app");
    let out = Command::new(almide_bin())
        .arg("test")
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Legs for a pair that can run on wasm: native, wasm, and the default lane.
fn all_legs() -> Vec<Vec<&'static str>> {
    let mut legs = vec![vec!["--target", "native"]];
    if wasmtime_available() {
        legs.push(vec!["--target", "wasm"]);
        legs.push(vec![]);
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg and the default lane are not exercised");
    }
    legs
}

/// The failure is the helper's alone: one failed file, reported under the
/// helper's path with the source test name, and the app is not failed.
fn assert_only_helper_failed(leg: &[&str], code: i32, out: &str) {
    assert_eq!(code, 1, "{leg:?}: the helper's test fails\n{out}");
    let failed_files: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("FAILED: ") || l.starts_with("FAIL "))
        .collect();
    assert_eq!(failed_files.len(), 1, "{leg:?}: one test failed, so one file fails\n{out}");
    assert!(failed_files[0].ends_with("src/helper.almd"), "{leg:?}: the failure is the helper's\n{out}");
    assert!(!out.contains("almide_rt_"), "{leg:?}: a mangled Rust test name leaked into the report\n{out}");
    assert_eq!(out.matches("from helper").count(), 1, "{leg:?}: the helper's test ran more than once\n{out}");
}

#[test]
fn a_failing_imported_test_fails_only_its_own_file_native() {
    for leg in [vec!["--target", "native"], vec![]] {
        let (code, out) = run(NATIVE_HELPER, NATIVE_APP, &leg);
        assert_only_helper_failed(&leg, code, &out);
    }
}

#[test]
fn a_failing_imported_test_fails_only_its_own_file_on_every_leg() {
    for leg in all_legs() {
        let (code, out) = run(&pure_helper(5), PURE_APP, &leg);
        assert_only_helper_failed(&leg, code, &out);
    }
}

#[test]
fn a_directory_run_runs_every_test_exactly_once() {
    for leg in all_legs() {
        let mut args = leg.clone();
        args.push("--show-output");
        let (code, out) = run(&pure_helper(4), PURE_APP, &args);
        assert_eq!(code, 0, "{leg:?}: both tests pass\n{out}");
        assert!(out.contains("2 tests in 2 files"), "{leg:?}: two tests, each counted once\n{out}");
        assert_eq!(out.matches("from helper").count(), 1, "{leg:?}: the helper's test ran more than once\n{out}");
    }
}
