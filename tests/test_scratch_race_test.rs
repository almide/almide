//! #1877: `almide test` keyed its scratch wasm module and native worker dir
//! on the test file's RELATIVE path under one shared temp dir, so two
//! invocations on same-named files from different directories wrote the same
//! `x_test_almd.wasm` and whichever wrote first ran wasmtime on the OTHER
//! file's module — a failing file printed "All 1 test file(s) passed".
//!
//! The race window is the gap between the write and wasmtime opening the
//! file, a few milliseconds, so one pair of processes hits it a few percent
//! of the time; eight processes over eight rounds make a wrong verdict near
//! certain on the pre-fix binary while the fixed binary (per-run root, keyed
//! on the absolute path) can never share a path and is deterministically
//! green.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

const PASSING: &str = "test \"same name\" {\n  assert_eq(1 + 1, 2)\n}\n";
const FAILING: &str = "test \"same name\" {\n  assert_eq(1 + 1, 3)\n}\n";

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-1877-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// Write `x_test.almd` with `body` into `dir` and return the dir.
fn case_dir(root: &Path, name: &str, body: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("case dir");
    std::fs::write(dir.join("x_test.almd"), body).expect("write case");
    dir
}

/// Run `almide test x_test.almd` from inside `dir` (the RELATIVE name is
/// what the pre-fix harness keyed on), returning (success, stderr).
fn run_test_in(dir: &Path) -> (bool, String) {
    let out = Command::new(almide_bin())
        .args(["test", "x_test.almd"])
        .current_dir(dir)
        .output()
        .expect("spawn almide test");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Every verdict from `rounds` rounds of `pairs` concurrent (passing,
/// failing) same-named pairs, as (expected_pass, actual_pass, stderr).
fn race(root: &Path, rounds: usize, pairs: usize) -> Vec<(bool, bool, String)> {
    let mut verdicts = Vec::new();
    for _ in 0..rounds {
        let handles: Vec<_> = (0..pairs)
            .flat_map(|j| {
                let p = case_dir(root, &format!("p{j}"), PASSING);
                let f = case_dir(root, &format!("f{j}"), FAILING);
                [(true, p), (false, f)]
            })
            .map(|(expected, dir)| std::thread::spawn(move || {
                let (ok, err) = run_test_in(&dir);
                (expected, ok, err)
            }))
            .collect();
        for h in handles {
            verdicts.push(h.join().expect("race thread"));
        }
    }
    verdicts
}

fn assert_all_right(verdicts: &[(bool, bool, String)]) {
    let wrong: Vec<String> = verdicts
        .iter()
        .filter(|(expected, actual, _)| expected != actual)
        .map(|(expected, _, err)| format!("expected {}: {err}", if *expected { "PASS" } else { "FAIL" }))
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} verdicts wrong — the scratch artifacts of same-named files are shared:\n{}",
        wrong.len(),
        verdicts.len(),
        wrong.join("\n")
    );
}

#[test]
fn same_named_files_in_different_directories_get_their_own_verdicts_under_parallel_runs() {
    let root = fresh_root("pair");
    let verdicts = race(&root, 8, 4);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(verdicts.len(), 64);
    assert_all_right(&verdicts);
}

#[test]
fn the_same_file_run_twice_in_parallel_passes_both_times() {
    let root = fresh_root("same");
    let dir = case_dir(&root, "one", PASSING);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || run_test_in(&dir))
        })
        .collect();
    let results: Vec<(bool, String)> = handles.into_iter().map(|h| h.join().expect("thread")).collect();
    let _ = std::fs::remove_dir_all(&root);
    for (ok, err) in &results {
        assert!(ok, "a parallel run of one passing file failed:\n{err}");
    }
}

#[test]
fn same_named_files_in_one_invocation_get_their_own_verdicts() {
    let root = fresh_root("one-run");
    case_dir(&root, "a", PASSING);
    case_dir(&root, "b", FAILING);
    let out = Command::new(almide_bin())
        .args(["test", "."])
        .current_dir(&root)
        .output()
        .expect("spawn almide test");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&root);
    assert!(!out.status.success(), "b/x_test.almd must fail the run:\n{stderr}");
    assert!(stderr.contains("FAILED: ./b/x_test.almd"), "the failing file is b/x_test.almd:\n{stderr}");
    assert!(!stderr.contains("FAILED: ./a/x_test.almd"), "a/x_test.almd must pass:\n{stderr}");
}

#[test]
fn scratch_root_is_removed_on_exit_and_kept_under_the_flag() {
    let root = fresh_root("keep");
    let dir = case_dir(&root, "one", PASSING);
    let out = Command::new(almide_bin())
        .args(["test", "x_test.almd"])
        .env("ALMIDE_KEEP_SCRATCH", "1")
        .current_dir(&dir)
        .output()
        .expect("spawn almide test");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let kept = stderr
        .lines()
        .find_map(|l| l.strip_prefix("scratch kept (ALMIDE_KEEP_SCRATCH): "))
        .unwrap_or_else(|| panic!("no scratch-kept line:\n{stderr}"))
        .split(" (native build cache: ")
        .next()
        .unwrap()
        .trim()
        .to_string();
    let kept = PathBuf::from(kept);
    assert!(kept.join("wasm").is_dir(), "kept root has no wasm/: {}", kept.display());
    let _ = std::fs::remove_dir_all(&kept);

    let (ok, stderr) = run_test_in(&dir);
    assert!(ok, "{stderr}");
    assert!(!stderr.contains("scratch kept"), "the root must be removed without the flag:\n{stderr}");
    let _ = std::fs::remove_dir_all(&root);
}
