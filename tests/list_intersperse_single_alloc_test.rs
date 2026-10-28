//! wasm `list.intersperse` builds its result with ONE allocation: the length
//! is known up front (2·len − 1 slots, or none for an empty list), so each
//! element and separator is stored into place. It used to push slot by slot,
//! regrowing the result by doubling: 1000 Ints in took 11 allocations and
//! 40,480 bytes, and how much of the regrowth the allocator could extend in
//! place depended on the heap, so the stdlib inspection's bytes-moved meter
//! fit no class (GCE c4-standard-4, develop 595a37476).
//!
//! The outputs pin both legs to the same answer, including handle elements
//! (every slot — each copied element and each separator — holds its own
//! credit, so the source list and the separator stay whole).
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  println("${list.intersperse([1, 2, 3], 0)}")
  let e: List[Int] = []
  println("${list.intersperse(e, 9)} ${list.intersperse([7], 9)}")
  println("${list.intersperse([1.5, -2.0], 0.25)}")
  let ss = ["a", "bb", "ccc"]
  let j = list.intersperse(ss, ", ")
  println("${j} ${ss} ${list.join(j, "")}")
  let ls = [[1], [2, 2], []]
  let k = list.intersperse(ls, [0])
  println("${k} ${list.len(ls)}")
  println("${list.intersperse([true, false], true)}")
}
"#;

const EXPECTED: &str = r#"[1, 0, 2, 0, 3]
[] [7]
[1.5, 0.25, -2]
["a", ", ", "bb", ", ", "ccc"] ["a", "bb", "ccc"] a, bb, ccc
[[1], [0], [2, 2], [0], []] 3
[true, true, false]
"#;

/// 1000 Ints in, 1999 out: the range's block (8000 bytes) and the result's
/// (15,992), nothing else.
const SIZED: &str = r#"fn main() -> Unit = {
  let xs = list.range(0, 1000)
  let p = list.intersperse(xs, -1)
  println("${list.len(p)}")
}
"#;

fn run(src: &str, wasm: bool, count: bool) -> (String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("intersperse.almd");
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
fn intersperse_prints_the_same_on_both_legs() {
    for wasm in [false, true] {
        assert_eq!(run(SOURCE, wasm, false).0, EXPECTED, "wasm={wasm}");
    }
}

#[test]
fn wasm_intersperse_allocates_the_result_once() {
    let (out, err) = run(SIZED, true, true);
    assert_eq!(out, "1999\n");
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("alloc count line");
    for want in ["allocs=2 ", "bytes=23992 ", "live=0"] {
        assert!(line.contains(want), "expected {want:?} in {line}");
    }
}
