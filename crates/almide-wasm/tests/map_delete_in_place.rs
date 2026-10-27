//! #3640 — `map.remove` on a receiver the op owns deletes IN PLACE on the
//! wasm leg, as native's `AlmideMap::remove` does. Each shape — the rebind
//! of the functional `map.remove` in a loop, a self tail call and a fold
//! callback — runs 200
//! and 400 deletes of present and of absent keys at two map sizes with the
//! allocation counter armed (`almide_wasm::alloc_count`). The bytes the last
//! 200 deletes request (the difference of the two runs) must not grow with
//! the map. Before the fix every delete copied the whole map, an absent key
//! included: 200 deletes at n = 2000 → 8000 requested 6.4M → 25.7M bytes,
//! four times per doubling.
//!
//! The shared receivers that must still be copied — an alias bound before
//! the delete, a map handed to a second remove chain, the source of a fold —
//! print the native leg's bytes (recorded with `almide run`), and every
//! module balances (`allocs == frees`).

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("map_delete_in_place.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

const BUILD: &str = "fn build(n: Int) -> Map[Int, Int] = list.range(0, n) |> list.fold(map.new(), (m, i) => map.set(m, i, i * 2))

fn go(m: Map[Int, Int], i: Int, k: Int, n: Int) -> Map[Int, Int] = if i >= k then m else go(map.remove(m, KEY), i + 1, k, n)
";

/// One shape at map size `n` with `k` deletes of `key` (an expression over
/// the step `i` and `n`).
fn program(shape: &str, key: &str, n: u32, k: u32) -> String {
    let body = match shape {
        "loop" => format!("  var m: Map[Int, Int] = build(n)\n  var i = 0\n  while i < {k} {{\n    m = map.remove(m, {key})\n    i = i + 1\n  }}"),
        "tail" => format!("  let m0: Map[Int, Int] = build(n)\n  let m = go(m0, 0, {k}, n)"),
        "fold" => format!("  let m0: Map[Int, Int] = build(n)\n  let m = list.range(0, {k}) |> list.fold(m0, (m, i) => map.remove(m, {key}))"),
        _ => unreachable!(),
    };
    format!(
        "{}\neffect fn main() -> Unit = {{\n  let n = {n}\n{body}\n  println(\"${{map.len(m)}} ${{map.get(m, 1) ?? -1}} ${{map.get(m, n - 1) ?? -1}}\")\n}}\n",
        BUILD.replace("KEY", key)
    )
}

/// The bytes the LAST 200 of 400 deletes request at size `n` — the run with
/// 400 minus the run with 200, so a one-time cost cancels out (the first
/// remove copying a receiver its caller still holds, as `m0` is in the tail
/// and fold shapes) — and the 400-delete run's stdout.
fn delete_bytes(shape: &str, key: &str, n: u32) -> (u64, String) {
    let (out, full, a, f) = counts(&program(shape, key, n, 400));
    let (_, half, a0, f0) = counts(&program(shape, key, n, 200));
    assert_eq!(a, f, "{shape} {key} n={n}: leaked {} block(s)", a - f);
    assert_eq!(a0, f0, "{shape} {key} n={n} (200 deletes): leaked {} block(s)", a0 - f0);
    (full.saturating_sub(half), out)
}

#[test]
fn an_owned_receiver_deletes_in_place() {
    let mut failures = Vec::new();
    for shape in ["loop", "tail", "fold"] {
        for (what, key) in [("absent", "n + 7 + i"), ("present", "i * 3")] {
            let (small, out_s) = delete_bytes(shape, key, 2000);
            let (large, out_l) = delete_bytes(shape, key, 8000);
            eprintln!("{shape} {what}: {small} -> {large} delete bytes ({} / {})", out_s.trim(), out_l.trim());
            let want_len = if what == "absent" { [2000, 8000] } else { [1600, 7600] };
            assert!(out_s.starts_with(&format!("{} ", want_len[0])), "{shape} {what}: {out_s}");
            assert!(out_l.starts_with(&format!("{} ", want_len[1])), "{shape} {what}: {out_l}");
            // A copy per delete quadruples over n = 2000 → 8000; in place the
            // deletes request (almost) nothing at either size.
            if large > small * 2 + 4096 {
                failures.push(format!("{shape} {what}: {small} -> {large} delete bytes"));
            }
        }
    }
    assert!(failures.is_empty(), "map deletes that copy the map on wasm:\n{}", failures.join("\n"));
}

/// The receivers that must keep their copies: each row's stdout is the
/// native leg's, recorded with `almide run`.
const SHARED: &str = r#"fn build(n: Int) -> Map[Int, Int] = list.range(0, n) |> list.fold(map.new(), (m, i) => map.set(m, i, i * 2))

fn go(m: Map[Int, Int], i: Int) -> Map[Int, Int] = if i >= 10 then m else go(map.remove(m, i), i + 1)

effect fn main() -> Unit = {
  var m: Map[Int, Int] = build(40)
  let keep = m
  m = map.remove(m, 3)
  m = map.remove(m, 99)
  var m2 = m
  let keep2 = m2
  m2 = map.remove(m2, 5)
  m2 = map.remove(m2, 77)
  let base: Map[Int, Int] = build(30)
  let t = go(base, 0)
  let f = list.range(20, 25) |> list.fold(base, (acc, i) => map.remove(acc, i))
  let r = list.range(0, 3) |> list.fold(m2, (acc, i) => map.remove(acc, i + 100))
  m = map.remove(m, 1)
  m = map.set(m, 3, 33)
  m = map.remove(m, 30)
  m = map.set(m, 30, 300)
  println("${map.len(keep)} ${map.len(m)} ${map.len(keep2)} ${map.len(m2)} ${map.len(base)} ${map.len(t)} ${map.len(f)} ${map.len(r)}")
  println("${map.keys(m)}")
  println("${map.get(m, 3)} ${map.get(m, 30)} ${map.get(m, 31)} ${map.get(m, 1)} ${map.get(keep, 3)} ${map.get(keep2, 5)}")
  println("${map.keys(t)}")
  println("${map.keys(f)}")
  println("${map.keys(keep)}")
}
"#;

const SHARED_NATIVE: &str = "40 39 39 38 30 20 25 38
[0, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 3, 30]
some(33) some(300) some(62) none some(6) some(10)
[10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29]
[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 25, 26, 27, 28, 29]
[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39]
";

#[test]
fn shared_receivers_keep_their_copies_and_match_native() {
    let (out, _, allocs, frees) = counts(SHARED);
    assert_eq!(out, SHARED_NATIVE);
    assert_eq!(allocs, frees, "the shared-receiver probe leaked {} block(s)", allocs - frees);
}
