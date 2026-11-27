//! A fallible callback whose block ends in a written `ok(v)!` (a dojo
//! task-bank shape: `list.fold([], (acc, s) => { let n = int.parse(s)!;
//! ok(acc + [n])! })!`). The fallible-tail lift wraps the List-typed tail in
//! `ok(..)`, so the callback's frame returns `ok(ok(acc + [n])!)`: an
//! extraction out of a constructor. The witness gate admitted `err(m)!` and
//! declined every other constructor operand (`payload:Unwrap`), so the wasm
//! certificate was INCOMPLETE and the program was refused (C-462), where
//! v0.69.1 ran it. A constructor is a certainly-fresh carrier and takes the
//! owned-carrier route of the `!` site; the program is now certified and runs
//! the same on both targets without `--allow-incomplete-verification`.

use std::process::Command;

fn run(src: &std::path::Path, target: Option<&str>) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

const PROGRAM: &str = r#"
fn parse_all(xs: List[String]) -> List[Int]! =
  xs |> list.fold([], (acc, s) => {
    let n = int.parse(s)!
    ok(acc + [n])!
  })!

fn tag_all(xs: List[String]) -> List[String]! =
  xs |> list.fold([], (acc, s) => {
    let n = int.parse(s)!
    ok(acc + ["n" + int.to_string(n)])!
  })!

fn bump(x: Int) -> Int? = {
  let y = some(x + 1)!
  some(y * 2)
}

fn label(s: String) -> String? = {
  let t = some(s + "-x")!
  some(t + "!")
}

effect fn main() -> Unit = {
  var total = 0
  for i in 0..<500 {
    let ys = parse_all(["1", "2", int.to_string(i)]) ?? []
    total = total + list.fold(ys, 0, (a, y) => a + y)
  }
  println(int.to_string(total))
  println(list.join(tag_all(["4", "5"]) ?? ["bad"], ","))
  println(list.join(tag_all(["4", "x"]) ?? ["bad"], ","))
  println(int.to_string(bump(3) ?? 0))
  println(label("a") ?? "none")
}
"#;

#[test]
fn a_constructor_unwrapped_in_a_fallible_callback_is_certified_on_wasm() {
    let dir = std::env::temp_dir().join(format!("almide-ctor-unwrap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("main.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let want = "126250\nn4,n5\nbad\n8\na-x!\n";
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(&src, target);
        assert!(ok, "{target:?} failed:\n{stderr}");
        assert_eq!(stdout, want, "{target:?}:\n{stderr}");
        assert!(!stderr.contains("INCOMPLETE") && !stderr.contains("UNCERTIFIED"), "{target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
