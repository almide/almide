//! Executable stdlib doc-examples (law L-docs, STDLIB-EXCELLENCE.md).
//!
//! Every `// Example: <boolean expr>` line in a stdlib module is a CLAIM.
//! This harness extracts each one, wraps it in a program, runs it through
//! the interpreter (the executable spec), and requires it to print `true`.
//! Doc, test, and spec are one artifact — Roc's doctrine, executable here
//! because the greenfield executes. Adopted from the 9-compiler survey
//! (canon law 9); the count floor below is a shrink-proof witness that the
//! harness actually found the examples it gates.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Modules that are NOT auto-imported and need an explicit `import` line in
/// the generated example program. Extended as modules gain examples.
const NEEDS_IMPORT: &[&str] = &["url", "json", "fs", "http", "env", "io", "random", "regex", "process", "testing", "net", "zlib", "html"];

#[test]
fn every_stdlib_example_evaluates_to_true() {
    let root = workspace_root();
    let mut examples: Vec<(String, String, String)> = Vec::new(); // (module, file, expr)
    for entry in std::fs::read_dir(root.join("stdlib")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_none_or(|e| e != "almd") {
            continue;
        }
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        let module = stem.split('_').next().unwrap_or(&stem).to_string();
        for line in std::fs::read_to_string(&p).unwrap().lines() {
            if let Some(expr) = line.trim().strip_prefix("// Example: ") {
                examples.push((module.clone(), stem.clone(), expr.trim().to_string()));
            }
        }
    }
    assert!(
        examples.len() >= 12,
        "example count fell to {} — the harness floor is shrink-proof; add examples, never delete silently",
        examples.len()
    );

    let mut failures = Vec::new();
    for (i, (module, file, expr)) in examples.iter().enumerate() {
        let import_line = if NEEDS_IMPORT.contains(&module.as_str()) {
            format!("import {module}\n\n")
        } else {
            String::new()
        };
        let program = format!(
            "{import_line}effect fn main() -> Unit = println(if ({expr}) then \"true\" else \"false\")\n"
        );
        let name = format!("example_{i}_{file}.almd");
        match almide_spine::s5::run_file(&name, &program) {
            Ok(out) if out.exit == 0 && out.stdout == "true\n" => {}
            Ok(out) => failures.push(format!(
                "{file}: `{expr}` -> exit {} stdout {:?} stderr {:?}",
                out.exit, out.stdout, out.stderr
            )),
            Err(e) => failures.push(format!("{file}: `{expr}` failed front end: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} stdlib examples are false or broken:\n{}",
        failures.len(),
        examples.len(),
        failures.join("\n")
    );
    println!("stdlib examples: {} evaluated true", examples.len());
}

/// Path is a red herring in run_file only for diagnostics attribution; make
/// sure the harness never depends on a real file existing.
#[allow(dead_code)]
fn _doc(_: &Path) {}
