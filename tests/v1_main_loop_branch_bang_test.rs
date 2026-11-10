//! A `!` in an arm of an `if`/`match` STATEMENT inside a loop of the void
//! `main`, with more statements after the branch (`examples/minesweeper.almd`:
//! `if is_first then { let mines = place_mines(..)!; adj = build_adjacent(mines)
//! … } else ()` three branches deep in its game loop). The loop's die
//! continuation reached only a block's own statements and its tail, so the `!`
//! stayed in place, the loop fell to the one-iteration model, and the render
//! refused the heap reassignment beside it. The `!` is now rewritten where it
//! stands: the die arm never returns, so the arm's own block is its whole
//! continuation.
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

/// The same branch statement in a loop of a `Result`-returning fn: its `!`
/// must deliver the `Err` after the loop (the error-flag form), which places
/// a `!` only in a block's last branch. Still refused.
const RESULT_FN: &str = r#"effect fn mk(n: Int) -> Result[List[Int], String] = if n > 3 then err("big ${n}") else ok([n, n])

effect fn run(k: Int) -> Result[Int, String] = {
  var n = 0
  var i = 0
  while i < k {
    if i % 2 == 0 then {
      let m = mk(i)!
      n = n + list.len(m)
    } else ()
    i = i + 1
  }
  ok(n)
}

effect fn main() -> Unit = {
  println(match run(3) { ok(n) => "ok ${n}", err(e) => "err ${e}" })
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
fn the_same_branch_in_a_result_fn_loop_is_still_refused() {
    let err = render(RESULT_FN).expect_err("the error-flag form does not place this `!`");
    assert!(err.contains("cannot take the both-arms linearization"), "{err}");
}
