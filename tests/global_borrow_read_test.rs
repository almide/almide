//! A module-level `var` handed to a read-only (borrowed) parameter on the
//! native leg (#4562): `map.get(cache, k)`, `list.get(xs, i)`,
//! `string.starts_with(g, p)`, or a user fn's borrowed param.
//!
//! A heap global lives in a `RefCell<Rc<T>>`, and its bare read is a deep copy
//! of `T`. A borrowed argument took `&` of that copy, so every lookup in a
//! module-level map copied the whole map. The argument now borrows a snapshot
//! of the cell's `Rc`: a reference count, not a copy. The values must not
//! change, and that includes a callee that writes the global while it holds
//! the argument: the argument keeps the call site's value.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const SRC: &str = r#"
var m: Map[Int, Int] = map.new()
var xs: List[Int] = []
var g: String = ""

fn bump() -> Unit = {
  m = map.set(m, 1, 100)
}

fn read_after_bump(x: Map[Int, Int]) -> Int = {
  bump()
  map.get(x, 1) ?? 0
}

fn look(k: Int) -> Int = map.get(m, k) ?? 0

fn has(k: Int) -> Bool = map.contains(m, k)

fn at(i: Int) -> Int = list.get(xs, i) ?? 0

fn count() -> Int = list.len(xs)

fn pre() -> Bool = string.starts_with(g, "he")

effect fn main() -> Unit = {
  m = map.set(m, 1, 1)
  let a = read_after_bump(m)
  let b = look(1)
  xs = [1, 2, 3]
  g = "hello"
  println("${a} ${b} ${has(1)} ${has(2)} ${at(2)} ${count()} ${pre()}")
}
"#;

const WANT: &str = "1 100 true false 3 3 true\n";

/// The other read positions of a heap global (#4562, second half): a field
/// read, a `for` head, a `match` subject, `==`, interpolation, a tuple and a
/// record destructure, and a fn that returns the global. `walk` writes the
/// global it is iterating: the loop still walks the value it started on.
const PLACES_SRC: &str = r#"
type St = { items: List[Int], count: Int, name: String }

var st: St = { items: [], count: 0, name: "" }
var tl: (List[Int], Int) = ([], 0)
var ol: Option[List[Int]] = none
var ws: String = ""
var wl: List[Int] = []

fn field_n() -> Int = st.count

fn field_len() -> Int = list.len(st.items)

fn field_items() -> List[Int] = st.items

fn walk() -> Int = {
  var a = 0
  for x in wl {
    wl = []
    a = a + x
  }
  a
}

fn opt_len() -> Int = match ol {
  some(v) => list.len(v),
  none => 0,
}

fn same() -> Bool = wl == []

fn shout() -> String = "${ws}!"

fn pair() -> Int = {
  let (a, b) = tl
  b + list.len(a)
}

fn rec() -> Int = {
  let { items, count } = st
  count + list.len(items)
}

fn whole() -> List[Int] = wl

effect fn main() -> Unit = {
  st = { items: [1, 2, 3], count: 7, name: "n" }
  tl = ([4, 5], 6)
  ol = some([1, 2])
  ws = "hi"
  wl = [1, 2, 3]
  let w = walk()
  println("${field_n()} ${field_len()} ${list.len(field_items())} ${w} ${list.len(wl)} ${same()} ${opt_len()} ${shout()} ${pair()} ${rec()} ${list.len(whole())}")
}
"#;

const PLACES_WANT: &str = "7 3 3 6 0 true 2 hi! 8 10 0\n";

fn source_file(tag: &str) -> std::path::PathBuf {
    source_file_of(tag, SRC)
}

fn source_file_of(tag: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-global-borrow-read-{tag}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    file
}

fn run(target: Option<&str>) -> String {
    run_file(source_file(target.unwrap_or("native")), target)
}

fn run_file(file: std::path::PathBuf, target: Option<&str>) -> String {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn borrowed_global_arg_keeps_its_values_natively() {
    let out = run(None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn borrowed_global_arg_keeps_its_values_on_wasm() {
    let out = run(Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// The body of the emitted `fn name`, up to the next top-level `}`.
fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let at = rust.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no `fn {name}` in the emitted Rust:\n{rust}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
}

/// Every read-only use of a global in the emitted Rust borrows a snapshot of
/// the cell's `Rc` and never deep-copies the global out of its cell. This reads
/// the standard codegen, which `--target rust` prints. The certified native
/// render serves the same reads from an `Rc` snapshot of its heap-slot cell;
/// that leg is held by native_v1_differential_test and the perf ratchet's
/// GLOBAL READ relation.
#[test]
fn borrowed_global_arg_is_not_copied() {
    let file = source_file("emit");
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    for name in ["look", "has", "at", "count", "pre"] {
        let body = fn_body(&rust, name);
        assert!(body.contains("std::rc::Rc::clone(&*__almide_cell.borrow())"), "`{name}` does not borrow the global's Rc:\n{body}");
        assert!(!body.contains("__almide_cell.borrow()).clone()"), "`{name}` copies the global out of its cell:\n{body}");
    }
}

#[test]
fn global_read_places_keep_their_values_natively() {
    let out = run_file(source_file_of("places-native", PLACES_SRC), None);
    assert!(out.contains(PLACES_WANT), "native:\n{out}");
}

#[test]
fn global_read_places_keep_their_values_on_wasm() {
    let out = run_file(source_file_of("places-wasm", PLACES_SRC), Some("wasm"));
    assert!(out.contains(PLACES_WANT), "wasm:\n{out}");
}

/// No read-only position copies the whole global out of its cell: each reads
/// a snapshot of the cell's `Rc`, and clones at most the part it keeps.
/// `whole` returns the global, so it keeps exactly one copy (it had two).
#[test]
fn global_read_places_are_not_copied() {
    let file = source_file_of("places-emit", PLACES_SRC);
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    for name in ["field_n", "field_len", "field_items", "walk", "opt_len", "same", "shout", "pair", "rec"] {
        let body = fn_body(&rust, name);
        assert!(body.contains("std::rc::Rc::clone(&*__almide_cell.borrow())"), "`{name}` does not read a snapshot of the global's Rc:\n{body}");
        assert!(!body.contains("__almide_cell.borrow()).clone()"), "`{name}` copies the global out of its cell:\n{body}");
    }
    let whole = fn_body(&rust, "whole");
    assert_eq!(whole.matches(".clone()").count(), 1, "`whole` must copy the global once:\n{whole}");
}
