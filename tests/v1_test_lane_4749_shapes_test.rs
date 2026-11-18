//! Four shapes the native test lane refused after it moved to the certified
//! render only (#4749). Each was a test-lane compile error on v0.70.0-rc1
//! while `almide build` and the wasm lane ran it:
//!
//! - a `List[String]` literal whose element is a field of a call result
//!   (`[tok(ts, k).text]`): the list builder admitted a field element only
//!   over a tracked container, although the field borrow already binds a
//!   fresh container to a temp;
//! - a module-level String built in eight `let`s (`D2 = D1 + D1` …): the
//!   literal fold gave up at a fixed depth instead of folding what it can
//!   produce;
//! - two loop copies of outer lists where one branch rebinds the first and
//!   aliases the second onto it (`dnames = ["p"]; need = dnames`): the
//!   ownership verifier unified the two slots from a stale snapshot and
//!   reported a disagreement although each path holds one reference per slot;
//! - the same with a propagating call in the branch: the continuation lift
//!   typed its call to a fn that declares `-> Result[Int, String]` as a
//!   Result of that Result, and the caller read the Ok payload slot as a
//!   nested Result.
//!
//! A fifth (the batch-1115d http test, #4754): `==` over a runtime-backed
//! value such as `HttpRequest` walled the assertion, having no comparison on
//! the certified render; it now calls the type's declared equality.
//!
//! Each test file passes on the native test lane and on wasm, and the main
//! program prints the same lines on both, its native build the certified
//! render's own.

use std::path::Path;
use std::process::Command;

const LIB: &str = r#"type Tok = { kind: String, text: String }

fn tok(ts: List[Tok], i: Int) -> Tok = list.get(ts, i) ?? Tok { kind: "", text: "" }

fn segs_of(ts: List[Tok], k: Int) -> List[String] = {
  var segs: List[String] = [
    tok(ts, k).text
  ]
  segs
}

fn pair_of(ts: List[Tok], k: Int) -> List[String] = [tok(ts, k).kind, tok(ts, k + 1).text]
"#;

const OWN: &str = r#"effect fn names(k: String) -> Result[List[String], String] = ok([k])

effect fn collect(kinds: List[String]) -> Result[Int, String] = {
  let direct = names("a")!
  let required = names("b")!
  var total = 0
  for k in kinds {
    var dnames = direct
    var need = required
    if k == "python" then {
      dnames = ["p", "q"]
      need = dnames
    } else ()
    total = total + dnames.len() + need.len()
  }
  ok(total)
}
"#;

const LOAD: &str = r#"effect fn names(k: String) -> Result[List[String], String] =
  if k == "bad" then err("no names for bad") else ok([k, k])

effect fn collect(kinds: List[String]) -> Result[Int, String] = {
  let direct = names("a")!
  let required = names("b")!
  var total = 0
  for k in kinds {
    var dnames = direct
    var need = required
    if k == "python" or k == "bad" then {
      dnames = names(k)!
      need = dnames
    } else ()
    total = total + dnames.len() + need.len()
  }
  ok(total)
}
"#;

const MAIN: &str = r#"import self.lib
import self.own
import self.load

effect fn main() -> Unit = {
  let ts = [lib.Tok { kind: "id", text: "a" }, lib.Tok { kind: "op", text: "+" }]
  println(list.join(lib.segs_of(ts, 0), ","))
  println(list.join(lib.segs_of(ts, 5), ","))
  println(list.join(lib.pair_of(ts, 0), ","))
  println(int.to_string(own.collect(["js", "python"])!))
  println(int.to_string(load.collect(["js", "python"])!))
  match load.collect(["js", "bad"]) {
    ok(n) => println("ok ${n}"),
    err(e) => println("err ${e}"),
  }
}
"#;

const MAIN_OUT: &str = "a\n\nid,+\n6\n8\nerr no names for bad\n";

const TESTS: [(&str, &str); 4] = [
    (
        "field_of_call_test.almd",
        r#"import self.lib
test "a list literal of a call's field" {
  let ts = [lib.Tok { kind: "id", text: "a" }, lib.Tok { kind: "op", text: "+" }]
  assert_eq(lib.segs_of(ts, 0), ["a"])
  assert_eq(lib.segs_of(ts, 9), [""])
  assert_eq(lib.pair_of(ts, 0), ["id", "+"])
}
"#,
    ),
    (
        "global_chain_test.almd",
        r#"let D1 = "0123456789"
let D2 = D1 + D1
let D3 = D2 + D2
let D4 = D3 + D3
let D5 = D4 + D4
let D6 = D5 + D5
let D7 = D6 + D6
let D8 = D7 + D7
let W = ["x"] + ["y"]
let W2 = W + W
let W3 = W2 + W2
let W4 = W3 + W3
let W5 = W4 + W4
let W6 = W5 + W5
test "a module-level String built in eight steps" {
  assert_eq(string.len(D8), 1280)
  assert_eq(string.slice(D8, 1275, 1280), "56789")
  assert_eq(list.len(W6), 64)
  assert_eq(list.get(W6, 63) ?? "", "y")
}
"#,
    ),
    (
        "aliased_slots_test.almd",
        r#"import self.own
test "outer lists copied per iteration, aliased in one branch" {
  assert_eq(own.collect(["js", "python"]), ok(6))
  assert_eq(own.collect(["python", "python", "go"]), ok(10))
}
"#,
    ),
    (
        "aliased_slots_bang_test.almd",
        r#"import self.load
test "the same, with a propagating call in the branch" {
  assert_eq(load.collect(["js", "python"]), ok(8))
  assert_eq(load.collect(["python", "bad", "go"]), err("no names for bad"))
}
"#,
    ),
];

fn write_project(dir: &Path) {
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"shapes\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    for (name, src) in [("lib.almd", LIB), ("own.almd", OWN), ("load.almd", LOAD), ("main.almd", MAIN)] {
        std::fs::write(dir.join("src").join(name), src).unwrap();
    }
    for (name, src) in TESTS {
        std::fs::write(dir.join("tests").join(name), src).unwrap();
    }
}

fn almide(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .output()
        .expect("almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn each_shape_passes_on_the_native_test_lane_and_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    write_project(dir.path());
    for (name, _) in TESTS {
        let file = format!("tests/{name}");
        for target in ["native", "wasm"] {
            let (ok, stdout, stderr) = almide(dir.path(), &["test", "--target", target, &file]);
            assert!(ok, "{name} on {target}:\n{stdout}\n{stderr}");
            assert!(!stderr.contains("native v1 test render"), "{name} on {target}:\n{stderr}");
        }
    }
}

#[test]
fn the_shapes_print_the_same_lines_on_native_and_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    write_project(dir.path());
    for target in [None, Some("wasm")] {
        let mut args = vec!["run", "src/main.almd"];
        args.extend(target.map(|t| ["--target", t]).into_iter().flatten());
        let (ok, stdout, stderr) = almide(dir.path(), &args);
        assert!(ok, "main on {target:?}:\n{stderr}");
        assert_eq!(stdout, MAIN_OUT, "main on {target:?}:\n{stderr}");
    }
}

/// `==` over a runtime-backed value (`HttpRequest`, `HttpResponse`) on the
/// native test lane: the certified render compares it through its module's
/// declared equality (`stdlib_info::runtime_backed_eq_surface`), the runtime
/// struct's own `PartialEq`, instead of walling the assertion's `if`. Only the
/// runtime-backed types reach this route — a user record compares field by
/// field — so the fixture needs `http`.
const RUNTIME_EQ: &str = r#"import http

fn same(a: HttpRequest, b: HttpRequest) -> Bool = a == b
fn differ(a: HttpResponse, b: HttpResponse) -> Bool = a != b

test "runtime-backed values compare by value" {
  let req = http.new_request("POST", "/", "ab", [("X-A", "1")])
  let a = http.set_body(req, "x")
  assert_eq(a, http.set_body(req, "x"))
  assert(a != http.set_body(req, "y"))
  assert(same(a, http.set_body(req, "x")))
  assert(not same(a, req))
  assert(differ(http.response(200, "a"), http.response(200, "b")))
  assert(not differ(http.response(200, "a"), http.response(200, "a")))
}
"#;

#[test]
fn a_runtime_backed_value_compares_on_the_native_test_lane_and_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("runtime_eq_test.almd"), RUNTIME_EQ).unwrap();
    for target in ["native", "wasm"] {
        let (ok, stdout, stderr) = almide(dir.path(), &["test", "--target", target, "runtime_eq_test.almd"]);
        assert!(ok, "runtime_eq_test on {target}:\n{stdout}\n{stderr}");
        assert!(!stderr.contains("native v1 test render"), "runtime_eq_test on {target}:\n{stderr}");
    }
}

/// Shapes 5 and 6 (#4749, reported on rc2), each with the neighbours the
/// generated sweep found refused the same way.
///
/// - Shape 5: a record `var` rebuilt inside a loop over its own list field,
///   with an element field read after the rebuild. The loop iterates a copy
///   taken before the rebuild; the ownership verifier and the certificate
///   ended every view of the old record at the rebind, the elements of that
///   copy included, and reported a use-after-free. A view below a parent the
///   frame holds its own reference to now stays that parent's, and a slot
///   whose old block is a borrowed param's leaves every view to the caller.
/// - Shape 6: `some(NOTE)` / `ok(NOTE)` over a module-level heap value the
///   function has not read yet. The Option/Result payload builder knew only
///   locals and params, so the lambda body refused and the `list.map` walled.
const WF: &str = r#"type Node = { name: String, kind: String }
type Raw = { id: String, nodes: List[Node] }
type Detail = { graph: Raw }

fn add_node(r: Raw, name: String, kind: String) -> Raw = Raw { id: r.id, nodes: r.nodes + [Node { name: name, kind: kind }] }

fn detail_of(id: String) -> Detail = Detail { graph: Raw { id: id, nodes: [Node { name: "x", kind: "task" }] } }

fn details(raw: Raw, kids: List[String]) -> Raw = {
  var out = raw
  for child in kids {
    if child != "" then {
      let detail = detail_of(raw.id)
      for node in detail.graph.nodes {
        out = add_node(out, node.name, node.kind)
      }
    } else ()
  }
  if kids.len() > 1 then {
    var previous = ""
    for node in out.nodes {
      if node.kind == "task" then {
        if previous != "" then {
          out = add_node(out, previous, "seq")
        } else ()
        previous = node.name
      } else ()
    }
  } else ()
  out
}

fn chain_param(raw: Raw, ks: List[String]) -> Raw = {
  var out = raw
  var prev = ""
  for node in raw.nodes {
    if prev != "" then { out = add_node(out, prev, "seq") } else ()
    prev = node.name
  }
  out
}

fn chain_own(raw: Raw) -> Raw = {
  var out = add_node(add_node(raw, "x", "task"), "y", "task")
  var prev = ""
  for node in out.nodes {
    if prev != "" then { out = add_node(out, prev, "seq") } else ()
    prev = node.name
  }
  out
}
"#;

const FX: &str = r#"type R = { note: Option[String], k: Int }
type Q = { s: String }

let NOTE = "n"
let QV = Q { s: "q" }

fn notes(xs: List[Int]) -> List[Option[String]] = list.map(xs, (x) => some(NOTE))
fn note() -> Option[String] = some(NOTE)
fn recs(xs: List[Int]) -> List[R] = list.map(xs, (x) => R { note: some(NOTE), k: x })
fn qs(xs: List[Int]) -> List[Option[Q]] = list.map(xs, (x) => some(QV))
fn both(xs: List[Int], p: String) -> List[(Option[String], String)] = list.map(xs, (x) => (some(NOTE), p))
fn twice() -> List[Option[String]] = {
  let o = some(NOTE)
  [o, o]
}
fn oks(xs: List[Int]) -> Int = {
  let rs: List[Result[String, String]] = list.map(xs, (x) => ok(NOTE))
  rs |> list.filter((r) => result.is_ok(r)) |> list.len
}
"#;

const SHAPES_56: [(&str, &str); 2] = [
    (
        "w_test.almd",
        r#"import self.wf
test "record var rebuilt in nested branches" {
  let r = wf.details(wf.Raw { id: "g", nodes: [] }, ["a", "b"])
  assert_eq(r.nodes.len(), 3)
  let two = wf.Raw { id: "g", nodes: [wf.Node { name: "x", kind: "task" }, wf.Node { name: "y", kind: "task" }] }
  assert_eq(wf.chain_param(two, []).nodes.len(), 3)
  assert_eq(wf.chain_own(wf.Raw { id: "g", nodes: [] }).nodes.len(), 3)
  assert_eq((list.last(wf.chain_own(wf.Raw { id: "g", nodes: [] }).nodes) ?? wf.Node { name: "", kind: "" }).name, "x")
}
"#,
    ),
    (
        "e_test.almd",
        r#"import self.fx
test "closure wraps a module-level String in some" {
  assert_eq(fx.notes([1, 2]), [some("n"), some("n")])
  assert_eq(fx.note(), some("n"))
  assert_eq(fx.recs([1]), [fx.R { note: some("n"), k: 1 }])
  assert_eq(fx.qs([1]), [some(fx.Q { s: "q" })])
  assert_eq(fx.both([1], "p"), [(some("n"), "p")])
  assert_eq(fx.twice(), [some("n"), some("n")])
  assert_eq(fx.oks([1, 2]), 2)
}
"#,
    ),
];

#[test]
fn shapes_5_and_6_pass_on_the_native_test_lane_and_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"shapes56\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::create_dir_all(dir.path().join("tests")).unwrap();
    std::fs::write(dir.path().join("src/wf.almd"), WF).unwrap();
    std::fs::write(dir.path().join("src/fx.almd"), FX).unwrap();
    for (name, src) in SHAPES_56 {
        std::fs::write(dir.path().join("tests").join(name), src).unwrap();
    }
    for (name, _) in SHAPES_56 {
        let file = format!("tests/{name}");
        for target in ["native", "wasm"] {
            let (ok, stdout, stderr) = almide(dir.path(), &["test", "--target", target, &file]);
            assert!(ok, "{name} on {target}:\n{stdout}\n{stderr}");
            assert!(!stderr.contains("native v1 test render"), "{name} on {target}:\n{stderr}");
        }
    }
}
