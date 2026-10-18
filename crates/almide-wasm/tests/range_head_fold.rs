//! #2045 — a `list.range(a, b)` that a fold consumes once, directly or
//! through pure map/filter stages fused into the fold, is COUNTED on the wasm
//! leg, never built: native renders it `(a..b).fold(..)`, and a leg that
//! materialized the list died `Error: out of memory` on a span wasm32 cannot
//! hold. Each shape runs at two spans with the allocation counter armed; the
//! bytes the run requests must not depend on the span (a built head would ask
//! for 8 bytes per element). The cross-target observable is pinned by
//! spec/wasm_cross/range_head_fold_{huge,edges}.almd.

mod harness;
use harness::run_wasm;

fn requested(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("range_head_fold.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

fn program(fold: &str, n: u64) -> String {
    format!("effect fn main() -> Unit = {{\n  let r = list.range(3, {n}) {fold}\n  println(\"${{r}}\")\n}}\n")
}

const SHAPES: [(&str, &str); 3] = [
    ("bare fold", "|> list.fold(0, (a, x) => a + x)"),
    ("fused map/filter fold", "|> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0) |> list.fold(0, (a, x) => a + x)"),
    ("fold with a heap accumulator", "|> list.fold(\"\", (a, x) => if x == 7 then a + \"seven\" else a)"),
];

#[test]
fn a_range_head_folded_once_allocates_nothing_per_element() {
    let mut failures = Vec::new();
    for (name, fold) in SHAPES {
        let (_, small, a1, f1) = requested(&program(fold, 1_000));
        let (_, large, a2, f2) = requested(&program(fold, 100_000));
        assert_eq!(a1, f1, "{name}: n=1000 leaked {} block(s)", a1 - f1);
        assert_eq!(a2, f2, "{name}: n=100000 leaked {} block(s)", a2 - f2);
        // A built head is 8 bytes per element: 792 KB more at the larger span.
        if large != small {
            failures.push(format!("{name}: {small} -> {large} bytes requested as the span grew 100x"));
        }
    }
    assert!(failures.is_empty(), "range heads materialized on wasm:\n{}", failures.join("\n"));
}

#[test]
fn a_counted_range_head_reads_the_built_lists_sequence() {
    // The same values in the same order the list held: empty and reversed
    // spans fold zero times, and the counter stops AT an i64::MAX end.
    let cases = [
        ("list.range(5, 5) |> list.fold(42, (a, x) => a + x)", "42"),
        ("list.range(5, 2) |> list.fold(42, (a, x) => a + x)", "42"),
        ("list.range(-3, 3) |> list.fold(0, (a, x) => a * 2 + x)", "-132"),
        ("list.range(9223372036854775805, 9223372036854775807) |> list.fold(0, (a, x) => a * 0 + x)", "9223372036854775806"),
        ("list.range(0, 6) |> list.filter((x) => x % 2 == 1) |> list.fold(\"\", (a, x) => a + int.to_string(x))", "135"),
    ];
    for (expr, want) in cases {
        let src = format!("effect fn main() -> Unit = {{\n  let r = {expr}\n  println(\"${{r}}\")\n}}\n");
        let (out, ..) = requested(&src);
        assert_eq!(out.trim(), want, "{expr}");
    }
}
