//! The v1 native build's runtime crate profile (#4006, owner ruling
//! 2026-11-02).
//!
//! A program the v1 trust spine renders carries its runtime part — the shared
//! string buffer and the runtime bodies it takes along — in one module, which
//! the build compiles as a crate of its own: without debug assertions, as v0
//! compiles its runtime rlib, and with overflow checks pinned off. The
//! program's own code keeps `[profile.dev]` as it was, its debug assertions
//! included.
//!
//! The build compiles the two crates with bare rustc (no cargo), and records
//! the two commands in `<project>/almide-rustc.json`. What this test holds, on
//! a real build:
//! - the program builds as two crates with the runtime's two settings pinned
//!   and the program's dev profile (opt-level 1, debug assertions) kept;
//! - overflow wraps on the native leg, in the program's code and through the
//!   runtime (`int.abs` of the minimum), as before the split;
//! - replaying the recorded commands, a `debug_assert!` placed in the PROGRAM
//!   crate still fires (the negative control: the ruling must not switch off
//!   the program's checks);
//! - the same `debug_assert!` placed in the RUNTIME crate does not.

use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM: &str = r#"fn main() -> Unit = {
  let big = 9223372036854775807
  println(int.to_string(big + 1))
  println(int.to_string(int.abs(-9223372036854775807 - 1)))
}
"#;

const WRAPPED: &str = "-9223372036854775808\n-9223372036854775808\n";

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-rt-profile-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// The project directory the build wrote under `tmp`.
fn run_dir(tmp: &Path) -> PathBuf {
    std::fs::read_dir(tmp)
        .expect("tmp")
        .filter_map(|e| e.ok().map(|e| e.path().join("run")))
        .find(|p| p.join("almide-rustc.json").is_file())
        .unwrap_or_else(|| panic!("no rustc-built project under {}", tmp.display()))
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array().expect("argv").iter().map(|a| a.as_str().expect("arg").to_string()).collect()
}

fn has_pair(argv: &[String], a: &str, b: &str) -> bool {
    argv.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// The value after `flag`.
fn after(argv: &[String], flag: &str) -> String {
    let i = argv.iter().position(|a| a == flag).unwrap_or_else(|| panic!("no {flag} in {argv:?}"));
    argv[i + 1].clone()
}

/// Run a recorded rustc command with some of its paths replaced.
fn replay(argv: &[String], swap: &[(&str, &str)]) {
    let args: Vec<String> = argv
        .iter()
        .map(|a| swap.iter().fold(a.clone(), |acc, (from, to)| acc.replace(from, to)))
        .collect();
    let out = Command::new("rustc").args(&args).output().expect("spawn rustc");
    assert!(out.status.success(), "replayed rustc failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let p = e.path();
        if p.is_dir() {
            copy_tree(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).expect("copy");
        }
    }
}

/// Insert `stmt` at the start of the first fn whose header line contains `header`.
fn plant(file: &Path, header: &str, stmt: &str) {
    let src = std::fs::read_to_string(file).expect("read");
    let at = src.find(header).unwrap_or_else(|| panic!("`{header}` not in {}", file.display()));
    let open = at + src[at..].find('{').expect("fn body") + 1;
    let planted = format!("{}\n{stmt}\n{}", &src[..open], &src[open..]);
    std::fs::write(file, planted).expect("write");
}

#[test]
fn the_runtime_crate_builds_without_debug_assertions_and_the_program_keeps_them() {
    let work = scratch("build");
    let tmp = work.join("tmp");
    std::fs::create_dir_all(&tmp).expect("tmp");
    let src = work.join("p.almd");
    std::fs::write(&src, PROGRAM).expect("write program");
    let bin = work.join("p");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .env("TMPDIR", &tmp)
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .output()
        .expect("spawn almide");
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "almide build failed:\n{log}");
    assert!(log.contains("native: v1 trust-spine render"), "the probe must render on v1:\n{log}");
    let ran = Command::new(&bin).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), WRAPPED, "overflow must wrap on the native leg");

    // The build: two rustc commands, no cargo; the runtime's profile pinned,
    // the program's dev profile kept.
    let run = run_dir(&tmp);
    // (`target/` is the build cache's binary store on every route; a manifest
    // is written only by the cargo route.)
    assert!(!run.join("Cargo.toml").exists(), "cargo built the program; the rustc path was expected");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run.join("almide-rustc.json")).expect("record")).expect("json");
    let rt_argv = strings(record.get("runtime").expect("no runtime crate was split out"));
    let prog_argv = strings(&record["program"]);
    assert!(has_pair(&rt_argv, "-C", "debug-assertions=off"), "runtime keeps debug assertions: {rt_argv:?}");
    assert!(has_pair(&rt_argv, "-C", "overflow-checks=off"), "runtime overflow checks not pinned: {rt_argv:?}");
    assert!(has_pair(&prog_argv, "-C", "opt-level=1"), "program profile changed: {prog_argv:?}");
    assert!(has_pair(&prog_argv, "-C", "debug-assertions=on"), "program lost its debug assertions: {prog_argv:?}");
    assert!(has_pair(&prog_argv, "-C", "overflow-checks=off"), "program overflow checks not pinned: {prog_argv:?}");

    let main_rs = run.join("src/main.rs");
    let rt_dir = after(&rt_argv, "--out-dir");
    let prog_out = after(&prog_argv, "-o");

    // Negative control: a debug_assert! in the PROGRAM crate fires.
    let user = work.join("user");
    copy_tree(&run.join("src"), &user.join("src"));
    plant(&user.join("src/main.rs"), "fn main()", "debug_assert!(false, \"program-check\");");
    let user_bin = user.join("almide-out");
    replay(
        &prog_argv,
        &[(main_rs.to_str().unwrap(), user.join("src/main.rs").to_str().unwrap()), (&prog_out, user_bin.to_str().unwrap())],
    );
    let o = Command::new(&user_bin).output().expect("run");
    assert!(!o.status.success(), "a debug_assert! in the program crate did not fire");
    assert!(String::from_utf8_lossy(&o.stderr).contains("program-check"), "{}", String::from_utf8_lossy(&o.stderr));

    // The same assertion in the RUNTIME crate (the integer printer every line
    // goes through) does not.
    let rt = work.join("rt");
    std::fs::create_dir_all(&rt).expect("mkdir");
    std::fs::copy(Path::new(&rt_dir).join("lib.rs"), rt.join("lib.rs")).expect("copy runtime");
    plant(&rt.join("lib.rs"), "pub fn from_i64(", "debug_assert!(false, \"runtime-check\");");
    replay(&rt_argv, &[(&rt_dir, rt.to_str().unwrap())]);
    let rt_bin = rt.join("almide-out");
    replay(&prog_argv, &[(&prog_out, rt_bin.to_str().unwrap()), (&rt_dir, rt.to_str().unwrap())]);
    let o = Command::new(&rt_bin).output().expect("run");
    assert!(o.status.success(), "a debug_assert! in the runtime crate fired:\n{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(String::from_utf8_lossy(&o.stdout), WRAPPED);
    let _ = std::fs::remove_dir_all(&work);
}
