//! The RC arc's acceptance FLOOR (#1587, W-8 doctrine), landed BEFORE
//! the mechanism: a churn program allocates ~8 KB per iteration and
//! drops every block immediately, so its LIVE set is tiny while its
//! CUMULATIVE allocation is ~90 MB.
//!
//!   - Under today's bump-only allocator the cumulative total is the
//!     footprint: the run OOMs under a 16 MiB budget — with the DEFINED
//!     C-197 surface (exit 1, "Error: out of memory"), never a raw trap.
//!   - Under a 256 MiB budget the same program completes: the small
//!     budget's failure is the budget's doing, not the program's.
//!
//! When RC + block reuse land, the 16 MiB case FLIPS to completing —
//! this test goes red exactly then, and re-ratifying `BUMP_ONLY = false`
//! turns both cases into the grain-shaped pair: fits-under-small-budget
//! (reclamation works) + must-OOM-under-tiny-budget (the gate is not
//! vacuous). The flip is the arc's acceptance event, pinned executable.

mod harness;
use almide_wasm_run::run_wasm_capped;

/// Today's memory model: bump, no reclamation. The RC arc flips this.
const BUMP_ONLY: bool = false;

const CHURN: &str = r#"fn main() -> Unit = {
  var round = 0
  var keep = 0
  while round < 10000 {
    var xs: List[Int] = []
    var i = 0
    while i < 1000 {
      list.push(xs, i)
      i = i + 1
    }
    keep = keep + xs[999]
    round = round + 1
  }
  println(int.to_string(keep))
}
"#;

fn emit() -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("rc_budget.almd", CHURN).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn churn_under_small_budget() {
    let bytes = emit();
    let r = run_wasm_capped(&bytes, 16 * 1024 * 1024).expect("engine");
    if BUMP_ONLY {
        assert_eq!(r.exit, 1, "bump-only churn must OOM under 16 MiB; got exit {}", r.exit);
        assert!(
            r.stderr.contains("Error: out of memory"),
            "OOM must be the DEFINED C-197 surface, got: {}",
            r.stderr
        );
    } else {
        // The RC acceptance shape: reclamation makes the live set fit.
        assert_eq!(r.exit, 0, "RC reclamation must fit 16 MiB: {}", r.stderr);
        assert_eq!(r.stdout, "9990000\n");
    }
}

#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn churn_under_large_budget_completes() {
    let bytes = emit();
    let r = run_wasm_capped(&bytes, 256 * 1024 * 1024).expect("engine");
    assert_eq!(r.exit, 0, "the large budget must complete: {}", r.stderr);
    assert_eq!(r.stdout, "9990000\n");
}

/// Anti-vacuous twin (W-8: "小さい予算では OOM する対のケース"): a program
/// whose LIVE set exceeds the budget must take the defined OOM
/// regardless of how good reclamation gets — nothing dead exists to
/// reclaim. (The original 128 KiB churn twin died of success: RC-5's
/// share-at-bind + class reuse fit the whole churn inside 128 KiB.)
#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn live_set_over_budget_always_ooms() {
    const LIVE: &str = r#"fn main() -> Unit = {
  var keep: List[Int] = []
  var i = 0
  while i < 4000000 {
    list.push(keep, i)
    i = i + 1
  }
  println(int.to_string(keep[0]))
}
"#;
    let ir = almide_spine::s5::lower_to_ir("rc_live.almd", LIVE).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm_capped(&bytes, 16 * 1024 * 1024).expect("engine");
    assert_eq!(r.exit, 1, "a 32 MB live list can never fit 16 MiB; got exit {}", r.exit);
    assert!(r.stderr.contains("Error: out of memory"), "got: {}", r.stderr);
}
