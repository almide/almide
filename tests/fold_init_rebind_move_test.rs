//! native: `x = list.fold(xs, x, f)` MOVES the dying accumulator into the
//! fold's `init` instead of copying it.
//!
//! The stdlib inspection (String instantiation) measured `list.fold` moving
//! O(n) bytes per call on native and O(1) on wasm: the inline chain lowering
//! rendered `x = (xs).into_iter().fold(x.clone(), …)`. The statement
//! overwrites `x`, so that read is its last one — the reassignment move
//! (#3404) applies to it exactly as it does to `x = f(x)`, `x = map.fold(m,
//! x, …)` and the tail-call accumulator. It missed the fold because the
//! use walk flags every occurrence inside an iterator chain node, and the
//! reassignment rule refused any flagged read; but a chain's `init` (like
//! its source) runs once, before the iteration — only the step / collector
//! lambdas run per element, and those are already excluded as loop bodies.
//!
//! The positive cells cover the inline fold over an owned and a borrowed
//! source, a fused `map |> fold`, and pin the accumulator siblings that
//! already moved (`map.fold`, `set.fold`, `list.scan`, the self tail call).
//! The negative cells keep the copy where `x` is read again: inside the
//! callback, a second time in the same right side, and a non-rebind whose
//! source is read afterwards. The differential runs the program on both
//! legs and byte-compares stdout.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_fold_init_rebind_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const PROGRAM: &str = r#"fn a0() -> List[String] = list.range(0, 2) |> list.map((i) => "s${100000000 + i}")

fn plain(n: Int) -> String = {
  var x: String = string.repeat("aé", 50)
  var i = 0
  while i < n {
    x = list.fold(a0(), x, (p0, p1) => p0)
    i = i + 1
  }
  x
}

fn fused(n: Int) -> String = {
  var x: String = "f"
  for _i in 0..<n {
    x = a0() |> list.map((s) => string.take(s, 2)) |> list.fold(x, (a, b) => a + b)
  }
  x
}

fn borrowed(n: Int) -> String = {
  let ys = a0()
  var x: String = "b"
  for _i in 0..<n {
    x = list.fold(ys, x, (a, b) => a + string.take(b, 1))
  }
  x
}

fn siblings(n: Int) -> String = {
  let m: Map[String, Int] = map.from_list([("k", 1)])
  let st: Set[String] = set.from_list(["e"])
  var x: String = "s"
  for _i in 0..<n {
    x = map.fold(m, x, (acc, k, v) => acc + k)
    x = set.fold(st, x, (acc, e) => acc + e)
    x = list.last(list.scan(a0(), x, (a, b) => a + "~")) ?? ""
  }
  x
}

fn captured(n: Int) -> String = {
  var x: String = "c"
  for _i in 0..<n {
    x = list.fold(a0(), x, (a, b) => a + string.take(x, 1))
  }
  x
}

fn twice(n: Int) -> String = {
  var x: String = "w"
  for _i in 0..<n {
    x = list.fold(a0(), x, (a, b) => a + "-") + x
  }
  x
}

fn kept(x: String) -> String = {
  let y = list.fold(a0(), x, (a, b) => a + "+")
  "${x}${y}"
}

fn go(xs: List[String], acc: String, k: Int) -> String = if k == 0 then acc else go(xs, list.fold(xs, acc, (p, q) => p + "g"), k - 1)

effect fn main() -> Unit = {
  println("${string.len(plain(3))}")
  println(fused(2))
  println(borrowed(2))
  println(siblings(2))
  println(captured(3))
  println(twice(2))
  println(kept("k"))
  println(go(a0(), "t", 2))
}
"#;

const EXPECTED: &str = "100\nfs1s1s1s1\nbssss\nske~~ke~~\nccccccc\nw--w--w--w\nkk++\ntgggg\n";

fn emit_rust() -> String {
    let file = scratch("emit").join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide())
        .args([file.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("almide emit");
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The body of `pub fn {name}` in the emitted Rust.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("fn {name} is emitted"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}\n").expect("fn ends")]
}

#[test]
fn a_dying_accumulator_moves_into_the_fold_init() {
    let rust = emit_rust();
    for name in ["plain", "fused", "borrowed"] {
        let b = body(&rust, name);
        assert!(b.contains(".fold(x, "), "{name}: the fold init does not move `x`:\n{b}");
        assert!(!b.contains("x.clone()"), "{name}: the dying accumulator is still copied:\n{b}");
    }
}

#[test]
fn the_accumulator_siblings_keep_moving() {
    let rust = emit_rust();
    let s = body(&rust, "siblings");
    for needle in ["almide_rt_map_fold(&m, x,", "almide_rt_set_fold(&st, x,", "almide_rt_list_scan(a0(), x,"] {
        assert!(s.contains(needle), "siblings: expected `{needle}`:\n{s}");
    }
    assert!(!s.contains("x.clone()"), "siblings: an accumulator is copied:\n{s}");
    let g = body(&rust, "go");
    assert!(g.contains(".fold(acc, "), "go: the tail-call accumulator is copied:\n{g}");
}

#[test]
fn an_accumulator_read_again_is_still_copied() {
    let rust = emit_rust();
    for name in ["captured", "twice", "kept"] {
        let b = body(&rust, name);
        assert!(!b.contains(".fold(x, "), "{name}: `x` is read again, so the init must not move it:\n{b}");
    }
}

#[test]
fn native_and_wasm_agree() {
    let file = scratch("run").join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    for target in [None, Some("wasm")] {
        let mut cmd = Command::new(almide());
        cmd.args(["run", file.to_str().unwrap()]);
        if let Some(t) = target {
            cmd.args(["--target", t]);
        }
        let out = cmd.output().expect("almide run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{target:?} stdout");
    }
}
