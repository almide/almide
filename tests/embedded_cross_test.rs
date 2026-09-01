//! The native⇄EMBEDDED-lane cross net (#1710 increment 1). The stock
//! sweeps (`spec/wasm_cross` + the WASI/browser gates) cannot cover a
//! host op the p1 shim does not serve — the build-path op audit refuses
//! those artifacts by design. This net covers the lane those fns DO run
//! on: every `spec/embedded_cross/*.almd` fixture runs `almide run`
//! native and `almide run --target wasm` (the embedded almide.* host)
//! and must be byte-identical — stdout AND exit code. Deterministic by
//! construction: the fixtures probe closed ports and invalid names, no
//! live network.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().expect("utf8 path").to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[cfg_attr(debug_assertions, ignore = "embedded-cross sweep is release-only (CI: release-shape job)")]
#[test]
fn embedded_lane_fixtures_match_native_byte_for_byte() {
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the embedded lane cannot run; failing (CI installs it)");
        panic!("wasmtime required");
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross");
    let mut checked = 0usize;
    let mut diverged: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("spec/embedded_cross exists") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("almd") {
            continue;
        }
        let run = |extra: &[&str]| {
            let mut c = Command::new(almide_bin());
            c.arg("run").arg(&path).args(extra);
            // The availability matrix declares these fns by their BUILD
            // verdict; the embedded lane is the one being measured here.
            c.env("ALMIDE_NO_AVAIL_CHECK", "1");
            c.output().expect("almide runs")
        };
        let native = run(&[]);
        let wasm = run(&["--target", "wasm"]);
        if native.stdout != wasm.stdout || native.status.code() != wasm.status.code() {
            diverged.push(format!(
                "{}: native exit={:?} vs wasm exit={:?}\n  native: {}\n  wasm:   {}",
                path.file_name().expect("name").to_string_lossy(),
                native.status.code(),
                wasm.status.code(),
                String::from_utf8_lossy(&native.stdout).lines().next().unwrap_or(""),
                String::from_utf8_lossy(&wasm.stdout).lines().next().unwrap_or(""),
            ));
        }
        checked += 1;
    }
    assert!(checked > 0, "no fixtures found — the net went blind");
    assert!(
        diverged.is_empty(),
        "{} embedded-lane fixture(s) diverge from native:\n{}",
        diverged.len(),
        diverged.join("\n")
    );
}
