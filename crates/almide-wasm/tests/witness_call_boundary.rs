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
        ("pass", "ibamd\n"),
        ("both", "ibamd\nibamd\n"),
        // `mk(3)` in tail position is a `return_call`: the callee hands its
        // credit straight to this frame's caller, and nothing after the
        // jump runs here (#2756: the recorder treats it as dead code), so
        // the result object is never born in this frame.
        ("owned_tail", "\n"),
        ("fresh_tail", "im\n"),
        // bind (i), read as the argument (b, #3259), share into take (a m),
        // released before the jump (d).
        ("bind_then_pass", "ibamd\n"),
        ("temp_arg", "im\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed (gate declined it); witnessed: {:?}", w.keys()));
        assert_eq!(got, cert, "{name}");
        assert!(almide_wasm::witness::balanced(got), "{name} must balance");
    }
    // A SELF tail call is loop-converted (tco.rs, #1988) and, since #2757,
    // certified as the next activation of the frame. `self_tail` only ever
    // passes `a` straight back to itself, so param_borrow.rs makes `a`
    // BORROWED: the loop-back lends it as is — no share, no release, an
    // empty stream. (Before the gate learned the shape the recorder wrote
    // `iamdd` for it, an over-release: the epilogue after a loop-back is
    // dead on that path. The owned-param case is gate.sh's `count_down`.)
    assert_eq!(w.get("self_tail").map(String::as_str), Some("\n"), "self_tail is the next activation");
    // The callee `take` itself: one owned param, released at the epilogue.
    assert_eq!(w.get("take").map(String::as_str), Some("id\n"));
}
