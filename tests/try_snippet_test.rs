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
fn let_in_across_newline_triggers_letin_diagnostic() {
    // dojo data: 70b writes OCaml-style `let x = expr\n  in <body>` and
    // pre-fix the parser fell through to a generic "Expected expression
    // (got In 'in')" parse error. Ensure the let-in detection fires across
    // an intervening newline AND emits the chain-by-newline try: snippet.
    let p = write_tmp("try_letin_newline.almd", r#"
fn sum_digits(n: Int) -> Int =
  let abs_n = int.abs(n)
  in if abs_n == 0 then 0
     else (abs_n % 10) + sum_digits(abs_n / 10)
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("`let ... in <expr>` is OCaml/Haskell syntax"),
        "let-in detection didn't fire across newline:\n{}", out);
    assert!(out.contains("let x = 1\n      let y = 2"),
        "missing chain-by-newline try: snippet:\n{}", out);
    // After parser recovery, the partial Stmt::Let should survive so the
    // E001 fn-body Unit-leak snippet can name the actual binding.
    assert!(out.contains("let abs_n = ...") || out.contains("let abs_n ="),
        "specialized E001 snippet didn't pick up `abs_n`:\n{}", out);
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
fn e001_fn_body_trailing_let_emits_specialized_snippet() {
    // Top dojo E001 pattern, specialized: when the fn body's last stmt is
    // `let <name> = ...`, the try: snippet should name the binding so the
    // model has copy-pasteable code, not a `<placeholder>` template.
    let p = write_tmp("try_e001_fn_trailing_let.almd", r#"
fn sum_digits(n: Int) -> Int = {
    let abs_n = int.abs(n)
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E001]"), "out:\n{}", out);
    assert!(out.contains("type mismatch in fn 'sum_digits'"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    // Key payoff: the binding name appears in the snippet, as a standalone
    // trailing expression the model can copy.
    assert!(out.contains("let abs_n = ..."), "not specialized:\n{}", out);
    assert!(out.contains("abs_n                         // <-- add this line"),
        "missing concrete tail:\n{}", out);
    assert!(out.contains("returns Int"), "type missing:\n{}", out);
}

#[test]
fn e001_fn_body_non_let_uses_generic_fallback() {
    // When the body tail is NOT a `let` (e.g. an effectful call), we fall
    // back to the generic template since there's no single binding name
    // to splice in.
    let p = write_tmp("try_e001_fn_generic.almd", r#"
fn returns_int() -> Int = {
    println("side effect")
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E001]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    // Generic template (no specific binding name available)
    assert!(out.contains("fn body ends with a statement"), "generic body missing:\n{}", out);
    assert!(out.contains("evaluates to Int"), "type missing:\n{}", out);
}

#[test]
fn e004_arg_count_emits_sig_placeholder() {
    // dojo data: `string.join(xs)` forgets the separator. The try: snippet
    // should show the full signature with named placeholders.
    let p = write_tmp("try_e004.almd", r#"
fn greet(xs: List[String]) -> String = {
    string.join(xs)
}
"#);
    let (ok, out) = check(&p);
    assert!(!ok);
    assert!(out.contains("error[E004]"), "out:\n{}", out);
    assert!(out.contains("try:"), "missing try:\n{}", out);
    // placeholder must name both params with types
    assert!(out.contains("<list: List[String]>"), "first placeholder missing\n{}", out);
    assert!(out.contains("<sep: String>"), "second placeholder missing\n{}", out);
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
