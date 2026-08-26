//! Incremental diagnostic stability (unit 4 debt; adopted from Zig's
//! `test/incremental/` — the only such suite in the 9-compiler field).
//!
//! THE property: for every step of every edit script,
//!   diagnostics(incremental db after applying edits 1..=k)
//!     == diagnostics(fresh db given step k's text directly).
//! An incremental compiler that ever disagrees with its own from-scratch
//! answer is wrong no matter how fast it is. Each scenario also asserts the
//! incremental side actually MEMOIZED (untouched files never re-check), so
//! the equivalence is not vacuously "recompute everything".
//!
//! Scenarios cover the classic incremental bug classes: error introduced →
//! fixed → reintroduced (state-leak detector across the env-template clone),
//! span-only shifts (line numbers must UPDATE — stale-span detector),
//! decl add/remove, parse-error recovery, import-set changes (template-cache
//! key switching), and a neighbor file that must stay untouched throughout.

use almide_spine::s3::{check_file_json_v3, CheckOutput, FILE_CHECK_EXECUTIONS};
use almide_spine::{SourceFile, SpineDb};
use salsa::Setter;
use std::sync::atomic::Ordering;

fn fresh(text: &str) -> CheckOutput {
    let db = SpineDb::default();
    let f = SourceFile::new(&db, "scenario.almd".to_string(), text.to_string());
    check_file_json_v3(&db, f)
}

/// Run one edit script: apply each step to a persistent db (with an
/// untouched neighbor file alongside) and compare against a from-scratch
/// answer at every step.
fn run_scenario(name: &str, steps: &[&str]) {
    let mut db = SpineDb::default();
    let neighbor = SourceFile::new(
        &db,
        "neighbor.almd".to_string(),
        "fn neighbor_ok() -> Int = 40 + 2\n".to_string(),
    );
    let file = SourceFile::new(&db, "scenario.almd".to_string(), steps[0].to_string());
    let _ = check_file_json_v3(&db, neighbor);

    for (k, text) in steps.iter().enumerate() {
        if k > 0 {
            file.set_text(&mut db).to(text.to_string());
        }
        FILE_CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        let inc = check_file_json_v3(&db, file);
        let ran = FILE_CHECK_EXECUTIONS.load(Ordering::Relaxed);
        // Memoization witness: the edited file re-checks (k>0), the neighbor
        // never does.
        assert!(ran <= 1, "{name} step {k}: {ran} checks ran (expected <=1)");
        FILE_CHECK_EXECUTIONS.store(0, Ordering::Relaxed);
        let _ = check_file_json_v3(&db, neighbor);
        assert_eq!(
            FILE_CHECK_EXECUTIONS.load(Ordering::Relaxed),
            0,
            "{name} step {k}: neighbor re-checked despite being untouched"
        );

        let scratch = fresh(text);
        assert_eq!(
            inc, scratch,
            "{name} step {k}: incremental result diverged from from-scratch"
        );
    }
}

fn error_introduced_fixed_reintroduced() {
    // State-leak detector: the same error must appear, vanish, and reappear
    // identically across template-clone reuse.
    let ok = "fn f() -> Int = 1 + 2\n";
    let bad = "fn f() -> Int = 1 + \"two\"\n";
    run_scenario("err-toggle", &[ok, bad, ok, bad, ok, bad, ok]);
}

fn span_only_shift_updates_line_numbers() {
    // The diagnostic must MOVE with the code: same error, new line number.
    let bad_l1 = "fn f() -> Int = 1 + \"two\"\n";
    let bad_l3 = "\n\nfn f() -> Int = 1 + \"two\"\n";
    run_scenario("span-shift", &[bad_l1, bad_l3, bad_l1]);
    // And the two forms must genuinely differ (line moved) — guard against a
    // span-erased diagnostic that would hide stale positions.
    assert_ne!(fresh(bad_l1), fresh(bad_l3), "line numbers must differ across the shift");
}

fn decl_added_then_removed() {
    let one = "fn f() -> Int = helper()\nfn helper() -> Int = 7\n";
    let two = "fn f() -> Int = helper()\nfn helper() -> Int = 7\nfn extra() -> Int = 8\n";
    let gone = "fn f() -> Int = helper()\n"; // helper removed → undefined fn
    run_scenario("decl-add-remove", &[one, two, one, gone, one]);
}

fn parse_error_recovery() {
    let broken = "fn f() -> Int = {\n";
    let fixed = "fn f() -> Int = { 3 }\n";
    run_scenario("parse-recovery", &[fixed, broken, fixed, broken, fixed]);
}

fn unused_variable_warning_toggles() {
    let unused = "fn f() -> Int = {\n  let x = 5\n  1\n}\n";
    let used = "fn f() -> Int = {\n  let x = 5\n  x\n}\n";
    run_scenario("unused-toggle", &[used, unused, used, unused]);
    let got = fresh(unused);
    assert!(
        got.diags.iter().any(|d| d.contains("unused")),
        "the unused-var warning must actually fire in the unused step; diags = {:?}",
        got.diags
    );
}

fn import_set_change_switches_template() {
    // Adding/removing an import changes the env-template cache key — the
    // classic place for a stale-template bug.
    let without = "fn f() -> Int = 2\n";
    let with_json = "import json\n\nfn f() -> Int = 2\n";
    run_scenario("import-switch", &[without, with_json, without, with_json]);
}

/// One sequential entry point: the executions counter and template cache are
/// process-global, so scenarios must not interleave.
#[test]
fn all_incremental_scenarios() {
    error_introduced_fixed_reintroduced();
    span_only_shift_updates_line_numbers();
    decl_added_then_removed();
    parse_error_recovery();
    unused_variable_warning_toggles();
    import_set_change_switches_template();
}
