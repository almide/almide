//! Declared variants on the certified native render (#4640): equality over
//! a payload slot that constructors use for different heap kinds, and a
//! `match` over a declared variant as a call argument or a destructured bind.
//!
//! The equality engine read both operands' payloads with no declared type, so
//! a slot holding a String in one constructor and a list in another had no
//! single typed read and the fn was refused; each read now carries the
//! compared constructor's field type. A `match` operand ran the value routers
//! only for an Option/Result subject; a declared variant subject now takes
//! the same router the let-bound match does.
use std::process::Command;

/// The program on the native leg under the ownership certificate with no
/// fallback to the standard codegen, and on wasm: both print `expected`.
fn renders_and_agrees(name: &str, source: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, source).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} {extra:?}");
    }
}

/// One payload slot: a String, a list, an Int, nothing.
const MIXED_SLOT_EQ: &str = r##"type Op2 = | Name(String) | Nums(List[Int]) | Lit(Int) | Nil

effect fn main() -> Unit = {
  let a = Name("x")
  let b = Nums([1, 2])
  println(if a == b then "eq" else "ne")
  println(if a == Name("x") then "eq" else "ne")
  println(if b == Nums([1, 2]) then "eq" else "ne")
  println(if b == Nums([1, 3]) then "eq" else "ne")
  println(if Lit(3) == Lit(3) then "eq" else "ne")
  println(if Nil == Lit(3) then "eq" else "ne")
}
"##;

const MIXED_SLOT_EQ_STDOUT: &str = "ne\neq\neq\nne\neq\nne\n";

/// A destructured match over a variant with heap tuples in every arm, then loops and index reads.
const EMIT: &str = r##"type Rule = | Eps | Tok(String) | Seq(List[Rule]) | Rep(Rule) | Left(String, Rule, Rule) | Prec(String, Rule, List[Rule])
type Op = { kind: Int, a: Int, b: Int }

fn emit(r: Rule, mut ops: List[Op], mut texts: List[String], mut kids: List[Int]) -> Int = {
  let (kind, text, subs) = match r {
    Eps => (0, "", []),
    Tok(k) => (1, k, []),
    Seq(xs) => (2, "", xs),
    Rep(x) => (3, "", [x]),
    Left(k, x, y) => (4, k, [x, y]),
    Prec(k, x, ys) => (5, k, [x] + ys),
  }
  var ids: List[Int] = []
  for s in subs {
    list.push(ids, emit(s, ops, texts, kids))
  }
  let op = if kind == 2 or kind == 5 then {
    let first = list.len(kids)
    for id in ids { list.push(kids, id) }
    Op { kind: kind, a: first, b: list.len(ids) }
  } else if kind == 4 then Op { kind: kind, a: list.get(ids, 0) ?? 0, b: list.get(ids, 1) ?? 0 }
  else Op { kind: kind, a: list.get(ids, 0) ?? -1, b: 0 }
  list.push(ops, op)
  list.push(texts, text)
  list.len(ops) - 1
}

effect fn main() -> Unit = {
  var ops: List[Op] = []
  var texts: List[String] = []
  var kids: List[Int] = []
  let root = emit(Seq([Tok("a"), Rep(Tok("b")), Left("l", Eps, Tok("c")), Prec("p", Tok("d"), [Eps, Tok("e")])]), ops, texts, kids)
  println(int.to_string(root))
  for o in ops { println("${o.kind} ${o.a} ${o.b}") }
  println(list.join(texts, ","))
  println(kids |> list.map((k) => int.to_string(k)) |> list.join(","))
}
"##;

const EMIT_STDOUT: &str = "10\n1 -1 0\n1 -1 0\n3 1 0\n0 -1 0\n1 -1 0\n4 3 4\n1 -1 0\n0 -1 0\n1 -1 0\n5 0 3\n2 3 4\na,b,,,c,l,d,,e,p,\n6,7,8,0,2,5,9\n";

#[test]
fn equality_reads_each_constructors_own_payload_kind() {
    renders_and_agrees("mixed_slot_eq", MIXED_SLOT_EQ, MIXED_SLOT_EQ_STDOUT);
}

#[test]
fn a_destructured_match_over_a_declared_variant_runs_its_arm() {
    renders_and_agrees("emit", EMIT, EMIT_STDOUT);
}
