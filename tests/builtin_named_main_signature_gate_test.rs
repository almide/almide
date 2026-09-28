//! #2880 gate: what `check` accepts, native builds.
//!
//! An effect fn whose body ends in a statement (its value is the builtin
//! `Unit`) was accepted whatever Ok type it declared ("while loops, guard
//! patterns return via control flow"). Native then emitted `Ok(())` for a
//! `Result<T, String>` and failed rustc, reported as a compiler bug. Since
//! #2866 the same hole took in the ordinary-looking
//! `effect fn main() -> Unit` of an entry program that declares `type Unit`,
//! because a file's own declaration answers its bare spelling in every
//! position, signatures included (module-system §4.5).
//!
//! Cells are enumerated from the resolver's `BUILTIN_TYPE_HEADS`, never from a
//! hand list. For each cell, `check` passing must mean the native build runs,
//! and the cells whose program is ill-typed under §4.5 must be refused by
//! `check` with a diagnostic that says which `Unit` is which.

use std::path::Path;
use std::process::{Command, Output};

use almide_frontend::canonicalize::resolve::{BUILTIN_TYPE_HEADS, TypeSpelling};

/// `ALMIDE_BIN` runs the cells against another build (A/B); the default is
/// this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide()).current_dir(dir).args(args).output().expect("run almide")
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Every bare builtin head a program can declare (`!` / `?` are markers).
fn declarable_bare_heads() -> Vec<&'static str> {
    let mut names: Vec<&str> = BUILTIN_TYPE_HEADS
        .iter()
        .filter(|h| h.arity.sample() == TypeSpelling::Bare)
        .map(|h| h.name)
        .filter(|n| n.chars().all(|c| c.is_ascii_alphanumeric()) && n.starts_with(|c: char| c.is_ascii_uppercase()))
        .collect();
    names.dedup();
    assert!(names.len() >= 18 && names.contains(&"Unit"), "the builtin table looks truncated: {names:?}");
    names
}

/// One program, checked; when `check` passes, run natively and require the
/// run to succeed with `want` on stdout. Returns whether `check` passed.
fn check_then_native(program: &str, want: &str, failures: &mut Vec<String>, label: &str) -> (bool, String) {
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::write(root.path().join("t.almd"), program).expect("write");
    let checked = run(root.path(), &["check", "t.almd"]);
    if !checked.status.success() {
        return (false, text(&checked));
    }
    let native = run(root.path(), &["run", "t.almd"]);
    if !native.status.success() || String::from_utf8_lossy(&native.stdout) != want {
        failures.push(format!("{label}: `check` passed, native did not run it:\n{}", text(&native).lines().filter(|l| l.starts_with("error")).take(3).collect::<Vec<_>>().join("\n")));
    }
    (true, String::new())
}

/// The entry program declares a record under each builtin's name and keeps
/// the usual `effect fn main() -> Unit`. `Unit` there is the builtin unless
/// the cell declares `Unit`; then it is the record and the body, which yields
/// the builtin, is refused.
#[test]
fn an_entry_record_named_like_a_builtin_checks_only_what_native_builds() {
    let mut failures = Vec::new();
    for name in declarable_bare_heads() {
        let (field, ty, value) = if name == "Bool" { ("n", "Int", "7") } else { ("ok", "Bool", "true") };
        let decl = format!("type {name} = {{ {field}: {ty} }}\n\n");
        let body = format!("  let f = {name} {{ {field}: {value} }}\n  println(\"${{f}}\")\n");
        let want = format!("{name} {{ {field}: {value} }}\n");
        let signature_is_the_record = name == "Unit";
        for main in ["effect fn main() -> Unit", "fn main() -> Unit"] {
            let program = format!("{decl}{main} = {{\n{body}}}\n");
            let label = format!("`type {name}` with `{main}`");
            let (checked, diag) = check_then_native(&program, &want, &mut failures, &label);
            if checked == signature_is_the_record {
                failures.push(format!("{label}: `check` {} it\n{diag}", if checked { "accepted" } else { "refused" }));
            }
            if !checked && !diag.contains("the declared `type Unit`") && !diag.contains("module-system §4.5") {
                failures.push(format!("{label}: the refusal does not say which `Unit` is which:\n{diag}"));
            }
        }
        // A main that returns the record it declares is well typed (an effect
        // main may declare any Ok type) and runs.
        if signature_is_the_record {
            let program = format!("{decl}effect fn main() -> {name} = {{\n{body}  f\n}}\n");
            let (checked, diag) = check_then_native(&program, &want, &mut failures, "`type Unit` returned by main");
            if !checked {
                failures.push(format!("`effect fn main() -> Unit` returning the record is refused:\n{diag}"));
            }
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

/// The hole itself, with no declaration involved: an effect fn declared
/// `-> T` whose body ends in a statement. For every builtin `T`, `check`
/// passing must mean native runs it. `Unit` is the one cell that must pass,
/// so a `check` that refuses everything cannot satisfy the gate.
#[test]
fn an_effect_fn_whose_body_is_a_statement_checks_only_what_native_builds() {
    let mut failures = Vec::new();
    for name in declarable_bare_heads() {
        let program = format!(
            "effect fn f() -> {name} = {{\n  println(\"x\")\n}}\n\neffect fn main() -> Unit = {{\n  let _ = f()\n  println(\"done\")\n}}\n"
        );
        let label = format!("`effect fn f() -> {name}` with a statement body");
        let (checked, diag) = check_then_native(&program, "x\ndone\n", &mut failures, &label);
        if name == "Unit" && !checked {
            failures.push(format!("{label}: refused\n{diag}"));
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}
