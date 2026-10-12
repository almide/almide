//! #3643 — `map.merge(a, b)` over a left operand the op owns extends it IN
//! PLACE on the wasm leg (`@reuse(a)` on the declaration), as native's owned
//! twin does: the rebind `m = map.merge(m, small)` in a loop, a self tail call
//! and a fold callback run 200 and 400 two-entry merges (one key the map has,
//! one it does not) at two map sizes with the allocation counter armed
//! (`almide_wasm::alloc_count`). The bytes the last 200 merges request (the
//! difference of the two runs) must not grow with the map. Before the fix
//! every merge copied the whole left map.
//!
//! The shared left operands that must still be copied — an alias, a holder
//! and a capture bound before the merge, a right operand that is the left
//! map itself or an alias of it — are the `spec/wasm_cross/map_merge_in_place`
//! fixture, whose stdout here is the native leg's (recorded with
//! `almide run`), and every module balances (`allocs == frees`).

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("map_merge_in_place.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

const BUILD: &str = "fn build(n: Int) -> Map[Int, Int] = list.range(0, n) |> list.fold(map.new(), (m, i) => map.set(m, i, i * 2))

fn small(i: Int) -> Map[Int, Int] = map.from_list([(i % 7, 0 - i), (1000000 + i, i)])

fn go(m: Map[Int, Int], i: Int, k: Int) -> Map[Int, Int] = if i >= k then m else go(map.merge(m, small(i)), i + 1, k)
";

/// One shape at map size `n` with `k` merges.
fn program(shape: &str, n: u32, k: u32) -> String {
    let body = match shape {
        "loop" => format!(
            "  var m: Map[Int, Int] = build(n)\n  var i = 0\n  while i < {k} {{\n    m = map.merge(m, small(i))\n    i = i + 1\n  }}"
        ),
        "tail" => format!("  let m0: Map[Int, Int] = build(n)\n  let m = go(m0, 0, {k})"),
        "fold" => format!(
            "  let m0: Map[Int, Int] = build(n)\n  let m = list.range(0, {k}) |> list.fold(m0, (acc, i) => map.merge(acc, small(i)))"
        ),
        _ => unreachable!(),
    };
    format!(
        "{BUILD}\neffect fn main() -> Unit = {{\n  let n = {n}\n{body}\n  println(\"${{map.len(m)}} ${{map.get(m, 3) ?? -1}} ${{map.get(m, 1000005) ?? -1}} ${{map.get(m, n - 1) ?? -1}}\")\n}}\n"
    )
}

/// The bytes the LAST 200 of 400 merges request at size `n` — the run with
/// 400 minus the run with 200, so a one-time cost cancels out (the first
/// merge copying a receiver its caller still holds) — and the 400-merge
/// run's stdout.
fn merge_bytes(shape: &str, n: u32) -> (u64, String) {
    let (out, full, a, f) = counts(&program(shape, n, 400));
    let (_, half, a0, f0) = counts(&program(shape, n, 200));
    assert_eq!(a, f, "{shape} n={n}: leaked {} block(s)", a - f);
    assert_eq!(a0, f0, "{shape} n={n} (200 merges): leaked {} block(s)", a0 - f0);
    (full.saturating_sub(half), out)
}

#[test]
fn an_owned_left_operand_is_extended_in_place() {
    let mut failures = Vec::new();
    for shape in ["loop", "tail", "fold"] {
        let (small, out_s) = merge_bytes(shape, 1100);
        let (large, out_l) = merge_bytes(shape, 4400);
        eprintln!("{shape}: {small} -> {large} merge bytes ({} / {})", out_s.trim(), out_l.trim());
        // 400 new keys; key 3 last overridden by the merge of i = 395.
        assert_eq!(out_s, "1500 -395 5 2198\n", "{shape} n=1100");
        assert_eq!(out_l, "4800 -395 5 8798\n", "{shape} n=4400");
        // A copy per merge quadruples over n = 1100 → 4400; in place the
        // merges request only their small right operands at either size.
        if large > small * 2 + 4096 {
            failures.push(format!("{shape}: {small} -> {large} merge bytes"));
        }
    }
    assert!(failures.is_empty(), "map merges that copy the left map on wasm:\n{}", failures.join("\n"));
}

/// The native leg's stdout of the fixture, recorded with `almide run`.
const SHARED_NATIVE: &str = "k0=v5,k1=v6,k2=v2,n0=w0,n1=w1,n2=w2,k3=v3,n3=w3,k4=v4,n4=w4,n5=w5,n6=w6
k0=a,k1=b,k2=c | k0=a,k1=b,k2=c 1 3 | k0=a,k1=b,k2=c
k3=v3,k0=v5,n0=w0,k1=v1,n1=w1,k2=v2,n2=w2,n3=w3,k4=v4,n4=w4,n5=w5
k4=v4,k0=v10,n0=w0,k2=v2,n2=w2,n4=w4,k1=v6,n6=w6,k3=v8,n8=w8,n10=w10
p=1,q=3,r=4 | p=1,q=2
k0=a,k1=b,k2=c 3
86 40 some(-3) some(9) some(29) [51, 117, 54, 118, 57, 119, 60, 120, 63, 121, 66, 122, 69, 123, 72, 124, 75, 125, 78, 126, 81, 127, 84, 128, 87, 129]
";

#[test]
fn shared_left_operands_keep_their_copies_and_match_native() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/wasm_cross/map_merge_in_place.almd");
    let src = std::fs::read_to_string(path).expect("fixture");
    let (out, _, allocs, frees) = counts(&src);
    assert_eq!(out, SHARED_NATIVE);
    assert_eq!(allocs, frees, "the shared-operand fixture leaked {} block(s)", allocs - frees);
}
