//! A list whose elements are tuples holding a variant, a map or an Option
//! grows in a loop on the certified native render (#4640): `list.push` in a
//! `while` body, a `for` body and an `if` arm inside a loop, `xs = xs + [t]`,
//! and a tuple that comes from a `let` or from a `!`. The list is then folded,
//! iterated or measured.
//!
//! Each program runs on the native leg under the ownership certificate with no
//! fallback to the standard codegen, and on wasm.
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

/// A `List[(variant, Int)]`: `list.push` in a `while` body.
const PUSH_WHILE_TREE_INT: &str = r##"type Tree =
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
fn usep(p: (Tree, Int)) -> Int = {
  let (a, b) = p
  size(a) + b
}
fn f(n: Int) -> Int = {
  var acc: List[(Tree, Int)] = []
  var i = 0
  while i < n {
    list.push(acc, (Node(Leaf(i), Tag("t" + s(i))), i))
    i = i + 1
  }
  let r = list.fold(acc, 0, (a, p) => a + usep(p))
  r
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    println(s(f(k)))
  }
}
"##;

/// A `List[(variant, variant)]`: `list.push` in a `for` body.
const PUSH_FOR_TREE_TREE: &str = r##"type Tree =
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
fn mk(x: Int) -> Result[(Tree, Tree), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((Leaf(x), Many([Leaf(x), Tag("m" + s(x))])))
fn usep(p: (Tree, Tree)) -> Int = {
  let (a, b) = p
  size(a) * 10 + size(b)
}
fn f(n: Int) -> Int = {
  var acc: List[(Tree, Tree)] = []
  for i in 0..<n {
    list.push(acc, (Leaf(i), Many([Leaf(i), Tag("m" + s(i))])))
  }
  var r = 0
  for p in acc {
    r = r + usep(p)
  }
  r
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    println(s(f(k)))
  }
}
"##;

/// A `List[(Map[String, Int], String)]`: `list.push` in an `if` arm inside a loop.
const PUSH_IF_MAP_STR: &str = r##"type Tree =
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
fn usep(p: (Map[String, Int], String)) -> Int = {
  let (a, b) = p
  map.len(a) + string.len(b)
}
fn f(n: Int) -> Int = {
  var acc: List[(Map[String, Int], String)] = []
  for i in 0..<n {
    if i % 2 == 0 then {
      list.push(acc, (map.from_list([("k" + s(i), i)]), "v" + s(i)))
    }
  }
  let r = list.fold(acc, 0, (a, p) => a + usep(p))
  r
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    println(s(f(k)))
  }
}
"##;

/// A `List[(Option[String], Int)]`: `xs = xs + [t]` in a `while` body.
const CONCAT_WHILE_OPT_INT: &str = r##"type Tree =
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
fn usep(p: (Option[String], Int)) -> Int = {
  let (a, b) = p
  string.len(a ?? "") + b
}
fn f(n: Int) -> Int = {
  var acc: List[(Option[String], Int)] = []
  var i = 0
  while i < n {
    acc = acc + [(optv(i), i)]
    i = i + 1
  }
  var r = 0
  for p in acc {
    r = r + usep(p)
  }
  r
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    println(s(f(k)))
  }
}
"##;

/// A `List[(variant, Int)]`: a tuple bound to a `let` and then pushed.
const PUSH_LET_TREE_INT: &str = r##"type Tree =
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
fn usep(p: (Tree, Int)) -> Int = {
  let (a, b) = p
  size(a) + b
}
fn f(n: Int) -> Int = {
  var acc: List[(Tree, Int)] = []
  for i in 0..<n {
    let t = (Node(Leaf(i), Tag("t" + s(i))), i)
    list.push(acc, t)
  }
  let r = list.len(acc) * 7
  r
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    println(s(f(k)))
  }
}
"##;

/// A `List[(variant, variant)]`: a tuple taken from a `!` in the loop body and then pushed.
const PUSH_BANG_TREE_TREE: &str = r##"type Tree =
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
fn mk(x: Int) -> Result[(Tree, Tree), String] =
  if x % 4 == 3 then err("bad " + s(x)) else ok((Leaf(x), Many([Leaf(x), Tag("m" + s(x))])))
fn usep(p: (Tree, Tree)) -> Int = {
  let (a, b) = p
  size(a) * 10 + size(b)
}
fn f(n: Int) -> Result[Int, String] = {
  var acc: List[(Tree, Tree)] = []
  for i in 0..<n {
    let p = mk(i)!
    list.push(acc, p)
  }
  let r = list.fold(acc, 0, (a, p) => a + usep(p))
  ok(r)
}
effect fn main() -> Unit = {
  for k in 0..<6 {
    match f(k) {
      ok(p) => println("ok " + s(p)),
      err(e) => println("err " + e),
    }
  }
}
"##;

#[test]
fn push_while_tree_int() {
    renders_and_agrees("push_while_tree_int", PUSH_WHILE_TREE_INT, "0\n3\n7\n12\n18\n25\n");
}

#[test]
fn push_for_tree_tree() {
    renders_and_agrees("push_for_tree_tree", PUSH_FOR_TREE_TREE, "0\n13\n26\n39\n52\n65\n");
}

#[test]
fn push_if_map_str() {
    renders_and_agrees("push_if_map_str", PUSH_IF_MAP_STR, "0\n3\n3\n6\n6\n9\n");
}

#[test]
fn concat_while_opt_int() {
    renders_and_agrees("concat_while_opt_int", CONCAT_WHILE_OPT_INT, "0\n2\n3\n7\n10\n16\n");
}

#[test]
fn push_let_tree_int() {
    renders_and_agrees("push_let_tree_int", PUSH_LET_TREE_INT, "0\n7\n14\n21\n28\n35\n");
}

#[test]
fn push_bang_tree_tree() {
    renders_and_agrees("push_bang_tree_tree", PUSH_BANG_TREE_TREE, "ok 0\nok 13\nok 26\nok 39\nerr bad 3\nerr bad 3\n");
}
