// ── eval_test.rs, part 4: result.to_list over every payload class ──
//
// include!-spliced into `tests/eval_test.rs`; the `run` helper is the test
// binary's own. The 2026-09-04 nightly recorded 23 findings on ONE shape —
// `result.to_list(ok(v))` answering `[]` (a String / Bool / Float payload)
// or its raw slot once `prim.handle` of a Result stopped abstaining and the
// Int-typed registry body (`result_to_list`, stdlib/result_core.almd) ran
// for every payload type. `to_list` is value-level now, like
// `option.to_list`; these pin every payload class against the backends'
// rendering (measured native first, per crates/almide-interp/CLAUDE.md).

#[test]
fn result_to_list_over_every_payload_class() {
    let src = r#"fn main() -> Unit = {
  let i: Result[Int, String] = ok(1)
  let s: Result[String, String] = ok("Hello, World!")
  let b: Result[Bool, String] = ok(true)
  let f: Result[Float, String] = ok(2.718281828459045)
  let u: Result[Unit, String] = ok(())
  let l: Result[List[Int], String] = ok([1, 2])
  let e: Result[Int, String] = err("boom")
  let es: Result[String, String] = err("boom")
  println("${result.to_list(i)}")
  println("${result.to_list(s)}")
  println("${result.to_list(b)}")
  println("${result.to_list(f)}")
  println("${list.len(result.to_list(u))}")
  println("${result.to_list(l)}")
  println("${result.to_list(e)}")
  println("${result.to_list(es)}")
  println("${list.len(result.to_list(es))}")
}
"#;
    let (code, out, err) = run(src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out.trim(),
        "[1]\n[\"Hello, World!\"]\n[true]\n[2.718281828459045]\n1\n[[1, 2]]\n[]\n[]\n0"
    );
}

#[test]
fn result_to_list_feeds_the_list_family() {
    // The shapes the findings folded / joined / measured: the list a
    // `to_list` yields is an ordinary list to every combinator.
    let src = r#"fn main() -> Unit = {
  let a: Result[String, String] = ok("")
  let b: Result[Bool, String] = ok(true)
  let c: Result[String, String] = ok("Ⅷ")
  println("${list.fold(result.to_list(a), 3, ((acc, _x) => acc + 1))}")
  println("${list.is_empty(result.to_list(b))}")
  println("${string.len(list.join(result.to_list(c), "😀"))}")
}
"#;
    let (code, out, err) = run(src);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "4\nfalse\n1");
}
