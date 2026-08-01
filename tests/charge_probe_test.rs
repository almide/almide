//! Stage 1 charge-trace preservation gate (ALMIDE_FUEL_PROBE).
//!
//! Three layers, per research/spike/charge-probe/REPORT.md:
//!   1. DYNAMIC: run each fixture on BOTH targets with the probe env and
//!      compare the full triple (stdout, consumed, trace_hash). The trace is
//!      order-sensitive, so a dropped, duplicated, or reordered charge on
//!      either leg diverges here.
//!   2. STATIC: render both legs in-process and compare the FIRST-OCCURRENCE
//!      charge-site sequences extracted from the artifacts (the certificate
//!      form — survives legitimate BCE body duplication, catches drops and
//!      reorders at emit time, before anything runs).
//!   3. WALL HONESTY: a fixture whose native leg walls must FAIL LOUDLY under
//!      the probe (the silent v0-fallback miss the spike discovered), never
//!      silently report nothing.
//!
//! The probe env is passed to SUBPROCESSES only (layer 1/3) or set in this
//! test binary's own process (layer 2) — integration tests are separate
//! processes, so no other test binary observes it.

use std::path::{Path, PathBuf};
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

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("research/spike/charge-probe/fixtures")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// (stdout, probe line) from one probed run; None probe if absent.
fn probed_run(fixture: &Path, wasm: bool) -> (bool, String, Option<String>) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(fixture);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    cmd.env("ALMIDE_FUEL_PROBE", "1");
    let out = cmd.output().expect("failed to spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let probe = stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("__ALMD_PROBE "))
        .map(|l| l.trim().to_string());
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        probe,
    )
}

/// The fixtures whose native leg renders on the v1 trust spine (comparable),
/// asserted MATCH in the spike. `list`/`bce` wall natively — layer 3 covers them.
const COMPARABLE: &[&str] =
    &["loop", "recursion", "branch", "strings", "mutual", "switch", "fusion", "nested"];
const NATIVE_WALLED: &[&str] = &["list", "bce"];

/// ONE combined test: the in-process renders need the probe env var, and
/// `set_var` is unsafe under concurrent threads (edition 2024) — a single
/// test fn keeps this binary effectively single-threaded while it is set.
#[test]
fn charge_probe_gate() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // reading the environment while it is written.
    unsafe { std::env::set_var("ALMIDE_FUEL_PROBE", "1") };
    static_certificate_first_occurrence_equality();
    native_wall_fails_loudly_under_probe();
    dynamic_three_point_comparison();
    bounded_deterministic_across_targets();
}

/// Stage 2: `fan.bounded` — result equality WITHOUT the probe (the shipped
/// semantics), probe-triple equality WITH it, and the deterministic budget
/// boundary: heavy(1000) costs exactly 1002 charge units (entry + 1001 loop
/// heads), so `compute.us(1001)` exhausts and `compute.us(1002)` succeeds —
/// at the SAME point on both targets. That flip is the Stage 2 claim.
fn bounded_deterministic_across_targets() {
    if !wasmtime_available() {
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    let dir = fixtures_dir();
    for name in ["bounded", "boundary"] {
        let fixture = dir.join(format!("{name}.almd"));
        // Plain runs (no probe env): the user-facing semantics.
        let plain = |wasm: bool| {
            let mut cmd = Command::new(almide_bin());
            cmd.arg("run").arg(&fixture);
            cmd.env_remove("ALMIDE_FUEL_PROBE");
            if wasm {
                cmd.args(["--target", "wasm"]);
            }
            let out = cmd.output().expect("spawn almide");
            assert!(out.status.success(), "{name}: run failed ({})", if wasm { "wasm" } else { "native" });
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let n = plain(false);
        let w = plain(true);
        assert_eq!(n, w, "{name}: bounded outputs diverged across targets");
        // Probed runs: the (consumed, trace) pair must also agree.
        let (n_ok, _, n_probe) = probed_run(&fixture, false);
        let (w_ok, _, w_probe) = probed_run(&fixture, true);
        assert!(n_ok && w_ok, "{name}: probed run failed");
        assert_eq!(n_probe, w_probe, "{name}: probe triple diverged over bounded");
    }
    // The flip point itself: EXHAUST through us=1001, OK from us=1002.
    let out = {
        let mut cmd = Command::new(almide_bin());
        cmd.arg("run").arg(dir.join("boundary.almd"));
        cmd.env_remove("ALMIDE_FUEL_PROBE");
        String::from_utf8_lossy(&cmd.output().unwrap().stdout).to_string()
    };
    assert!(out.contains("10011"), "us=1001 must exhaust (flag 1)");
    assert!(out.contains("10020"), "us=1002 must succeed (flag 0)");
    assert!(!out.contains("10021"), "us=1002 must not exhaust");
}

fn dynamic_three_point_comparison() {
    if !wasmtime_available() {
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    let dir = fixtures_dir();
    for name in COMPARABLE {
        let fixture = dir.join(format!("{name}.almd"));
        let (n_ok, n_out, n_probe) = probed_run(&fixture, false);
        let (w_ok, w_out, w_probe) = probed_run(&fixture, true);
        assert!(n_ok, "{name}: native run failed");
        assert!(w_ok, "{name}: wasm run failed");
        let n_probe = n_probe.unwrap_or_else(|| panic!("{name}: native probe line missing"));
        let w_probe = w_probe.unwrap_or_else(|| panic!("{name}: wasm probe line missing"));
        assert_eq!(n_out, w_out, "{name}: stdout diverged");
        assert_eq!(
            n_probe, w_probe,
            "{name}: charge-trace preservation FALSIFIED — (consumed, trace) diverged"
        );
    }
}

fn native_wall_fails_loudly_under_probe() {
    let dir = fixtures_dir();
    for name in NATIVE_WALLED {
        let fixture = dir.join(format!("{name}.almd"));
        let (ok, _out, probe) = probed_run(&fixture, false);
        assert!(
            !ok,
            "{name}: expected the probe to REFUSE the v0 fallback, but the run succeeded \
             (a silent unmeasured run is the exact miss the probe exists to prevent)"
        );
        assert!(probe.is_none(), "{name}: a walled run must not emit a probe line");
    }
}

fn static_certificate_first_occurrence_equality() {
    let dir = fixtures_dir();
    for name in COMPARABLE {
        let source = std::fs::read_to_string(dir.join(format!("{name}.almd"))).unwrap();
        let self_modules = almide_mir::pipeline::bundled_self_modules(&source);
        let wat = almide_mir::pipeline::try_render_wasm_source(&source, &self_modules, false)
            .unwrap_or_else(|e| panic!("{name}: wasm render failed: {e:?}"));
        let rs = almide_mir::pipeline::try_render_rust_source(&source)
            .unwrap_or_else(|e| panic!("{name}: native render failed: {e:?}"));
        let w_sites =
            almide_mir::charge_probe::first_occurrences(&almide_mir::charge_probe::wasm_charge_sites(&wat));
        let n_sites = almide_mir::charge_probe::first_occurrences(
            &almide_mir::charge_probe::native_charge_sites(&rs),
        );
        assert!(!w_sites.is_empty(), "{name}: no charges reached the wasm artifact");
        // The wasm leg links self-hosted runtime fns lowered THROUGH the same
        // charge-bearing path only for user fns; native meters user fns only.
        // The preserved claim is over the COMMON (user-fn) sites: every native
        // site must appear in wasm in the same first-occurrence order.
        let w_common: Vec<u32> =
            w_sites.iter().copied().filter(|s| n_sites.contains(s)).collect();
        assert_eq!(
            n_sites, w_common,
            "{name}: static charge certificate FALSIFIED — user-fn site order diverged"
        );
    }
}
