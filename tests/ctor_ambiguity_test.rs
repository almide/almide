//! The intra-module constructor-name collision (E019 extension, almide#1426,
//! edit-locality hunt V3): two variant types in the SAME module declaring the
//! same case name used to register both candidates silently — bare resolution
//! became registration-order-dependent, and the newer case was unreachable.
//! Now a hard error at registration. The cross-module qualified polarity is
//! pinned by spec/integration/modules/qualified_ctor_test.almd.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("e019.almd");
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
fn same_module_duplicate_ctor_name_is_e019() {
    let (ok, text) = check(
        r#"type Light = | Red | Green
type Alert = | Red | Amber

effect fn main() -> Unit = {
  println("x")
}
"#,
    );
    assert!(!ok, "intra-module duplicate ctor must be a hard error, got success:\n{text}");
    assert!(text.contains("E019"), "duplicate ctor must be E019, got:\n{text}");
    assert!(
        text.contains("'Light' and 'Alert'"),
        "E019 must name both declaring types, got:\n{text}"
    );
}

#[test]
fn distinct_ctor_names_in_one_module_stay_accepted() {
    let (ok, text) = check(
        r#"type Light = | Red | Green
type Alert = | Amber | Clear

effect fn main() -> Unit = {
  println("x")
}
"#,
    );
    assert!(ok, "distinct case names must stay accepted, got:\n{text}");
    assert!(!text.contains("E019"), "E019 must not overfire, got:\n{text}");
}
