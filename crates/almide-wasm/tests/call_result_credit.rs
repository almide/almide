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
