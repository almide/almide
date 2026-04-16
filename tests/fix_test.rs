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
