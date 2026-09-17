//! A non-`mut` record param handed bare to a callee's OWNED slot and READ
//! AGAIN afterwards stays borrowed (#2278): the site is cloned either way, so
//! that one site clones — `stored(t.clone(), 1)` — and the param's other
//! reads keep the borrow, instead of the whole param turning owned
//! (`t: Table`) and every caller that still needs its value paying the
//! clone. When the owned slot is the param's LAST use the param is owned:
//! the site moves, and only a caller that still needs its value clones (the
//! allocation ledger pins that an unconditional site clone costs more). A
//! list-combinator callback the fusion pass inlines is a scope, not a
//! closure: a param it only reads stays borrowed and the chain step renders
//! without `move` or a capture bind. A stdlib slot that consumes
//! (`list.map`'s list) still owns: it lowers into a chain that takes the
//! value itself.
//!
//! The rule is stated in docs/specs/codegen.md ("Parameter passing on the
//! native target"); this pins the emitted shapes and that both legs agree.
use std::process::Command;

const PROGRAM: &str = r#"import json
type Table = { names: List[String], sizes: List[Int] }
type Pigment: Codec = { r: Int }
type Holder = { t: Table, n: Int }

fn plain(t: Table) -> Int = list.len(t.names) + list.len(t.sizes)

fn captured(t: Table) -> Int = list.fold(list.map(t.sizes, (x) => x + list.len(t.names)), 0, (a, b) => a + b)

fn stored(t: Table, n: Int) -> Holder = Holder { t: t, n: n }

fn stored_then_read(t: Table) -> Int = { let h = stored(t, 1); h.n + plain(t) }

fn stored_last(t: Table) -> Int = { let n = plain(t); stored(t, n).n }

fn mapped(ns: List[Int]) -> List[Int] = list.map(ns, (n) => n + 1)

fn heads(xs: List[String]) -> List[Int] = list.map(xs, (x) => int.parse(x)!) ?? [0]

fn shown(p: Pigment) -> String = json.stringify(Pigment.encode(p)) + int.to_string(p.r)

fn main() -> Unit = {
  let t = Table { names: ["a", "b"], sizes: [1] }
  println(int.to_string(plain(t) + captured(t) + stored_then_read(t) + stored_last(t)))
  println(int.to_string(list.len(mapped([1, 2]))))
  println(int.to_string(list.len(t.names)))
  println(int.to_string(list.len(heads(["1", "2"]))) + shown(Pigment { r: 4 }))
}
"#;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_sig_and_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[start..].split("\n}").next().unwrap()
}

#[test]
fn a_param_kept_by_a_callee_stays_borrowed_and_clones_at_that_site() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();

    let s = fn_sig_and_body(&rust, "stored_then_read");
    assert!(s.starts_with("pub fn stored_then_read(t: &Table)"), "the param stays borrowed:\n{s}");
    assert!(s.contains("stored(t.clone(), 1i64)"), "the kept site clones:\n{s}");
    assert!(s.contains("plain(t)"), "the other read keeps the borrow:\n{s}");
    // The owned slot is the param's last use: owned, and the site moves.
    let s = fn_sig_and_body(&rust, "stored_last");
    assert!(s.starts_with("pub fn stored_last(t: Table)"), "{s}");
    assert!(s.contains("stored(t, n)"), "{s}");
    // The callee that keeps the value is owned by construction (a record field).
    assert!(fn_sig_and_body(&rust, "stored").starts_with("pub fn stored(t: Table, n: i64)"));
    // A read inside an inlined chain callback keeps the param borrowed, and
    // the step is a plain borrowing closure; a consuming stdlib slot owns.
    let s = fn_sig_and_body(&rust, "captured");
    assert!(s.starts_with("pub fn captured(t: &Table)"), "{s}");
    assert!(!s.contains("move |") && !s.contains("__cap_"), "the chain step borrows its capture:\n{s}");
    assert!(fn_sig_and_body(&rust, "mapped").starts_with("pub fn mapped(ns: Vec<i64>)"), "{}", fn_sig_and_body(&rust, "mapped"));
    // A monomorphised stdlib instance's slot (`list.map` with a fallible
    // callback) at the param's last use: owned. A derived codec fn's slot
    // followed by another read (`p.r`): borrowed, the site clones.
    assert!(fn_sig_and_body(&rust, "heads").starts_with("pub fn heads(xs: Vec<String>)"), "{}", fn_sig_and_body(&rust, "heads"));
    let s = fn_sig_and_body(&rust, "shown");
    assert!(s.starts_with("pub fn shown(p: &Pigment)") && s.contains("Pigment_encode(p.clone())"), "{s}");
    // Callers pass a borrow to the borrowed fns: no clone at the call site.
    let m = fn_sig_and_body(&rust, "__almide_main");
    assert!(m.contains("stored_then_read(&t)") && m.contains("stored_last(t.clone())"), "{m}");

    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "13\n2\n2\n2{\"r\":4}4", "{target}");
    }
}
