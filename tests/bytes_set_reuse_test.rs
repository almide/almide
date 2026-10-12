//! #3577: `bytes.set` is declared `@reuse(b)`, so a source at its last use —
//! the rhs of `b = bytes.set(b, …)`, a fold's accumulator, a self tail call's
//! argument — is written in place on both legs instead of copied whole on
//! every step. Native calls the owned twin `almide_rt_bytes_set_owned` with the
//! binding's buffer (`AlmideRcCow::into_inner`: moved when it is the only
//! holder, copied when shared); wasm hands the dying block to `$cow`.
//!
//! VALUES: the shapes that must still copy — a source shared with another
//! binding, captured by a closure, held in a container, read again by another
//! argument — print what they printed before the change (the outputs below
//! are the pre-change compiler's, both legs). COST: 2,000 steps over a
//! 100,000-byte buffer in each rebind shape allocate 316 KB on wasm (the
//! embedded host's counter); the pre-change compiler took a copy per step,
//! 600 MB in all.
use std::process::Command;

const SOURCE: &str = r#"fn tail_b(b: Bytes, n: Int) -> Bytes =
  if n == 0 then b else tail_b(bytes.set(b, n % 4, n), n - 1)

effect fn growth() -> Unit = {
  var g = bytes.new(100000)
  var i = 0
  while i < 2000 {
    g = bytes.set(g, i % 9, i)
    i = i + 1
  }
  let t = tail_b(bytes.new(100000), 2000)
  let f = list.fold(list.range(0, 2000), bytes.new(100000), (acc, k) => bytes.set(acc, k % 5, k))
  println("${bytes.len(g)} ${bytes.get(g, 2) ?? -1} ${bytes.len(t)} ${bytes.get(t, 1) ?? -1} ${bytes.get(f, 4) ?? -1}")
}

effect fn main() -> Unit = {
  growth()!
  // 1. shared source keeps value semantics
  var x = bytes.from_list([1, 2, 3])
  let y = x
  x = bytes.set(x, 0, 9)
  println("${bytes.to_list(x)} ${bytes.to_list(y)}")
  // 2. closure captures the source before the edit
  var c = bytes.from_list([5, 6])
  let f = () => bytes.get(c, 0) ?? -1
  c = bytes.set(c, 0, 50)
  println("${f()} ${bytes.to_list(c)}")
  // 3. held in a container
  var h = bytes.from_list([7, 7])
  let box = [h]
  h = bytes.set(h, 1, 8)
  println("${list.map(box, (b) => bytes.to_list(b))} ${bytes.to_list(h)}")
  // 4. read again by another argument
  var r = bytes.from_list([1, 2, 3])
  r = bytes.set(r, 0, bytes.len(r) + (bytes.get(r, 2) ?? 0))
  println("${bytes.to_list(r)}")
  // 5. out of range and wrap
  var o = bytes.from_list([1, 2])
  o = bytes.set(o, -1, 4)
  o = bytes.set(o, 2, 4)
  o = bytes.set(o, 1, 300)
  println("${bytes.to_list(o)}")
  // 6. a list of Bytes edited through list.set keeps each element whole
  var l = [bytes.from_list([1]), bytes.from_list([2])]
  let e = bytes.set(list.get(l, 0) ?? bytes.new(0), 0, 3)
  l = list.set(l, 1, e)
  println("${list.map(l, (b) => bytes.to_list(b))}")
  // 7. a temporary source: a call's result goes to the owned twin as is
  println("${bytes.to_list(bytes.set(bytes.from_list([2, 2, 100]), 0, -1))}")
  let tb = bytes.from_list([1, 2])
  println("${bytes.to_list(bytes.set(bytes.concat(tb, tb), 3, 7))} ${bytes.to_list(tb)}")
}
"#;

const EXPECTED: &str = r#"100000 199 100000 1 207
[9, 2, 3] [1, 2, 3]
50 [50, 6]
[[7, 7]] [7, 8]
[6, 2, 3]
[1, 44]
[[1], [3]]
[255, 2, 100]
[1, 2, 1, 7] [1, 2]
"#;

fn write_source() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("bytes_set.almd");
    std::fs::write(&file, SOURCE).expect("source");
    (dir, file)
}

fn run(wasm: bool, count: bool) -> (String, String) {
    let (_dir, file) = write_source();
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    if count {
        c.env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn bytes_set_keeps_value_semantics_on_both_legs() {
    assert_eq!(run(false, false).0, EXPECTED, "native");
    assert_eq!(run(true, false).0, EXPECTED, "wasm");
}

#[test]
fn wasm_bytes_set_writes_a_dying_source_in_place() {
    let (_, err) = run(true, true);
    let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("alloc count line");
    let bytes: u64 = line
        .split_whitespace()
        .find_map(|w| w.strip_prefix("bytes="))
        .and_then(|v| v.parse().ok())
        .expect("bytes=");
    assert!(bytes < 1_000_000, "a copy per step is back: {line}");
    assert!(line.contains("live=0"), "{line}");
}

#[test]
fn native_bytes_set_rebind_calls_the_owned_twin() {
    let (_dir, file) = write_source();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg(&file)
        .args(["--target", "rust"])
        .output()
        .expect("emit");
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(
        rust.contains("almide_rt_bytes_set_owned(AlmideRcCow::into_inner(g)"),
        "the loop rebind should hand its buffer to the owned twin"
    );
}
