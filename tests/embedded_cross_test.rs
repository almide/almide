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
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

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
    let mut refused: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("spec/embedded_cross exists") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("almd") {
            continue;
        }
        // A fixture that never returns fails here by name (120 s per run,
        // build included) instead of holding the job until its timeout.
        let run = |extra: &[&str]| {
            let mut c = Command::new(almide_bin());
            c.arg("run").arg(&path).args(extra);
            capped_run::output_capped(c, Duration::from_secs(120))
                .unwrap_or_else(|e| panic!("{} {:?}: {e}", path.display(), extra))
        };
        // #4350: a fixture that does not check fails the same way on both
        // legs (empty stdout, exit 1), so the comparison below would pass
        // and prove nothing. Check it first: it must check clean unless its
        // own header declares it a compile-error fixture (`// @check-error:`).
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut check = Command::new(almide_bin());
        check.arg("check").arg(&path);
        let checked_out = capped_run::output_capped(check, Duration::from_secs(120))
            .unwrap_or_else(|e| panic!("{} check: {e}", path.display()));
        let name = path.file_name().expect("name").to_string_lossy().to_string();
        match (checked_out.status.success(), declares_check_error(&source)) {
            (false, false) => {
                refused.push(format!(
                    "{name}: `almide check` exits {:?} and the file does not declare `// @check-error:`\n  {}",
                    checked_out.status.code(),
                    String::from_utf8_lossy(&checked_out.stderr).lines().chain(String::from_utf8_lossy(&checked_out.stdout).lines()).take(3).collect::<Vec<_>>().join("\n  "),
                ));
                continue;
            }
            (true, true) => {
                refused.push(format!("{name}: declares `// @check-error:` but `almide check` passes"));
                continue;
            }
            _ => {}
        }
        let native = run(&[]);
        let wasm = run(&["--target", "wasm"]);
        if native.stdout != wasm.stdout || native.status.code() != wasm.status.code() {
            diverged.push(format!(
                "{}: native exit={:?} vs wasm exit={:?}\n  native: {}\n  wasm:   {}",
                path.file_name().expect("name").to_string_lossy(),
                native.status.code(),
                wasm.status.code(),
                first_difference(&native.stdout, &wasm.stdout).0,
                first_difference(&native.stdout, &wasm.stdout).1,
            ));
        }
        checked += 1;
    }
    assert!(checked > 0, "no fixtures found — the net went blind");
    assert!(
        refused.is_empty(),
        "{} embedded-lane fixture(s) do not check as they declare (#4350):\n{}",
        refused.len(),
        refused.join("\n")
    );
    assert!(
        diverged.is_empty(),
        "{} embedded-lane fixture(s) diverge from native:\n{}",
        diverged.len(),
        diverged.join("\n")
    );
}

/// The first line on which two outputs differ (a missing line reads as
/// `<end>`), so a divergence past line 1 names itself.
fn first_difference(a: &[u8], b: &[u8]) -> (String, String) {
    let (a, b) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    let (mut la, mut lb) = (a.lines(), b.lines());
    loop {
        match (la.next(), lb.next()) {
            (Some(x), Some(y)) if x == y => continue,
            (None, None) => return (String::new(), String::new()),
            (x, y) => return (x.unwrap_or("<end>").to_string(), y.unwrap_or("<end>").to_string()),
        }
    }
}

/// A compile-error fixture says so in its own header: a `// @check-error:`
/// line naming the error it expects (#4350). Read from the file, never from a
/// list of names here.
fn declares_check_error(source: &str) -> bool {
    source.lines().any(|l| l.trim_start().starts_with("// @check-error:"))
}
