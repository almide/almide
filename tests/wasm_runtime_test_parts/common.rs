// Shared execution substrate for the `wasm_runtime_*` test binaries.
//
// Each `tests/*.rs` file is its own crate, so the binaries pull this in with
// `include!` (the repo's part-file idiom) instead of importing it: the
// compiler-binary lookup, the native / wasm run-and-capture legs, and the
// byte-compare assertions the inline regression tests are written against.
// A binary that uses only part of it carries `#![allow(dead_code)]`.
//
// Requires: wasmtime in PATH (Node.js WASI is `run_wasm`'s fallback).

use std::process::Command;
use std::path::Path;


fn almide_bin() -> String {
    // Try: ALMIDE_BIN env → cargo build output → PATH
    if let Ok(bin) = std::env::var("ALMIDE_BIN") { return bin; }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/release/almide");
    if cargo_bin.exists() { return cargo_bin.to_str().unwrap().to_string(); }
    "almide".to_string()
}


/// Compile and run an .almd program on the Rust target, return stdout.
fn run_rust(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    std::fs::write(&src_path, source).unwrap();

    let output = Command::new(almide_bin())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .expect("failed to run almide");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Rust compilation failed:\n{}", stderr);
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Compile an .almd program to WASM, run it with Node.js, return stdout.
fn run_wasm(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let wasm_path = dir.path().join("test.wasm");

    std::fs::write(&src_path, source).unwrap();

    // Compile to WASM
    let output = Command::new(almide_bin())
        .args(["build", src_path.to_str().unwrap(), "--target", "wasm", "-o", wasm_path.to_str().unwrap()])
        .output()
        .expect("failed to build WASM");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("WASM compilation failed:\n{}", stderr);
    }

    // Run with wasmtime (preferred) or Node.js WASI (fallback)
    let output = Command::new("wasmtime")
        .arg("--dir=/")
        .arg("-S")
        .arg("inherit-env=y")
        .arg(wasm_path.to_str().unwrap())
        .output();

    let output = match output {
        Ok(o) if o.status.code() != Some(127) => o, // wasmtime found
        _ => {
            // Fallback: Node.js WASI
            let js_runner = format!(r#"
const {{ readFileSync }} = require('fs');
const {{ WASI }} = require('wasi');
const wasi = new WASI({{ version: 'preview1', args: [], env: {{}} }});
const buf = readFileSync('{}');
const mod = new WebAssembly.Module(buf);
const inst = new WebAssembly.Instance(mod, wasi.getImportObject());
wasi.start(inst);
"#, wasm_path.to_str().unwrap().replace('\\', "/"));

            let js_path = dir.path().join("run.cjs");
            std::fs::write(&js_path, &js_runner).unwrap();

            Command::new("node")
                .arg(js_path.to_str().unwrap())
                .output()
                .expect("failed to run node or wasmtime")
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("WASM execution failed:\n{}", stderr);
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Assert that a program produces identical output on Rust and WASM targets.
fn assert_cross_target(source: &str) {
    // Skip if almide binary or node not available (e.g. CI without make install)
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("node").arg("--version").output().is_err() { return; }

    let rust_out = run_rust(source);
    let wasm_out = run_wasm(source);
    assert_eq!(
        rust_out, wasm_out,
        "\nCross-target mismatch!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
}

// ── List layout tests ──

/// Compile+run on the native target; return (exit_code, stdout, stderr).
/// Builds to a binary (compiler diagnostics discarded) THEN runs it, so the
/// captured stderr is the PROGRAM's runtime stderr — not the compiler's warnings
/// — matching the wasm path (build then wasmtime). Using `almide run` would mix
/// compile-time warnings into stderr and spuriously diverge from wasm.
fn run_native_capture(source: &str) -> (i32, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let bin_path = dir.path().join("test_native_bin");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        .args(["build", src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .output()
        .expect("failed to build native");
    if !build.status.success() {
        return (
            build.status.code().unwrap_or(-1),
            String::new(),
            String::from_utf8_lossy(&build.stderr).trim().to_string(),
        );
    }
    let out = Command::new(&bin_path).output().expect("failed to run native binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

/// Compile to wasm + run via wasmtime; return (exit_code, stdout, stderr).
/// `None` ONLY when wasmtime itself cannot be spawned. A guest exit of 127 is
/// an ordinary comparable observable (#991): the old `!= Some(127)` guard
/// conflated it with wasmtime-absence, and the corpus gate `return`ed green
/// mid-run on the first such fixture — discarding every remaining fixture AND
/// every failure already accumulated.
fn run_wasm_capture(source: &str) -> Option<(i32, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let wasm_path = dir.path().join("test.wasm");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        .args(["build", src_path.to_str().unwrap(), "--target", "wasm", "-o", wasm_path.to_str().unwrap()])
        .output()
        .expect("failed to build wasm");
    assert!(build.status.success(), "wasm build failed:\n{}", String::from_utf8_lossy(&build.stderr));
    match Command::new("wasmtime").arg("--dir=/").arg("-S").arg("inherit-env=y").arg(wasm_path.to_str().unwrap()).output() {
        Ok(o) => Some((
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        Err(_) => None,
    }
}
