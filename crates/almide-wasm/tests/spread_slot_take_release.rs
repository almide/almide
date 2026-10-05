//! #3441 — a spread rebuild whose base dies there moves a field read
//! `b.f` of an overridden field out of its slot (#3406, dying_move.rs). A
//! moved slot is an OWNED value, so it may only be taken where the consumer
//! spends the credit: the field's own store, a call argument, a `let`
//! value. A binary op, an interpolation, an index, a match subject or a
//! further field read only READS its operand — a take there left the old
//! slot's block live forever: `sc = { ...sc, funs: sc.funs + [e] }` leaked
//! one list per rebuild (10 blocks at exit in
//! spec/wasm_cross/tuple_bound_list_mapped_in_loop.almd), native none.
//!
//! Each row is a consumer of `sc.<f>` inside the rebuild; every one must
//! exit with 0 live blocks (the live-at-exit instrument, alloc_ledger.rs).
//! Before the fix every reading row leaked 1–2 blocks per rebuild (3–6 at
//! exit), and so did the `if` row whose arm concatenates; the other rows are
//! consumers that keep the move and must keep balancing. Stdout is native's.

mod harness;
use harness::run_wasm;

fn live(field: &str) -> (i64, String) {
    let src = format!(
        r#"type In = {{ xs: List[Int] }}
type Sc = {{ funs: List[Int], s: String, inner: In, n: Int }}
fn myf(xs: List[Int]) -> List[Int] = xs
effect fn main() -> Unit = {{
  var sc = Sc {{ funs: [5, 6], s: "a" + int.to_string(1), inner: In {{ xs: [3] }}, n: 0 }}
  for _ in 0..<3 {{
    sc = {{ ...sc, {field} }}
  }}
  println("${{list.len(sc.funs)}} ${{sc.s}} ${{list.len(sc.inner.xs)}}")
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("spread_slot_take.almd", &src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "{field}: {}", r.stderr);
    (r.alloc_count.expect("the armed module exports the counters").live(), r.stdout)
}

const ROWS: &[(&str, &str)] = &[
    // Reading consumers: the read must share, not move.
    ("funs: sc.funs + [1]", "5 a1 1\n"),
    ("funs: [1] + sc.funs", "5 a1 1\n"),
    ("s: \"${sc.s}!\"", "2 a1!!! 1\n"),
    ("s: sc.s + \"x\"", "2 a1xxx 1\n"),
    ("funs: [sc.funs[0]]", "1 a1 1\n"),
    ("funs: match sc.funs { [] => [0], _ => [1] }", "1 a1 1\n"),
    ("inner: In { xs: sc.inner.xs + [1] }", "2 a1 4\n"),
    ("inner: In { xs: sc.inner.xs }", "2 a1 1\n"),
    // Consumers that spend the moved credit.
    ("funs: sc.funs", "2 a1 1\n"),
    ("funs: myf(sc.funs)", "2 a1 1\n"),
    ("funs: myf(sc.funs) + [1]", "5 a1 1\n"),
    ("funs: sc.funs |> list.map((x) => x + 1)", "2 a1 1\n"),
    ("funs: if sc.n == 0 then sc.funs + [1] else []", "5 a1 1\n"),
    ("funs: { let t = sc.funs\n  t + [1] }", "5 a1 1\n"),
    ("funs: Some(sc.funs) ?? []", "2 a1 1\n"),
];

#[test]
fn every_consumer_of_a_moved_slot_balances() {
    let mut bad = Vec::new();
    for (field, want) in ROWS {
        let (n, out) = live(field);
        if n != 0 || out != *want {
            bad.push(format!("`{field}`: {n} live at exit, stdout {out:?} (want {want:?})"));
        }
    }
    assert!(bad.is_empty(), "a spread rebuild's field read leaked or misread:\n{}", bad.join("\n"));
}
