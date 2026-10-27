//! #3794: a `let { … } = <record expression>` destructure releases the
//! record on the wasm leg. The subject is an anonymous (structural) record —
//! a literal, a block whose tail is one, or a `fan` record — which the
//! destructure-subject naming did not list, so the block stayed live at
//! exit. Native drops it; the stdout below is native's.

mod harness;
use harness::run_wasm;

fn run_armed(src: &str) -> (String, almide_wasm_run::AllocCount) {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _armed = almide_wasm::alloc_count::CountGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "stderr: {}", r.stderr);
    (r.stdout, r.alloc_count.expect("the armed module exports the counters"))
}

#[test]
fn destructuring_a_record_literal_releases_it() {
    let src = r#"effect fn note(v: Int) -> Result[Int, String] = ok(v)

effect fn main() -> Unit = {
  let { x, y } = { x: note(3)!, y: note(4)! }
  println(int.to_string(x + y))
}
"#;
    let (out, c) = run_armed(src);
    assert_eq!(out, "7\n");
    assert_eq!(c.live(), 0, "{c}");
}

#[test]
fn destructuring_a_block_whose_tail_is_a_record_releases_it() {
    let src = r#"effect fn note(v: Int) -> Result[Int, String] = ok(v)

effect fn main() -> Unit = {
  let { a, b } = {
    let (p, q) = (note(1)!, note(2)!)
    { a: p, b: q }
  }
  println(int.to_string(a + b))
}
"#;
    let (out, c) = run_armed(src);
    assert_eq!(out, "3\n");
    assert_eq!(c.live(), 0, "{c}");
}

#[test]
fn destructuring_a_record_with_heap_fields_releases_it() {
    let src = r#"effect fn main() -> Unit = {
  let n = list.len([1, 2, 3])
  let { s, t } = { s: "a" + int.to_string(n), t: [n, n + 1] }
  println(s + int.to_string(list.len(t)))
}
"#;
    let (out, c) = run_armed(src);
    assert_eq!(out, "a32\n");
    assert_eq!(c.live(), 0, "{c}");
}

#[test]
fn destructuring_a_fan_record_releases_it() {
    let src = r#"effect fn note(v: Int) -> Result[Int, String] = ok(v)

effect fn main() -> Unit = {
  let { x, y } = fan { x: note(3)!, y: note(4)! }
  println(int.to_string(x + y))
}
"#;
    let (out, c) = run_armed(src);
    assert_eq!(out, "7\n");
    assert_eq!(c.live(), 0, "{c}");
}
