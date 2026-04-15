//! `try:` snippet regression tests.
//!
//! Diagnostics that emit copy-pasteable fix snippets are part of Almide's
//! LLM-retry contract. These tests pin each snippet so a refactor can't
//! silently strip them.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(file: &str) -> (bool, String) {
    let out = Command::new(almide())
        .args(["check", file])
        .output()
        .expect("run almide check");
    // check prints to stderr
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

fn write_tmp(name: &str, body: &str) -> String {
    let path = format!("{}/{}", std::env::temp_dir().display(), name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn e002_stdlib_alias_emits_try_snippet() {
    let p = write_tmp("try_e002_alias.almd", r#"
fn main() -> String = {
    string.to_uppercase("hi")
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E002]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try: section\n{}", out);
    assert!(out.contains("string.to_upper(...)"), "try: body missing\n{}", out);
}

#[test]
fn e002_fuzzy_match_emits_try_snippet() {
    let p = write_tmp("try_e002_fuzzy.almd", r#"
fn main() -> List[Int] = {
    let xs = [1, 2, 3]
    list.maps(xs, (x) => x + 1)
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E002]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    assert!(out.contains("list.map(...)"), "try: body missing\n{}", out);
}

#[test]
fn e002_method_call_emits_try_snippet() {
    let p = write_tmp("try_e002_method.almd", r#"
fn main() -> String = {
    let s = "hi"
    s.to_uppercase()
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E002]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    assert!(out.contains("string.to_upper(x)"), "try: body missing\n{}", out);
}

#[test]
fn e003_undefined_module_emits_import_snippet() {
    let p = write_tmp("try_e003_import.almd", r#"
fn main() -> String = {
    json.to_string([1, 2, 3])
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E003]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    assert!(out.contains("import json"), "try: body missing\n{}", out);
}

#[test]
fn e009_let_reassign_emits_var_snippet() {
    let p = write_tmp("try_e009.almd", r#"
effect fn main() -> Unit = {
    let counter = 0
    counter = counter + 1
    println(int.to_string(counter))
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E009]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    assert!(out.contains("var counter ="), "try: body missing\n{}", out);
}

#[test]
fn e002_freetext_alias_suppresses_try_snippet() {
    // `string.all` aliases to "string.chars + list.all" — a free-text
    // composition, not a bare fn name. try: must be suppressed rather
    // than splice the free-text blob into `X(...)`.
    let p = write_tmp("try_e002_freetext.almd", r#"
fn main() -> Bool = {
    string.all("hello", (c) => c == "l")
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E002]"), "out:\n{}", out);
    assert!(out.contains("Did you mean `string.chars + list.all`"), "hint missing\n{}", out);
    assert!(!out.contains("try:"), "try: must be suppressed for free-text alias\n{}", out);
}
