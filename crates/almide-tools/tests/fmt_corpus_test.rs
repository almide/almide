//! #1309 — the formatter's executable contract, over the real corpus.
//!
//! For every `.almd` under `spec/` and `examples/` (the CI fmt-gate set):
//!   1. `verify_format` passes: formatting preserves parse-ability, the AST,
//!      and every line comment;
//!   2. formatting is idempotent: `fmt(parse(fmt(p))) == fmt(p)` — the rule
//!      almide-tools/CLAUDE.md states, now gated instead of trusted.
//!
//! Plus unit tests that the verifier actually detects each corruption class.

use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_tools::fmt::{format_program, verify_format};

fn parse(src: &str) -> Result<almide_lang::ast::Program, String> {
    let tokens = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(p) if parser.errors.is_empty() => Ok(p),
        Ok(_) => Err(format!(
            "{} parse error(s), first: {}",
            parser.errors.len(),
            parser.errors[0].display()
        )),
        Err(e) => Err(e),
    }
}

fn collect_almd(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_almd(&path, out);
        } else if path.extension().is_some_and(|e| e == "almd") {
            out.push(path);
        }
    }
}

#[test]
fn corpus_survives_verify_and_is_idempotent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    collect_almd(&root.join("spec"), &mut files);
    collect_almd(&root.join("examples"), &mut files);
    assert!(
        files.len() > 100,
        "corpus walk found only {} files — wrong root?",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).expect("read corpus file");
        let rel = path.strip_prefix(&root).unwrap_or(path).display().to_string();
        let program = match parse(&src) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{rel}: does not parse clean: {e}"));
                continue;
            }
        };
        let formatted = format_program(&program);
        if let Err(why) = verify_format(&src, &program, &formatted) {
            failures.push(format!("{rel}: verify failed: {why}"));
            continue;
        }
        // Idempotency: a second pass must be byte-identical.
        match parse(&formatted) {
            Ok(second) => {
                let twice = format_program(&second);
                if twice != formatted {
                    failures.push(format!("{rel}: fmt is not idempotent"));
                }
            }
            Err(e) => failures.push(format!("{rel}: formatted output does not parse: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} corpus file(s) violate the fmt contract:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ── Detector unit tests: each corruption class must be caught ────────────

#[test]
fn verifier_accepts_a_clean_format() {
    let src = "// keep me\nfn main() -> Unit = {\n  println(\"ok\")\n}\n";
    let program = parse(src).expect("fixture parses");
    let formatted = format_program(&program);
    verify_format(src, &program, &formatted).expect("clean format must verify");
}

#[test]
fn verifier_catches_unparseable_output() {
    let src = "fn main() -> Unit = {\n  println(\"ok\")\n}\n";
    let program = parse(src).expect("fixture parses");
    let why = verify_format(src, &program, "fn broken (").unwrap_err();
    assert!(
        why.contains("parse"),
        "must report a parse failure, got: {why}"
    );
}

#[test]
fn verifier_catches_an_ast_change() {
    let src = "fn answer() -> Int = 41\n";
    let program = parse(src).expect("fixture parses");
    let why = verify_format(src, &program, "fn answer() -> Int = 42\n").unwrap_err();
    assert!(
        why.contains("AST changed"),
        "must report the AST divergence, got: {why}"
    );
    assert!(
        why.contains("decls[0]"),
        "must point at the diverging path, got: {why}"
    );
}

#[test]
fn verifier_catches_a_lost_line_comment() {
    let src = "// load-bearing comment\nfn answer() -> Int = 41\n";
    let program = parse(src).expect("fixture parses");
    let why = verify_format(src, &program, "fn answer() -> Int = 41\n").unwrap_err();
    assert!(
        why.contains("comment"),
        "must report the lost comment, got: {why}"
    );
}
