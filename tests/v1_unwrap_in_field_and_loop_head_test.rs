//! A `!` nested in a record literal's field (`ok(Param { name: .., ty: parse(..)! })`)
//! or under a call in a `for` head (`for part in string.split(read(p)!, sep)`).
//! The ordered lift that turns a nested `!` into a `let` before its statement
//! walked calls, tuples, lists and operators but neither record fields nor a
//! loop's iterable, so the first fell to "heap-result `match` outside the
//! executable subset" and the second to "unwrap `!` in a call-argument
//! position". Both are now lifted like an argument: operands written before
//! the `!` are bound first, in order.
use std::process::Command;

/// The record-field `!` in a match arm of a Result fn, and the `!` under two
/// calls in a loop head, each with its failing input.
const SHAPE: &str = r##"type Param = { name: String, ty: String }

fn parse_ty(s: String) -> Result[String, String] =
  if s == "" then err("empty type") else ok(string.trim(s))

fn parse_param(raw: String) -> Result[Param, String] = {
  let s = string.trim(raw)
  match string.index_of(s, ":") {
    none => err("parameter without a type: `" + s + "`"),
    some(i) => ok(Param { name: string.trim(string.take(s, i)), ty: parse_ty(string.drop(s, i + 1))! }),
  }
}

fn show(r: Result[Param, String]) -> String = match r {
  ok(p) => "ok ${p.name}: ${p.ty}",
  err(e) => "err ${e}",
}

fn words(src: String) -> Result[String, String] =
  if src == "" then err("no words") else ok(src)

effect fn lengths(src: String, sep: String) -> Int = {
  var n = 0
  for part in string.split(string.trim(words(src)!), sep) {
    if part != "" then {
      n = n + string.len(part)
      println("part " + part)
    } else ()
  }
  n
}

effect fn main() -> Unit = {
  println(show(parse_param("x: Int")))
  println(show(parse_param("x")))
  println(show(parse_param("x:")))
  println(int.to_string(lengths(" ab,,cde ", ",")!))
  match lengths("", ",") {
    ok(n) => println("ok ${n}"),
    err(e) => println("err " + e),
  }
}
"##;

const SHAPE_STDOUT: &str = "ok x: Int\nerr parameter without a type: `x`\nerr empty type\npart ab\npart cde\n5\nerr no words\n";

/// Order of evaluation: a field written before the failing `!` has already
/// run when the error propagates, one written after it never runs, and a
/// failing `!` in an inner loop head stops the outer loop.
const CONTROL: &str = r##"type Pair = { left: String, right: String }

fn note(tag: String) -> Result[String, String] = {
  println("ran " + tag)
  if string.starts_with(tag, "bad") then err("failed at " + tag) else ok(tag)
}

fn plain(tag: String) -> String = {
  println("ran " + tag)
  tag
}

fn build(a: String, b: String) -> Result[Pair, String] =
  ok(Pair { left: plain(a), right: note(b)! })

fn build_first(a: String, b: String) -> Result[Pair, String] =
  ok(Pair { left: note(a)!, right: plain(b) })

fn show(r: Result[Pair, String]) -> String = match r {
  ok(p) => "ok " + p.left + "/" + p.right,
  err(e) => "err " + e,
}

effect fn each(tags: List[String]) -> Int = {
  var n = 0
  for t in tags {
    for part in string.split(note(t)!, "-") {
      n = n + 1
    }
  }
  n
}

effect fn main() -> Unit = {
  println(show(build("a", "b")))
  println(show(build("c", "bad1")))
  println(show(build_first("bad2", "d")))
  println(show(build_first("e", "f")))
  match each(["p-q", "bad3", "never"]) {
    ok(n) => println("ok ${n}"),
    err(e) => println("err " + e),
  }
  println(int.to_string(each(["r-s-t", "u"])!))
}
"##;

const CONTROL_STDOUT: &str = "ran a\nran b\nok a/b\nran c\nran bad1\nerr failed at bad1\nran bad2\nerr failed at bad2\nran e\nran f\nok e/f\nran p-q\nran bad3\nerr failed at bad3\nran r-s-t\nran u\n4\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src).unwrap_or_else(|e| panic!("{name} walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_bang_in_a_record_field_or_under_a_loop_head_call_propagates() {
    runs_unwalled("unwrap_field_head", SHAPE, SHAPE_STDOUT);
}

#[test]
fn fields_and_heads_around_a_failing_bang_run_in_written_order() {
    runs_unwalled("unwrap_field_head_order", CONTROL, CONTROL_STDOUT);
}
