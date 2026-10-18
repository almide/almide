//! #3552: `set.insert` / `set.remove` are declared `@reuse(s)`, so a source at
//! its last use — the rhs of `s = set.insert(s, …)`, a fold's accumulator, a
//! self tail call's argument — is edited in place natively through the owned
//! twins instead of cloned whole per step (the stdlib inspection measured the
//! native rebind shapes at n bytes moved per call; wasm already edited in
//! place).
//!
//! VALUES: the shapes that must still copy — a source shared with another
//! binding, captured by a closure, held in a container, read again by another
//! argument — print what they printed before the change (the outputs below are
//! the pre-change compiler's, both legs).
use std::process::Command;

const SOURCE: &str = r#"fn tail_ins(s: Set[Int], n: Int) -> Set[Int] =
  if n == 0 then s else tail_ins(set.insert(s, n % 50), n - 1)

effect fn growth() -> Unit = {
  var g = list.range(0, 20000) |> set.from_list
  var i = 0
  while i < 2000 {
    g = set.insert(g, i + 100000)
    g = set.remove(g, i)
    i = i + 1
  }
  let t = tail_ins(set.new(), 2000)
  let f = list.fold(list.range(0, 2000), set.from_list([1, 2]), (acc, k) => set.remove(set.insert(acc, k), k - 1))
  println("${set.len(g)} ${set.contains(g, 100000)} ${set.contains(g, 0)} ${set.len(t)} ${set.len(f)}")
}

effect fn main() -> Unit = {
  growth()!
  // 1. shared source keeps value semantics
  var x = set.from_list(["a", "b"])
  let y = x
  x = set.insert(x, "c")
  x = set.remove(x, "a")
  println("${set.to_list(x)} ${set.to_list(y)}")
  // 2. closure captures the source before the edit
  var c = set.from_list([1, 2])
  let h = () => set.len(c)
  c = set.insert(c, 3)
  println("${h()} ${set.to_list(c)}")
  // 3. held in a container
  var k = set.from_list([7])
  let box = [k]
  k = set.insert(k, 8)
  println("${list.map(box, (s) => set.to_list(s))} ${set.to_list(k)}")
  // 4. read again by another argument
  var r = set.from_list([1, 2, 3])
  r = set.insert(r, set.len(r) * 10)
  r = set.remove(r, set.len(r) - 2)
  println("${set.to_list(r)}")
  // 5. present / absent no-ops and order
  var o = set.from_list([3, 1, 2])
  o = set.insert(o, 1)
  o = set.remove(o, 9)
  o = set.insert(set.remove(o, 3), 3)
  println("${set.to_list(o)}")
  // 6. a temporary source
  println("${set.to_list(set.insert(set.from_list([5]), 6))} ${set.to_list(set.remove(set.from_list([5, 6]), 5))}")
}
"#;

const EXPECTED: &str = r#"20000 true false 50 1
["b", "c"] ["a", "b"]
3 [1, 2, 3]
[[7]] [7, 8]
[1, 3, 30]
[1, 2, 3]
[5, 6] [6]
"#;

fn write_source() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("set_edit.almd");
    std::fs::write(&file, SOURCE).expect("source");
    (dir, file)
}

fn run(wasm: bool) -> String {
    let (_dir, file) = write_source();
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn set_edits_keep_value_semantics_on_both_legs() {
    assert_eq!(run(false), EXPECTED, "native");
    assert_eq!(run(true), EXPECTED, "wasm");
}

#[test]
fn native_set_edit_rebinds_call_the_owned_twins() {
    let (_dir, file) = write_source();
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    let rust = String::from_utf8_lossy(&out.stdout);
    for want in ["almide_rt_set_insert_owned(g, ", "almide_rt_set_remove_owned(g, "] {
        assert!(rust.contains(want), "the loop rebind should move its set into `{want}`");
    }
}
