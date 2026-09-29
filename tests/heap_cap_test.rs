//! #1530 (attack-list A1-5, the grain makeGcProgram model): the heap-cap
//! knob — `almide build --heap-cap <bytes>` — turns a silent leak into a
//! deterministic OOM at the boundary. On wasm the ceiling sits on the bump
//! FRONTIER (peak arena footprint: free-list reuse never moves it, a dropped
//! rc_dec starves reuse and the frontier climbs); on native it sits on LIVE
//! bytes in a counting global allocator. Both exceedances are the DEFINED
//! abort — "Error: out of memory" on stderr, exit 1 — the same shape as real
//! memory exhaustion (C-197).
//!
//! Layers:
//!   1. CHURN CORPUS UNDER THE CAP: the RC-churn spec fixtures render with a
//!      generous ceiling and run byte-identical to their uncapped renders —
//!      the knob observes, it never perturbs. (The WAT-text rc_dec mutant
//!      layer was the retired incumbent renderer's; the structural leg's
//!      mutation net lives in crates/almide-wasm.)
//!   2. NATIVE ENFORCEMENT: a `--heap-cap 1` native binary dies with the
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

/// The wasm leg's module for `source` with the cap as the memory's declared
/// maximum, in the stock-WASI form `almide build` ships.
fn render_with_cap(source: &str, cap: u32) -> Vec<u8> {
    let _cap = (cap > 0).then(|| almide_wasm::heap_cap::HeapCapGuard::set(cap));
    let ir = almide::wasm_leg::lower_to_ir("t.almd", source).expect("churn fixtures lower on the wasm leg");
    let (bytes, ops) = almide_wasm::emit_program_with_ops(&ir).expect("churn fixtures emit on the wasm leg");
    let ops: Vec<i32> = ops.iter().copied().collect();
    almide_wasm_run::wasi::to_wasi(&bytes, &ops).expect("to_wasi")
}

/// (exit code, stdout, stderr) of one wasmtime run of a module.
fn run_wat(dir: &Path, name: &str, wasm: &[u8]) -> (i32, String, String) {
    let path = dir.join(name);
    std::fs::write(&path, wasm).expect("write wasm");
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
        let (code_p, out_p, _) = run_wat(dir.path(), &format!("{name}-plain.wasm"), &plain);
        let (code_c, out_c, err_c) = run_wat(dir.path(), &format!("{name}-capped.wasm"), &capped);
        assert_eq!(code_p, 0, "{name}: uncapped run failed");
        assert_eq!(
            code_c, 0,
            "{name}: capped run failed under a generous {CORPUS_CAP}-byte ceiling: {err_c}"
        );
        assert_eq!(out_p, out_c, "{name}: the cap perturbed observable output");
    }
}

/// The C-300 leak-regression pin: multi-entry map literals whose intermediate
/// maps drop as call-argument temps (`__hvf_at`'s recursion). Before the fix,
/// the flat len-slot sweep freed only the KEY half of each intermediate's
/// 2n-slot block — (n−1) value blocks leaked per literal, output-invisible;
/// this harness caught it on its first live run. Under the ceiling, a
/// regression is a deterministic OOM, not a green run.
const MAP_LITERAL_CHURN: &str = r#"effect fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 15000 {
    let v: Option[Int] = (if i % 3 == 0 then none else some(i))
    let m = ["k0": v, "k1": some(i + 1), "k2": none]
    acc = acc + map.len(m) + (map.get_or(m, "k1", some(0)) ?? 0)
    let hv = ["a": [i], "b": [i + 1], "c": [i + 2]]
    acc = acc + map.len(hv)
    i = i + 1
  }
  println(int.to_string(acc))
}
"#;
const MAP_LITERAL_CHURN_EXPECTED: &str = "112597500";

#[test]
fn map_literal_intermediates_do_not_leak_under_the_cap() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let capped = render_with_cap(MAP_LITERAL_CHURN, PROBE_CAP);
    let (code, out, err) = run_wat(dir.path(), "map-churn.wasm", &capped);
    assert_eq!(
        code, 0,
        "map-literal churn must run flat under the {PROBE_CAP}-byte ceiling \
         (a red here is the C-300 intermediate-map leak back): {err}"
    );
    assert_eq!(out, MAP_LITERAL_CHURN_EXPECTED, "map-literal churn output");
}

/// #1857: a `let`-bound range read ONLY as a `for-in` head allocates nothing
/// on native even when the program falls off the v1 trust-spine onto the v3
/// codegen. The top-level `let` keeps the program off the v1 render (rung 1
/// has no top-level lets) — it used to be the `list.len` / index reads of
/// `idx`, until `list.range` and `list.len` joined the native floor (#1869)
/// — so `main` renders through v3 — where,
/// before the fix, `big` was materialized as a 1 GiB `Vec<i64>` (the nightly
/// fuzzer's seed 539646620663 index 1415 carried 16 GiB: 33.6 s native vs
/// 0.7 s wasm, byte-identical). Under a 4 MiB ceiling that materialization is
/// the deterministic OOM; the head-only deferral (`RangeCountingVarsPass`)
/// is what makes this a green run.
const WALLED_RANGE_PROBE: &str = r#"let bump = 0

effect fn main() -> Unit = {
  let idx = 0..<5
  println("len=" + int.to_string(list.len(idx)) + " at2=" + int.to_string(idx[2]))
  let big = 0..<134217728
  var e = 0
  for _i in big {
    e = e + 1 + bump
  }
  println("e=" + int.to_string(e))
}
"#;
const WALLED_RANGE_EXPECTED: &str = "len=5 at2=2\ne=134217728";
const WALLED_RANGE_CAP: u32 = 4 * 1024 * 1024;

#[test]
fn walled_head_only_range_allocates_nothing_natively() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("walled-range.almd");
    std::fs::write(&src, WALLED_RANGE_PROBE).expect("write probe");
    let bin = dir.path().join("walled-range-capped");

    // ALMIDE_VERIFIED_DEBUG names the wall that rerouted (the note below).
    let build = Command::new(almide_bin())
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .args(["build", "--heap-cap", &WALLED_RANGE_CAP.to_string()])
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
    let build_note = String::from_utf8_lossy(&build.stderr);
    // The probe is only a witness while it actually walls v1: if rung 1 ever
    // admits top-level lets, this program stops exercising the v3 fallback
    // and the gate must move to a shape that still does (it moved once
    // already, when `list.range` / `list.len` joined the floor — #1869).
    assert!(
        build_note.contains("verified native render walled"),
        "the walled-range probe no longer walls the v1 native render — pick a shape that does: {build_note}"
    );

    let run = Command::new(&bin).output().expect("spawn capped probe");
    let stdout = String::from_utf8_lossy(&run.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(
        run.status.code(),
        Some(0),
        "a head-only bound range must not materialize on the v3 fallback \
         (a red here is #1857 back: the 1 GiB Vec<i64> under a {WALLED_RANGE_CAP}-byte ceiling): {stderr}"
    );
    assert_eq!(stdout, WALLED_RANGE_EXPECTED, "walled-range probe output");
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
