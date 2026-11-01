//! The http call handle on the WASI 0.3 component (ADR-0023 §4.3, C-366):
//! the p3 shim over wasi:http's async `send` answers what native's call core
//! answers. The deterministic probes of spec/embedded_cross/http_call_handle_errs.almd
//! — malformed limits and a closed local port, no live server — are built
//! by the default `almide build --target wasm` (an http client op routes
//! there), run on a stock wasmtime with `-S http`, and compared with the
//! native run, stdout and exit code; the loopback behaviour (limits firing,
//! cancel, the streamed read, drop) is tests/http_call_handle_test.rs's p3
//! leg.
//!
//! The runtime floor is the pin policy's (wasmtime 46); below it the test
//! skips, and under ALMIDE_EXPECT_TOOLS (CI) that is a failure.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// The installed wasmtime runs p3 components (the pin policy's floor, 46).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v.split_whitespace().nth(1).and_then(|ver| ver.split('.').next()).and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
    if major < 46 {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor (46)", v.trim());
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

/// stdout, the exit code and stderr, under a 120 s cap per run (a read that
/// never returns becomes a named failure, not a cancelled job).
fn observe(mut cmd: Command, what: &str) -> (String, Option<i32>, String) {
    cmd.stdin(Stdio::null());
    let out = capped_run::output_capped(cmd, Duration::from_secs(120)).unwrap_or_else(|e| panic!("{what}: {e}"));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_call_handle_fixture_runs_alike_on_native_and_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross/http_call_handle_errs.almd");
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut native = Command::new(almide_bin());
    native.arg("run").arg(&src);
    let native = observe(native, "almide run");
    assert!(native.0.contains("stream-limits:"), "native did not finish:\n{}\n{}", native.0, native.2);
    let wasm = scratch.path().join("calls.wasm");
    let built = Command::new(almide_bin())
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "-o", wasm.to_str().expect("utf8")])
        .env_remove("ALMIDE_COMPONENT_P3")
        .stdin(Stdio::null())
        .output()
        .expect("almide build");
    assert!(built.status.success(), "the default build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    let mut run = Command::new("wasmtime");
    run.args(["run", "-S", "http"]).arg(&wasm);
    let p3 = observe(run, "wasmtime run");
    assert_eq!((&p3.0, p3.1), (&native.0, native.1), "the p3 component diverged from native\n--- p3 stderr\n{}", p3.2);
}
