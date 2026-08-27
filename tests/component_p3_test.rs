//! #1628 stage 2, increment 1: `ALMIDE_COMPONENT_P3=1` + `--component`
//! emits a WASI 0.3 component — stdio over component-model streams on the
//! async canonical ABI (sync stream/future builtins inside an async-lifted
//! `run`). The observable contract is the same as every other leg: stdout,
//! stderr and the exit code are byte-identical to the plain wasm artifact
//! (and thereby to native, which the run-manifest already pins for the
//! fixture programs).
//!
//! The runtime needs wasmtime 46+ with `component-model-async` +
//! `component-model-more-async-builtins` (the 🚝 sync builtins) and
//! `-S p3=y`; CI installs 47.x. An environment whose wasmtime lacks the
//! features skips the execution half (the emission half always runs).

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

fn build_p3(src: &Path, out: &Path) -> String {
    let o = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "--component",
            "-o",
            out.to_str().unwrap(),
        ])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "p3 build failed:\n{stderr}");
    stderr
}

fn build_core(src: &Path, out: &Path) {
    let o = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "core build failed:\n{}", String::from_utf8_lossy(&o.stderr));
}

/// Run a module/component under wasmtime with the p3 feature set.
/// `None` = this wasmtime cannot host the p3 feature set (skip);
/// `Some((stdout, stderr, code))` otherwise.
fn run_p3(module: &Path, stdin: &str) -> Option<(String, String, i32)> {
    let mut child = Command::new("wasmtime")
        .args([
            "run",
            "-W",
            "component-model-async=y,component-model-more-async-builtins=y",
            "-S",
            "p3=y",
            module.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait wasmtime");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // A wasmtime without the flags/feature refuses at the CLI or the
    // validator — that is an environment gap, not a product failure.
    if !out.status.success()
        && (stderr.contains("unexpected argument")
            || stderr.contains("unknown")
            || stderr.contains("requires the component model"))
    {
        eprintln!("skipping p3 execution: this wasmtime lacks the p3 async feature set");
        return None;
    }
    Some((
        String::from_utf8_lossy(&out.stdout).to_string(),
        stderr,
        out.status.code().unwrap_or(-1),
    ))
}

fn run_core(module: &Path, stdin: &str) -> (String, String, i32) {
    let mut child = Command::new("wasmtime")
        .args(["run", module.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait wasmtime");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join("almide-component-p3");
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

// The stdin family (cursor op-35 + read-to-end op-31), interpolation, and
// both output streams — the five-op host surface minus clock/entropy,
// which are covered by the abort fixture below and the full-surface
// smoke in the PR record.
const ECHO: &str = r#"import io

effect fn main() -> Unit = {
  let b = io.read_byte()
  let l = io.read_line()!
  let rest = io.read_all()
  eprintln("err-side")
  println("b=${b} l=${l} rest=${rest}")
}
"#;

const ECHO_STDIN: &str = "Xline-one\nrest-of-it";

// Clock + a `!` unwrap abort: the trap→exit rewrite must land exit code 1
// through `wasi:cli/exit@0.3.0` exactly as p1/p2 spell it.
const ABORT: &str = r#"import datetime

effect fn main() -> Unit = {
  let t = datetime.now()
  if t > 0 then println("clock ok") else println("clock BAD")
  let xs: List[Int] = []
  println(int.to_string(list.get(xs, 5)!))
}
"#;

#[test]
fn p3_component_emits_and_matches_core() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("echo.almd");
    std::fs::write(&src, ECHO).expect("write");
    let core = d.join("echo_core.wasm");
    let p3 = d.join("echo_p3.wasm");
    build_core(&src, &core);
    let line = build_p3(&src, &p3);
    assert!(
        line.contains("WASI 0.3 component (direct, async ABI)"),
        "the p3 build must name its leg:\n{line}"
    );
    // Component layer marker (bytes 6..8: layer 1 = component).
    let bytes = std::fs::read(&p3).expect("read");
    assert_eq!(&bytes[6..8], &[1, 0], "not a component-layer artifact");

    if !wasmtime_available() {
        return;
    }
    let (core_out, core_err, core_code) = run_core(&core, ECHO_STDIN);
    assert_eq!(core_out, "b=88 l=line-one rest=rest-of-it\n");
    assert_eq!(core_code, 0);
    let Some((p3_out, p3_err, p3_code)) = run_p3(&p3, ECHO_STDIN) else {
        return;
    };
    assert_eq!(p3_out, core_out, "p3 stdout diverged from the core module");
    assert_eq!(p3_err, core_err, "p3 stderr diverged from the core module");
    assert_eq!(p3_code, core_code, "p3 exit code diverged");
}


// fan.* on the p3 component (#1628 stage 2, the deterministic half): the
// C-004 contract's combinators — list-order map, first-success any,
// settle — must answer byte-identically through the async canonical ABI.
// Deterministic fan needs NO overlap machinery (arms evaluate in list
// order by the language's own semantics); the async-I/O overlap half
// arrives with an async import surface (wasi:http p3) and rides the SAME
// plumbing this pins.
const FAN: &str = r#"effect fn dbl(x: Int) -> Result[Int, String] = ok(x * 2)

effect fn main() -> Unit = {
  let doubled = fan.map([1, 2, 3, 4], (x) => dbl(x))!
  println(doubled |> list.map((x) => int.to_string(x)) |> list.join(","))
  let first = fan.any([9, 10], (x) => dbl(x)) ?? -1
  println(int.to_string(first))
}
"#;

#[test]
fn p3_component_runs_fan_deterministically() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("fan.almd");
    std::fs::write(&src, FAN).expect("write");
    let core = d.join("fan_core.wasm");
    let p3 = d.join("fan_p3.wasm");
    build_core(&src, &core);
    build_p3(&src, &p3);
    if !wasmtime_available() {
        return;
    }
    let (core_out, _, core_code) = run_core(&core, "");
    assert_eq!(core_out, "2,4,6,8\n18\n");
    assert_eq!(core_code, 0);
    let Some((p3_out, _, p3_code)) = run_p3(&p3, "") else {
        return;
    };
    assert_eq!(p3_out, core_out, "fan output diverged on the p3 leg");
    assert_eq!(p3_code, core_code);
}

#[test]
fn p3_component_abort_answers_exit_one() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("abort.almd");
    std::fs::write(&src, ABORT).expect("write");
    let p3 = d.join("abort_p3.wasm");
    build_p3(&src, &p3);
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3(&p3, "") else {
        return;
    };
    assert_eq!(out, "clock ok\n");
    assert!(err.contains("Error: none"), "abort message missing:\n{err}");
    assert_eq!(code, 1, "the abort must answer exit 1 on the p3 leg");
}
