//! #4958: a fallible fn whose tail tuple holds an inline spread record
//! (`(Stock { ...s, lots: e }, n)`, the dojo task `fifo-cost-lots`) walled the
//! verified native render: the record was moved into the Ok tuple and also
//! released by the arm's scope-end drop, and the ownership self-check refused
//! the double release. Every row below now renders on the certified v1 render
//! (no wall, no standard-codegen fallback) and prints what wasm prints.

use std::process::Command;

const HEAD: &str = "type Stock = { sku: String, lots: List[Int] }\n\nfn split(xs: List[Int]) -> (List[Int], Int) = (list.drop(xs, 1), list.len(xs))\n\n";

const MAIN: &str = r#"
effect fn main() -> Unit = {
  var total = 0
  for i in 0..<300 {
    let s = Stock { sku: "pen" + int.to_string(i), lots: [1, 2, 3] }
    let (a, n) = ship(s)!
    total = total + list.len(a.lots) + n
  }
  let (a, n) = ship(Stock { sku: "pen", lots: [1, 2, 3] })!
  println("${a.sku} ${int.to_string(list.len(a.lots))} ${int.to_string(n)} ${int.to_string(total)}")
  match ship(Stock { sku: "x", lots: [] }) { ok(_) => println("ok"), err(e) => println(e) }
}
"#;

/// The `ship` bodies of the shape table (#4958's draft): with and without the
/// guard, with a destructured call before the tail, and the tail spelled `ok(..)`.
const SHIPS: &[(&str, &str)] = &[
    ("guarded", "fn ship(s: Stock) -> (Stock, Int)! = {\n  guard list.len(s.lots) > 0 else err(\"empty\")\n  (Stock { ...s, lots: list.drop(s.lots, 1) }, 3)\n}\n"),
    ("destructured", "fn ship(s: Stock) -> (Stock, Int)! = {\n  guard list.len(s.lots) > 0 else err(\"empty\")\n  let (left, n) = split(s.lots)\n  (Stock { ...s, lots: left }, n)\n}\n"),
    ("ok-spelled", "fn ship(s: Stock) -> (Stock, Int)! = {\n  guard list.len(s.lots) > 0 else err(\"empty\")\n  let (left, n) = split(s.lots)\n  ok((Stock { ...s, lots: left }, n))\n}\n"),
    ("unguarded", "fn ship(s: Stock) -> (Stock, Int)! = {\n  let (left, n) = split(s.lots)\n  (Stock { ...s, lots: left }, n)\n}\n"),
];

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .output()
        .expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_spread_record_in_a_fallible_tail_tuple_renders_verified_and_matches_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    for (name, ship) in SHIPS {
        let file = format!("{}.almd", name.replace('-', "_"));
        std::fs::write(dir.path().join(&file), format!("{HEAD}{ship}{MAIN}")).unwrap();
        let (ok, native, err) = run(dir.path(), &["run", &file]);
        assert!(ok, "{name} native:\n{err}");
        assert!(err.contains("native: v1 trust-spine render") && !err.contains("walled"), "{name}: not the certified render:\n{err}");
        let (ok, wasm, err) = run(dir.path(), &["run", &file, "--target", "wasm"]);
        assert!(ok, "{name} wasm:\n{err}");
        assert_eq!(native, wasm, "{name}: native and wasm differ");
        let last = if *name == "unguarded" { "ok\n" } else { "empty\n" };
        assert!(native.starts_with("pen 2 ") && native.ends_with(last), "{name}:\n{native}");
    }
}

#[test]
fn the_dojo_task_shape_renders_verified_on_the_native_test_lane() {
    let dir = tempfile::tempdir().expect("scratch");
    let src = format!(
        "{HEAD}{}\ntest \"ship\" {{\n  let (a, n) = ship(Stock {{ sku: \"pen\", lots: [1, 2, 3] }})!\n  assert_eq(a.lots, [2, 3])\n  assert_eq(n, 3)\n  assert_eq(ship(Stock {{ sku: \"x\", lots: [] }}), err(\"empty\"))\n}}\n",
        SHIPS[1].1
    );
    std::fs::write(dir.path().join("ship_test.almd"), src).unwrap();
    let (ok, out, err) = run(dir.path(), &["test", "--target", "rust", "ship_test.almd"]);
    assert!(ok, "{out}{err}");
    assert!(err.contains("native: v1 trust-spine render (test harness)") && !err.contains("walled"), "not the certified render:\n{err}");
}
