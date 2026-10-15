//! `option.is_some` / `option.is_none` only read their argument, so they
//! take it by reference (`@borrow_ref(o)`, `&Option<T>` in the runtime).
//! Taken by value, every read-only check copied a heap payload. In
//! `list.any(xs, (o) => option.is_none(o))` (the shape option.collect's
//! own all-some check has) each element was cloned to be inspected: a
//! full copy of every String in the list.
use std::process::Command;

const SRC: &str = r#"fn any_missing(xs: List[Option[String]]) -> Bool = list.any(xs, (o) => option.is_none(o))

fn keeps(o: Option[String]) -> String = if option.is_some(o) then o ?? "" else "-"

effect fn main() -> Unit = {
  let xs = [some("a"), none, some("bc")]
  println("${any_missing(xs)} ${any_missing([some("x")])}")
  println(keeps(some("kept")) + keeps(none))
}
"#;

fn emitted() -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("inspect.almd");
    std::fs::write(&file, SRC).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &rust[start..];
    &rest[..rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len())]
}

#[test]
fn a_read_only_option_check_copies_no_element() {
    let rust = emitted();
    let any = fn_body(&rust, "any_missing");
    assert!(!any.contains(".cloned()") && !any.contains(".clone()"), "the all-some check must not copy the elements:\n{any}");
    let keeps = fn_body(&rust, "keeps");
    assert!(keeps.contains("almide_rt_option_is_some(&"), "is_some borrows its argument:\n{keeps}");
    assert!(!keeps.contains("o.clone()"), "inspecting `o` before using it must not copy it:\n{keeps}");
}

#[test]
fn the_checks_answer_the_same_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("inspect.almd");
    std::fs::write(&file, SRC).expect("source");
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "true false\nkept-\n", "{target:?}");
    }
}
