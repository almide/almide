//! The allocation counter's own A/B (#2407): the `alloc_count` switch is an
//! instrument that must cost nothing when off and count exactly when on.
//!
//!  - OFF: the module carries no counters (`RunResult::alloc_count` is
//!    None — absent, not zero) and its bytes are those of a build that
//!    never heard of the switch (the size ratchet and the alloc ledger
//!    hold that corpus-wide; here the bytes are compared directly).
//!  - ON: the module runs to the same stdout and the same `__heap`
//!    watermark as the unarmed one, and reports its allocations.
//!  - One deliberate extra allocation moves `allocs` by exactly 1.
//!  - Churn a watermark cannot see: a loop that allocates and releases a
//!    fresh string per iteration reaches nearly the same watermark as a
//!    single allocation, while the counter reports every one of them and
//!    the free-list pops that hid them.

mod harness;
use harness::run_wasm;

fn emit(src: &str, armed: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _guard = armed.then(almide_wasm::alloc_count::CountGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

const ONE: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  println("${list.len(xs)}")
}
"#;

const TWO: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  let ys = [4, 5, 6]
  println("${list.len(xs) + list.len(ys)}")
}
"#;

const CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<1000 {
    let s = "row " + int.to_string(i)
    acc = acc + string.len(s)
  }
  println("${acc}")
}
"#;

#[test]
fn an_unarmed_module_carries_no_counters_and_no_extra_bytes() {
    let off = emit(ONE, false);
    let r = run_wasm(&off).expect("run");
    assert_eq!(r.stdout, "3\n");
    assert_eq!(r.alloc_count, None, "a shipped module has no counters — absent, not zero");
    // The guard restores the previous state on drop: a build after an
    // armed one is byte-identical to a build before it.
    let _armed = emit(ONE, true);
    assert_eq!(emit(ONE, false), off, "the switch leaves no residue in the next build");
    for name in almide_wasm::alloc_count::EXPORTS {
        assert!(
            !off.windows(name.len()).any(|w| w == name.as_bytes()),
            "the unarmed module must not spell the `{name}` export"
        );
    }
}

#[test]
fn an_armed_module_counts_without_perturbing_the_run() {
    let off = run_wasm(&emit(ONE, false)).expect("run");
    let on = run_wasm(&emit(ONE, true)).expect("run");
    assert_eq!(on.stdout, off.stdout, "the counter changes no output");
    assert_eq!(on.heap_end, off.heap_end, "the counter changes no watermark");
    let c = on.alloc_count.expect("the armed module exports the counters");
    assert_eq!((c.allocs, c.reused, c.frees), (1, 0, 1), "one list literal: one block, freed at exit");
    assert_eq!(c.bytes, 24, "three 8-byte slots requested");
}

#[test]
fn one_extra_allocation_moves_the_count_by_exactly_one() {
    let one = run_wasm(&emit(ONE, true)).expect("run").alloc_count.expect("counters");
    let two = run_wasm(&emit(TWO, true)).expect("run").alloc_count.expect("counters");
    assert_eq!(two.allocs, one.allocs + 1, "the second list literal is one more `$alloc`");
    assert_eq!(two.bytes, one.bytes + 24, "and 24 more payload bytes");
    assert_eq!(two.frees, one.frees + 1, "released at exit like the first");
    assert_eq!(two.reused, one.reused, "nothing was free to reuse between them");
}

#[test]
fn churn_the_watermark_hides_is_what_the_counter_shows() {
    let one = run_wasm(&emit(ONE, true)).expect("run");
    let churn = run_wasm(&emit(CHURN, true)).expect("run");
    assert_eq!(churn.stdout, "6890\n");
    let (w1, wc) = (one.heap_end.expect("__heap export"), churn.heap_end.expect("__heap export"));
    let c = churn.alloc_count.expect("counters");
    // 1000 iterations, two blocks each (the int's text and the concat):
    // the watermark moved by well under one iteration's worth of bytes
    // because the free lists recycled nearly every block.
    assert_eq!(c.allocs, 2000, "two allocations per iteration");
    assert_eq!(c.frees, 2000, "each released before the next iteration");
    assert!(c.reused >= 1990, "the free lists served almost all of them: reused={}", c.reused);
    assert!(c.bytes > 9000, "the bytes requested add up across the loop: bytes={}", c.bytes);
    assert!(
        wc - w1 < 256,
        "the watermark barely moved (churn {wc} vs one block {w1}): reuse hides {} allocations of {} bytes",
        c.allocs,
        c.bytes
    );
}
