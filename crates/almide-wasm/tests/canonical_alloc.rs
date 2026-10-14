//! The canonical-allocator switch: the inspection meter's fixed allocator.
//!
//! The complexity criterion's bytes-moved series must be a function of n.
//! Under the shipped allocator it is also a function of the heap's state:
//! - a class free-list pop or a large-list take can hand a growing buffer a
//!   block with spare capacity;
//! - the large list keeps a remainder under 64 KiB on the block it hands out.
//! Either way a growth copy is skipped, and the series bends. Measured on
//! option.collect, set.map, set.from_list and list.unique.
//!
//! Under the switch, `$alloc` never reuses: every request bumps, so a
//! program's copies depend only on its requests. Reuse can only remove
//! copies, so the canonical series is an upper bound of the shipped one.
//! The approach follows the cohort's test-scoped allocators (Rust's
//! `Allocator` parameter, Roc's TestEnv); none of them measures copies under
//! a reusing allocator.
//!
//! - OFF: the module's bytes are those of a build that never heard of the
//!   switch.
//! - ON: the same stdout, and no allocation is served by reuse, whether the
//!   small-block or the large-block (> 64 KiB) free list is involved.

mod harness;
use harness::run_wasm;

fn emit(src: &str, canonical: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _count = almide_wasm::alloc_count::CountGuard::set();
    let _canonical = canonical.then(almide_wasm::canonical_alloc::CanonicalGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

fn emit_plain(src: &str, canonical: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _canonical = canonical.then(almide_wasm::canonical_alloc::CanonicalGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

/// Small blocks: a fresh string per iteration, released at its end.
const SMALL_CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<1000 {
    let s = "row " + int.to_string(i)
    acc = acc + string.len(s)
  }
  println("${acc}")
}
"#;

/// Large blocks: a 20,000-element list (160 KB, above the 64 KiB class
/// ceiling) built and released per iteration.
const LARGE_CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<8 {
    let xs = list.map(list.range(0, 20000), (x) => x + i)
    acc = acc + list.len(xs)
  }
  println("${acc}")
}
"#;

#[test]
fn an_unarmed_build_is_byte_identical_to_one_that_never_heard_of_the_switch() {
    let off = emit_plain(SMALL_CHURN, false);
    let _armed = emit_plain(SMALL_CHURN, true);
    assert_eq!(emit_plain(SMALL_CHURN, false), off, "the switch leaves no residue in the next build");
    assert_ne!(emit_plain(SMALL_CHURN, true), off, "the armed build differs: its $alloc skips the free lists");
}

#[test]
fn the_canonical_allocator_never_serves_a_request_by_reuse() {
    for (name, src, out) in [("small", SMALL_CHURN, "6890\n"), ("large", LARGE_CHURN, "160000\n")] {
        let shipped = run_wasm(&emit(src, false)).expect("run");
        let canonical = run_wasm(&emit(src, true)).expect("run");
        assert_eq!(shipped.stdout, out, "{name}");
        assert_eq!(canonical.stdout, out, "{name}: the canonical allocator changes no output");
        let s = shipped.alloc_count.expect("counters");
        let c = canonical.alloc_count.expect("counters");
        assert!(s.reused > 0, "{name}: the shipped allocator reuses here (else this program tests nothing): {s:?}");
        assert_eq!(c.reused, 0, "{name}: the canonical allocator bumps every request: {c:?}");
        assert_eq!(c.allocs, s.allocs, "{name}: the same requests, only served differently");
    }
    // The large program exercises the large-block list, not only the small
    // classes. Shipped, each 160 KB list reuses the one the previous
    // iteration released. Canonically, each bumps a fresh one.
    let shipped = run_wasm(&emit(LARGE_CHURN, false)).expect("run").heap_end.expect("watermark");
    let canonical = run_wasm(&emit(LARGE_CHURN, true)).expect("run").heap_end.expect("watermark");
    assert!(
        canonical >= shipped + 4 * 160_000,
        "the large lists are reused when shipped and bumped when canonical: shipped heap_end {shipped}, canonical {canonical}"
    );
}
