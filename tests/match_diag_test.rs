//! The match expression's negative half, cited by ALS-E18 (the accepted
//! pattern forms are pinned by `spec/wasm_cross/match_forms.almd`, C-247):
//! a non-exhaustive match is check-time E010, and the diagnostic must NAME
//! the missing case — "add more arms" without the case name would leave the
//! reader enumerating the type by hand.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn non_exhaustive_match_is_e010_naming_the_missing_case() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("m.almd");
    std::fs::write(
        &file,
        "effect fn main() -> Unit = {\n  let o: Int? = some(5)\n  let v = match o {\n    some(x) => x,\n  }\n  println(int.to_string(v))\n}\n",
    )
    .expect("write fixture");
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
    assert!(text.contains("E010"), "non-exhaustive match must be E010, got:\n{text}");
    assert!(
        text.contains("missing none"),
        "E010 must NAME the missing case, got:\n{text}"
    );
}
