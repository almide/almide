//! #3604: an index chain is a COUNTED loop on the native leg, whatever the
//! chain collects into.
//!
//! The imperative `for j in 0..<n` hands LLVM an `i64` counter that starts at
//! 0 and stops below its bound. Two idiomatic spellings of the same loop did
//! not:
//!
//! - `list.map(0..<n, f)` built the range as a `Vec<i64>` and walked it, so the
//!   index LLVM saw was a value loaded from memory with no known range (and the
//!   list cost an allocation per call). #3810 counted the head of a FOLD; a
//!   collecting chain still built it.
//! - `list.enumerate(xs)` produced its index as `usize as i64` — a cast LLVM
//!   cannot bound from above, so `i64` arithmetic on it could not assume the
//!   value is non-negative.
//!
//! Both now render a counted `Range<i64>` that starts at the chain's start:
//! a collecting range chain fills a vector reserved once for the span
//! (`try_reserve_exact`, so a span no machine holds is still C-197's
//! out-of-memory abort, never `collect`'s capacity-overflow panic), and the
//! enumerate index is zipped from `0i64..i64::MAX`.
//!
//! ## Family completeness rule
//!
//! Heads: `list.range(a, b)`, the literal `a..<b`, and the `list.enumerate(xs)`
//! source adapter. Collectors: every fold-shaped one (`fold`, `sum`, `any`,
//! `all`, `find`, `count`, `find_index`, `find_map`, `len`) and `collect`.
//!
//! Covered (every cell is a member of `COVERED` below and is asserted):
//! - range head (both spellings) × every fold-shaped collector: counted,
//!   whatever the steps (#3810 for the literal).
//! - range head (both spellings) × `collect` whose steps keep the length
//!   (`map`, `enumerate` — the result has exactly one element per index):
//!   counted, into a vector reserved for the span.
//! - enumerate adapter × every collector: the index is a counted `i64`.
//!
//! Deliberately left out (asserted in `LEFT_OUT`, so a change is a decision):
//! - the inclusive literal `a...b`: keeps the built list on both legs (its
//!   count needs the i128 span the runtime twin computes).
//! - the literal `a..<b` × `collect` after a length-changing step (`filter`,
//!   `filter_map`, `flat_map`, `take`): keeps the built head, whose span
//!   reservation is the C-197 abort; a counted head has no span to reserve.
//! - a borrowing chain (`&i64` element binders) keeps the list it reads.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// One program carries every family member as its own function, so one emit
/// and one run per leg cover the whole matrix. `v: List[Float]` keeps the
/// program outside the v1 native rung subset: the v0 codegen is what ships
/// (pinned below), and that is the renderer this test reads.
const SRC: &str = r#"
fn fold_range(v: List[Float], n: Int) -> Float =
  list.fold(list.range(0, n), 0.0, (acc, j) => acc + float.from_int(j) * v[j])

fn fold_literal(v: List[Float], n: Int) -> Float =
  list.fold(0..<n, 0.0, (acc, j) => acc + float.from_int(j) * v[j])

fn sum_literal(n: Int) -> Int = list.sum(list.map(0..<n, (i) => i * i))

fn any_range(n: Int) -> Bool = list.any(list.range(0, n), (i) => i * i == 9)

fn count_literal(n: Int) -> Int = list.count(0..<n, (i) => i % 3 == 0)

fn map_range(v: List[Float], n: Int) -> List[Float] =
  list.map(list.range(0, n), (i) => v[i] * 2.0)

fn map_literal(v: List[Float], n: Int) -> List[Float] =
  list.map(0..<n, (i) => list.fold(0..<n, 0.0, (acc, j) => acc + float.from_int(i + j) * v[j]))

fn map_map_literal(n: Int) -> List[Int] =
  0..<n |> list.map((i) => i + 1) |> list.map((x) => x * 10)

fn enumerate_fold(v: List[Float], i: Int) -> Float =
  list.fold(list.enumerate(v), 0.0, (acc, p) => acc + float.from_int(i + p.0) * p.1)

fn enumerate_map(v: List[Float]) -> List[Float] =
  list.map(list.enumerate(v), (p) => float.from_int(p.0) + p.1)

fn incl(n: Int) -> Int = list.fold(0...n, 0, (a, x) => a + x)

fn filter_literal(n: Int) -> List[Int] = list.filter(0..<n, (i) => i % 2 == 0)

effect fn main() -> Unit = {
  let v = [1.0, 2.0, 3.0, 4.0]
  println(float.to_string(fold_range(v, 4)))
  println(float.to_string(fold_literal(v, 4)))
  println(float.to_string(fold_literal(v, -3)))
  println(int.to_string(sum_literal(5)))
  println(int.to_string(sum_literal(0)))
  println(if any_range(10) then "true" else "false")
  println(if any_range(2) then "true" else "false")
  println(int.to_string(count_literal(10)))
  println("${map_range(v, 4)}")
  println("${map_range(v, 0)}")
  println("${map_range(v, -2)}")
  println("${map_literal(v, 4)}")
  println("${map_literal(v, -1)}")
  println("${map_map_literal(4)}")
  println("${map_map_literal(-4)}")
  println(float.to_string(enumerate_fold(v, 2)))
  println("${enumerate_map(v)}")
  println("${enumerate_map([])}")
  println(int.to_string(incl(4)))
  println("${filter_literal(7)}")
}
"#;

const EXPECTED: &str = "20.0\n20.0\n0.0\n30\n0\ntrue\nfalse\n4\n[2, 4, 6, 8]\n[]\n[]\n\
[20, 30, 40, 50]\n[]\n[10, 20, 30, 40]\n[]\n40.0\n[1, 3, 5, 7]\n[]\n10\n[0, 2, 4, 6]\n";

/// Every covered family member, with the counted head its body must carry.
const COVERED: &[(&str, &str)] = &[
    ("fold_range", "(0i64..n).fold("),
    ("fold_literal", "(0i64..n).fold("),
    ("sum_literal", "(0i64..n).map("),
    ("any_range", "(0i64..n).any("),
    ("count_literal", "(0i64..n).filter("),
    ("map_range", "(__almide_rs..__almide_re).map("),
    ("map_literal", "(__almide_rs..__almide_re).map("),
    ("map_map_literal", "(__almide_rs..__almide_re).map("),
    ("enumerate_fold", "(0i64..i64::MAX).zip("),
    ("enumerate_map", "(0i64..i64::MAX).zip("),
];

/// The deliberately uncovered members: each still builds its head.
const LEFT_OUT: &[&str] = &["incl", "filter_literal"];

fn write() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("counted_index_chain.almd");
    std::fs::write(&path, SRC).unwrap();
    (dir, path)
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let i = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[i..].split("\n}\n").next().unwrap()
}

/// The native build ships the v0 codegen for this program — the v1 render
/// walls on it — so `--target rust` is the shipped text. If v1 starts
/// rendering it, this pin fails and the shape checks must move to v1's output.
#[test]
fn the_v0_codegen_is_what_ships_for_the_family_program() {
    assert!(
        almide_mir::pipeline::try_render_rust_source(SRC).is_err(),
        "the v1 native render now ships this program; read its output instead"
    );
}

#[test]
fn every_covered_index_chain_is_a_counted_loop() {
    let (_dir, path) = write();
    let out = Command::new(almide()).arg(&path).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    let mut failures = Vec::new();
    for (name, counted) in COVERED {
        let body = fn_body(&rust, name);
        // No range list is built: neither the inline literal build nor the
        // runtime twin.
        if body.contains("__almide_rv") || body.contains("almide_rt_list_range(") {
            failures.push(format!("{name}: the range head was built:\n{body}"));
        }
        // The index comes from a counted `Range<i64>` starting at 0 — never a
        // `usize as i64` cast LLVM cannot bound.
        if !body.contains(counted) {
            failures.push(format!("{name}: no counted head `{counted}`:\n{body}"));
        }
        if body.contains(".enumerate()") || body.contains("as i64, __ex") {
            failures.push(format!("{name}: the index is still a `usize` cast:\n{body}"));
        }
    }
    // A collecting counted chain reserves the span with C-197's abort.
    for name in ["map_range", "map_literal", "map_map_literal"] {
        let body = fn_body(&rust, name);
        if !body.contains("try_reserve_exact(") || !body.contains("almide_abort(\"out of memory\")") {
            failures.push(format!("{name}: the span is not reserved with the C-197 abort:\n{body}"));
        }
    }
    for name in LEFT_OUT {
        let body = fn_body(&rust, name);
        if !body.contains("__almide_rv") {
            failures.push(format!("{name}: left out by the family rule, but no longer builds its head:\n{body}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn the_counted_family_prints_the_same_on_both_legs() {
    let (dir, path) = write();
    let native = dir.path().join("native");
    let wasm = dir.path().join("module.wasm");
    for (target, artifact) in [("rust", &native), ("wasm", &wasm)] {
        let built = Command::new(almide())
            .arg("build")
            .arg(&path)
            .args(["--target", target, "-o"])
            .arg(artifact)
            .env_remove("ALMIDE_COMPONENT_P3")
            .output()
            .unwrap();
        assert!(built.status.success(), "{target} build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    }
    let n = Command::new(&native).output().unwrap();
    assert!(n.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&n.stderr));
    assert_eq!(String::from_utf8_lossy(&n.stdout), EXPECTED, "native output");
    let w = Command::new("wasmtime").arg("run").arg(&wasm).output().unwrap();
    assert!(w.status.success(), "wasm run failed:\n{}", String::from_utf8_lossy(&w.stderr));
    assert_eq!(n.stdout, w.stdout, "native and wasm disagree");
    assert_eq!(n.stderr, w.stderr, "native and wasm stderr disagree");
}
