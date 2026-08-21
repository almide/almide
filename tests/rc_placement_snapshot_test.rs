//! #1529 (attack-list A1-4, the koka parc model): the post-RC-insertion
//! drop/dup PLACEMENT is committed as expected output BESIDE the runtime
//! result, per RC-critical fixture. Runtime output alone cannot see a
//! placement drift that only costs performance or only breaks under aliasing
//! not yet in the corpus — a moved `rc_dec` that still balances is invisible
//! to every value-comparing gate and silently changes the free schedule.
//!
//! Mechanism: render each fixture through the SAME v1 wasm pipeline the
//! backend ships, then extract every RC-relevant line from the WAT — the
//! `$rc_inc` / `$rc_dec` calls and the drop-family loops — per function, in
//! emitted order, operands included (so swapping WHICH value drops is as
//! loud as swapping WHEN). The extraction is committed under
//! `tests/rc_placement/*.snap`; a diff is a red gate.
//!
//! Regenerate (then REVIEW the diff — that review is the gate's whole
//! point): `ALMIDE_UPDATE_RC_SNAPSHOTS=1 cargo test --release -p almide
//! --test rc_placement_snapshot_test`.

use std::fmt::Write as _;
use std::path::PathBuf;

/// The RC-critical roster: the koka_parc* seed family plus the alias/share/
/// churn shapes whose whole point is reference-count placement. 21 shapes
/// (the issue's floor is 20).
const ROSTER: &[&str] = &[
    "koka_parc3_guard_chain",
    "koka_parc6_local_scrutinee",
    "koka_parc13_closure_escape",
    "koka_parc14_generic_pick",
    "koka_parc18_two_scrutinees",
    "koka_parc21_eval_order",
    "koka_parcleak1_half_consumed",
    "alias_combinator_rc",
    "assign_alias_rc",
    "list_heapelem_rc",
    "loop_outer_inplace_mutate_rc",
    "nested_unwrap_or_share",
    "r5_wasm_interp_alias_rc",
    "rc_alloc_stress",
    "rc_reclaim_churn",
    "ref_roc_shared_cow",
    "spread_record_share",
    "string_passthrough_share",
    "binary_search_duplicate_keys",
    "gleam_bool_shortcircuit",
    "compound_eq",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every RC-relevant line of one WAT module, grouped per function in emitted
/// order. A line is RC-relevant iff it mentions the refcount prims or a
/// drop-family symbol; whitespace is normalized so indentation churn is not
/// placement churn.
fn extract_rc_placement(wat: &str) -> String {
    let mut out = String::new();
    let mut current_fn: Option<&str> = None;
    let mut wrote_header = false;
    for line in wat.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(func $") {
            let name = rest.split([' ', '(', ')']).next().unwrap_or("?");
            current_fn = Some(name);
            wrote_header = false;
            continue;
        }
        // CALL-form lines only: comments and the runtime's own prose also
        // mention rc/drop, but placement is about who CALLS the refcount and
        // drop machinery, where. (The hand-written prelude bodies that call
        // `$rc_dec` — the drop_list loops — ride along; they are frozen by
        // the WAT prelude audit, so their lines never churn.)
        if t.starts_with(";;") {
            continue;
        }
        let rc_relevant = t.contains("(call $rc_inc")
            || t.contains("(call $rc_dec")
            || t.contains("(call $__drop")
            || t.contains("(call $drop_");
        if !rc_relevant {
            continue;
        }
        if let Some(f) = current_fn {
            if !wrote_header {
                let _ = writeln!(out, "fn ${f}");
                wrote_header = true;
            }
            let _ = writeln!(out, "  {t}");
        }
    }
    out
}

#[test]
fn rc_placement_matches_committed_snapshots() {
    let root = repo_root();
    let snap_dir = root.join("tests/rc_placement");
    let update = std::env::var("ALMIDE_UPDATE_RC_SNAPSHOTS").is_ok_and(|v| v == "1");
    if update {
        std::fs::create_dir_all(&snap_dir).expect("mkdir snapshots");
    }

    let mut failures: Vec<String> = Vec::new();
    for name in ROSTER {
        let src_path = root.join(format!("spec/wasm_cross/{name}.almd"));
        let source = std::fs::read_to_string(&src_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", src_path.display()));
        let modules = almide_mir::pipeline::bundled_self_modules(&source);
        let wat = match almide_mir::pipeline::try_render_wasm_source(&source, &modules, false) {
            Ok(w) => w,
            Err(e) => {
                // A walled fixture has no placement to pin — but a fixture
                // that STOPS rendering is itself a loud change, so the wall
                // reason becomes the snapshot body.
                format!("WALLED: {e}\n")
            }
        };
        let got = if wat.starts_with("WALLED:") {
            wat
        } else {
            extract_rc_placement(&wat)
        };

        let snap_path = snap_dir.join(format!("{name}.snap"));
        if update {
            std::fs::write(&snap_path, &got).expect("write snapshot");
            continue;
        }
        let want = std::fs::read_to_string(&snap_path).unwrap_or_default();
        if got != want {
            failures.push(format!(
                "{name}: RC placement drifted from tests/rc_placement/{name}.snap \
                 (regen with ALMIDE_UPDATE_RC_SNAPSHOTS=1 and REVIEW the diff)"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "RC-placement snapshot gate:\n{}",
        failures.join("\n")
    );
}
