//! A `!` in an operand of a destructuring bind's value, on the certified
//! native render (#4640).
//!
//! `let (a, b) = f(x, g(y)!)` holds its propagation in a call argument. The
//! operand hoist that brings such a `!` to its own `let` read plain binds,
//! assignments and expression statements, not destructuring binds, so the `!`
//! stayed inside the value: in a loop body the loop's error flag form declined
//! it and the fn was refused.
//!
//! The program runs on the native leg under the ownership certificate, with no
//! fallback to the standard codegen, and on wasm; both print the expected text.
use std::process::Command;

/// The destructuring bind inside a `match` arm of a `for` body, with a nested
/// loop after it, on the ok path and an err path; and in straight-line code.
const DESTRUCTURE_OPERAND: &str = r##"fn is_test(p: String) -> Bool = string.ends_with(p, "_test")

fn lang_of(p: String) -> String? = if string.starts_with(p, "a") then some("A") else none

fn other(p: String) -> String? = if string.starts_with(p, "b") then some("B") else none

effect fn sym(p: String) -> Result[Int, String] =
  if string.len(p) > 4 then err("too long " + p) else ok(string.len(p))

fn fact(p: String, lg: String, n: Int, m: Int) -> (String, List[Int]) = (p + ":" + lg, [n, m])

effect fn gather(paths: List[String]) -> Result[Int, String] = {
  var fs: List[String] = []
  var funcs: List[Int] = []
  var unsupported: List[(String, String)] = []
  for p in paths {
    if is_test(p) then ()
    else match lang_of(p) {
      some(lg) => {
        let abs = "/" + p
        let (ff, fn_list) = fact(p, lg, sym(abs)!, sym(p)!)
        list.push(fs, ff)
        for f in fn_list { list.push(funcs, f) }
      },
      none => match other(p) {
        some(l) => list.push(unsupported, (p, l)),
        none => (),
      },
    }
  }
  println(list.join(fs, ","))
  ok(list.len(fs) * 100 + list.len(funcs) * 10 + list.len(unsupported))
}

effect fn one(p: String) -> Result[String, String] = {
  let (ff, ns) = fact(p, "L", sym(p)!, sym(p + p)!)
  ok(ff + " " + int.to_string(list.len(ns)))
}

fn show(r: Result[Int, String]) -> String = match r {
  ok(k) => "ok " + int.to_string(k),
  err(e) => "err " + e,
}

effect fn main() -> Unit = {
  println(show(gather(["a1", "b1", "a_test", "c", "a22"])))
  println(show(gather(["a1", "a2345", "a3"])))
  println(show(gather([])))
  println(match one("ab") { ok(s) => s, err(e) => "err " + e })
  println(match one("abc") { ok(s) => s, err(e) => "err " + e })
}
"##;

const DESTRUCTURE_OPERAND_STDOUT: &str = "a1:A,a22:A\nok 241\nerr too long /a2345\n\nok 0\nab:L 2\nerr too long abcabc\n";

#[test]
fn a_propagation_in_an_operand_of_a_destructuring_bind_renders_and_propagates() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("destructure_operand.almd");
    std::fs::write(&file, DESTRUCTURE_OPERAND).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), DESTRUCTURE_OPERAND_STDOUT, "{extra:?}");
    }
}
