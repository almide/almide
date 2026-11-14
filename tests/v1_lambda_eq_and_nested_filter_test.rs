//! Two shapes the certified native render refused (#4640 walls):
//!
//! - a structural `==` over a recursive variant inside a LAMBDA
//!   (`list.all(xs, (o) => o == Var(k))`). The synthesized eq helper is named
//!   after the lambda and lived only in the lambda's lowering context, which
//!   dropped it, so the lifted lambda called a function no module defined.
//!   The parent now takes the lambda's helpers over with its lifted lambdas;
//! - a `list.filter` whose callback runs a loop of its own (`list.find` inside
//!   the callback). The filtered prefix's final length was refused because the
//!   filter loop contained any nested loop. It is now admitted when the nested
//!   loop rebinds neither counter nor the builder, and its breaks leave only it.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-lambda-eq-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const LAMBDA_EQ: &str = r##"type Term =
  | Var(Int)
  | App(Term, Term)
  | Const(String, List[Int])

fn ordered(xs: List[Term], n: Int) -> Bool =
  list.all(list.enumerate(xs), ((i, o)) => o == Var(n - 1 - i))

fn nested(xss: List[List[Term]], t: Term) -> Int =
  list.len(list.filter(xss, (xs) => list.any(xs, (x) => x == t or x == App(t, t))))

fn both(a: Term, xs: List[Term]) -> Bool = a == Var(0) and list.any(xs, (x) => x == a)

fn keep(names: List[String], idx: List[(String, List[String])], seen: List[String]) -> List[String] =
  names |> list.filter((n) => match list.find(idx, ((m, _)) => m == n) {
    some((_, deps)) => list.all(deps, (d) => list.contains(seen, d)),
    none => string.len(n) > 1,
  })

fn main() -> Unit = {
  println("${ordered([Var(1), Var(0)], 2)} ${ordered([Var(0), Var(1)], 2)}")
  let t = Const("c", [1, 2])
  println("${nested([[Var(1)], [t], [App(t, t), Var(2)], []], t)}")
  println("${both(Var(0), [App(Var(0), Var(0)), Var(0)])} ${both(Var(0), [Var(1)])} ${both(Var(1), [Var(1)])}")
  println(list.join(keep(["a", "b", "cc", "d"], [("a", ["x"]), ("b", ["y"]), ("d", [])], ["x"]), ","))
}
"##;

#[test]
fn a_recursive_variant_eq_inside_a_lambda_renders() {
    prints("lambda-eq", LAMBDA_EQ, "true false\n2\ntrue false false\na,cc,d\n");
}

const FILTER_WITH_SEARCH: &str = r##"fn direct(heads: List[(String, Int)], n: String) -> Bool = match list.find(heads, ((m, _)) => m == n) {
  some((_, k)) => k > 0,
  none => true,
}

fn in_filter(names: List[String], heads: List[(String, Int)]) -> List[String] =
  names |> list.filter((n) => match list.find(heads, ((m, _)) => m == n) {
    some((_, k)) => k > 0,
    none => true,
  })

fn in_filter3(names: List[String], heads: List[(String, List[String], List[String])], reach: List[String]) -> List[String] =
  names |> list.filter((n) => match list.find(heads, ((m, _, _)) => m == n) {
    some((_, hs, _)) => list.all(hs, (h) => list.contains(reach, h)),
    none => true,
  })

fn main() -> Unit = {
  let heads = [("a", 1), ("b", 0)]
  println("${direct(heads, "a")} ${direct(heads, "b")} ${direct(heads, "c")}")
  println(list.join(in_filter(["a", "b", "c"], heads), ","))
  let h3 = [("s1", ["f"], ["g"]), ("s2", ["g"], [])]
  println(list.join(in_filter3(["s1", "s2", "s3"], h3, ["f"]), ","))
}
"##;

#[test]
fn a_filter_whose_callback_searches_commits_its_prefix() {
    prints("filter-search", FILTER_WITH_SEARCH, "true false true\na,c\ns1,s3\n");
}
