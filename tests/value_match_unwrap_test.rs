//! #1421: the value-position scalar match with a propagating (`!`) arm
//! once made `lower_scalar_match_operand` emit INVALID wasm (an i32
//! Result block where the merge expects i64 — wasmtime: "type mismatch").
//! The A/B probe at closure (2026-08-28, guard lifted vs intact) showed
//! the guarded branch no longer fires on ANY constructible surface form:
//! every route now either lowers the shape correctly or DECLINES to the
//! honest wall — which is exactly the issue's stated exit ("lower the
//! propagation arm correctly or DECLINE it").
//!
//! This test pins that end state: the `?? f(..)!` forms (bind and value
//! position) and the hand-written ARG-position match with a `!` arm run
//! correctly on native and on the wasm leg.

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

// let-bound `??` with a propagating fallback (the filing's repro).
const QQ_PROP: &str = "effect fn pick_int() -> Result[String, String] = {\n  let n = int.parse(\"x\") ?? int.parse(\"7\")!\n  ok(int.to_string(n))\n}\n\neffect fn main() -> Unit = {\n  println(pick_int() ?? \"ERR\")\n}\n";

// The same `??`, value position (an argument, no let).
const QQ_VALUE: &str = "effect fn show() -> Result[String, String] = {\n  ok(int.to_string(int.parse(\"x\") ?? int.parse(\"7\")!))\n}\n\neffect fn main() -> Unit = {\n  println(show() ?? \"ERR\")\n}\n";

// The desugared match form, let-bound.
const MATCH_LET: &str = "effect fn pick_int() -> Result[String, String] = {\n  let n = match int.parse(\"x\") { ok(v) => v, err(_) => int.parse(\"7\")! }\n  ok(int.to_string(n))\n}\n\neffect fn main() -> Unit = {\n  println(pick_int() ?? \"ERR\")\n}\n";

// The match in true VALUE position (an argument) — the shape whose
// (retired) incumbent-leg lowering was the invalid emit.
const MATCH_ARG: &str = "effect fn pick() -> Result[String, String] = {\n  ok(int.to_string(match int.parse(\"x\") { ok(v) => v, err(_) => int.parse(\"7\")! }))\n}\n\neffect fn main() -> Unit = {\n  println(pick() ?? \"ERR\")\n}\n";

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("almide-value-match-unwrap");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write");
    p
}

fn run_native(src: &Path) -> (bool, String) {
    let o = Command::new(almide_bin()).args(["run", src.to_str().unwrap()]).output().expect("spawn");
    (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string())
}

fn run_wasm(src: &Path) -> (bool, String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["run", src.to_str().unwrap(), "--target", "wasm"]);
    let o = cmd.output().expect("spawn");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[test]
fn propagating_fallback_runs_on_every_leg() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    for (name, src) in
        [("qq_prop.almd", QQ_PROP), ("qq_value.almd", QQ_VALUE), ("match_let.almd", MATCH_LET)]
    {
        let p = write(name, src);
        let (ok, out) = run_native(&p);
        assert!(ok, "{name}: native run failed");
        assert_eq!(out, "7\n", "{name}: native output");
        if !wasmtime_available() {
            continue;
        }
        let (ok, out, err) = run_wasm(&p);
        assert!(ok, "{name}: wasm run failed:\n{err}");
        assert_eq!(out, "7\n", "{name}: wasm output");
    }
}

#[test]
fn arg_position_match_runs_correctly() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let p = write("match_arg.almd", MATCH_ARG);
    let (ok, out) = run_native(&p);
    assert!(ok);
    assert_eq!(out, "7\n");
    if !wasmtime_available() {
        return;
    }
    let (ok, out, err) = run_wasm(&p);
    assert!(ok, "wasm leg failed:\n{err}");
    assert_eq!(out, "7\n");
}
