//! A pattern binder of a match on a chain lambda's own element is fresh per
//! call, like the element itself.
//!
//! The clone pass treats a lambda body as a loop body: only what each call
//! binds afresh (its params and its top-level `let`s) may be moved, and
//! everything else is cloned at its last use. A `match o { some(v) => v }`
//! on the element `o` binds `v` afresh too, but `v` was not counted, so
//! `list.map(xs, (o) => match o { some(v) => v, … })` rendered
//! `Some(v) => v.clone()` over an `into_iter()` that already owned `o`: one
//! full String copy per element.
//!
//! The rule stays exact. A binder is fresh only when its scrutinee is. A
//! match on a CAPTURED value binds into a value the closure keeps, and
//! moving out of it is E0507, so that binder is still cloned.
use std::process::Command;

const SRC: &str = r#"fn unwrap_all(xs: List[Option[String]]) -> List[String] =
  list.map(xs, (o) => match o { some(v) => v, none => "" })

fn with_outer(xs: List[String], outer: Option[String]) -> List[String] =
  list.map(xs, (s) => match outer { some(v) => v + s, none => s })

effect fn main() -> Unit = {
  println(list.join(unwrap_all([some("a"), none, some("bc")]), ","))
  println(list.join(with_outer(["x", "y"], some("p")), ","))
}
"#;

fn write() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("binder.almd");
    std::fs::write(&file, SRC).expect("source");
    (dir, file)
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len())]
}

#[test]
fn a_binder_of_the_element_moves_and_a_binder_of_a_capture_is_cloned() {
    let (_dir, file) = write();
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout);
    let unwrap = fn_body(&rust, "unwrap_all");
    assert!(unwrap.contains("Some(v) => v,"), "the element's payload moves out:\n{unwrap}");
    assert!(!unwrap.contains("v.clone()"), "no copy of an owned element's payload:\n{unwrap}");
    let outer = fn_body(&rust, "with_outer");
    // The captured value is copied per call, never moved out of the closure:
    // either its payload (`v.clone()`) or the whole scrutinee (`outer.clone()`).
    assert!(outer.contains("v.clone()") || outer.contains("outer.clone()"), "a captured value is copied, not moved:\n{outer}");
}

#[test]
fn both_legs_print_the_same() {
    let (_dir, file) = write();
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a,,bc\npx,py\n", "{target:?}");
    }
}
