//! #2119: the direct WASI 0.2 component's HOST-DRIVEN out-of-memory takes
//! C-197's form — `Error: out of memory` on stderr, exit 1 — and not a raw
//! trap.
//!
//! `cabi_realloc` is where a host-produced value lands in guest memory, and
//! it used to answer a refused `memory.grow` by calling `wasi:cli/exit`.
//! That call is inadmissible: the canonical ABI runs realloc inside the
//! lowering window, where the component may not call imports (wasmtime
//! `component/func.rs`: "while this is running the component is forbidden
//! from calling imports"; the lowered-import trampoline's `check_may_leave`
//! traps `CannotLeaveComponent`). So the abort came out as
//! `wasm trap: cannot leave component instance` with exit 134 — a raw trap
//! naming the component's structure for what was a memory shortage.
//!
//! The shims now RESERVE each landing before the import that produces it,
//! in ordinary guest code where the abort is admissible. This test pins
//! both directions: the defined line and exit 1 must appear, and the trap
//! wording and its exit code must not.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

/// Each `io.read_line` takes one op-35 blocking-read whose bytes land via
/// `cabi_realloc`, and the bump region is never freed — so the frontier
/// climbs with the input regardless of what the program keeps. Under a cap
/// this is the shortest route to a host-driven exhaustion.
const DRAIN: &str = r#"import io

effect fn count(n: Int) -> Int = {
  let line = io.read_line()!
  if string.len(line) == 0 then n else count(n + 1)
}

effect fn main() -> Unit = println(int.to_string(count(0)!))
"#;

/// 1 MiB: well past the program's own working set and well under the input
/// the frontier accumulates, so the cap decides the run rather than the
/// machine.
const CAP: &str = "1048576";

fn build(dir: &Path, heap_cap: Option<&str>) -> PathBuf {
    let src = dir.join("drain.almd");
    std::fs::write(&src, DRAIN).expect("write source");
    let out = dir.join(match heap_cap {
        Some(_) => "drain-capped.wasm",
        None => "drain.wasm",
    });
    let mut args = vec![
        "build".to_string(),
        src.to_str().unwrap().to_string(),
        "--target".to_string(),
        "wasm".to_string(),
        "--component".to_string(),
        "-o".to_string(),
        out.to_str().unwrap().to_string(),
    ];
    if let Some(cap) = heap_cap {
        args.push("--heap-cap".to_string());
        args.push(cap.to_string());
    }
    let o = Command::new(almide_bin()).args(&args).output().expect("spawn almide");
    assert!(
        o.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    out
}

fn run(component: &Path, stdin: &str) -> (String, String, Option<i32>) {
    let mut child = Command::new("wasmtime")
        .args(["run", component.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    // A run that aborts partway stops reading, so the write ends in a
    // broken pipe — that IS the expected shape here, not a harness error.
    let _ = child.stdin.as_mut().expect("stdin").write_all(stdin.as_bytes());
    drop(child.stdin.take());
    let o = child.wait_with_output().expect("wait wasmtime");
    (
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
        o.status.code(),
    )
}

/// 20k short lines: ~1.2 MB of input, so the never-freed landing region
/// crosses a 1 MiB cap partway through the drain.
fn input() -> String {
    "x".repeat(60).to_string() + "\n"
}

#[test]
fn host_driven_exhaustion_in_the_component_is_the_defined_abort() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let capped = build(dir.path(), Some(CAP));
    let stdin = input().repeat(20_000);
    let (stdout, stderr, code) = run(&capped, &stdin);

    assert!(
        stderr.contains("Error: out of memory"),
        "expected C-197's line on stderr, got:\nstdout={stdout}\nstderr={stderr}"
    );
    assert_eq!(code, Some(1), "expected exit 1, got {code:?}; stderr={stderr}");
    // The regression's exact shape: a host call from inside the canonical
    // ABI's lowering window. Its absence is what the reservation buys.
    assert!(
        !stderr.contains("cannot leave component instance"),
        "the landing still calls an import from the lowering window:\n{stderr}"
    );
    assert!(
        !stderr.contains("wasm trap"),
        "the abort is a raw trap, which ALS-T6 forbids:\n{stderr}"
    );
}

#[test]
fn the_same_drain_uncapped_answers_what_native_answers() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let uncapped = build(dir.path(), None);
    let stdin = input().repeat(20_000);
    let (stdout, stderr, code) = run(&uncapped, &stdin);
    assert_eq!(code, Some(0), "uncapped run failed: {stderr}");
    assert_eq!(stdout.trim(), "20000", "stderr={stderr}");

    let src = dir.path().join("drain.almd");
    let native = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.as_mut().expect("stdin").write_all(stdin.as_bytes())?;
            c.wait_with_output()
        })
        .expect("native run");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout).trim(),
        stdout.trim(),
        "native and component disagree"
    );
}
