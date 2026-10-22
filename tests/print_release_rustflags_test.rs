//! `almide build --print-release-rustflags` prints the codegen flags a native
//! `--release` build adds to the release profile, and builds nothing. The perf
//! harness (research/benchmark/perf/bench.py) compiles every hand-written Rust
//! reference with these flags, so an Almide-vs-Rust ratio compares one set of
//! flags: x86_64 `--release` builds for `x86-64-v3` by default (#3604), and a
//! reference built for the generic CPU read that choice as Almide's own win or
//! loss (#3883).

use std::process::Command;

/// Run from an empty directory with no source file named: the flag builds
/// nothing, so it must not ask for a project.
fn print(args: &[&str]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("almide-print-rustflags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .arg("build")
        .args(args)
        .arg("--print-release-rustflags")
        .env_remove("CARGO_BUILD_TARGET")
        .output()
        .expect("run almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_flags_follow_the_target_and_the_cpu_flag() {
    let llvm = "-C llvm-args=-force-ordered-reductions=false";
    let (ok, out, err) = print(&["--target", "x86_64-unknown-linux-gnu"]);
    assert!(ok, "{err}");
    assert_eq!(out, format!("-C target-cpu=x86-64-v3 {llvm}"));
    let (ok, out, err) = print(&["--target", "aarch64-unknown-linux-gnu"]);
    assert!(ok, "{err}");
    assert_eq!(out, llvm, "aarch64 stays on the generic CPU");
    let (ok, out, err) = print(&[
        "--target",
        "x86_64-unknown-linux-gnu",
        "--target-cpu",
        "portable",
    ]);
    assert!(ok, "{err}");
    assert_eq!(out, llvm);
    let (ok, out, err) = print(&["--target-cpu", "x86-64-v4"]);
    assert!(ok, "{err}");
    assert_eq!(out, format!("-C target-cpu=x86-64-v4 {llvm}"));
}

#[test]
fn the_host_default_matches_the_hosts_arch() {
    let (ok, out, err) = print(&[]);
    assert!(ok, "{err}");
    if std::env::consts::ARCH == "x86_64" {
        assert!(out.starts_with("-C target-cpu=x86-64-v3 "), "{out}");
    } else {
        assert!(!out.contains("target-cpu"), "{out}");
    }
}

#[test]
fn a_wasm_build_has_no_rustflags_to_print() {
    let (ok, _, err) = print(&["--target", "wasm"]);
    assert!(!ok);
    assert!(err.contains("--print-release-rustflags"), "{err}");
}
