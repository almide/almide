//! `almide run --target wasm`'s compile cache, and the one main-less rule
//! both `run` legs follow.
//!
//! The cache (`src/cli/wasm_run_cache.rs`): an unchanged program reruns from
//! the module the previous run rendered; an edit misses; an entry whose bytes
//! no longer hash to its recorded digest is rendered over (#4180's rule for
//! the native binaries). The hit/miss is read off `ALMIDE_TIME_PHASES`' lap
//! names, the only place a hit is visible.
//!
//! The main-less rule (`run_mainless`): a file without `main` runs its test
//! blocks on either leg, with `almide test`'s verdicts and exit codes —
//! 0 when they pass, 1 when one fails, 5 when there is nothing to run.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("almide-wasm-run-cache-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr, exit) of `almide run <file> <extra…>` with the run cache
/// in `cache_dir`.
fn run(file: &Path, extra: &[&str], cache_dir: &Path) -> (String, String, i32) {
    let out = Command::new(almide())
        .arg("run")
        .arg(file)
        .args(extra)
        .env("ALMIDE_RUN_PROJECT_DIR", cache_dir)
        .env("ALMIDE_TIME_PHASES", "1")
        .output()
        .expect("spawn almide");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn entries(cache_dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(cache_dir.join("target/wasm"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.retain(|p| p.extension().is_none());
    v.sort();
    v
}

const PROG: &str = "fn main() -> Unit = {\n  let xs = [1, 2, 3]\n  let total = list.fold(xs, 0, (a, x) => a + x)\n  println(\"sum=${total}\")\n}\n";

#[test]
fn an_unchanged_program_reruns_from_the_cache_and_an_edit_misses() {
    let dir = scratch("hit");
    let cache = dir.join("cache");
    let file = dir.join("app.almd");
    std::fs::write(&file, PROG).unwrap();

    let (out1, err1, code1) = run(&file, &["--target", "wasm"], &cache);
    assert_eq!((out1.as_str(), code1), ("sum=6\n", 0), "first run: {err1}");
    assert!(err1.contains("] render ") && !err1.contains("cache hit"), "first run must render: {err1}");
    assert_eq!(entries(&cache).len(), 1, "the first run records one entry");

    let (out2, err2, code2) = run(&file, &["--target", "wasm"], &cache);
    assert_eq!((out2.as_str(), code2), ("sum=6\n", 0), "rerun: {err2}");
    assert!(err2.contains("render (cache hit)"), "an unchanged program must hit: {err2}");

    // One literal changed: a different program, a different entry.
    std::fs::write(&file, PROG.replace("[1, 2, 3]", "[1, 2, 4]")).unwrap();
    let (out3, err3, code3) = run(&file, &["--target", "wasm"], &cache);
    assert_eq!((out3.as_str(), code3), ("sum=7\n", 0), "edited run: {err3}");
    assert!(!err3.contains("cache hit"), "an edit must miss: {err3}");
    assert_eq!(entries(&cache).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tampered_entry_is_rendered_over_not_run() {
    let dir = scratch("tamper");
    let cache = dir.join("cache");
    let file = dir.join("app.almd");
    std::fs::write(&file, PROG).unwrap();
    let (_, err1, code1) = run(&file, &["--target", "wasm"], &cache);
    assert_eq!(code1, 0, "{err1}");
    let entry = entries(&cache).pop().expect("an entry");
    let mut bytes = std::fs::read(&entry).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&entry, bytes).unwrap();

    let (out, err, code) = run(&file, &["--target", "wasm"], &cache);
    assert_eq!((out.as_str(), code), ("sum=6\n", 0), "{err}");
    assert!(!err.contains("cache hit"), "an entry off its digest is not a hit: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mainless_file_runs_its_tests_on_both_legs() {
    let dir = scratch("mainless");
    let cache = dir.join("cache");
    let cases = [
        ("pass.almd", "fn double(x: Int) -> Int = x * 2\n\ntest \"doubles\" {\n  assert_eq(double(2), 4)\n}\n", 0),
        ("fail.almd", "fn double(x: Int) -> Int = x * 2\n\ntest \"doubles\" {\n  assert_eq(double(2), 5)\n}\n", 1),
        // Neither `main` nor a test: nothing to run, the zero rule (#2204).
        ("empty.almd", "fn double(x: Int) -> Int = x * 2\n", 5),
    ];
    for (name, src, want) in cases {
        let file = dir.join(name);
        std::fs::write(&file, src).unwrap();
        for leg in ["rust", "wasm"] {
            let (_, err, code) = run(&file, &["--target", leg], &cache);
            assert_eq!(code, want, "{name} on {leg}: {err}");
            assert!(err.contains("declares no `main`; running its test blocks"), "{name} on {leg}: {err}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
