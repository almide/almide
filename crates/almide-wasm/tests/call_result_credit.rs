//! #1986 — a user fn's droppable result arrives with exactly ONE owned
//! credit (the callee's ret-inc, or its certainly-fresh alloc), and the
//! caller's bind / assign / return routes must not add another. Before
//! the fix every returned List/Str/Bytes ended at rc 1 forever: 64 B per
//! call, invisible to stdout and pinned silently by the alloc ledger as a
//! higher watermark. Pinned here as a FLAT high-water mark across N.

mod harness;
use harness::run_wasm;

fn program(n: u32, body: &str) -> String {
    format!(
        r#"fn mk(n: Int) -> List[Int] = {{
  let a = [n, n, n]
  a
}}

fn mk2(n: Int) -> List[Int] = [n, n, n]

fn via(n: Int) -> List[Int] = mk(n)

fn take(xs: List[Int]) -> Int = 3

fn bind_then_tail(n: Int) -> Int = {{
  let x = mk(n)
  take(x)
}}

fn grow(n: Int) -> String = {{
  let s = "x" + "y"
  s
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

fn heap_after(n: u32, body: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("credit.almd", &program(n, body)).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

fn flat(name: &str, body: &str, expect_1000: &str, expect_8000: &str) {
    let (h1, o1) = heap_after(1000, body);
    let (h8, o8) = heap_after(8000, body);
    assert_eq!(o1, expect_1000, "{name}: output at N=1000");
    assert_eq!(o8, expect_8000, "{name}: output at N=8000");
    assert_eq!(h1, h8, "{name}: the high-water mark must not grow with N (N=1000 {h1} B, N=8000 {h8} B — {} B per call leaked)", (h8 - h1) / 7000);
}

#[test]
fn a_bound_call_result_is_released() {
    flat("let x = mk(i)", "    let x = mk(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn a_certainly_fresh_tail_result_is_released() {
    flat("let x = mk2(i)", "    let x = mk2(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn a_call_in_tail_position_hands_over_one_credit() {
    flat("let x = via(i)", "    let x = via(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn an_assigned_call_result_is_released() {
    flat(
        "x = mk(i)",
        "    var x = mk(i)\n    x = mk(i + 1)\n    total = total + list.len(x)",
        "3000",
        "24000",
    );
}

#[test]
fn a_fresh_string_result_is_released() {
    flat("let s = grow(i)", "    let s = grow(i)\n    total = total + string.len(s)", "2000", "16000");
}


#[test]
fn a_return_call_releases_the_owned_locals_before_the_jump() {
    // The tail call replaces the frame, so the epilogue never runs — the
    // bound local must be released before the jump (a sibling of #1986,
    // found by the phase-B1 witness: the local's stream ended `iam`).
    flat("let x = mk(i); take(x)", "    total = total + bind_then_tail(i)", "3000", "24000");
}

/// The ERROR exits — `f()!` propagating an err, and a raised `err(..)` —
/// are function exits like any other: the frame's owned locals and its
/// droppable params must be released before the `return`. Measured as a
/// per-call growth that must not exceed the ok path's (both paths leak
/// the same Result carrier today; the err path leaked the param and the
/// local on top).
fn err_exit_heap(n: u32, fail: bool) -> (u64, String) {
    let cmp = if fail { ">=" } else { "<" };
    // `guard` PASSES when its condition holds — the failing probe wants
    // the negation.
    let ncmp = if fail { "<" } else { ">=" };
    let src = format!(
        r#"effect fn boom(n: Int) -> Int = if n {cmp} 0 then err("no") else ok(1)

effect fn work(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n, n]
  let v = boom(n)!
  v + list.len(xs) + list.len(y)
}}

effect fn raise(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n]
  if n {cmp} 0 then err("raised") else ok(list.len(xs) + list.len(y))
}}

effect fn guarded(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n]
  guard n {ncmp} 0 else err("guarded")
  ok(list.len(xs) + list.len(y))
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
    total = total + (match work([i, i], i) {{ ok(v) => v, err(_) => 1 }})
    total = total + (match raise([i], i) {{ ok(v) => v, err(_) => 1 }})
    total = total + (match guarded([i], i) {{ ok(v) => v, err(_) => 1 }})
  }}
  println("${{total}}")
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("errexit.almd", &src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

#[test]
fn an_error_exit_releases_the_frame_like_the_ok_exit() {
    let (ok1, o1) = err_exit_heap(1000, false);
    let (ok8, o8) = err_exit_heap(8000, false);
    let (er1, e1) = err_exit_heap(1000, true);
    let (er8, e8) = err_exit_heap(8000, true);
    assert_eq!((o1.as_str(), o8.as_str()), ("12000", "96000"));
    assert_eq!((e1.as_str(), e8.as_str()), ("3000", "24000"));
    let ok_growth = (ok8 - ok1) / 7000;
    let err_growth = (er8 - er1) / 7000;
    assert!(
        err_growth <= ok_growth,
        "the err exit leaks what the ok exit releases: {err_growth} B per call on the err path vs {ok_growth} B on the ok path"
    );
}

/// The registry-table tail call (`lower_linked_call`'s `return_call`) is
/// the third tail site — found by scripts/check-exit-sites.sh the day it
/// went in (#1995): a user fn whose tail is `string.to_upper(s)` replaced its
/// frame with no release, so the owned `s` stayed at rc 1 on every call.
#[test]
fn a_linked_tail_call_releases_the_frame() {
    fn heap(n: u32) -> (u64, String) {
        let src = format!(
            r#"fn wrap(s: String) -> String = string.to_upper(s)

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
    // Bound, not inline: a fresh temporary handed to a native op is a
    // leak class of its own (#2004) and would mask this row; `to_upper`
    // rather than `trim` because string_trim leaks inside its own body
    // (#2005, 96 B per call) — the wrapper is what this row measures.
    let s = " x" + "y"
    let t = wrap(s)
    total = total + string.len(t)
  }}
  println("${{total}}")
}}
"#
        );
        let ir = almide_spine::s5::lower_to_ir("linked_tail.almd", &src).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
        let out = run_wasm(&bytes).expect("run");
        assert_eq!(out.exit, 0, "{}", out.stderr);
        (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
    }
    let (h1, o1) = heap(1000);
    let (h8, o8) = heap(8000);
    assert_eq!(o1, "3000");
    assert_eq!(o8, "24000");
    assert_eq!(h1, h8, "linked tail call: the high-water mark must not grow with N (N=1000 {h1} B, N=8000 {h8} B — {} B per call leaked)", (h8 - h1) / 7000);
}

/// #1990 — a `bytes.append_*` loop must grow LINEARLY: the linked impl
/// returns a fresh buffer on every call and the old one has to go.
/// Before the fix the old buffer stayed at rc 1 on every append (the
/// module-space wrapper never released its param at the tail site and a
/// `prim.alloc_*` result carried an extra credit), so N=2000 peaked at
/// 3.84× N=1000 — quadratic.
fn append_heap(n: u32) -> (u64, String) {
    let src = format!(
        r#"effect fn main() -> Unit = {{
  var b = bytes.new(0)
  for i in 0..<{n} {{
    bytes.append_u16_le(b, i)
  }}
  println("${{bytes.len(b)}}")
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("append.almd", &src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

#[test]
fn a_registry_tail_call_releases_its_owned_param() {
    let (h1, o1) = append_heap(1000);
    let (h2, o2) = append_heap(2000);
    assert_eq!(o1, "2000");
    assert_eq!(o2, "4000");
    // Linear: doubling N at most doubles the live buffer (plus the fixed
    // floor). The quadratic leak measured 3.84×.
    assert!(
        (h2 as f64) < 2.5 * (h1 as f64),
        "bytes.append_* loop is not linear: N=1000 {h1} B, N=2000 {h2} B"
    );
}
