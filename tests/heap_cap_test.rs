//! #1530 (attack-list A1-5, the grain makeGcProgram model): the heap-cap
//! knob — `almide build --heap-cap <bytes>` — turns a silent leak into a
//! deterministic OOM at the boundary. On wasm the ceiling sits on the bump
//! FRONTIER (peak arena footprint: free-list reuse never moves it, a dropped
//! rc_dec starves reuse and the frontier climbs); on native it sits on LIVE
//! bytes in a counting global allocator. Both exceedances are the DEFINED
//! abort — "Error: out of memory" on stderr, exit 1 — the same shape as real
//! memory exhaustion (C-197).
//!
//! Three layers:
//!   1. CHURN CORPUS UNDER THE CAP: the RC-churn spec fixtures render with a
//!      generous ceiling and run byte-identical to their uncapped renders —
//!      the knob observes, it never perturbs.
//!   2. THE HARNESS BITES (the #1487 kill-evidence discipline, built in):
//!      delete one `(call $rc_dec …)` line from the rendered churn probe —
//!      the exact mutant class the RC-placement snapshots pin, and a leak
//!      RUNTIME OUTPUT CANNOT SEE (every value stays correct; only the free
//!      schedule breaks). Under the same cap the healthy module passes, and
//!      at least one such mutant must hit the deterministic OOM.
//!   3. NATIVE ENFORCEMENT: a `--heap-cap 1` native binary dies with the
//!      defined abort before doing anything, proving the native leg's
//!      allocator ceiling is live, with the same message and exit code as
//!      the wasm leg.

use std::path::{Path, PathBuf};
use std::process::Command;

/// 20k iterations of allocate-use-drop: strings and lists born and freed
/// every round, so the healthy steady-state footprint is tiny while a leaked
/// block per iteration accumulates past any reasonable ceiling fast.
const CHURN_PROBE: &str = r#"effect fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 20000 {
    let s = "x" + int.to_string(i)
    let xs = [i, i + 1, i + 2]
    acc = acc + string.len(s) + list.len(xs)
    i = i + 1
  }
  println(int.to_string(acc))
}
"#;
const CHURN_EXPECTED: &str = "168890";

/// 256 KiB: an order of magnitude above the probe's healthy peak (measured
/// well under 3 KiB of steady state at the CLI during #1530 bring-up) and an
/// order of magnitude below what one leaked block per iteration accumulates
/// (~20k blocks), so neither side of the gate sits near the boundary.
const PROBE_CAP: u32 = 256 * 1024;

/// Generous corpus ceiling: none of the RC-churn fixtures comes near 8 MiB;
/// the corpus run asserts the knob's PRESENCE changes nothing, not a bound.
const CORPUS_CAP: u32 = 8 * 1024 * 1024;

/// The RC-churn slice of the cross-target corpus — fixtures whose whole
/// point is allocate/release cycling, i.e. where a leak would live.
const CHURN_CORPUS: &[&str] = &[
    "rc_alloc_stress",
    "rc_reclaim_churn",
    "loop_outer_inplace_mutate_rc",
    "string_passthrough_share",
    "ref_roc_shared_cow",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = repo_root().join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn render_with_cap(source: &str, cap: u32) -> String {
    let _cap = (cap > 0).then(|| almide_mir::heap_cap::HeapCapGuard::set(cap));
    let modules = almide_mir::pipeline::bundled_self_modules(source);
    almide_mir::pipeline::try_render_wasm_source(source, &modules, false)
        .expect("churn fixtures render on the v1 leg")
}

/// (exit code, stdout, stderr) of one wasmtime run of a WAT text.
fn run_wat(dir: &Path, name: &str, wat: &str) -> (i32, String, String) {
    let path = dir.join(name);
    std::fs::write(&path, wat).expect("write wat");
    let out = Command::new("wasmtime").arg(&path).output().expect("spawn wasmtime");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn churn_corpus_runs_identically_under_the_cap() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    for name in CHURN_CORPUS {
        let src_path = repo_root().join(format!("spec/wasm_cross/{name}.almd"));
        let source = std::fs::read_to_string(&src_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", src_path.display()));
        let plain = render_with_cap(&source, 0);
        let capped = render_with_cap(&source, CORPUS_CAP);
        assert_ne!(plain, capped, "{name}: the cap must actually render into the module");
        let (code_p, out_p, _) = run_wat(dir.path(), &format!("{name}-plain.wat"), &plain);
        let (code_c, out_c, err_c) = run_wat(dir.path(), &format!("{name}-capped.wat"), &capped);
        assert_eq!(code_p, 0, "{name}: uncapped run failed");
        assert_eq!(
            code_c, 0,
            "{name}: capped run failed under a generous {CORPUS_CAP}-byte ceiling: {err_c}"
        );
        assert_eq!(out_p, out_c, "{name}: the cap perturbed observable output");
    }
}

#[test]
fn a_dropped_rc_dec_meets_the_cap_as_deterministic_oom() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let healthy = render_with_cap(CHURN_PROBE, PROBE_CAP);

    let (code, out, err) = run_wat(dir.path(), "healthy.wat", &healthy);
    assert_eq!(code, 0, "healthy probe must pass under the cap: {err}");
    assert_eq!(out, CHURN_EXPECTED, "healthy probe output");

    // Every `(call $rc_dec …)` line is a candidate leak site: removing the
    // whole line is stack-neutral (the operand goes with the call), so most
    // mutants still validate — and each one is a compiler bug shape that
    // keeps every printed value CORRECT while never freeing one block per
    // pass. The gate: at least one such mutant must die on the defined OOM
    // under the SAME cap the healthy module passes.
    let lines: Vec<&str> = healthy.lines().collect();
    let mut candidates = 0;
    let mut oom_hits = 0;
    for (i, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("(call $rc_dec") {
            continue;
        }
        let mutant: String = lines
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, l)| format!("{l}\n"))
            .collect();
        // A removal that breaks the module (an unfolded operand left behind)
        // is not a runnable leak — skip it, it is not this gate's subject.
        let Ok(bytes) = wat::parse_str(&mutant) else { continue };
        if wasmparser::validate(&bytes).is_err() {
            continue;
        }
        candidates += 1;
        let (code, _out, err) = run_wat(dir.path(), &format!("mutant-{i}.wat"), &mutant);
        if code == 1 && err.contains("Error: out of memory") {
            oom_hits += 1;
        }
    }
    eprintln!("rc_dec-removal mutants: {candidates} runnable, {oom_hits} deterministic OOM under the cap");
    assert!(candidates > 0, "no runnable rc_dec-removal mutants — the probe render changed shape");
    assert!(
        oom_hits > 0,
        "no rc_dec-removal mutant hit the cap: the leak harness does not bite \
         ({candidates} runnable mutants, cap {PROBE_CAP})"
    );
}

#[test]
fn native_cap_enforcement_is_live() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("probe.almd");
    std::fs::write(&src, CHURN_PROBE).expect("write probe");
    let bin = dir.path().join("probe-capped");

    let build = Command::new(almide_bin())
        .args(["build", "--heap-cap", "1"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn almide build");
    assert!(
        build.status.success(),
        "cap build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // A 1-byte ceiling cannot survive process startup: the FIRST allocation
    // past it must produce the defined abort, byte-for-byte the wasm shape.
    let run = Command::new(&bin).output().expect("spawn capped probe");
    assert_eq!(run.status.code(), Some(1), "capped native binary must exit 1");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("Error: out of memory"),
        "capped native binary must die on the defined OOM message, got: {stderr}"
    );
}
