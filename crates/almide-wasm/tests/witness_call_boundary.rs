//! #1696 step 4 phase B1 — the call boundary in the structural witness,
//! still certificate v0. The callee-owned convention as the recorder sees
//! it: a droppable Var argument shares (`a`, the site's real `rc_inc`) and
//! its credit moves into the callee (`m`); a fresh temporary is born and
//! moves (`im`); a user fn's droppable result is a received credit (`i`,
//! #1986); an owned tail moves out (`im`); a `return_call` releases the
//! owned params before the jump (`d`). Every certificate here balances
//! under the mirror of the proven rule and is the exact text
//! `proofs/gate.sh` feeds the extracted checker.

const PROGRAM: &str = r#"fn take(xs: List[Int]) -> Int = 3

fn mk(n: Int) -> List[Int] = {
  let a = [n, n]
  a
}

fn pass(a: List[Int]) -> Int = take(a)

fn both(a: List[Int], b: List[Int]) -> Int = {
  let r = take(a)
  take(b)
}

fn owned_tail() -> List[Int] = mk(3)

fn fresh_tail() -> List[Int] = [1, 2]

fn bind_then_pass() -> Int = {
  let x = mk(3)
  take(x)
}

fn temp_arg() -> Int = take([1, 2])

fn self_tail(a: List[Int], n: Int) -> Int = self_tail(a, n)

effect fn main() -> Unit =
  println("${pass([1])} ${both([1], [2])} ${bind_then_pass()} ${temp_arg()} ${fresh_tail() |> list.len} ${owned_tail() |> list.len}")
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("b1.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn the_call_boundary_shapes_witness_exactly_and_balance() {
    let w = witnesses();
    let expect = [
        ("pass", "iamd\n"),
        ("both", "iamd\niamd\n"),
        ("owned_tail", "im\n"),
        ("fresh_tail", "im\n"),
        // bind (i), share into take (a m), released before the jump (d).
        ("bind_then_pass", "iamd\n"),
        ("temp_arg", "im\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed (gate declined it); witnessed: {:?}", w.keys()));
        assert_eq!(got, cert, "{name}");
        assert!(almide_wasm::witness::balanced(got), "{name} must balance");
    }
    // A SELF tail call is loop-converted (tco.rs, #1988): the frame lives
    // on, the loop-back rebinds the param and the epilogue releases it
    // again — a loop, which the straight-line recorder must decline rather
    // than certify (it recorded `iamdd`, an over-release, before the gate
    // learned the shape).
    assert!(w.get("self_tail").is_none(), "self_tail is a loop, out of the straight-line subset");
    // The callee `take` itself: one owned param, released at the epilogue.
    assert_eq!(w.get("take").map(String::as_str), Some("id\n"));
}
