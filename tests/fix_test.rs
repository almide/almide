//! `almide fix` — MVP coverage. Auto-import + manual-fix reporting.
//!
//! The feature will grow as more `try:` snippets become mechanically
//! applicable; these tests guard the current contract so Phase 3-1
//! iterations don't silently regress the import-fix path.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write_tmp(name: &str, body: &str) -> String {
    let path = format!("{}/{}", std::env::temp_dir().display(), name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn fix_adds_missing_json_import() {
    let path = write_tmp("fix_adds_import.almd", r#"
effect fn main() -> Unit = {
    let v = json.stringify(json.from_int(42))
    println(v)
}
"#);
    let out = Command::new(almide()).args(["fix", &path]).output().unwrap();
    assert!(out.status.success(), "stderr:\n{}", String::from_utf8_lossy(&out.stderr));
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("import json"), "import not added:\n{}", after);
}

#[test]
fn fix_dry_run_does_not_write() {
    let src = r#"
effect fn main() -> Unit = {
    let v = json.stringify(json.from_int(1))
    println(v)
}
"#;
    let path = write_tmp("fix_dry_run.almd", src);
    let out = Command::new(almide()).args(["fix", &path, "--dry-run"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("would apply"), "stdout:\n{}", stdout);
    assert!(stdout.contains("Added `import json`"), "stdout:\n{}", stdout);
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, src, "file was modified on --dry-run");
}

#[test]
fn fix_reports_non_auto_fixable_diagnostics() {
    // fn body ends with a `let` binding (E001 Unit-leak) — not auto-applicable
    // by this MVP, but the try: snippet should be flagged as needing manual work.
    let path = write_tmp("fix_reports.almd", r#"
fn clamp(x: Int) -> Int = {
    let abs_x = int.abs(x)
}
"#);
    let out = Command::new(almide()).args(["fix", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("manual application"), "manual notice missing:\n{}", stderr);
    assert!(stderr.contains("[E001]"), "code missing:\n{}", stderr);
}

#[test]
fn fix_no_op_on_clean_file() {
    let path = write_tmp("fix_noop.almd", r#"
fn add(a: Int, b: Int) -> Int = a + b
"#);
    let out = Command::new(almide()).args(["fix", &path]).output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // no auto-fixes, no manual pointers
    assert!(!stderr.contains("Added `import"));
    assert!(!stderr.contains("manual application"));
}

#[test]
fn fix_rewrites_let_in_to_newline_chain() {
    // dojo 70b sum-digits pattern: OCaml-style `let x = expr\n  in <body>`.
    // After `almide fix`, the `in` keyword is gone and the body parses
    // as a newline-chained continuation.
    let src = r#"
fn sum_digits(n: Int) -> Int =
  let abs_n = int.abs(n)
  in if abs_n == 0 then 0
     else (abs_n % 10) + sum_digits(abs_n / 10)

effect fn main() -> Unit = {
  println(int.to_string(sum_digits(12345)))
}
"#;
    let path = write_tmp("fix_letin.almd", src);
    let out = Command::new(almide()).args(["fix", &path]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Removed") && stderr.contains("in` keyword"),
        "stderr:\n{}", stderr);

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(!after.contains(" in if "), "`in` not removed:\n{}", after);
    assert!(after.contains("let abs_n = int.abs(n)"), "body lost:\n{}", after);
    assert!(after.contains("else (abs_n % 10) + sum_digits"), "tail lost:\n{}", after);

    // File must now type-check and run.
    let check = Command::new(almide()).args(["check", &path]).output().unwrap();
    assert!(check.status.success(),
        "check failed after fix:\n{}", String::from_utf8_lossy(&check.stderr));
    let run = Command::new(almide()).args(["run", &path]).output().unwrap();
    assert!(run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("15"), "expected sum_digits(12345)=15, got:\n{}", stdout);
}

#[test]
fn fix_does_not_touch_into_keyword_lookalike() {
    // Sanity: the `in` word-boundary check must not clip `into`, `in_foo`,
    // etc. This file has no let-in error but does have `into` as part of
    // an identifier; the fix should leave it untouched.
    let src = r#"
fn translate_into(n: Int) -> Int = n + 1

effect fn main() -> Unit = {
  let into = translate_into(5)
  println(int.to_string(into))
}
"#;
    let path = write_tmp("fix_into.almd", src);
    let out = Command::new(almide()).args(["fix", &path]).output().unwrap();
    assert!(out.status.success());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("translate_into"), "`into` was clipped:\n{}", after);
}
