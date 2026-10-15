//! #3216 — a `for` binder read only inside a fused chain lambda is iterated
//! by reference on native.
//!
//! The clone pass iterates `for j in js` by reference when every use of `j`
//! only reads it (`only_borrowed_uses`). An occurrence inside a fused chain
//! (`list.any(ids, (x) => x == j)`) was refused outright, so the loop
//! rendered `for j in js.iter().cloned()`: one String copy per element for
//! a body that only compares. A chain lambda runs inside the loop iteration
//! and never outlives it, so a read there is a borrow like any other. A
//! chain that keeps the binder (`list.map(ids, (x) => j)`) still owns it.
use std::process::Command;

const SRC: &str = r#"fn hits(js: List[String], ids: List[String]) -> Int = {
  var n = 0
  for j in js {
    if list.any(ids, (x) => x == j) then n = n + 1
  }
  n
}

fn pairs(js: List[String], ids: List[String]) -> List[String] = {
  var out: List[String] = []
  for j in js {
    out = out + list.map(ids, (x) => x + j)
  }
  out
}

fn keeps(js: List[String], ids: List[String]) -> List[String] = {
  var out: List[String] = []
  for j in js {
    out = out + list.map(ids, (x) => j)
  }
  out
}

effect fn main() -> Unit = {
  println(int.to_string(hits(["a", "b", "c"], ["b", "c", "d"])))
  println(list.join(pairs(["1", "2"], ["a", "b"]), ","))
  println(list.join(keeps(["1", "2"], ["a", "b"]), ","))
}
"#;

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len())]
}

#[test]
fn a_binder_read_in_a_chain_is_iterated_by_reference() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("chain.almd");
    std::fs::write(&file, SRC).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    // `hits` only reads `j`; `pairs` and `keeps` copy it where the chain
    // needs an owned value (`j.clone()` inside the lambda), so the loop need
    // not copy each element first either.
    for name in ["hits", "pairs", "keeps"] {
        let body = fn_body(&rust, name);
        assert!(body.contains("for j in js.iter() {"), "{name}: the binder is iterated by reference:\n{body}");
    }
}

#[test]
fn every_shape_prints_the_same_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("chain.almd");
    std::fs::write(&file, SRC).expect("source");
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "2\na1,b1,a2,b2\n1,1,2,2\n", "{target:?}");
    }
}
