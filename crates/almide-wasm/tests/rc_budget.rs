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
const BUMP_ONLY: bool = true;

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

/// Anti-vacuous twin (W-8: "小さい予算では OOM する対のケース"): under a
/// budget smaller than even the fixed runtime preamble + one round's
/// live set, the defined OOM fires REGARDLESS of reclamation — if this
/// ever passes, the budget knob itself stopped working.
#[cfg_attr(debug_assertions, ignore = "budget sweep is release-only (CI: release-shape job)")]
#[test]
fn tiny_budget_always_ooms() {
    let bytes = emit();
    let r = run_wasm_capped(&bytes, 128 * 1024).expect("engine");
    assert_eq!(r.exit, 1, "128 KiB can never hold the churn; got exit {}", r.exit);
    assert!(r.stderr.contains("Error: out of memory"), "got: {}", r.stderr);
}
