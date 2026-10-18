//! #3217 — a `for` over a list the fn owns, at that list's last use, moves
//! its elements out instead of copying each one.
//!
//! `for x in xs { if f(x) then … }` with `f` taking its argument owned
//! rendered `for x in xs.iter().cloned()` over an OWNED `xs: Vec<T>`: the
//! caller copied the list to hand it over (`xs.clone()`) and the loop then
//! copied every element again. The loop is `xs`'s last use, so it consumes
//! it (`into_iter`) and each element moves into `f`. A list read after the
//! loop is still iterated by copy.
use std::process::Command;

const SRC: &str = r#"fn count_hits(xs: List[String], f: (String) -> Bool) -> Int = {
  var n = 0
  for x in xs {
    if f(x) then n = n + 1
  }
  n
}

fn count_then_len(xs: List[String], f: (String) -> Bool) -> Int = {
  var n = 0
  for x in xs {
    if f(x) then n = n + 1
  }
  n + list.len(xs)
}

effect fn main() -> Unit = {
  let xs = ["a", "bb", "ccc"]
  println(int.to_string(count_hits(xs, (s) => string.len(s) > 1)))
  println(int.to_string(count_then_len(xs, (s) => string.len(s) > 1)))
  println(int.to_string(list.len(xs)))
}
"#;

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len())]
}

#[test]
fn a_last_use_loop_moves_the_elements_out() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("consume.almd");
    std::fs::write(&file, SRC).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    let hits = fn_body(&rust, "count_hits");
    assert!(!hits.contains(".iter().cloned()"), "the last use of an owned list moves its elements:\n{hits}");
    let after = fn_body(&rust, "count_then_len");
    assert!(after.contains(".iter().cloned()"), "a list read after the loop is iterated by copy:\n{after}");
}

#[test]
fn both_legs_print_the_same() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("consume.almd");
    std::fs::write(&file, SRC).expect("source");
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "2\n5\n3\n", "{target:?}");
    }
}
