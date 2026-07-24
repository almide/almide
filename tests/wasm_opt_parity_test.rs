//! wasm-opt parity gate — the differential-testing evidence behind
//! `almide build --target wasm --wasm-opt`'s guarantee.
//!
//! `--wasm-opt` runs Binaryen's `wasm-opt` on the trust-spine-verified module
//! AFTER it's rendered — an external, unverified transform, deliberately kept
//! out of the default path (see docs/WASM-OUTPUT.md). Its own trust story is
//! NOT "wasm-opt is popular so it's fine" — it's "on every shape of wasm
//! Almide itself generates, running it changes nothing observable." This
//! test is that claim, checked mechanically: every `spec/wasm_cross/*.almd`
//! fixture is built twice (with and without `--wasm-opt`) and both builds
//! must produce byte-identical (exit code, stdout, stderr) under wasmtime.
//!
//! A failure here means `--wasm-opt` is not safe for that program shape —
//! the flag's docs and this gate must be revisited together, never just
//! silenced.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") { return bin; }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() { return cargo_bin.to_str().unwrap().to_string(); }
    "almide".to_string()
}

/// Build `src_path` to wasm (optionally with `--wasm-opt`) and run it under
/// wasmtime. Returns `None` if wasmtime is unavailable (skip, don't fail).
fn build_and_run(src_path: &Path, dir: &Path, wasm_opt: bool) -> Option<(i32, String, String)> {
    let wasm_path = dir.join(if wasm_opt { "opt.wasm" } else { "plain.wasm" });
    let mut args = vec!["build", src_path.to_str().unwrap(), "--target", "wasm", "-o", wasm_path.to_str().unwrap()];
    if wasm_opt { args.push("--wasm-opt"); }
    let build = Command::new(almide_bin()).args(&args).output().expect("failed to build wasm");
    assert!(build.status.success(), "wasm build failed ({}):\n{}", if wasm_opt { "--wasm-opt" } else { "plain" }, String::from_utf8_lossy(&build.stderr));

    match Command::new("wasmtime").arg("--dir=/").arg("-S").arg("inherit-env=y").arg(&wasm_path).output() {
        Ok(o) if o.status.code() != Some(127) => Some((
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        _ => None,
    }
}

#[test]
fn wasm_opt_parity_spec() {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("wasmtime").arg("--version").output().is_err() { return; }
    if Command::new("wasm-opt").arg("--version").output().is_err() { return; }

    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    if !spec_dir.exists() { return; }

    let mut entries: Vec<_> = std::fs::read_dir(&spec_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() { return; }

    let mut passed = 0;
    let mut failed: Vec<String> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let dir = tempfile::tempdir().unwrap();

        let plain = match std::panic::catch_unwind(|| build_and_run(&path, dir.path(), false)) {
            Ok(Some(r)) => r,
            Ok(None) => return, // wasmtime unavailable mid-run → skip the gate
            Err(_) => { failed.push(format!("{name}: plain build/run panicked")); continue; }
        };
        let opt = match std::panic::catch_unwind(|| build_and_run(&path, dir.path(), true)) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(_) => { failed.push(format!("{name}: --wasm-opt build/run panicked")); continue; }
        };

        if plain == opt {
            passed += 1;
        } else {
            let (pc, pout, perr) = &plain;
            let (oc, oout, oerr) = &opt;
            failed.push(format!(
                "{name}: wasm-opt changed observable behavior\n  plain:     exit={pc} stdout={pout:?} stderr={perr:?}\n  wasm-opt:  exit={oc} stdout={oout:?} stderr={oerr:?}"
            ));
        }
    }

    eprintln!("\nwasm_opt_parity_spec (gate): {passed} equal, {} mismatch(es)", failed.len());
    if !failed.is_empty() {
        panic!("\n{} wasm-opt parity gate problem(s):\n\n{}", failed.len(), failed.join("\n\n"));
    }
}
