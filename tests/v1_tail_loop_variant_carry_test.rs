//! A self-recursive parser that carries a variant value through a pure call
//! in its tail call (`calls(ts, p1, list.fold(args, f, ..))`, `f` a variant).
//! The loop conversion classed the carry with the accumulators and started
//! its slot as `f + []`, a copy only a list or a String has; at a variant
//! type the bind refused ("BinOp heap bind cannot be faithfully
//! materialized"). A carry of any other type starts as a `var` bound to the
//! parameter, as an owned-only carry of that type already did.
use std::process::Command;

/// A variant grown by a fold in two tail calls, with the propagating
/// destructures between them, on the ok path, the err path, and in a loop.
const SHAPE: &str = r##"type Raw = | RVar(String) | RApp(Raw, Raw) | RField(Raw, String)

fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn ident(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => ok((t, p + 1)),
    none => err("eof"),
  }

fn args_list(ts: List[String], p: Int, acc: List[Raw]) -> Result[(List[Raw], Int), String] = {
  let (a, p1) = ident(ts, p)!
  let acc2 = acc + [RVar(a)]
  if is_sym(ts, p1, ",") then args_list(ts, p1 + 1, acc2)
  else if is_sym(ts, p1, ")") then ok((acc2, p1 + 1))
  else err("expected )")
}

fn calls(ts: List[String], p: Int, f: Raw) -> Result[(Raw, Int), String] =
  if is_sym(ts, p, "(") then {
    let (args, p1) = args_list(ts, p + 1, [])!
    calls(ts, p1, list.fold(args, f, (acc, a) => RApp(acc, a)))
  } else if is_sym(ts, p, ".") then {
    let (x, p1) = ident(ts, p + 1)!
    calls(ts, p1, list.fold(string.split(x, "/"), f, (acc, g) => RField(acc, g)))
  } else ok((f, p))

fn show(r: Raw) -> String = match r {
  RVar(s) => s,
  RApp(f, a) => "${show(f)}(${show(a)})",
  RField(f, g) => "${show(f)}.${g}",
}

fn run(ts: List[String]) -> String = match calls(ts, 0, RVar("f")) {
  ok((r, p)) => "${show(r)} ${int.to_string(p)}",
  err(e) => e,
}

fn main() -> Unit = {
  println(run(["(", "a", ",", "b", ")", ".", "x/y", "(", "c", ")"]))
  println(run(["(", "a"]))
  println(run([]))
  var n = 0
  for i in 0..<20000 {
    n = n + string.len(run(["(", "a", ",", int.to_string(i), ")", ".", "x/y", "(", "c", ")"]))
  }
  println(int.to_string(n))
}
"##;

const SHAPE_STDOUT: &str = "f(a)(b).x.y(c) 10\nexpected )\nf 0\n408890\n";

/// Carries that keep the copy: a list accumulator and a String accumulator.
const CONTROL: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn gather(ts: List[String], p: Int, acc: List[String]) -> (List[String], Int) =
  if is_sym(ts, p, ")") then (acc, p + 1)
  else if p >= list.len(ts) then (acc, p)
  else gather(ts, p + 1, acc + [list.get(ts, p) ?? ""])

fn joined(ts: List[String], p: Int, acc: String) -> String =
  if p >= list.len(ts) then acc
  else joined(ts, p + 1, acc + (list.get(ts, p) ?? ""))

fn main() -> Unit = {
  let (xs, p) = gather(["a", "b", ")", "c"], 0, ["seed"])
  println("${list.join(xs, ",")} ${int.to_string(p)}")
  println(joined(["a", "b", "c"], 0, ">"))
}
"##;

const CONTROL_STDOUT: &str = "seed,a,b 3\n>abc\n";

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
fn a_variant_carried_through_a_pure_call_in_a_tail_call_becomes_a_loop() {
    runs_unwalled("tail_loop_variant_carry", SHAPE, SHAPE_STDOUT);
}

#[test]
fn list_and_string_accumulators_keep_their_copy() {
    runs_unwalled("tail_loop_variant_carry_control", CONTROL, CONTROL_STDOUT);
}
