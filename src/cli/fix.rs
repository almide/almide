//! `almide fix` — apply mechanically-safe fixes to a source file.
//!
//! **MVP scope**: runs the formatter's `auto_imports` pass (covering E003
//! "Add `import json`" style diagnostics), then re-checks and reports any
//! remaining `try:` snippets as "manual fix needed". The value is a single
//! callable entry-point that LLM harnesses can invoke before retrying —
//! today it fixes import omissions; future iterations will auto-apply
//! more `try:` snippets (let-in → newline chain, cons pattern → list.first,
//! etc) once the rewrite infrastructure is in place.

use crate::{parse_file, project, project_fetch};
use almide::fmt::{auto_imports, format_program};

pub fn cmd_fix(file: &str, dry_run: bool) {
    let (mut program, source_text, _parse_errors) = parse_file(file);

    let (dep_names, dep_submodules): (Vec<String>, std::collections::HashMap<String, String>) =
        if std::path::Path::new("almide.toml").exists() {
            match project::parse_toml(std::path::Path::new("almide.toml")) {
                Ok(proj) => {
                    let fetched = project_fetch::fetch_all_deps(&proj).unwrap_or_default();
                    let names: Vec<String> = fetched.iter().map(|fd| fd.pkg_id.name.clone()).collect();
                    (names, std::collections::HashMap::new())
                }
                Err(_) => (vec![], std::collections::HashMap::new()),
            }
        } else {
            (vec![], std::collections::HashMap::new())
        };

    let messages = auto_imports(&mut program, &dep_names, &dep_submodules);

    let has_import_changes = !messages.is_empty();
    let new_source = if has_import_changes {
        format_program(&program)
    } else {
        source_text.clone()
    };

    if dry_run {
        if has_import_changes {
            println!("--- would apply ---");
            for m in &messages {
                println!("  {}", m);
            }
            println!("\n--- new file contents ---");
            println!("{}", new_source);
        } else {
            println!("no import-level fixes needed");
        }
    } else if has_import_changes {
        if let Err(e) = std::fs::write(file, &new_source) {
            eprintln!("error: failed to write {}: {}", file, e);
            std::process::exit(1);
        }
        eprintln!("{}:", file);
        for m in &messages {
            eprintln!("  {}", m);
        }
    }

    // After import fixes, report any remaining `try:` snippets so callers
    // know what's left to do by hand.
    report_manual_fixes(file, &new_source);
}

fn report_manual_fixes(file: &str, source: &str) {
    use almide::check::Checker;
    use almide::canonicalize;
    use almide::diagnostic;

    // Re-parse the (possibly modified) source and type-check.
    let tokens = almide::lexer::Lexer::tokenize(source);
    let mut parser = almide::parser::Parser::new(tokens);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(_) => return,
    };
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.set_source(file, source);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut prog);

    let manual: Vec<&diagnostic::Diagnostic> = diagnostics.iter()
        .chain(parser.errors.iter())
        .filter(|d| d.level == diagnostic::Level::Error && d.try_snippet.is_some())
        .collect();

    if manual.is_empty() {
        return;
    }
    eprintln!("\n{} diagnostic(s) have `try:` snippets that need manual application:",
        manual.len());
    for d in &manual {
        let loc = match (d.line, d.col) {
            (Some(l), Some(c)) => format!("{}:{}", l, c),
            (Some(l), None) => format!("{}", l),
            _ => "?".into(),
        };
        let code = d.code.as_deref().unwrap_or("E???");
        eprintln!("  [{code}] {file}:{loc}  {}", d.message);
    }
    eprintln!("\nRun `almide check {}` for the full text of each `try:` snippet.", file);
}
