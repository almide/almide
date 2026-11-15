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
  assert_eq(some(a), some(http.set_body(req, "x")))
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
