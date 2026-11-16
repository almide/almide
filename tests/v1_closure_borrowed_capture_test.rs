//! A closure that cannot outlive its frame borrows the heap parameters it
//! captures on the native render (#4700): the copy its co-own `Dup` stands
//! for is the parameter itself. A closure that leaves its frame keeps its own
//! copy, and one copied out of a borrowed closure inside the callee owns every
//! capture (`__AlmdClosure<'static>`).
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-borrowed-capture-{}-{tag}", std::process::id()));
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

/// `source` prints `expected` on the native render and on wasm; the native
/// build is the certified render's own, and it lends a capture iff `lends`.
fn prints(tag: &str, source: &str, expected: &str, lends: bool) {
    let rust = match almide_mir::pipeline::try_render_rust_source(source) {
        Ok(r) => r,
        Err(e) => panic!("{tag} left the certified render: {e:?}"),
    };
    assert_eq!(rust.contains("// lent to a closure"), lends, "{tag}: lends a capture");
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const RECORD_THROUGH_RECURSION: &str = r##"type T = | V(Int) | A(T, T)

type Env = { syms: Map[String, Int] }

fn count(env: Env, t: T) -> Int! = match t {
  V(k) => {
    guard map.contains(env.syms, "f") else err("no f")
    ok(k)
  },
  A(f, a) => {
    let parts = [f, a] |> list.map((x) => count(env, x)!)!
    ok(list.sum(parts))
  },
}

fn total(env: Env, ts: List[T]) -> Int! = {
  let all = ts |> list.map((t) => count(env, t)!)!
  ok(list.sum(all))
}

fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok ${n}",
  err(e) => "err ${e}",
}

fn main() -> Unit = {
  let env = Env { syms: ["f": 1] }
  let bad = Env { syms: ["g": 1] }
  println(show(total(env, [A(V(1), A(V(2), V(3))), V(4)])))
  println(show(total(env, [V(5)])))
  println(show(total(bad, [V(5)])))
}
"##;

#[test]
fn a_record_parameter_is_lent_down_a_recursive_fallible_map() {
    prints("record", RECORD_THROUGH_RECURSION, "ok 10\nok 5\nerr no f\n", true);
}

const STRING_AND_LIST: &str = r##"fn each(xs: List[Int], f: (Int) -> String) -> List[String] =
  xs |> list.map((x) => f(x))

fn sizes(xs: List[Int], f: (Int) -> Int) -> List[Int] =
  xs |> list.map((x) => f(x))

fn tags(p: String, xs: List[Int]) -> List[String] = each(xs, (x) => "${p}${x}")

fn shifted(base: List[Int], xs: List[Int]) -> List[Int] = sizes(xs, (x) => list.len(base) + x)

fn named(names: List[String], xs: List[Int]) -> List[String] = each(xs, (x) => list.get(names, x) ?? "?")

fn main() -> Unit = {
  println("${tags("t", [1, 2])}")
  println("${shifted([7, 8, 9], [1, 2])}")
  println("${named(["a", "b"], [1, 0, 5])}")
}
"##;

#[test]
fn string_and_list_parameters_are_lent_to_a_map_callback() {
    prints("string-list", STRING_AND_LIST, "[\"t1\", \"t2\"]\n[4, 5]\n[\"b\", \"a\", \"?\"]\n", true);
}

const RETURNED: &str = r##"fn greeter(p: String) -> (Int) -> String = (x) => "${p}${x}"

fn main() -> Unit = {
  let g = greeter("hi")
  println(g(2))
}
"##;

#[test]
fn a_returned_closure_keeps_its_own_copy() {
    prints("returned", RETURNED, "hi2\n", false);
}

/// The callee keeps the closure it is lent: the copy it stores owns every
/// capture, so the lending frame may end first.
const KEPT_BY_THE_CALLEE: &str = r##"fn keep(f: (Int) -> String) -> List[(Int) -> String] = {
  let first = f(0)
  if first == "" then [] else [f, f]
}

fn kept(p: String) -> String = {
  let fs = keep((x) => "${p}:${x}")
  match list.get(fs, 1) {
    some(f) => f(1),
    none => "",
  }
}

fn main() -> Unit = {
  println(kept("k"))
}
"##;

#[test]
fn a_closure_the_callee_keeps_owns_its_captures() {
    prints("kept", KEPT_BY_THE_CALLEE, "k:1\n", true);
}
