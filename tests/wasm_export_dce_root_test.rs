//! Lock for #457: an exported `pub fn` that is unreachable from `main`/`_start`
//! must NOT be dead-code-eliminated to `unreachable`.
//!
//! The standard host-driven WASM pattern has the JS/host call exported callbacks
//! (`render_frame(time)`, `on_pointer_*`, any JS-called `pub fn`) that `main` never
//! calls. Before the fix, WASM DCE ran before the export set was collected and
//! seeded roots only from `main`, so such an export kept its slot in the export
//! section but had its body stubbed to `unreachable` — trapping on the first host
//! call. The fix makes exported `pub fn`s DCE roots.
//!
//! This test builds a `pub fn` unreachable from `main`, then invokes it through the
//! export with `wasmtime --invoke` and asserts it returns the right value instead
//! of trapping. Skips cleanly when `almide` or `wasmtime` is unavailable.

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

/// `wasmtime --invoke` (experimental) is required; node can't invoke a named
/// export as cleanly, so this lock is wasmtime-only and skips otherwise.
fn tools_available() -> bool {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return false;
    }
    Command::new("wasmtime").arg("--version").output().is_ok()
}

fn build_wasm(source: &str, dir: &Path) -> std::path::PathBuf {
    let src_path = dir.join("dce_export.almd");
    let wasm_path = dir.join("dce_export.wasm");
    std::fs::write(&src_path, source).unwrap();
    let output = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build WASM");
    assert!(
        output.status.success(),
        "WASM build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    wasm_path
}

/// Invoke an exported function with one i64 arg via wasmtime; return its stdout
/// (the returned value) on success, or `Err` with stderr if it trapped/failed.
fn invoke_export(wasm: &Path, func: &str, arg: i64) -> Result<String, String> {
    let out = Command::new("wasmtime")
        .args(["--invoke", func, wasm.to_str().unwrap(), &arg.to_string()])
        .output()
        .expect("failed to run wasmtime");
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[test]
fn exported_pub_fn_unreachable_from_main_is_callable() {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    // `callback` is exported but never called from `main` — the #457 repro.
    let src = r#"
pub fn callback(x: Int) -> Int = x + 1

effect fn main() -> Unit = {
  println("main ran")
}
"#;
    let dir = tempfile::tempdir().unwrap();
    let wasm = build_wasm(src, dir.path());
    match invoke_export(&wasm, "callback", 5) {
        Ok(v) => assert_eq!(v, "6", "exported callback returned the wrong value"),
        Err(e) => panic!(
            "exported `pub fn callback` trapped instead of returning 6 (#457 regression): {}",
            e
        ),
    }
}

#[test]
fn exported_pub_fn_reachable_from_main_still_works() {
    // Control: an export that IS referenced from main must also work (it always did).
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    let src = r#"
pub fn callback(x: Int) -> Int = x + 1

effect fn main() -> Unit = {
  let _ = callback(0)
  println("main ran")
}
"#;
    let dir = tempfile::tempdir().unwrap();
    let wasm = build_wasm(src, dir.path());
    match invoke_export(&wasm, "callback", 41) {
        Ok(v) => assert_eq!(v, "42", "reachable exported callback returned the wrong value"),
        Err(e) => panic!("reachable exported callback trapped: {}", e),
    }
}
