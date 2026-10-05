//! The local-fn / selective-import collision (E050): a file that declares
//! `fn parse` while also importing `json.{parse}` used to TYPE-CHECK the bare
//! call against the local fn and LOWER it to `json.parse` — one call, two
//! resolutions (almide#1425, edit-locality hunt V2). The collision is now a
//! hard error at import-table build time; the accepted forms stay pinned by
//! spec/lang/selective_import_test.almd.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("e050.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn local_fn_colliding_with_selective_import_is_e050() {
    let (ok, text) = check(
        r#"import json.{parse, stringify}

fn parse(s: String) -> Int = string.len(s)

effect fn main() -> Unit = {
  let n = parse("hi")
  println("${n}")
}
"#,
    );
    assert!(!ok, "collision must be a hard error, got success:\n{text}");
    assert!(text.contains("E050"), "collision must be E050, got:\n{text}");
    assert!(
        text.contains("collides with selective import"),
        "E050 must name the collision, got:\n{text}"
    );
    assert!(
        text.contains("json.parse"),
        "E050 hint must spell the qualified escape, got:\n{text}"
    );
}

#[test]
fn non_colliding_local_fn_beside_selective_import_stays_accepted() {
    let (ok, text) = check(
        r#"import json.{parse, stringify}

fn parse_len(s: String) -> Int = string.len(s)

effect fn main() -> Unit = {
  let v = parse("[1]") ?? value.null()
  println("${parse_len(stringify(v))}")
}
"#,
    );
    assert!(ok, "no collision — must stay accepted, got:\n{text}");
    assert!(!text.contains("E050"), "E050 must not overfire, got:\n{text}");
}

/// A top-level `let` is the same split since a selective import binds lets
/// (#3388): the checker read the imported let, the lowering the file's own.
#[test]
fn local_let_colliding_with_selective_import_is_e050() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"pk\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(root.join("src/k.almd"), "let limit = 3\n").unwrap();
    std::fs::write(root.join("src/main.almd"), "import self.k.{limit}\n\nlet limit = 4\n\nfn f() -> Int = limit\n").unwrap();
    let out = Command::new(almide()).current_dir(root).args(["check", "src/main.almd"]).output().expect("run almide check");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "collision must be a hard error, got success:\n{text}");
    assert!(text.contains("E050") && text.contains("local let 'limit'"), "collision must be E050 naming the let, got:\n{text}");
    assert!(text.contains("k.limit"), "E050 hint must spell the qualified escape, got:\n{text}");
}
