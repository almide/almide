//! #3537 — a chained `map.set` / `map.upsert` / `map.update` writes in place on the wasm leg.
//!
//! `m = map.set(map.set(m, j, 1), j + 1, 2)` in a loop copied the whole map
//! once or twice per step, where a single `m = map.set(m, k, v)` writes in
//! place. The outer op decided whether its receiver was an owned temporary
//! BEFORE lowering it, and a module call's ownership mark is set only by its
//! own lowering, so the inner `map.set`'s fresh result read as borrowed and
//! was copied. The receiver is now lowered retained and the in-place judge
//! (`rc == 1`, map_inplace.rs) decides by count, as the `@reuse` list edits
//! do (#3533).
//!
//! The bytes the last 200 of 400 steps request must not grow with the map.
//! A receiver another binding still holds must still be copied (its value
//! is printed after the loop), and every module balances.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("map_set_chained_in_place.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r
        .alloc_count
        .expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

/// `k` chained steps over a map of `n` entries.
fn program(step: &str, n: u32, k: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  var m: Map[Int, Int] = map.new()
  var i = 0
  while i < {n} {{
    m = map.set(m, i, i)
    i = i + 1
  }}
  var j = 0
  while j < {k} {{
    m = {step}
    j = j + 1
  }}
  println(\"${{map.len(m)}} ${{map.get(m, 0) ?? -1}} ${{map.get(m, {k}) ?? -1}}\")
}}
"
    )
}

const SET: &str = "map.set(map.set(m, j, 1), j + 1, 2)";
const UPSERT: &str = "map.upsert(map.set(m, j, 1), j + 1, 0, (v) => v + 10)";
const UPDATE: &str = "map.update(map.set(m, j, 1), j + 1, (v) => v + 10)";

fn step_bytes(step: &str, n: u32) -> (u64, String) {
    let (out, full, a, f) = counts(&program(step, n, 400));
    let (_, half, a0, f0) = counts(&program(step, n, 200));
    assert_eq!(a, f, "n={n}: leaked {} block(s)", a - f);
    assert_eq!(a0, f0, "n={n} (200 steps): leaked {} block(s)", a0 - f0);
    (full - half, out)
}

#[test]
fn a_chained_set_does_not_copy_the_map_per_step() {
    // The expected lines are the native leg's (`almide run`, 400 steps).
    for (step, out_small, out_large) in [
        (SET, "2000 1 2\n", "8000 1 2\n"),
        (UPSERT, "2000 1 410\n", "8000 1 410\n"),
        (UPDATE, "2000 1 410\n", "8000 1 410\n"),
    ] {
        let (small, out_s) = step_bytes(step, 2000);
        let (large, out_l) = step_bytes(step, 8000);
        assert_eq!(out_s, out_small, "{step}");
        assert_eq!(out_l, out_large, "{step}");
        assert!(
            large <= small + small / 4,
            "{step}: the last 200 steps requested {small} bytes at n=2000 and {large} at n=8000 — a per-step copy grows with the map"
        );
    }
}

#[test]
fn a_receiver_another_binding_holds_is_still_copied() {
    let src = "effect fn main() -> Unit = {
  var m: Map[Int, Int] = map.new()
  m = map.set(m, 1, 1)
  let keep = m
  m = map.set(map.set(m, 1, 5), 2, 6)
  println(\"${map.get(keep, 1) ?? -1} ${map.len(keep)} ${map.get(m, 1) ?? -1} ${map.len(m)}\")
}
";
    let (out, _, a, f) = counts(src);
    assert_eq!(out, "1 1 5 2\n");
    assert_eq!(a, f, "leaked {} block(s)", a - f);
}
