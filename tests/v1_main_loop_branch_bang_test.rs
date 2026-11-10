//! A `!` in an arm of an `if`/`match` STATEMENT inside a loop of the void
//! `main`, with more statements after the branch (`examples/minesweeper.almd`:
//! `if is_first then { let mines = place_mines(..)!; adj = build_adjacent(mines)
//! … } else ()` three branches deep in its game loop). The loop's die
//! continuation reached only a block's own statements and its tail, so the `!`
//! stayed in place, the loop fell to the one-iteration model, and the render
//! refused the heap reassignment beside it. The `!` is now rewritten where it
//! stands: the die arm never returns, so the arm's own block is its whole
//! continuation. In a `Result`-returning fn the same branch sets the loop's
//! error flag and the rest of the iteration is guarded on it.
use std::process::Command;

/// The minesweeper loop's shape: a Result match statement, a flag arm that
/// writes a list element, a reveal arm whose first turn takes a `!` and
/// reassigns a list, statements after every branch.
const BRANCH_BANG: &str = r#"effect fn mk(n: Int) -> Result[List[Int], String] = if n > 7 then err("big ${n}") else ok([n, n + 1])

fn parse(i: Int) -> Result[{ flag: Bool, row: Int }, String] =
  if i % 4 == 3 then err("bad ${i}") else ok({ flag: i % 2 == 0, row: i })

effect fn main() -> Unit = {
  var adj: List[Int] = []
  var seen: List[Bool] = [false, false, false]
  var first = true
  var turns = 0
  var i = 0
  var over = false
  while not over {
    println("turn ${turns} adj ${list.len(adj)}")
    match parse(i) {
      err(msg) => { println(msg) },
      ok(cmd) => {
        if cmd.flag then {
          if seen[cmd.row % 3] then { println("seen") } else {
            seen[cmd.row % 3] = true
          }
        } else {
          if first then {
            let m = mk(cmd.row)!
            adj = m
            first = false
          } else {
            let more = mk(cmd.row + 2)!
            adj = adj + more
          }
          if list.len(adj) > 100 then { over = true } else ()
        }
        turns = turns + 1
      },
    }
    i = i + 1
  }
  println("unreachable ${turns}")
}
"#;

const BRANCH_BANG_STDOUT: &str = "turn 0 adj 0\nturn 1 adj 0\nturn 2 adj 2\nturn 3 adj 2\nbad 3\nturn 3 adj 2\nturn 4 adj 2\nturn 5 adj 4\nseen\nturn 6 adj 4\nbad 7\nturn 6 adj 4\nseen\nturn 7 adj 4\n";

/// The same branch statements in loops of `Result`-returning fns: the `!`
/// must deliver the `Err` after the loop (the error-flag form). The branch is
/// rewritten where it stands and the rest of the iteration runs only while no
/// exit has fired — a `while` and a `for`, an `if` and a `match` statement,
/// two branch statements in a row, heap and scalar accumulators.
const RESULT_FN: &str = r#"effect fn mk(n: Int) -> Result[List[Int], String] = if n > 7 then err("big ${n}") else ok([n, n + 1])
effect fn mi(n: Int) -> Result[Int, String] = if n > 5 then err("int ${n}") else ok(n * 2)

fn parse(i: Int) -> Result[{ flag: Bool, row: Int }, String] =
  if i % 4 == 3 then err("bad ${i}") else ok({ flag: i % 2 == 0, row: i })

// while, heap accumulator, branch statement then more statements
effect fn a(k: Int) -> Result[Int, String] = {
  var acc: List[Int] = []
  var i = 0
  while i < k {
    if i % 2 == 0 then {
      let m = mk(i)!
      acc = acc + m
    } else {
      acc = acc + [0 - i]
    }
    println("a ${i} ${list.len(acc)}")
    i = i + 1
  }
  ok(list.len(acc))
}

// match statement, both arms, nested if, two branch statements in a row
effect fn b(k: Int) -> Result[String, String] = {
  var s = ""
  var i = 0
  while i < k {
    match parse(i) {
      err(msg) => { s = s + "[" + msg + "]" },
      ok(cmd) => {
        if cmd.flag then {
          let m = mk(cmd.row)!
          s = s + int.to_string(list.len(m))
        } else ()
        s = s + "."
      },
    }
    if i % 3 == 0 then {
      let v = mi(i)!
      s = s + "<" + int.to_string(v) + ">"
    } else ()
    s = s + ";"
    i = i + 1
  }
  ok(s)
}

// for loop, the branch then a statement
effect fn c(xs: List[Int]) -> Result[List[String], String] = {
  var out: List[String] = []
  for x in xs {
    if x > 2 then {
      let m = mk(x)!
      out = out + [int.to_string(list.len(m))]
    } else ()
    out = out + ["x${x}"]
  }
  ok(out)
}

effect fn main() -> Unit = {
  let a1: Result[Int, String] = a(4)
  println("${a1}")
  let a2: Result[Int, String] = a(12)
  println("${a2}")
  let b1: Result[String, String] = b(5)
  println("${b1}")
  let b2: Result[String, String] = b(9)
  println("${b2}")
  let b3: Result[String, String] = b(12)
  println("${b3}")
  let c1: Result[List[String], String] = c([1, 2, 3, 4])
  println("${c1}")
  let c2: Result[List[String], String] = c([1, 9, 3])
  println("${c2}")
}
"#;

const RESULT_FN_STDOUT: &str = "a 0 2\na 1 3\na 2 5\na 3 6\nok(6)\na 0 2\na 1 3\na 2 5\na 3 6\na 4 8\na 5 9\na 6 11\na 7 12\nerr(\"big 8\")\nok(\"2.<0>;.;2.;[bad 3]<6>;2.;\")\nerr(\"int 6\")\nerr(\"int 6\")\nok([\"x1\", \"x2\", \"2\", \"x3\", \"2\", \"x4\"])\nerr(\"big 9\")\n";

/// A `!` in an arm of a VALUE `if` bound by `let` in the loop: the flag form
/// has no value to give the binding on the exit path. Still refused.
const LET_IF_BANG: &str = r#"effect fn mi(n: Int) -> Result[Int, String] = if n > 3 then err("big ${n}") else ok(n)

effect fn run(k: Int) -> Result[Int, String] = {
  var n = 0
  var i = 0
  while i < k {
    let d = if i % 2 == 0 then mi(i)! else 0
    n = n + d
    i = i + 1
  }
  ok(n)
}

effect fn main() -> Unit = {
  println(match run(3) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
  println(match run(6) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
}
"#;

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .expect("spawn")
        .join()
        .expect("render thread")
}

#[test]
fn a_bang_under_a_branch_statement_of_a_main_loop_renders_and_dies_on_the_err() {
    render(BRANCH_BANG).unwrap_or_else(|e| panic!("the main-loop branch `!` walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("branch_bang.almd");
    std::fs::write(&file, BRANCH_BANG).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert_eq!(String::from_utf8_lossy(&out.stdout), BRANCH_BANG_STDOUT);
    assert_eq!(out.status.code(), Some(1), "the `!` on an Err ends main with exit 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Error: big 11"), "{stderr}");
    assert!(!stderr.contains("walled"), "{stderr}");
}

#[test]
fn the_minesweeper_example_renders_on_the_native_route() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/minesweeper.almd");
    let src = std::fs::read_to_string(path).expect("examples/minesweeper.almd");
    render(&src).unwrap_or_else(|e| panic!("minesweeper walls: {e}"));
}

#[test]
fn a_bang_under_a_branch_statement_of_a_result_fn_loop_delivers_the_err_after_the_loop() {
    render(RESULT_FN).unwrap_or_else(|e| panic!("the result-fn loop branch `!` walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("result_fn.almd");
    std::fs::write(&file, RESULT_FN).expect("source");
    let run = |extra: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra).output().expect("almide run");
        assert!(out.status.success(), "{extra:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert_eq!(run(&[]), RESULT_FN_STDOUT, "native");
    assert_eq!(run(&["--target", "wasm"]), RESULT_FN_STDOUT, "wasm");
}

#[test]
fn a_bang_in_a_let_bound_value_if_of_a_result_fn_loop_is_still_refused() {
    let err = render(LET_IF_BANG).expect_err("the error-flag form does not place this `!`");
    assert!(err.contains("if-value arms disagree on type"), "{err}");
}
