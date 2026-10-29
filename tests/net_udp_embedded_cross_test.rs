//! UDP and name lookup on the embedded wasm lane (`almide run --target
//! wasm`) answer what native answers (#4306, ops 140..=145 over native's net
//! core): C-442 and C-443, whose fixtures are the judge's own, verbatim, in
//! spec/embedded_cross. That directory's sweep compares stdout and the exit
//! code only, so two legs that fail alike pass it; this test also requires
//! each program to run to its closing `done` line on both legs.
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

/// Each fixture finishes in seconds; 120 s per run (build included) turns a
/// hang (a datagram that never arrives) into a named failure instead of a
/// cancelled job.
fn run(path: &Path, extra: &[&str]) -> (String, Option<i32>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(path).args(extra);
    let leg = if extra.is_empty() { "native" } else { "embedded" };
    let out = capped_run::output_capped(cmd, Duration::from_secs(120))
        .unwrap_or_else(|e| panic!("{} ({leg}): {e}", path.display()));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn udp_and_lookup_fixtures_run_to_the_end_alike_on_native_and_the_embedded_lane() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross");
    for name in ["net_udp_loopback", "net_udp_errors"] {
        let path = dir.join(format!("{name}.almd"));
        let native = run(&path, &[]);
        assert!(native.0.ends_with("done\n"), "{name}: native did not finish:\n{}\n{}", native.0, native.2);
        let wasm = run(&path, &["--target", "wasm"]);
        assert!(wasm.0.ends_with("done\n"), "{name}: the embedded lane did not finish:\n{}\n{}", wasm.0, wasm.2);
        assert_eq!((&wasm.0, wasm.1), (&native.0, native.1), "{name}: the embedded lane diverged");
    }
}
