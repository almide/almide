//! #4907: `almide build --target wasi` of a program that imports `process`
//! failed in rustc with E0308. The native runtime is carried into the build
//! whole, and its signal syscall (`almide_process_signal`, the `send` that
//! `process.kill` hands the core) had a body only for unix and Windows, so on
//! wasm32-wasip1 the function returned `()`. Every target without signals now
//! gets a body that fails with ENOSYS.
//!
//! The novel case builds the program for wasm32-wasip1 and runs it: the build
//! succeeds and `process.kill` returns its fixed error text with ENOSYS. The
//! control is the same program built natively, where the same call reaches
//! kill(2) and answers ESRCH (pid 2147483647 is past every kernel's pid range,
//! so no process has it). Skips when the wasm32-wasip1 Rust target or
//! wasmtime is missing, unless `ALMIDE_EXPECT_TOOLS` is set.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The binary cargo built from this tree, never one on PATH.
fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn available() -> bool {
    let wasip1 = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == "wasm32-wasip1"));
    let wasmtime = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !(wasip1 && wasmtime) && std::env::var_os("ALMIDE_EXPECT_TOOLS").is_some() {
        panic!("ALMIDE_EXPECT_TOOLS is set but the wasm32-wasip1 target ({wasip1}) or wasmtime ({wasmtime}) is missing");
    }
    wasip1 && wasmtime
}

const PROGRAM: &str = r#"import process

effect fn main() -> Unit = {
  match process.kill(2147483647, 0) {
    ok(_) => println("kill ok"),
    err(e) => println("kill err: ${e}"),
  }
  println("done")
}
"#;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-4907-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("p.almd"), PROGRAM).unwrap();
    d
}

fn build(d: &Path, extra: &[&str], out: &str) {
    let o = Command::new(almide_bin())
        .args(["build", "p.almd"])
        .args(extra)
        .args(["-o", out])
        .current_dir(d)
        .output()
        .expect("spawn almide");
    assert!(
        o.status.success(),
        "almide build {extra:?} failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

#[test]
fn a_wasi_build_of_a_process_program_compiles_and_kill_fails_cleanly() {
    if !available() {
        return;
    }
    let d = dir("wasi");
    build(&d, &["--target", "wasi"], "p.wasm");
    let o = Command::new("wasmtime").arg(d.join("p.wasm")).current_dir(&d).output().expect("spawn wasmtime");
    let _ = std::fs::remove_dir_all(&d);
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "wasi run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    assert!(stdout.starts_with("kill err: process.kill(2147483647, 0): "), "{stdout}");
    assert!(stdout.contains("(errno 52)"), "ENOSYS as WASI numbers it: {stdout}");
    assert!(stdout.ends_with("done\n"), "{stdout}");
}

/// Control: natively the same call reaches kill(2), which finds no process.
#[cfg(unix)]
#[test]
fn the_same_program_built_natively_reaches_kill() {
    let d = dir("native");
    build(&d, &[], "p");
    let o = Command::new(d.join("p")).current_dir(&d).output().expect("spawn native");
    let _ = std::fs::remove_dir_all(&d);
    assert!(o.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(stdout.starts_with("kill err: process.kill(2147483647, 0): "), "{stdout}");
    assert!(!stdout.contains("errno 52"), "{stdout}");
    assert!(stdout.ends_with("done\n"), "{stdout}");
}
