//! wasm `list.take_while` builds its prefix with ONE allocation and one
//! `memory.copy`, the way `list.take` does. It used to push element by
//! element, regrowing the result by doubling: for a 900-element prefix that
//! was 9 allocations and up to twice the prefix's bytes copied, and how much
//! of that the allocator could extend in place depended on the heap. The
//! stdlib inspection's bytes-moved meter read that as a different constant at
//! each size (c4-standard-4, n=20000 one doubling step short of the larger
//! sizes), so the function could not be stamped.
//!
//! The outputs pin both legs to the same answer, including handle elements
//! (each prefix element is shared, never moved: the source stays whole) and
//! a rebind that takes the prefix of itself.
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3, 10, 4, 5]
  println("${list.take_while(xs, (x) => x < 5)}")
  println("${list.take_while(xs, (x) => x < 100)}")
  println("${list.take_while(xs, (x) => x > 100)}")
  let e: List[Int] = []
  println("${list.take_while(e, (x) => x > 0)}")
  let ss = ["a", "bb", "ccc", "", "dddd"]
  let p = list.take_while(ss, (s) => string.len(s) > 0)
  println("${p} ${ss}")
  let ls = [[1], [2, 2], [], [3]]
  let q = list.take_while(ls, (l) => list.len(l) > 0)
  println("${q} ${list.len(ls)}")
  let fs = [1.5, 2.5, -1.0, 3.0]
  println("${list.take_while(fs, (f) => f > 0.0)}")
  var acc: List[String] = ["x", "y", "z"]
  var i = 0
  while i < 3 {
    acc = list.take_while(acc, (s) => s != "z")
    i = i + 1
  }
  println("${acc}")
}
"#;

const EXPECTED: &str = r#"[1, 2, 3]
[1, 2, 3, 10, 4, 5]
[]
[]
["a", "bb", "ccc"] ["a", "bb", "ccc", "", "dddd"]
[[1], [2, 2]] 4
[1.5, 2.5]
["x", "y"]
"#;

/// 1000 Ints in, a 900-element prefix out: the range's block (8000 bytes)
/// and the prefix's (7200), nothing else.
const SIZED: &str = r#"fn main() -> Unit = {
  let xs = list.range(0, 1000)
  let p = list.take_while(xs, (x) => x < 900)
  println("${list.len(p)}")
}
"#;

fn run(src: &str, wasm: bool, count: bool) -> (String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("take_while.almd");
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    if count {
        c.env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn take_while_prints_the_same_prefix_on_both_legs() {
    for wasm in [false, true] {
        assert_eq!(run(SOURCE, wasm, false).0, EXPECTED, "wasm={wasm}");
    }
}

#[test]
fn wasm_take_while_allocates_the_prefix_once() {
    let (out, err) = run(SIZED, true, true);
    assert_eq!(out, "900\n");
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("alloc count line");
    for want in ["allocs=2 ", "bytes=15200 ", "live=0"] {
        assert!(line.contains(want), "expected {want:?} in {line}");
    }
}
