//! ADR-0013: generated native Rust spells the Almide `String`
//! type as `AlmideString`, declared once per render as the shared buffer
//! `AlmideStr` (docs/roadmap/active/adr-0013-string-sharing.md).
//! Both native renderers are pinned: the v0 walker (`--target rust`, what
//! `almide test` and every program v1 walls on use) and the v1 render (what
//! `almide build` / `run` ship when it does not wall).
use std::process::Command;

const SOURCE: &str = r#"fn greet(name: String, n: Int) -> String = "hi " + name + int.to_string(n)

fn first_word(s: String) -> Result[String, String] =
  match string.split(s, " ") |> list.get(0) {
    some(w) => ok(w),
    none => err("empty"),
  }

effect fn main() -> Unit = {
  let g = greet("almide", 3)
  println(g)
  println(first_word(g) ?? "none")
}
"#;

/// The `fn` signature lines of a render (the line that opens each item).
fn signatures(rust: &str) -> Vec<&str> {
    rust.lines().map(str::trim_start).filter(|l| l.starts_with("fn ") || l.starts_with("pub fn ")).collect()
}

/// `String` as its own identifier (not the tail of `AlmideString`).
fn has_bare_string(sig: &str) -> bool {
    sig.match_indices("String").any(|(i, _)| {
        let before = sig[..i].chars().next_back();
        let after = sig[i + "String".len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn assert_spelled(rust: &str, user_fns: &[&str], which: &str) {
    assert_eq!(rust.matches("type AlmideString = AlmideStr;").count(), 1, "{which}: the alias names the shared buffer exactly once:\n{rust}");
    assert_eq!(rust.matches("type AlmideString").count(), 1, "{which}: one alias declaration:\n{rust}");
    for f in user_fns {
        let sig = signatures(rust).into_iter().find(|l| l.contains(&format!("{f}("))).unwrap_or_else(|| panic!("{which}: no fn {f}:\n{rust}"));
        assert!(sig.contains("AlmideString"), "{which}: `{f}` must spell the Almide String as AlmideString: {sig}");
        assert!(!has_bare_string(sig), "{which}: `{f}` spells a bare String: {sig}");
    }
}

#[test]
fn the_walker_spells_the_almide_string_through_the_alias() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("alias.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit rust");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_spelled(&String::from_utf8_lossy(&out.stdout), &["greet", "first_word"], "walker");
}

#[test]
fn the_v1_render_spells_the_almide_string_through_the_alias() {
    let src = r#"fn greet(name: String, n: Int) -> String = "hi " + name + int.to_string(n)

effect fn main() -> Unit = {
  println(greet("almide", 3))
}
"#;
    let rust = almide_mir::pipeline::try_render_rust_source(src).unwrap_or_else(|e| panic!("the v1 native render walled: {e:?}"));
    assert_spelled(&rust, &["greet"], "v1");
}

#[test]
fn the_aliased_program_runs_unchanged() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("alias.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi almide3\nhi\n");
}
