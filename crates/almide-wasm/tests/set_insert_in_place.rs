//! #3552 — `set.insert` writes in place on the wasm leg when the set is the
//! op's alone.
//!
//! Every `set.insert` copied the whole set (`emit_copy_grow`), so building a
//! set one member at a time — `s = set.insert(s, x)` in a loop, or a fold
//! seeded with a set — was O(n²): 50k inserts took 0.43 s and 200k took
//! 6.89 s under wasmtime. The insert now takes the receiver's credit when it
//! can (a dying var, or any non-var receiver lowered retained) and the
//! `rc == 1` judge `set.remove` and `map.set` use decides: in place through
//! `$map_reserve`, else the functional copy.
//!
//! The bytes the last 200 of 400 rebind steps request must not grow with
//! the set. A set another binding holds keeps its members, and every module
//! balances.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("set_insert_in_place.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

/// `k` rebind steps (one new member, one present member each) and a fold of
/// `k` new members over a set of `n`.
fn program(n: u32, k: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  var s: Set[Int] = set.from_list(list.range(0, {n}))
  var j = 0
  while j < {k} {{
    s = set.insert(s, {n} + j)
    s = set.insert(s, j)
    j = j + 1
  }}
  let t = list.range(0, {k}) |> list.fold(s, (acc, i) => set.insert(acc, 5000000 + i))
  println(\"${{set.len(s)}} ${{set.len(t)}} ${{set.contains(t, 5000000 + {k} - 1)}}\")
}}
"
    )
}

fn step_bytes(n: u32) -> (u64, String) {
    let (out, full, a, f) = counts(&program(n, 400));
    let (_, half, a0, f0) = counts(&program(n, 200));
    assert_eq!(a, f, "n={n}: leaked {} block(s)", a - f);
    assert_eq!(a0, f0, "n={n} (200 steps): leaked {} block(s)", a0 - f0);
    (full - half, out)
}

#[test]
fn inserting_into_an_owned_set_does_not_copy_it() {
    // The expected lines are the native leg's (`almide run`).
    let (small, out_s) = step_bytes(2000);
    let (large, out_l) = step_bytes(8000);
    assert_eq!(out_s, "2400 2800 true\n");
    assert_eq!(out_l, "8400 8800 true\n");
    // A per-insert copy requests the whole set on each of the 200 steps; an
    // in-place append requests at most an amortized doubling of the block
    // inside the window. The bound is four copies of the final set (8-byte
    // members), which the copy exceeds fifty-fold.
    for (n, got) in [(2000u64, small), (8000, large)] {
        let four_copies = 4 * 8 * (n + 2 * 400);
        assert!(
            got < four_copies,
            "the last 200 steps requested {got} bytes at n={n}, over four copies of the set ({four_copies}) — a per-insert copy"
        );
    }
}

#[test]
fn a_set_another_binding_holds_keeps_its_members() {
    let src = "effect fn main() -> Unit = {
  var s: Set[String] = set.new()
  s = set.insert(s, \"a\")
  let keep = s
  s = set.insert(set.insert(s, \"b\"), \"c\")
  println(\"${set.len(keep)} ${set.len(s)} ${set.contains(keep, \"b\")}\")
}
";
    let (out, _, a, f) = counts(src);
    assert_eq!(out, "1 3 false\n");
    assert_eq!(a, f, "leaked {} block(s)", a - f);
}
