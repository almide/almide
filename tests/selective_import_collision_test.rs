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
