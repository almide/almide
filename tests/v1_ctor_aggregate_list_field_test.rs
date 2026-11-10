//! `ok((Ctor(name, xs), p))` in a branch arm where `xs` is a list of tuples
//! or of records bound from a destructure (`Record(String, List[(String, T)])`,
//! `Match(T, List[Arm])` — a parser's tree builders). A constructor took a
//! list field only when its elements were scalars, strings or the variant
//! itself, so these functions were refused ("heap-result `if` outside the
//! executable subset") and a file holding them could not build its tests.
use std::process::Command;

/// A constructor holding a list of pairs and one holding a list of records:
/// built in a branch arm from a destructured propagation, nested in each
/// other, on the ok and err paths, in a hot loop — and built from a list the
/// function then changes, which the built value must not follow.
const SHAPE: &str = r##"type Raw =
  | RName(String)
  | RRecord(String, List[(String, Raw)])
  | RMatch(Raw, List[Arm])
  | RList(List[Raw])

type Arm = { ctor: String, body: Raw }

fn show(r: Raw) -> String =
  match r {
    RName(s) => s,
    RRecord(s, fs) => s + "{" + (fs |> list.map(((k, v)) => k + ": " + show(v)) |> list.join(", ")) + "}",
    RMatch(x, as) => "match " + show(x) + " [" + (as |> list.map((a) => a.ctor + " => " + show(a.body)) |> list.join(" | ")) + "]",
    RList(xs) => "[" + (xs |> list.map((x) => show(x)) |> list.join(", ")) + "]",
  }

fn fields(n: Int) -> Result[(List[(String, Raw)], Int), String] =
  if n < 0 then err("neg") else ok(([("a", RName("x")), ("b", RName("y"))], n + 2))

fn arms(n: Int) -> Result[(List[Arm], Int), String] =
  if n < 0 then err("neg") else ok(([{ ctor: "c", body: RName("z") }], n + 1))

fn rec(name: String, n: Int) -> Result[(Raw, Int), String] =
  if n > 10 then err("big")
  else {
    let (fs, p1) = fields(n)!
    ok((RRecord(name, fs), p1))
  }

fn mat(name: String, n: Int) -> Result[(Raw, Int), String] =
  if n > 10 then err("big")
  else {
    let (as, p1) = arms(n)!
    ok((RMatch(RName(name), as), p1))
  }

fn nested(name: String, n: Int) -> Result[(Raw, Int), String] = {
  let (inner, p1) = rec(name, n)!
  let (m, p2) = mat(name, p1)!
  ok((RRecord("outer", [("r", inner), ("m", m)]), p2))
}

fn kept(name: String) -> String = {
  var fs = [("a", RName("x"))]
  let r = RRecord(name, fs)
  fs = fs + [("b", RName("y"))]
  show(r) + " / " + int.to_string(list.len(fs))
}

fn run(r: Result[(Raw, Int), String]) -> String =
  match r {
    ok((x, p)) => "${show(x)} ${int.to_string(p)}",
    err(e) => e,
  }

effect fn main() -> Unit = {
  println(run(rec("P", 1)))
  println(run(rec("P", -1)))
  println(run(rec("P", 11)))
  println(run(mat("m", 1)))
  println(run(mat("m", -1)))
  println(run(nested("N", 2)))
  println(run(nested("N", 9)))
  println(kept("K"))
  var total = 0
  for i in 0..<200000 {
    total = total + (match nested("N", i % 12) {
      ok((x, p)) => string.len(show(x)) + p,
      err(e) => string.len(e),
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "P{a: x, b: y} 3\nneg\nbig\nmatch m [c => z] 2\nneg\nouter{r: N{a: x, b: y}, m: match N [c => z]} 5\nbig\nK{a: x} / 2\n7800092\n";

/// The spellings that never met the refusal: a list of the variant itself,
/// nested in itself.
const CONTROL: &str = r##"type Raw =
  | RName(String)
  | RList(List[Raw])

fn show(r: Raw) -> String =
  match r {
    RName(s) => s,
    RList(xs) => "[" + (xs |> list.map((x) => show(x)) |> list.join(", ")) + "]",
  }

fn items(n: Int) -> Result[(List[Raw], Int), String] =
  if n < 0 then err("neg") else ok(([RName("x"), RList([RName("y")])], n + 2))

fn lst(n: Int) -> Result[(Raw, Int), String] =
  if n > 10 then err("big")
  else {
    let (xs, p1) = items(n)!
    ok((RList(xs), p1))
  }

fn run(r: Result[(Raw, Int), String]) -> String =
  match r {
    ok((x, p)) => "${show(x)} ${int.to_string(p)}",
    err(e) => e,
  }

effect fn main() -> Unit = {
  println(run(lst(1)))
  println(run(lst(-1)))
  println(run(lst(11)))
}
"##;

const CONTROL_STDOUT: &str = "[x, [y]] 3\nneg\nbig\n";

/// A constructor holding a list of its own variant where another constructor
/// of the type holds a list of pairs (a config tree: scalar, sequence, table):
/// built from a call result in a tuple arm, shared between two values, built
/// before and after the list grows, nested, and in a hot loop.
const VARIANT_LIST: &str = r##"type Y =
  | Sc(String)
  | Sq(List[Y])
  | Tb(List[(String, Y)])

fn inl(r: String) -> List[Y] = [Sc(r), Sc(r + "!")]

fn inlt(r: String) -> List[(String, Y)] = [(r, Sc(r))]

fn value_of(i: Int, rest: String) -> (Y, Int) =
  if rest == "" then (Sc("empty"), i)
  else if string.starts_with(rest, "[") then (Sq(inl(rest)), i + 1)
  else if string.starts_with(rest, "{") then (Tb(inlt(rest)), i + 1)
  else (Sc(string.trim(rest)), i + 1)

fn show(y: Y) -> String =
  match y {
    Sc(s) => "sc:" + s,
    Sq(xs) => "sq[" + list.join(list.map(xs, (x) => show(x)), ",") + "]",
    Tb(es) => "tb{" + list.join(list.map(es, (e) => e.0 + "=" + show(e.1)), ",") + "}",
  }

fn depth(y: Y) -> Int =
  match y {
    Sc(_) => 0,
    Sq(xs) => 1 + list.fold(xs, 0, (m, x) => int.max(m, depth(x))),
    Tb(es) => 1 + list.fold(es, 0, (m, e) => int.max(m, depth(e.1))),
  }

fn wrap(n: Int, y: Y) -> Y =
  if n == 0 then y else wrap(n - 1, Sq([y, Sc(int.to_string(n))]))

fn shared(r: String) -> (Y, Y) = {
  let xs = inl(r)
  let a = Sq(xs)
  let b = Sq(xs + [Sc("more")])
  (a, b)
}

fn grow(r: String) -> Y = {
  var xs = inl(r)
  let before = Sq(xs)
  list.push(xs, Sc("pushed"))
  let after = Sq(xs)
  Sq([before, after])
}

fn main() -> Unit = {
  for r in ["", "[a", "{b", " c "] {
    let (y, j) = value_of(3, r)
    println(show(y) + " " + int.to_string(j))
  }
  let (a, b) = shared("s")
  println(show(a) + " | " + show(b))
  println(show(grow("g")))
  let w = wrap(4, Sq(inl("w")))
  println(show(w) + " depth " + int.to_string(depth(w)))
  var total = 0
  for i in 0..<200000 {
    let (y, j) = value_of(i, if i % 3 == 0 then "[x" else if i % 3 == 1 then "{y" else "z")
    total = total + depth(y) + j % 7
    let s = Sq(inl("k"))
    total = total + depth(Sq([s, y]))
  }
  println(int.to_string(total))
}
"##;

const VARIANT_LIST_STDOUT: &str = "sc:empty 3\nsq[sc:[a,sc:[a!] 4\ntb{{b=sc:{b} 4\nsc:c 4\nsq[sc:s,sc:s!] | sq[sc:s,sc:s!,sc:more]\nsq[sq[sc:g,sc:g!],sq[sc:g,sc:g!,sc:pushed]]\nsq[sq[sq[sq[sq[sc:w,sc:w!],sc:4],sc:3],sc:2],sc:1] depth 5\n1133331\n";

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
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
fn a_constructor_holding_a_list_of_pairs_or_records_builds_in_a_branch_arm() {
    runs_unwalled("ctor_aggregate_list_field", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_constructor_holding_a_list_of_itself_runs_as_before() {
    runs_unwalled("ctor_aggregate_list_field_control", CONTROL, CONTROL_STDOUT);
}

#[test]
fn a_constructor_holding_a_list_of_its_variant_beside_a_pair_list_builds_in_a_tuple_arm() {
    runs_unwalled("ctor_variant_list_field", VARIANT_LIST, VARIANT_LIST_STDOUT);
}
