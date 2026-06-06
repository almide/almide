//! `almide run --target {rust|wasm}` CLI lock (Cluster-A item A1).
//!
//! Before this flag existed, `almide run app.almd --target wasm` SILENTLY ran
//! natively: the Run subcommand's `trailing_var_arg` swallowed `--target wasm`
//! as program args, so two independent agents' wasm checks were really native
//! runs (it masked 7+ real wasm bugs). The fix adds a real `--target` flag on
//! `run`, mirroring `cargo run` (almide's own flags are consumed before `--`;
//! program args go after `--`).
//!
//! The locks here:
//!   1. `run --target wasm` actually executes wasm — its (stdout, exit) match
//!      `build --target wasm` + the `wasmtime` CLI exactly.
//!   2. `run --target rust` ≡ default `run` (byte-identical).
//!   3. an unknown `--target` is rejected with an actionable message.
//!   4. `--target wasm` no longer leaks into `env.args()` — the documented `--`
//!      separator still forwards program args.
//!
//! Skips cleanly when the `almide` binary or `wasmtime` is unavailable.

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

fn tools_available() -> bool {
    let bin = almide_bin();
    Command::new(&bin).arg("--version").output().is_ok()
        && Command::new("wasmtime").arg("--version").output().is_ok()
}

/// (stdout, exit_code) of an `almide run …` invocation.
fn run(args: &[&str]) -> (String, i32) {
    let output = Command::new(almide_bin())
        .args(args)
        .output()
        .expect("failed to spawn almide");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

const PROG: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  let total = list.fold(xs, 0, (acc, x) => acc + x)
  println("sum=" + int.to_string(total))
  println("len=" + int.to_string(list.len(xs)))
}
"#;

#[test]
fn run_target_wasm_executes_wasm_matching_build_plus_wasmtime() {
    if !tools_available() {
        eprintln!("skip: almide or wasmtime unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("prog.almd");
    std::fs::write(&src, PROG).unwrap();
    let src_s = src.to_str().unwrap();

    // `run --target wasm` — the path under test.
    let (run_out, run_code) = run(&["run", src_s, "--target", "wasm"]);

    // Ground truth: `build --target wasm` then execute on the `wasmtime` CLI.
    let wasm = dir.path().join("prog.wasm");
    let build = Command::new(almide_bin())
        .args(["build", src_s, "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("build failed to spawn");
    assert!(build.status.success(), "build --target wasm failed: {}",
        String::from_utf8_lossy(&build.stderr));
    let wt = Command::new("wasmtime")
        .arg("--dir=/")
        .arg(wasm.to_str().unwrap())
        .output()
        .expect("wasmtime failed to spawn");
    let truth_out = String::from_utf8_lossy(&wt.stdout).to_string();
    let truth_code = wt.status.code().unwrap_or(-1);

    assert_eq!(run_out, truth_out,
        "`run --target wasm` stdout must match build+wasmtime");
    assert_eq!(run_code, truth_code,
        "`run --target wasm` exit code must match build+wasmtime");
    assert!(run_out.contains("sum=6") && run_out.contains("len=3"),
        "wasm program output unexpected: {:?}", run_out);
}

#[test]
fn run_target_rust_equals_default() {
    if !tools_available() {
        eprintln!("skip: almide unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("prog.almd");
    std::fs::write(&src, PROG).unwrap();
    let src_s = src.to_str().unwrap();

    let (default_out, default_code) = run(&["run", src_s]);
    let (rust_out, rust_code) = run(&["run", src_s, "--target", "rust"]);
    assert_eq!(default_out, rust_out, "`--target rust` must equal default run");
    assert_eq!(default_code, rust_code, "`--target rust` exit must equal default");

    // And cross-target: the wasm run must agree with native on stdout.
    if Command::new("wasmtime").arg("--version").output().is_ok() {
        let (wasm_out, _) = run(&["run", src_s, "--target", "wasm"]);
        assert_eq!(default_out, wasm_out,
            "native and wasm `run` must be byte-identical");
    }
}

#[test]
fn run_unknown_target_is_rejected_with_hint() {
    if !tools_available() {
        eprintln!("skip: almide unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("prog.almd");
    std::fs::write(&src, PROG).unwrap();

    let output = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap(), "--target", "llvm"])
        .output()
        .expect("failed to spawn almide");
    assert!(!output.status.success(), "unknown target must be a non-zero exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown run target 'llvm'"),
        "must name the bad target, got: {}", stderr);
    assert!(stderr.contains("wasm") && stderr.contains("rust"),
        "must list supported targets, got: {}", stderr);
}

#[test]
fn target_wasm_does_not_leak_into_program_args() {
    // The exact regression: `--target wasm` after the file used to be forwarded
    // to the program (`env.args()` saw `["--target", "wasm"]`). Now it is the
    // run target, and ONLY tokens after `--` reach the program.
    if !tools_available() {
        eprintln!("skip: almide unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("args.almd");
    std::fs::write(&src, "import env\neffect fn main() -> Unit = {\n  println(\"args=\" + list.join(env.args(), \",\"))\n}\n").unwrap();
    let src_s = src.to_str().unwrap();

    // `--target rust` keeps it native (env.args is a stub on wasm), and the flag
    // must NOT appear in argv.
    let (out, code) = run(&["run", src_s, "--target", "rust"]);
    assert_eq!(code, 0, "run failed: {:?}", out);
    assert!(out.contains("args=\n") || out.trim_end() == "args=",
        "--target must not leak into env.args(), got: {:?}", out);

    // The documented `--` separator still forwards real program args.
    let (out2, code2) = run(&["run", src_s, "--target", "rust", "--", "alpha", "beta"]);
    assert_eq!(code2, 0, "run with -- args failed: {:?}", out2);
    assert!(out2.contains("args=alpha,beta"),
        "program args after `--` must reach env.args(), got: {:?}", out2);
}
