//! #4955: the dojo task-bank shapes of a `!` whose operand is not a bound
//! carrier or a call, each refused on wasm as an INCOMPLETE certificate
//! (C-462) where v0.69.1 ran them:
//!
//! - a fallible callback ending in a written `ok(v)!`: the fallible-tail lift
//!   wraps the List-typed tail, so the frame returns `ok(ok(acc + [n])!)`;
//! - a callback ending in `match .. { some(o) => err(..), none => ok(..) }!`;
//! - `seats = apply(seats, parse(p)!, no)!`, which arg_temps.rs makes a block
//!   carrier `{ let t = parse(p)!; apply(seats, t, no) }!`;
//! - `some(v)!` in an Option fn.
//!
//! The witness gate admitted only `err(m)!` among them (`payload:Unwrap`,
//! `assign:Unwrap`). Each is an owned carrier born at the site (every value
//! leaf a constructor or a program call), the route `err(m)!` takes; the
//! program is now certified and runs the same on both targets without
//! `--allow-incomplete-verification`.

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

fn unique(xs: List[String]) -> List[String]! =
  xs |> list.fold([], (acc, s) => {
    let t = "k" + s
    match list.find(acc, (o) => o == t) {
      some(o) => err("duplicate ${o}"),
      none => ok(acc + [t]),
    }!
  })!

fn parse_seat(s: String) -> Int! = int.parse(s)!

fn place(seats: List[Int], n: Int) -> List[Int]! = {
  guard n > 0 else err("seat ${int.to_string(n)}")
  seats + [n]
}

fn board(xs: List[String]) -> List[Int]! = {
  var seats: List[Int] = []
  for s in xs {
    seats = place(seats, parse_seat(s)!)!
  }
  seats
}

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
  println(list.join(unique(["a", "b"]) ?? ["bad"], ","))
  match unique(["a", "b", "a"]) { ok(_) => println("ok"), err(e) => println(e) }
  var seated = 0
  for i in 0..<200 {
    seated = seated + list.len(board(["1", int.to_string(i + 1)]) ?? [])
  }
  println(int.to_string(seated))
  match board(["3", "0"]) { ok(_) => println("ok"), err(e) => println(e) }
  println(int.to_string(list.len(board(["3", "y"]) ?? [])))
  println(int.to_string(bump(3) ?? 0))
  println(label("a") ?? "none")
}
"#;

#[test]
fn a_constructor_branch_or_block_carrier_of_a_bang_is_certified_on_wasm() {
    let dir = std::env::temp_dir().join(format!("almide-ctor-unwrap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("main.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let want = "126250\nn4,n5\nbad\nka,kb\nduplicate ka\n400\nseat 0\n0\n8\na-x!\n";
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(&src, target);
        assert!(ok, "{target:?} failed:\n{stderr}");
        assert_eq!(stdout, want, "{target:?}:\n{stderr}");
        assert!(!stderr.contains("INCOMPLETE") && !stderr.contains("UNCERTIFIED"), "{target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
