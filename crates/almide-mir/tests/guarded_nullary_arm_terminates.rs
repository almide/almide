//! #2463: a guarded arm over a NULLARY constructor (`none`, a unit variant) made
//! `group_option_result_arms` report a rewrite whose output was identical to its
//! input on every pass after the first, and the `desugar_heap_branches` fixpoint
//! spun forever inside the compiler process (333 CPU-minutes measured; `check`
//! and `--target rust` were unaffected). The lowering must now reach a fixed
//! point on the three reported shapes — each is run on its own thread under a
//! wall-clock bound so a regression fails instead of hanging the test binary.
//! Reaching a fixed point and then WALLING (the v1 lowering declines, the CLI
//! falls back to the incumbent) is the honest outcome; only the spin is the bug.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Far above any honest lowering of a six-line program; far below "forever".
const BOUND: Duration = Duration::from_secs(20);

fn lowers_within_bound(name: &str, src: &'static str) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let r = almide_mir::pipeline::try_render_rust_source(src);
        let _ = tx.send(r.map(|_| ()).map_err(|e| e.reason().to_string()));
    });
    // A WALL is an acceptable outcome: the v1 native lowering refuses these shapes
    // today ("heap-result `match` outside the executable subset" / "custom-variant
    // statement match with an arm outside the scalar-field subset") and the CLI falls
    // back to the incumbent codegen, which runs them (spec/wasm_cross/
    // or_pattern_guarded_nullary.almd pins the output on both targets). What is NOT
    // acceptable is never answering.
    match rx.recv_timeout(BOUND) {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => assert!(
            !reason.is_empty(),
            "{name}: a refusal must name its reason so the fallback is an honest one"
        ),
        Err(_) => panic!("{name}: the lowering did not finish within {BOUND:?} — the #2463 fixpoint spin is back"),
    }
}

#[test]
fn two_guarded_arms_over_some_and_none_terminate() {
    lowers_within_bound(
        "c5",
        r#"fn main() -> Unit = {
  let r = match some(3) {
    some(_) if false => "taken",
    none if false => "taken",
    _ => "fell",
  }
  println(r)
}
"#,
    );
}

#[test]
fn or_pattern_over_some_and_none_with_a_guard_terminates() {
    lowers_within_bound(
        "c2",
        r#"fn main() -> Unit = {
  let r = match some(3) {
    some(_) | none if false => "taken",
    _ => "fell",
  }
  println(r)
}
"#,
    );
}

#[test]
fn or_pattern_over_unit_variants_with_a_guard_terminates() {
    lowers_within_bound(
        "c6",
        r#"type Color = | Red | Green | Blue
fn main() -> Unit = {
  let r = match Red {
    Red | Green if false => "taken",
    _ => "fell",
  }
  println(r)
}
"#,
    );
}

#[test]
fn fixpoint_fire_cap_is_a_bound_not_a_budget() {
    // The cap exists so a non-progressing row can never hang the compiler; it
    // must sit far above what any real body fires, or it becomes a silent
    // refusal on large programs.
    assert!(almide_mir::lower::MAX_FIXPOINT_FIRES >= 1_000);
}
