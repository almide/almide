//! #3533: `list.set` / `swap` / `update` / `drop_end` are declared `@reuse(xs)`,
//! so a source at its last use — the rhs of `x = f(x, …)`, a fold's
//! accumulator, a self tail call's argument — is written in place on both
//! legs instead of copied whole on every step.
//!
//! Two pins. VALUES: the shapes that must still copy — a source shared with
//! another binding, captured by a closure, viewed by a `match` binder, held
//! in a container, read again by an operand or by another argument — print
//! what they printed before the change (the outputs below are the pre-change
//! compiler's, both legs). COST: 2,000 steps over a 100,000-element list in
//! each rebind shape allocate 2.4 MB on wasm (the embedded host's counter);
//! the pre-change compiler took a copy per step, 11.1 GB in all.
use std::process::Command;

const SOURCE: &str = r#"
fn tail_g(p: List[Int], n: Int) -> List[Int] =
  if n == 0 then p else tail_g(list.swap(list.set(p, n % 7, n), 0, 1), n - 1)

effect fn growth() -> Unit = {
  var g = list.range(0, 100000)
  var i = 0
  while i < 2000 {
    g = list.set(g, i % 9, i)
    g = list.swap(g, 0, 3)
    g = list.update(g, 2, (e) => e + 1)
    g = list.drop_end(g, 1)
    i = i + 1
  }
  let t = tail_g(list.range(0, 100000), 2000)
  let f = list.fold(list.range(0, 2000), list.range(0, 100000), (acc, k) => list.drop_end(list.swap(acc, 0, k % 5), 1))
  println("${list.len(g)} ${list.get(g, 2) ?? -1} ${list.len(t)} ${list.get(t, 0) ?? -1} ${list.len(f)}")
}

fn tail_set(p: List[String], n: Int) -> List[String] =
  if n == 0 then p else tail_set(list.set(p, n % 3, "t${n}"), n - 1)

fn tail_swap(p: List[List[Int]], n: Int) -> List[List[Int]] =
  if n == 0 then p else tail_swap(list.swap(p, 0, n % 3), n - 1)

fn tail_upd(p: List[String], n: Int) -> List[String] =
  if n == 0 then p else tail_upd(list.update(p, 1, (s) => s + "u"), n - 1)

fn tail_drop(p: List[String], n: Int) -> List[String] =
  if n == 0 then p else tail_drop(list.drop_end(p, 1), n - 1)

fn tail_reads_other(p: List[Int], m: Int, n: Int) -> Int =
  if n == 0 then m + list.len(p) else tail_reads_other(list.set(p, 0, n), list.len(p) + m, n - 1)

fn tail_two_args(p: List[Int], q: List[Int], n: Int) -> List[Int] =
  if n == 0 then p + q else tail_two_args(list.set(p, 0, n), list.set(q, 0, list.get(p, 0) ?? 0), n - 1)

effect fn main() -> Unit = {
  growth()!
  // 1. shared source keeps value semantics
  var x = ["a", "b", "c"]
  let y = x
  x = list.set(x, 0, "z")
  x = list.swap(x, 1, 2)
  x = list.update(x, 1, (s) => s + "!")
  println("${x} ${y}")
  // 2. closure captures the source before the edit
  var c = [1, 2, 3]
  let f = () => list.len(c) + (list.get(c, 0) ?? 0)
  c = list.set(c, 0, 100)
  println("${f()} ${c}")
  // 3. match binder views the same block
  var m = ["p", "q", "r"]
  m = match m {
    [a, ..] => list.set(m, 2, a),
    [] => m,
  }
  println("${m}")
  // 4. out of range
  var o = ["o1", "o2"]
  o = list.set(o, -1, "bad")
  o = list.set(o, 2, "bad")
  o = list.swap(o, 0, 5)
  o = list.update(o, 9, (s) => s + "bad")
  println("${o} ${list.drop_end(o, -1)} ${list.drop_end(o, 5)}")
  var d = ["d1", "d2", "d3", "d4"]
  d = list.drop_end(d, 0)
  d = list.drop_end(d, 2)
  println("${d}")
  // 5. operands that read the source
  var s = [10, 20, 30]
  s = list.set(s, 0, list.len(s))
  s = list.update(s, 1, (e) => e + list.len(s))
  s = list.swap(s, 0, list.len(s) - 1)
  s = list.drop_end(s, list.len(s) - 2)
  println("${s}")
  // 6. nested handles in loops
  var h = [[1], [2], [3]]
  var i = 0
  while i < 50 {
    h = list.set(h, i % 3, [i, i])
    h = list.swap(h, 0, 2)
    h = list.update(h, 1, (e) => e + [i])
    i = i + 1
  }
  println("${list.map(h, (e) => list.len(e))}")
  // 7. fold shapes
  let ff = list.fold(list.range(0, 40), ["f0", "f1", "f2", "f3"], (acc, k) => list.set(acc, k % 4, "k${k}"))
  let fs = list.fold(list.range(0, 41), [[0], [1], [2]], (acc, k) => list.swap(acc, 0, k % 3))
  let fu = list.fold(list.range(0, 10), ["u"], (acc, k) => list.update(acc, 0, (e) => e + "${k}"))
  let fd = list.fold(list.range(0, 3), ["a", "b", "c", "d", "e"], (acc, _) => list.drop_end(acc, 1))
  println("${ff} ${fs} ${fu} ${fd}")
  // 8. tail shapes
  let base = ["b0", "b1", "b2"]
  println("${tail_set(base, 20)} ${base}")
  println("${tail_swap([[7], [8], [9]], 7)}")
  println("${tail_upd(["x", "y"], 5)}")
  println("${tail_drop(["1", "2", "3", "4", "5"], 3)}")
  println("${tail_reads_other([5, 6], 0, 4)}")
  println("${tail_two_args([1, 2], [3, 4], 3)}")
  // 9. alias kept in a container
  var a = ["k1", "k2"]
  let box = [a, a]
  a = list.set(a, 0, "K")
  a = list.drop_end(a, 1)
  println("${a} ${box}")
}
"#;

const EXPECTED: &str = r#"98000 2000 100000 1 98000
["z", "c!", "b"] ["a", "b", "c"]
103 [100, 2, 3]
["p", "q", "p"]
["o1", "o2"] [] []
["d1", "d2"]
[30, 23]
[2, 3, 2]
["k36", "k37", "k38", "k39"] [[0], [2], [1]] ["u0123456789"] ["a", "b"]
["t3", "t1", "t2"] ["b0", "b1", "b2"]
[[9], [8], [7]]
["x", "yuuuuu"]
["1", "2"]
10
[1, 2, 2, 4]
["K"] [["k1", "k2"], ["k1", "k2"]]
"#;

fn run(dir: &std::path::Path, wasm: bool) -> (String, String) {
    let file = dir.join("point_edits.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"])
            .env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn point_edits_write_a_dying_source_in_place_and_keep_value_semantics() {
    let dir = tempfile::tempdir().expect("scratch");
    let (native, _) = run(dir.path(), false);
    let (wasm, counts) = run(dir.path(), true);
    assert_eq!(native, EXPECTED, "native");
    assert_eq!(wasm, EXPECTED, "wasm");
    let field = |k: &str| -> u64 {
        counts
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {counts}"))
    };
    assert_eq!(field("live="), 0, "{counts}");
    let bytes = field("bytes=");
    assert!(
        bytes < 64_000_000,
        "the point edits allocated {bytes} bytes — a copy per step again? {counts}"
    );
}

#[test]
fn native_point_edits_move_the_dying_source_into_the_owned_twin() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("point_edits.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg(&file)
        .args(["--target", "rust"])
        .output()
        .expect("emit");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rust = String::from_utf8_lossy(&out.stdout);
    for twin in [
        "almide_rt_list_swap_owned(g,",
        "almide_rt_list_drop_end_owned(g,",
    ] {
        assert!(
            rust.contains(twin),
            "expected `{twin}` in the loop: the rebind should move `g`"
        );
    }
}
