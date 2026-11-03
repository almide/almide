//! #4376 (C-451) on the shipped artifact: `almide build --target wasm`
//! of spec/wasm_host_js/effect_export_scalar.almd, invoked export by export
//! with the stock wasmtime CLI. A scalar `effect fn` export prints its value
//! (it printed a Result block address before), and an `err` traps with its
//! message on stderr and a failing exit.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn have_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    // In CI a missing tool is a failure, never a silent pass (#985).
    assert!(ok || std::env::var("CI").as_deref() != Ok("true"), "wasmtime is not on PATH in CI");
    ok
}

fn invoke(wasm: &std::path::Path, export: &str, arg: &str) -> (bool, String, String) {
    let out = Command::new("wasmtime").args(["run", "--invoke", export]).arg(wasm).arg(arg).output().expect("wasmtime");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn the_wasmtime_cli_reads_a_scalar_effect_export_as_its_value() {
    if !have_wasmtime() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let wasm = dir.path().join("app.wasm");
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_host_js/effect_export_scalar.almd");
    let out = Command::new(almide()).args(["build", src, "--target", "wasm", "-o"]).arg(&wasm).output().expect("spawn almide");
    assert!(out.status.success(), "build failed:\n{}", String::from_utf8_lossy(&out.stderr));
    for (export, arg, want) in [("scaled", "5", "15"), ("half", "5", "2.5"), ("even", "4", "1"), ("even", "3", "0")] {
        let (ok, stdout, stderr) = invoke(&wasm, export, arg);
        assert!(ok, "{export}({arg}) failed:\n{stderr}");
        assert_eq!(stdout, want, "{export}({arg}) printed {stdout:?}, not the value");
    }
    for (export, arg) in [("scaled", "101"), ("half", "102"), ("note", "103")] {
        let (ok, _, stderr) = invoke(&wasm, export, arg);
        assert!(!ok, "{export}({arg}) erred but exited 0");
        assert!(stderr.contains(&format!("over 100: {arg}")), "{export}({arg}): stderr lacks the err's message:\n{stderr}");
    }
}
