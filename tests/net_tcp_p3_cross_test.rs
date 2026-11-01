//! TCP on the WASI 0.3 component (#3620, the net/tcp design step 4): the p3
//! shim over wasi:sockets@0.3 `tcp-socket` answers what native answers.
//! C-377, C-378 and C-379 promise the same transcript on native, the
//! embedded lane and the stock WASI 0.3 component; their fixtures
//! (spec/embedded_cross, the judge's own) are built as the component, run on
//! a stock wasmtime with the network granted (`-S inherit-network=y`), and
//! compared with the native run — stdout and the exit code identical, and
//! each program reaching its closing `done` line, so two legs that fail
//! alike cannot pass.
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
fn tcp_fixtures_run_alike_on_native_and_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross");
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut problems = Vec::new();
    for name in ["net_tcp_loopback", "net_tcp_errors", "net_tcp_timeouts", "net_tcp_fan_handles", "net_huge_read_lengths"] {
        let src = dir.join(format!("{name}.almd"));
        let mut native = Command::new(almide_bin());
        native.arg("run").arg(&src);
        let native = observe(native, "almide run");
        assert!(native.0.ends_with("done\n"), "{name}: native did not finish:\n{}\n{}", native.0, native.2);
        let wasm = scratch.path().join(format!("{name}.wasm"));
        let built = Command::new(almide_bin())
            .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", wasm.to_str().expect("utf8")])
            .env("ALMIDE_COMPONENT_P3", "1")
            .stdin(Stdio::null())
            .output()
            .expect("almide build");
        if !built.status.success() {
            problems.push(format!("{name}: the p3 build failed:\n{}", String::from_utf8_lossy(&built.stderr)));
            continue;
        }
        let mut run = Command::new("wasmtime");
        run.args(["run", "-S", "inherit-network=y", "-S", "tcp=y"]).arg(&wasm);
        let p3 = observe(run, "wasmtime run");
        if !p3.0.ends_with("done\n") || (&p3.0, p3.1) != (&native.0, native.1) {
            problems.push(format!(
                "{name}: the p3 component diverged from native\n--- native ({:?})\n{}--- p3 ({:?})\n{}--- p3 stderr\n{}",
                native.1, native.0, p3.1, p3.0, p3.2
            ));
        }
    }
    assert!(problems.is_empty(), "{} fixture(s) diverge:\n\n{}", problems.len(), problems.join("\n\n"));
}
