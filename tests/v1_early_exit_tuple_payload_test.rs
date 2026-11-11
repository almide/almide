//! A `!` whose ok payload is a tuple holding heap values other than Strings — a
//! variant, a record, a list, a map, an Option, a nested tuple — leaves the
//! function by the early return on the certified native render (#4640), in a
//! loop body, an `if` arm, an arm's value and a call argument. The err leaves
//! as the callee's own carrier when callee and fn share one type, and as a
//! fresh err of the fn's type otherwise: a generated carrier's, the scalar-Ok
//! built-in carrier's (`Result[Int, String]`) or the String-Ok built-in
//! carrier's (`Result[String, String]`).
//!
//! Each program calls its fn on inputs that stay on the ok path and on inputs
//! that reach the err, and runs on the native leg under the ownership
//! certificate with no fallback to the standard codegen, and on wasm.
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

/// A `(variant, Int)` payload as an `if` arm's value; the fn's carrier is a generated one, so the err leaves as a fresh err of this fn's type.
const TREE_INT_ARM_VALUE: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(Tree, Int), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((Node(Leaf(x), Tag("t" + s(x))), x))
fn mkf(x: Int) -> Result[(Tree, Int), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((Node(Leaf(x), Tag("t" + s(x))), x))
fn usep(p: (Tree, Int)) -> Int = {
  let (a, b) = p
  size(a) + b
}
fn f(n: Int) -> Result[(String, Int), String] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for j in 0..<2 {
    let (a, b) = if n > j then mk(n + j)! else (Node(Leaf(j), Tag("t" + s(j))), j)
    tot = tot + size(a) + b
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok(("g" + s(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p.0),
      err(e) => println("err " + e),
    }
  }
}
"##;

/// A `(List[String], Bool)` payload as an `if` arm's value; the fn returns `Result[Int, String]`, the scalar-Ok built-in carrier.
const STRS_BOOL_SCALAR_CARRIER: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(List[String], Bool), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((["a" + s(x), "b"], x % 2 == 0))
fn mkf(x: Int) -> Result[(List[String], Bool), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((["a" + s(x), "b"], x % 2 == 0))
fn usep(p: (List[String], Bool)) -> Int = {
  let (a, b) = p
  list.len(a) + (if b then 1 else 0)
}
fn f(n: Int) -> Result[Int, String] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for j in 0..<2 {
    let (a, b) = if n > j then mk(n + j)! else (["a" + s(j), "b"], j % 2 == 0)
    tot = tot + list.len(a) + (if b then 1 else 0)
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok(tot)
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + s(p)),
      err(e) => println("err " + e),
    }
  }
}
"##;

/// A `(List[variant], record)` payload as an `if` arm's value; the fn returns `Result[String, String]`, the String-Ok built-in carrier.
const TREES_REC_STRING_CARRIER: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(List[Tree], Rc), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok(([Leaf(x), Tag("q" + s(x))], { name: "z" + s(x), n: x }))
fn mkf(x: Int) -> Result[(List[Tree], Rc), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok(([Leaf(x), Tag("q" + s(x))], { name: "z" + s(x), n: x }))
fn usep(p: (List[Tree], Rc)) -> Int = {
  let (a, b) = p
  list.len(a) + b.n + string.len(b.name)
}
fn f(n: Int) -> Result[String, String] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for j in 0..<2 {
    let (a, b) = if n > j then mk(n + j)! else ([Leaf(j), Tag("q" + s(j))], { name: "z" + s(j), n: j })
    tot = tot + list.len(a) + b.n + string.len(b.name)
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok("s" + s(tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p),
      err(e) => println("err " + e),
    }
  }
}
"##;

/// A `(List[(String, variant)], variant)` payload handed to a call as its argument, with a variant error type.
const PAIRS_TREE_CALL_ARG: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(List[(String, Tree)], Tree), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok(([("p" + s(x), Leaf(x))], Tag("w" + s(x))))
fn mkf(x: Int) -> Result[(List[(String, Tree)], Tree), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok(([("p" + s(x), Leaf(x))], Tag("w" + s(x))))
fn usep(p: (List[(String, Tree)], Tree)) -> Int = {
  let (a, b) = p
  list.len(a) + size(b)
}
fn f(n: Int) -> Result[(String, Int), Fail] = {
  var tot = 0
  for i in 0..<n {
    tot = tot + usep(mkf(i)!)
  }
  ok(("f" + s(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p.0),
      err(e) => println("err " + showf(e)),
    }
  }
}
"##;

/// A `((String, variant), Int)` payload as an `if` arm's value; callee and fn have one type, so the carrier is handed back as it stands.
const NESTED_SAME_TYPE: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[((String, Tree), Int), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((inner(x), x))
fn mkf(x: Int) -> Result[((String, Tree), Int), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((inner(x), x))
fn usep(p: ((String, Tree), Int)) -> Int = {
  let (a, b) = p
  string.len(a.0) + size(a.1) + b
}
fn f(n: Int) -> Result[((String, Tree), Int), String] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for j in 0..<2 {
    let (a, b) = if n > j then mk(n + j)! else (inner(j), j)
    tot = tot + string.len(a.0) + size(a.1) + b
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok((inner(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + s(usep(p))),
      err(e) => println("err " + e),
    }
  }
}
"##;

/// A `(Map[String, Int], String)` payload destructured in a `while` body, with a variant error type and heap locals live at the exit.
const MAP_STR_WHILE: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(Map[String, Int], String), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((map.from_list([("k" + s(x), x)]), "v" + s(x)))
fn mkf(x: Int) -> Result[(Map[String, Int], String), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((map.from_list([("k" + s(x), x)]), "v" + s(x)))
fn usep(p: (Map[String, Int], String)) -> Int = {
  let (a, b) = p
  map.len(a) + string.len(b)
}
fn f(n: Int) -> Result[(String, Int), Fail] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  var i = 0
  while i < n {
    let (a, b) = mkf(i)!
    tot = tot + map.len(a) + string.len(b)
    i = i + 1
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok(("f" + s(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p.0),
      err(e) => println("err " + showf(e)),
    }
  }
}
"##;

/// An `(Option[String], Int)` payload as an `if` arm's value under the scalar-Ok built-in carrier, with heap locals live at the exit.
const OPTION_ARM_VALUE: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(Option[String], Int), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((optv(x), x))
fn mkf(x: Int) -> Result[(Option[String], Int), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((optv(x), x))
fn usep(p: (Option[String], Int)) -> Int = {
  let (a, b) = p
  string.len(a ?? "") + b
}
fn f(n: Int) -> Result[Int, String] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for j in 0..<2 {
    let (a, b) = if n > j then mk(n + j)! else (optv(j), j)
    tot = tot + string.len(a ?? "") + b
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok(tot)
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + s(p)),
      err(e) => println("err " + e),
    }
  }
}
"##;

/// A `(record, Int)` payload destructured in an `if` arm inside a loop, with a variant error type.
const RECORD_IF_ARM: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(Rc, Int), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok(({ name: "r" + s(x), n: x }, x + 1))
fn mkf(x: Int) -> Result[(Rc, Int), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok(({ name: "r" + s(x), n: x }, x + 1))
fn usep(p: (Rc, Int)) -> Int = {
  let (a, b) = p
  string.len(a.name) + a.n + b
}
fn f(n: Int) -> Result[(String, Int), Fail] = {
  let h1 = "h" + s(n)
  var acc: List[String] = []
  list.push(acc, h1)
  var tot = 0
  for i in 0..<n {
    if i % 2 == 1 then {
      let (a, b) = mkf(i)!
      tot = tot + string.len(a.name) + a.n + b
    }
  }
  tot = tot + list.len(acc) + string.len(h1)
  ok(("f" + s(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p.0),
      err(e) => println("err " + showf(e)),
    }
  }
}
"##;

/// An `(Int, variant, String)` payload bound whole in a `for` body, with a variant error type.
const TRIPLE_FOR_BODY: &str = r##"type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)
  | Many(List[Tree])
  | Tag(String)
type Rc = { name: String, n: Int }
type Fail =
  | Plain
  | Why(String)
fn size(t: Tree) -> Int = match t {
  Leaf(_) => 1,
  Node(l, r) => size(l) + size(r),
  Many(xs) => list.fold(xs, 0, (a, x) => a + size(x)),
  Tag(s) => string.len(s),
}
fn showf(e: Fail) -> String = match e {
  Plain => "plain",
  Why(s) => "why:" + s,
}
fn s(x: Int) -> String = int.to_string(x)
fn optv(x: Int) -> Option[String] = if x % 2 == 0 then some("o" + s(x)) else none
fn inner(x: Int) -> (String, Tree) = ("n" + s(x), Leaf(x))
fn mk(x: Int) -> Result[(Int, Tree, String), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((x, Node(Leaf(x), Leaf(1)), "c" + s(x)))
fn mkf(x: Int) -> Result[(Int, Tree, String), Fail] =
  if x % 4 == 3 then err(Why("w" + s(x))) else ok((x, Node(Leaf(x), Leaf(1)), "c" + s(x)))
fn usep(p: (Int, Tree, String)) -> Int = {
  let (a, b, c) = p
  a + size(b) + string.len(c)
}
fn f(n: Int) -> Result[(String, Int), Fail] = {
  var tot = 0
  for i in 0..<n {
    let p = mkf(i)!
    tot = tot + usep(p)
  }
  ok(("f" + s(tot), tot))
}
effect fn main() -> Unit = {
  for k in 0..<7 {
    match f(k) {
      ok(p) => println("ok " + p.0),
      err(e) => println("err " + showf(e)),
    }
  }
}
"##;

#[test]
fn tree_int_arm_value() {
    renders_and_agrees("tree_int_arm_value", TREE_INT_ARM_VALUE, "ok g10\nok g11\nerr bad 3\nerr bad 3\nok g18\nok g20\nerr bad 7\n");
}

#[test]
fn strs_bool_scalar_carrier() {
    renders_and_agrees("strs_bool_scalar_carrier", STRS_BOOL_SCALAR_CARRIER, "ok 8\nok 7\nerr bad 3\nerr bad 3\nok 8\nok 8\nerr bad 7\n");
}

#[test]
fn trees_rec_string_carrier() {
    renders_and_agrees("trees_rec_string_carrier", TREES_REC_STRING_CARRIER, "ok s12\nok s13\nerr bad 3\nerr bad 3\nok s20\nok s22\nerr bad 7\n");
}

#[test]
fn pairs_tree_call_arg() {
    renders_and_agrees("pairs_tree_call_arg", PAIRS_TREE_CALL_ARG, "ok f0\nok f3\nok f6\nok f9\nerr why:w3\nerr why:w3\nerr why:w3\n");
}

#[test]
fn nested_same_type() {
    renders_and_agrees("nested_same_type", NESTED_SAME_TYPE, "ok 14\nok 15\nerr bad 3\nerr bad 3\nok 22\nok 24\nerr bad 7\n");
}

#[test]
fn map_str_in_while() {
    renders_and_agrees("map_str_in_while", MAP_STR_WHILE, "ok f3\nok f6\nok f9\nok f12\nerr why:w3\nerr why:w3\nerr why:w3\n");
}

#[test]
fn option_arm_value() {
    renders_and_agrees("option_arm_value", OPTION_ARM_VALUE, "ok 6\nok 5\nerr bad 3\nerr bad 3\nok 14\nok 16\nerr bad 7\n");
}

#[test]
fn record_in_if_arm() {
    renders_and_agrees("record_in_if_arm", RECORD_IF_ARM, "ok f3\nok f3\nok f8\nok f8\nerr why:w3\nerr why:w3\nerr why:w3\n");
}

#[test]
fn triple_in_for_body() {
    renders_and_agrees("triple_in_for_body", TRIPLE_FOR_BODY, "ok f0\nok f4\nok f9\nok f15\nerr why:w3\nerr why:w3\nerr why:w3\n");
}
