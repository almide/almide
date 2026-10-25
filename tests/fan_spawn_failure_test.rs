//! A `fan { }` arm whose thread cannot start runs inline (#4141, C-004).
//!
//! Native `fan` started every arm with `Scope::spawn`, which panics when the OS
//! refuses the thread: a recursive two-arm fan of depth 11 under
//! `ulimit -u 2048` (Linux), or of depth 17 on a stock macOS, ended in exit 101
//! with `failed to spawn thread` and a std source path on stderr. Fan semantics
//! are the sequential evaluation (ADR-0024 D1), so an arm that gets no thread
//! runs on the thread that reached it, and the program answers what it answers
//! with threads.
//!
//! The refusal is injected portably: `RUST_MIN_STACK` sets the stack every
//! spawned Rust thread asks for, and one PiB is a stack no machine maps, so
//! every arm spawn fails (EAGAIN/ENOMEM) on Linux and macOS alike, whatever the
//! process limit. The main thread is not spawned and is unaffected.

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// Every leaf prints its path, so the arm order of the output is visible: a
/// fallback that ran an arm out of order, or lost a timeline, changes stdout.
const PROGRAM: &str = r#"effect fn f(n: Int, tag: String) -> Int = if n == 0 then {
  println(tag)
  1
} else {
  let (a, b) = fan { f(n - 1, tag + "0")!, f(n - 1, tag + "1")! }
  a + b
}

effect fn main() -> Unit = {
  println(int.to_string(f(5, "")!))
}
"#;

const NO_STACK: &str = "1125899906842624";

fn expected() -> String {
    let mut s: String = (0..32).map(|i| format!("{i:05b}\n")).collect();
    s.push_str("32\n");
    s
}

fn build(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::write(dir.join("main.almd"), PROGRAM).expect("write");
    let bin = dir.join("fanrec");
    let out = Command::new(almide())
        .args(["build", "main.almd", "-o"])
        .arg(&bin)
        .current_dir(dir)
        .output()
        .expect("almide build");
    assert!(
        out.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    bin
}

#[test]
fn a_refused_arm_thread_runs_the_arm_inline() {
    let dir = tempfile::Builder::new()
        .prefix("t4141")
        .tempdir()
        .expect("tempdir");
    let bin = build(dir.path());
    for min_stack in [None, Some(NO_STACK)] {
        let mut cmd = Command::new(&bin);
        if let Some(v) = min_stack {
            cmd.env("RUST_MIN_STACK", v);
        }
        let out = cmd.output().expect("run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(0),
            "RUST_MIN_STACK={min_stack:?}: exit\n{stderr}"
        );
        assert!(
            stderr.is_empty(),
            "RUST_MIN_STACK={min_stack:?}: no panic text, got:\n{stderr}"
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            expected(),
            "RUST_MIN_STACK={min_stack:?}: stdout"
        );
    }
}

/// The issue's shape on a real limit: Linux counts threads against
/// `RLIMIT_NPROC`, so a depth-12 fan under `ulimit -u 512` runs out of
/// threads. Skipped where the shell or the limit does not apply (macOS counts
/// processes only; root ignores the limit).
#[cfg(target_os = "linux")]
#[test]
fn a_recursive_fan_under_a_process_limit_answers() {
    let dir = tempfile::Builder::new()
        .prefix("t4141u")
        .tempdir()
        .expect("tempdir");
    std::fs::write(
        dir.path().join("main.almd"),
        PROGRAM
            .replace("f(5, \"\")", "f(12, \"\")")
            .replace("  println(tag)\n", ""),
    )
    .expect("write");
    let bin = dir.path().join("fanrec");
    let b = Command::new(almide())
        .args(["build", "main.almd", "-o"])
        .arg(&bin)
        .current_dir(dir.path())
        .output()
        .expect("almide build");
    assert!(
        b.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let out = Command::new("bash") // dash, Ubuntu's sh, has no `ulimit -u`
        .arg("-c")
        .arg(format!("ulimit -u 512 && exec {}", bin.display()))
        .output()
        .expect("run under ulimit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "exit under ulimit -u 512\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "no panic text, got:\n{stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "4096\n");
}
