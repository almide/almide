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

// ── Block comments (#1318): preserved where attachable, refused inline ───

#[test]
fn own_line_block_comments_survive_fmt() {
    let src = "/* file-level block comment\n   spanning two lines */\nfn main() -> Unit = {\n  /* own-line body comment */\n  println(\"ok\")\n}\n";
    let program = parse(src).expect("fixture parses");
    let formatted = format_program(&program);
    verify_format(src, &program, &formatted).expect("block comments must be conserved");
    assert!(
        formatted.contains("/* file-level block comment"),
        "file-level block comment must be reprinted, got:\n{formatted}"
    );
    assert!(
        formatted.contains("/* own-line body comment */"),
        "body block comment must be reprinted, got:\n{formatted}"
    );
    // And the result must be idempotent like everything else.
    let second = parse(&formatted).expect("formatted parses");
    assert_eq!(format_program(&second), formatted, "fmt must be idempotent");
}

/// SUPERSEDED BEHAVIOUR (#1404): this used to assert that an inline block
/// comment made the verifier REFUSE, because the printer had no slot for it.
/// It now attaches to the node it annotates and round-trips, so the assertion
/// is inverted — kept rather than deleted so the change of contract is visible
/// in the history at the place that pinned the old one.
#[test]
fn inline_block_comment_is_attached_and_conserved() {
    let src = "fn f(a: Int, b: Int) -> Int = a + b\n\nfn main() -> Unit = {\n  let x = f(1 /* inline */, 2)\n  println(\"ok\")\n}\n";
    let program = parse(src).expect("inline block comments stay legal to parse");
    let formatted = format_program(&program);
    verify_format(src, &program, &formatted)
        .expect("an attached inline comment is conserved, not refused");
    // TRAILING on `1`: it must not cross the comma onto `2`.
    assert!(
        formatted.contains("f(1 /* inline */, 2)"),
        "inline comment moved off the node it annotates:\n{formatted}"
    );
    let second = parse(&formatted).expect("formatted parses");
    assert_eq!(format_program(&second), formatted, "fmt must stay idempotent");
}

/// The half #1404 does NOT close, pinned so the boundary is explicit: a `//`
/// comment before a continuation line still refuses. Reprinting it inline
/// would comment out the rest of the line — an early attempt did exactly that
/// and the verifier caught `binary -> int`, i.e. the `+ 2` had vanished. It
/// needs line-aware placement, which the inline bracket is not.
#[test]
fn a_line_comment_before_a_continuation_still_refuses() {
    let src = "fn main() -> Unit = {\n  let y = 1 + // why\n    2\n  println(\"ok\")\n}\n";
    let program = parse(src).expect("parses");
    let formatted = format_program(&program);
    let why = verify_format(src, &program, &formatted)
        .expect_err("an unattachable line comment must refuse, never silently drop");
    assert!(why.contains("comment"), "got: {why}");
}

/// #1511: `Option[T]` and `T?` are one type — the canonicalizing rewrite to
/// the sugar must VERIFY as conserving (it used to refuse as E054, so a legal
/// spelled-out source could neither format nor live under the fmt gate).
#[test]
fn option_sugar_canonicalization_verifies_as_conserving() {
    let src = "fn pick(n: Int) -> Option[Int] = if n > 0 then some(n) else none\n\nfn main() -> Unit = {\n  let oo: Option[Int?] = some(some(9))\n  println(int.to_string((pick(3) ?? 0) + match oo { some(i) => i ?? 0, none => 0 - 1 }))\n}\n";
    let program = parse(src).expect("parses");
    let formatted = format_program(&program);
    assert!(
        formatted.contains("-> Int?") && formatted.contains("(Int?)?"),
        "fmt canonicalizes to the sugar:\n{formatted}"
    );
    verify_format(src, &program, &formatted)
        .expect("the Option->? spelling rename is the same type, not an AST change");
    let second = parse(&formatted).expect("formatted parses");
    assert_eq!(format_program(&second), formatted, "fmt must stay idempotent");
}
