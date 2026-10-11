//! #3549: a self-tail-recursive function is loop-converted on native whatever
//! it returns. The loop's result variable used to be initialised with a
//! placeholder of the return type, and only the types `default_for_type`
//! could build one for were rewritten. A record, a variant, a `Set` or `Bytes`
//! stayed plain recursion and overflowed the stack on native at a depth the
//! wasm leg runs in constant stack. The result is now declared without a value.
//!
//! Each function below recurses a million times; both legs must print the
//! same four lines.
use std::process::Command;

const SOURCE: &str = r#"type St = { n: Int, s: String }

type Tag = | Named(String) | Anon

fn rec_go(x: St, i: Int) -> St = if i <= 0 then x else rec_go({ n: x.n + 1, s: x.s }, i - 1)

fn tag_go(x: Tag, i: Int) -> Tag = if i <= 0 then x else tag_go(match x { Named(s) => Anon, Anon => Named("t") }, i - 1)

fn set_go(x: Set[Int], i: Int) -> Set[Int] = if i <= 0 then x else set_go(set.insert(x, i % 3), i - 1)

fn bytes_go(x: Bytes, i: Int) -> Bytes = if i <= 0 then x else bytes_go(bytes.slice(x, 0, 2), i - 1)

effect fn main() -> Unit = {
  let depth = 1000000
  println("${rec_go({ n: 0, s: "a" }, depth).n}")
  match tag_go(Anon, depth + 1) {
    Named(s) => println("named ${s}"),
    Anon => println("anon"),
  }
  println("${set.len(set_go(set.new(), depth))}")
  println("${bytes.len(bytes_go(bytes.new(8), depth))}")
}
"#;

const EXPECTED: &str = "1000000\nnamed t\n3\n2\n";

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn tail_recursion_runs_in_constant_stack_for_every_return_type() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("tco_any_return.almd");
    std::fs::write(&file, SOURCE).expect("source");
    assert_eq!(run(&file, false), EXPECTED, "native");
    assert_eq!(run(&file, true), EXPECTED, "wasm");
}
