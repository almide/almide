//! Golden snapshot for `almide ide outline @stdlib/<module>`.
//!
//! dojo (and any SYSTEM_PROMPT that embeds a stdlib snapshot) depends on
//! this output format being stable. A change here — whether adding a
//! function, renaming a parameter, or altering the rendered type format —
//! is a contract break that MUST be reviewed explicitly.
//!
//! Update with: `cargo insta review`.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run_outline(args: &[&str]) -> String {
    let output = Command::new(almide())
        .arg("ide")
        .arg("outline")
        .args(args)
        .output()
        .expect("failed to run almide");
    assert!(
        output.status.success(),
        "almide ide outline {:?} failed:\n{}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("non-utf8 output")
}

#[test]
fn stdlib_string_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/string"]));
}

#[test]
fn stdlib_list_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/list"]));
}

#[test]
fn stdlib_int_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/int"]));
}

#[test]
fn stdlib_option_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/option"]));
}

#[test]
fn stdlib_result_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/result"]));
}

#[test]
fn stdlib_map_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/map"]));
}

#[test]
fn stdlib_set_outline() {
    insta::assert_snapshot!(run_outline(&["@stdlib/set"]));
}

#[test]
fn stdlib_json_schema_shape() {
    let out = run_outline(&["@stdlib/option", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    assert_eq!(v["source"], "stdlib");
    assert_eq!(v["module"], "option");
    assert!(v["functions"].is_array());
    let f0 = &v["functions"][0];
    assert!(f0["name"].is_string());
    assert!(f0["params"].is_array());
    assert!(f0["ret"].is_string());
    assert!(f0["effect"].is_boolean());
}

#[test]
fn unknown_stdlib_module_errors_with_hint() {
    let output = Command::new(almide())
        .args(["ide", "outline", "@stdlib/nope"])
        .output()
        .expect("run");
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("not a stdlib module"), "stderr: {}", err);
    assert!(err.contains("hint:"), "stderr: {}", err);
}
