//! #3189: an anonymous record's structural identity is its field names AND
//! field types. A structural record (`Ty::Record` — an un-annotated literal,
//! or one annotated with an anonymous record type) IS a declared record type
//! exactly when it has that type's names and types, in any order; when
//! several declared types fit, a concrete one beats a generic one and the
//! first declared wins. Native and the interp matched on the sorted NAMES
//! alone, so `let r: { v: Int, n: Int } = { v: x, n: 1 }` beside
//! `type R = { v: String, n: Int }` was built as `R`'s struct (rustc E0308)
//! while wasm ran it; wasm in turn matched a GENERIC record only when the
//! program happened to spell that instance elsewhere.
//!
//! The matrix: four shape AXES (same names with other types, the same names
//! and types in another order, a field subset/superset, a record two declared
//! types could be) × six POSITIONS (a literal, a destructuring pattern, a fn
//! parameter/return, a generic instance, nesting inside list/option/record,
//! across a module boundary). Every cell is a line `axis/position: …` in some
//! program's expected output; `every_cell_is_written` refuses a missing one,
//! and the legs must print every program byte-identically: the interp here
//! (fast, every build), native and wasm in the release-shape job.

use std::path::{Path, PathBuf};
use std::process::Command;

const AXES: &[&str] = &["types", "order", "arity", "two"];
const POSITIONS: &[&str] = &["literal", "pattern", "fn", "generic", "nested", "module"];

struct Cell {
    name: &'static str,
    /// `m/mod.almd`, when the program has a module.
    module: Option<&'static str>,
    entry: &'static str,
    expected: &'static str,
}

const CELLS: &[Cell] = &[
    // The issue's program: the declared names with other field types.
    Cell {
        name: "types",
        module: None,
        entry: r#"type R = { v: String, n: Int }
type G[T] = { v: T, n: String }
fn sum(r: { v: Int, n: Int }) -> Int = r.v + r.n
fn mk(x: Int) -> { v: Int, n: Int } = { v: x, n: x * 2 }
fn id[T](x: T) -> T = x
effect fn main() -> Unit = {
  let x = 5
  let a: { v: Int, n: Int } = { v: x, n: 1 }
  println("types/literal: ${a} ${a.v}")
  let { v, n } = a
  println("types/pattern: ${v + n}")
  println("types/fn: ${sum(mk(3))} ${mk(4)}")
  let g = id({ v: 7, n: 8 })
  println("types/generic: ${g} ${g.v * g.n}")
  let xs: List[{ v: Int, n: Int }] = [{ v: 1, n: 2 }]
  let o = some({ v: 3, n: 4 })
  let w = { inner: { v: 5, n: 6 }, tag: "t" }
  println("types/nested: ${xs} ${o} ${w}")
  let r: R = { v: "s", n: 9 }
  println("types/control: ${r}")
}
"#,
        expected: r#"types/literal: { n: 1, v: 5 } 5
types/pattern: 6
types/fn: 9 { n: 8, v: 4 }
types/generic: { n: 8, v: 7 } 56
types/nested: [{ n: 2, v: 1 }] some({ n: 4, v: 3 }) { inner: { n: 6, v: 5 }, tag: "t" }
types/control: R { v: "s", n: 9 }"#,
    },
    // The declared names AND types in another order IS the declared type;
    // the same order swap with the types swapped too is not.
    Cell {
        name: "order",
        module: None,
        entry: r#"type R = { v: String, n: Int }
fn take(r: R) -> String = r.v
fn mk(s: String) -> { n: Int, v: String } = { n: 0, v: s }
fn id[T](x: T) -> T = x
effect fn main() -> Unit = {
  let a: { n: Int, v: String } = { n: 1, v: "x" }
  let b: { n: String, v: Int } = { n: "q", v: 3 }
  println("order/literal: ${a} ${b}")
  let { n, v } = a
  println("order/pattern: ${n} ${v}")
  println("order/fn: ${take(a)} ${mk("z")}")
  let g = id({ n: 2, v: "y" })
  println("order/generic: ${g} ${take(g)}")
  let xs = [{ n: 3, v: "l" }]
  let w = { inner: { n: 4, v: "r" }, tag: "t" }
  println("order/nested: ${xs} ${some({ n: 5, v: "o" })} ${w}")
}
"#,
        expected: r#"order/literal: R { v: "x", n: 1 } { n: "q", v: 3 }
order/pattern: 1 x
order/fn: x R { v: "z", n: 0 }
order/generic: R { v: "y", n: 2 } y
order/nested: [R { v: "l", n: 3 }] some(R { v: "o", n: 5 }) { inner: R { v: "r", n: 4 }, tag: "t" }"#,
    },
    // A field subset or superset of a declared record is never that record.
    Cell {
        name: "arity",
        module: None,
        entry: r#"type R = { v: String, n: Int }
fn sub(r: { v: String }) -> { v: String } = r
fn sup(x: Int) -> { v: String, n: Int, k: Int } = { v: "s", n: x, k: x + 1 }
fn id[T](x: T) -> T = x
effect fn main() -> Unit = {
  let a = { v: "a" }
  let b: { v: String, n: Int, k: Int } = { v: "b", n: 1, k: 2 }
  println("arity/literal: ${a} ${b}")
  let { v, n, k } = b
  println("arity/pattern: ${v} ${n + k}")
  println("arity/fn: ${sub(a)} ${sup(3)}")
  println("arity/generic: ${id({ v: "g" })} ${id(b)}")
  let xs = [{ v: "l" }]
  println("arity/nested: ${xs} ${some(b)} ${{ inner: { v: "i", n: 1, k: 2 }, tag: 0 }}")
  let r: R = { v: "r", n: 1 }
  println("arity/control: ${r}")
}
"#,
        expected: r#"arity/literal: { v: "a" } { k: 2, n: 1, v: "b" }
arity/pattern: b 3
arity/fn: { v: "a" } { k: 4, n: 3, v: "s" }
arity/generic: { v: "g" } { k: 2, n: 1, v: "b" }
arity/nested: [{ v: "l" }] some({ k: 2, n: 1, v: "b" }) { inner: { k: 2, n: 1, v: "i" }, tag: 0 }
arity/control: R { v: "r", n: 1 }"#,
    },
    // Two declared types could be the record: the field types pick; an exact
    // twin (`C` repeats `A`) loses to the first; a concrete type beats the
    // generic one declared before it, and the generic one takes what no
    // concrete type fits — when its parameter binds consistently.
    Cell {
        name: "two",
        module: None,
        entry: r#"type P[T] = { x: T, y: T }
type A = { x: Int, y: String }
type B = { x: String, y: Int }
type C = { x: Int, y: String }
fn take_b(b: B) -> Int = b.y
fn mk(i: Int) -> { y: String, x: Int } = { y: "m", x: i }
fn id[T](x: T) -> T = x
effect fn main() -> Unit = {
  let a = { x: 1, y: "p" }
  let b = { x: "q", y: 2 }
  let p = { x: 3, y: 4 }
  println("two/literal: ${a} ${b} ${p} ${{ x: true, y: false }}")
  let { x, y } = b
  println("two/pattern: ${x} ${y}")
  println("two/fn: ${take_b(b)} ${mk(7)}")
  println("two/generic: ${id(a)} ${id({ x: 5, y: 6 })}")
  println("two/nested: ${[a, mk(8)]} ${some(b)} ${{ inner: p, tag: 1 }}")
}
"#,
        expected: r#"two/literal: A { x: 1, y: "p" } B { x: "q", y: 2 } P { x: 3, y: 4 } P { x: true, y: false }
two/pattern: q 2
two/fn: 2 A { x: 7, y: "m" }
two/generic: A { x: 1, y: "p" } P { x: 5, y: 6 }
two/nested: [A { x: 1, y: "p" }, A { x: 8, y: "m" }] some(B { x: "q", y: 2 }) { inner: P { x: 3, y: 4 }, tag: 1 }"#,
    },
    // Every axis across a module boundary: the declared types live in `m`.
    Cell {
        name: "module",
        module: Some(r#"type R = { v: String, n: Int }
type A = { x: Int, y: String }
type B = { x: String, y: Int }
fn take(r: R) -> String = r.v
fn take_b(b: B) -> Int = b.y
fn mk() -> R = R { v: "m", n: 1 }
"#),
        entry: r#"import m
effect fn main() -> Unit = {
  let t: { v: Int, n: Int } = { v: 5, n: 1 }
  println("types/module: ${t} ${t.v + t.n}")
  let o: { n: Int, v: String } = { n: 2, v: "o" }
  println("order/module: ${o} ${m.take(o)} ${m.mk()}")
  let s = { v: "s" }
  let u = { v: "u", n: 1, k: 2 }
  println("arity/module: ${s} ${u}")
  let a = { x: 1, y: "p" }
  let b = { x: "q", y: 3 }
  println("two/module: ${a} ${b} ${m.take_b(b)}")
}
"#,
        expected: r#"types/module: { n: 1, v: 5 } 6
order/module: R { v: "o", n: 2 } o R { v: "m", n: 1 }
arity/module: { v: "s" } { k: 2, n: 1, v: "u" }
two/module: A { x: 1, y: "p" } B { x: "q", y: 3 } 3"#,
    },
];

#[test]
fn every_cell_is_written() {
    let lines: Vec<&str> = CELLS.iter().flat_map(|c| c.expected.lines()).collect();
    let missing: Vec<String> = AXES.iter()
        .flat_map(|a| POSITIONS.iter().map(move |p| format!("{a}/{p}:")))
        .filter(|tag| !lines.iter().any(|l| l.starts_with(tag.as_str())))
        .collect();
    assert!(missing.is_empty(), "matrix cells with no program line: {missing:?}");
}

fn project(cell: &Cell, leg: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join(format!("almide-anon-record-identity-{}-{leg}-{}", cell.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    if let Some(module) = cell.module {
        std::fs::create_dir_all(root.join("m")).expect("mkdir m");
        std::fs::write(root.join("m/mod.almd"), module).expect("write module");
    }
    std::fs::write(root.join("main.almd"), cell.entry).expect("write entry");
    root
}

fn run_interp(entry: &Path) -> almide_interp::RunOutcome {
    let source = std::fs::read_to_string(entry).expect("read entry");
    let ir = almide::wasm_leg::lower_to_ir(entry.to_str().unwrap(), &source).expect("front failed");
    almide_interp::Interpreter::new(&ir).run_main()
}

fn run(entry: &Path, target: &str) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", entry.to_str().unwrap(), "--target", target])
        .output()
        .expect("spawn almide run");
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[test]
fn the_interp_prints_every_cell() {
    let mut failures = Vec::new();
    for cell in CELLS {
        let root = project(cell, "interp");
        let out = run_interp(&root.join("main.almd"));
        if out.status != almide_interp::RunStatus::Ok || out.stdout.trim_end() != cell.expected {
            failures.push(format!("[{}] {:?}\n{}\n{}", cell.name, out.status, out.stdout, out.stderr));
        }
        let _ = std::fs::remove_dir_all(&root);
    }
    assert!(failures.is_empty(), "interp leg:\n{}", failures.join("\n---\n"));
}

#[cfg_attr(debug_assertions, ignore = "compiles every program on native and wasm (CI: release-shape job)")]
#[test]
fn native_and_wasm_print_every_cell() {
    let mut targets = vec!["rust"];
    if wasmtime_available() {
        targets.push("wasm");
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
    }
    let mut failures = Vec::new();
    for cell in CELLS {
        for target in &targets {
            let root = project(cell, target);
            let (ok, out, err) = run(&root.join("main.almd"), target);
            if !ok || out.trim_end() != cell.expected {
                failures.push(format!("[{} / {target}] ok={ok}\n{out}\n{err}", cell.name));
            }
            let _ = std::fs::remove_dir_all(&root);
        }
    }
    assert!(failures.is_empty(), "compiled legs:\n{}", failures.join("\n---\n"));
}
