//! option.collect reserves its result once.
//!
//! On success the result has exactly as many values as the input has
//! elements, so the growth ruling (2026-10-15: a result whose count is known
//! on success reserves exactly n) applies. Built with `list.filter_map`, the
//! result grew by doubling: about twice its final size was allocated and
//! copied on the way, measured as 25.5 MB against 12.5 MB of wasm memory
//! traffic for 200k strings.
//!
//! The canonical allocator makes the heap watermark a function of the
//! requests alone (no reuse), so the bytes `collect` adds over the program
//! that only builds its input are exactly what it allocated.

mod harness;
use harness::run_wasm;

const N: usize = 20_000;

fn heap_end(body: &str) -> u64 {
    let src = format!(
        "fn main() -> Unit = {{\n  let xs = list.map(list.range(0, {N}), (i) => some(i))\n  {body}\n}}\n"
    );
    let ir = almide_spine::s5::lower_to_ir("probe.almd", &src).expect("front");
    let _count = almide_wasm::alloc_count::CountGuard::set();
    let _canonical = almide_wasm::canonical_alloc::CanonicalGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let run = run_wasm(&bytes).expect("run");
    assert!(!run.stdout.is_empty(), "the program printed: {body}");
    run.heap_end.expect("watermark") as u64
}

#[test]
fn collect_allocates_its_result_once() {
    let input_only = heap_end("println(\"${list.len(xs)}\")");
    let collected = heap_end("println(\"${list.len(option.collect(xs) ?? [])}\")");
    let result = (N * 8) as u64;
    let added = collected - input_only;
    assert!(
        added <= result + result / 4,
        "collect over {N} Ints allocated {added} bytes; its result is {result} — a doubling build allocates about twice that"
    );
}
