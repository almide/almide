//! Fan arm separators: `,`/newline are the sibling separators; `;` between
//! arms is a targeted parse error naming the rule (sequencing vs parallel
//! siblings) and the mechanical fix. This file pins BOTH sides: the comma
//! forms must be accepted (fmt canonicalizes them to newline-per-arm, so an
//! fmt-gated spec file cannot carry them) and the `;` diagnostic must name
//! the rule. Canonical-form runtime behavior lives in
//! `spec/lang/fan_arm_comma_test.almd`.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run_native(dir: &std::path::Path, source: &str) -> (String, bool) {
    let file = dir.join("sep_run.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("run")
        .arg(&file)
        .output()
        .expect("run almide");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text, out.status.success())
}

fn check(dir: &std::path::Path, source: &str) -> (String, bool) {
    let file = dir.join("sep.almd");
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
    (text, out.status.success())
}

const HELPERS: &str =
    "effect fn ea() -> Result[Int, String] = ok(1)\n\neffect fn eb() -> Result[Int, String] = ok(2)\n\n";

#[test]
fn semicolon_between_fan_block_arms_is_the_targeted_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(
        dir.path(),
        &format!("{HELPERS}effect fn main() -> Unit = {{\n  let (x, y) = fan {{ ea(); eb() }}\n  println(int.to_string(x + y))\n}}\n"),
    );
    assert!(!ok, "`;` between fan arms must fail, got:\n{out}");
    assert!(
        out.contains("separated by `,` or a newline, not `;`"),
        "must name the separator rule, got:\n{out}"
    );
    assert!(
        out.contains("parallel siblings"),
        "must explain WHY (siblings vs sequencing), got:\n{out}"
    );
}

#[test]
fn semicolon_between_head_arms_is_the_same_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(
        dir.path(),
        &format!("{HELPERS}effect fn main() -> Unit = {{\n  let n = fan.any {{ ea(); eb() }} ?? -1\n  println(int.to_string(n))\n}}\n"),
    );
    assert!(!ok, "`;` between fan.any arms must fail, got:\n{out}");
    assert!(out.contains("fan.any arms are separated by"), "got:\n{out}");
}

#[test]
fn comma_separated_arms_parse_and_run() {
    // One-liners with `,`, multiline with a trailing comma — the enumeration
    // spellings every other sibling list in the language already accepts.
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = run_native(
        dir.path(),
        &format!(
            "{HELPERS}effect fn main() -> Unit = {{\n  let (x, y) = fan {{ ea(), eb() }}\n  let n = fan.any {{ ea(), eb() }} ?? -1\n  let (ra, rb) = fan.settle {{\n    ea(),\n    eb(),\n  }}\n  println(int.to_string(x + y + n + (ra ?? -1) + (rb ?? -1)))\n}}\n"
        ),
    );
    assert!(ok, "comma forms must parse and run, got:\n{out}");
    assert!(out.contains('7'), "3 + 1 + 3 = 7, got:\n{out}");
}

#[test]
fn block_arm_internal_semicolons_stay_legal() {
    // `;` INSIDE a block arm is sequencing and must keep working — only the
    // separator BETWEEN arms is the error.
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = run_native(
        dir.path(),
        &format!(
            "{HELPERS}effect fn main() -> Unit = {{\n  let n = fan.any {{ {{ let t = ea()!; ok(t + 10) }}, eb() }} ?? -1\n  println(int.to_string(n))\n}}\n"
        ),
    );
    assert!(ok, "block-arm internal `;` must stay legal, got:\n{out}");
    assert!(out.contains("11"), "got:\n{out}");
}

#[test]
fn the_hint_shows_where_semicolons_stay_legal() {
    // The error steers to the block-arm escape hatch — `;` really is legal
    // INSIDE an arm, and the hint must say so or writers will flatten their
    // sequential setup into separate arms.
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, _) = check(
        dir.path(),
        &format!("{HELPERS}effect fn main() -> Unit = {{\n  let n = fan.any {{ ea(); eb() }} ?? -1\n  println(int.to_string(n))\n}}\n"),
    );
    assert!(
        out.contains("{ let x = f(); g(x) }"),
        "the hint must show the block-arm form, got:\n{out}"
    );
}
