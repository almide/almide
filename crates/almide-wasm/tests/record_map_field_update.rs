//! #3406 — a map held in a record, updated through a spread whose base dies
//! there, is written in place, not copied per update.
//!
//! `b = add(b, i)` with `fn add(b: Box, i: Int) -> Box = { ...b, m:
//! map.set(b.m, …), n: b.n + 1 }` copied the whole map on every call: the
//! spread copied `b`, the field read shared `b.m` into `map.set`, and the
//! functional set copied any receiver it did not own — 3.8 s for 20 000
//! updates, and the bytes allocated grew with the square of the loop. (The
//! watermark does not show it: each copy freed its predecessor, so the
//! free list hid the churn — the allocation counter is the instrument.)
//! The dying base now hands its credit over
//! (dying_move.rs): a reassignment moves `b` into `add`, the spread at its
//! tail rebuilds the record in place and moves `b.m` out of its slot, and
//! `map.set` writes an owned receiver in place when its count is 1.
//!
//! Each row runs its loop at N=250 and N=2000 under the allocation counter
//! and pins the growth of the bytes requested between the two sizes to the
//! linear bound: 8× the updates may request at most 12× the bytes. Measured
//! on the call form: 108 928 -> 872 474 bytes (8.0×) with the fix, 357 208 ->
//! 16 848 458 (47×) on 0.64.0. The answer is native's.

mod harness;
use harness::run_wasm;

fn run(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("record_map_field_update.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.alloc_count.expect("the armed module exports the counters").bytes, out.stdout)
}

const BOX: &str = "type Box = { m: Map[String, List[Int]], n: Int }";

const DECL: &str = "var b = Box { m: [:], n: 0 }";

fn linear(name: &str, fns: &str, step: &str, (expect_250, expect_2000): (&str, &str)) {
    let src = |n: u32| {
        format!("{BOX}\n{fns}\nfn main() -> Unit = {{\n  {DECL}\n  for i in 0..<{n} {{\n    {step}\n  }}\n  println({SHOW})\n}}\n")
    };
    let (h250, o250) = run(&src(250));
    let (h2000, o2000) = run(&src(2000));
    assert_eq!(o250, expect_250, "{name}: stdout at N=250");
    assert_eq!(o2000, expect_2000, "{name}: stdout at N=2000");
    assert!(
        h2000 < 12 * h250,
        "{name}: 8x the updates requested {}x the bytes ({h250} -> {h2000}) — an update copies the map",
        h2000 / h250.max(1)
    );
}

const SHOW: &str = r#""${int.to_string(map.len(b.m))} ${int.to_string(b.n)}""#;

/// The issue's shape: the spread at a fn's tail, called as a reassignment.
#[test]
fn the_issue_program_is_linear() {
    let add = "fn add(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, int.to_string(i), list.range(0, 50)), n: b.n + 1 }";
    linear(
        "call form",
        add,
        "b = add(b, i)",
        ("250 250\n", "2000 2000\n"),
    );
}

/// The same update written inline in the loop.
#[test]
fn the_inline_update_is_linear() {
    linear(
        "inline form",
        "",
        "b = { ...b, m: map.set(b.m, int.to_string(i), list.range(0, 50)), n: b.n + 1 }",
        ("250 250\n", "2000 2000\n"),
    );
}

/// An owned map param moved into `map.set` at a fn's tail.
#[test]
fn a_map_param_at_the_tail_is_linear() {
    let put = "fn put(m: Map[String, List[Int]], i: Int) -> Map[String, List[Int]] = map.set(m, int.to_string(i), list.range(0, 50))";
    linear(
        "map param",
        put,
        "b = { ...b, m: put(b.m, i), n: b.n + 1 }",
        ("250 250\n", "2000 2000\n"),
    );
}
