//! ALS-D2: the parser-recovery nodes (`ExprKind::Error` / `Stmt::Error`) are
//! RECOVERY VOCABULARY — they exist so one parse error does not hide the rest
//! of a file's diagnostics, and they must NEVER appear in an accepted
//! program. Both directions asserted:
//!  - the whole spec/wasm_cross corpus (accepted, both-target-executed
//!    programs) parses with ZERO recovery nodes;
//!  - a deliberately broken file recovers: its parse yields diagnostics AND
//!    keeps parsing past the error (the following declaration is still seen).

use almide::lexer::Lexer;
use almide::parser::Parser;

fn parse_source(src: &str) -> (almide::ast::Program, Vec<String>) {
    let tokens = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(p) => (p, parser.errors.iter().map(|d| d.display()).collect()),
        Err(e) => {
            let empty = Lexer::tokenize("");
            let mut p2 = Parser::new(empty);
            (p2.parse().expect("empty parses"), vec![e])
        }
    }
}

fn count_error_nodes(program: &almide::ast::Program) -> usize {
    // Serialize the AST and count the recovery variants structurally — the
    // serde names are the enum variant tags, so a new nesting position cannot
    // silently escape this count.
    let json = serde_json::to_string(program).expect("serialize AST");
    json.matches("\"Error\"").count()
}

#[test]
fn accepted_corpus_contains_zero_recovery_nodes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).expect("read spec/wasm_cross") {
        let path = entry.expect("entry").path();
        if path.extension().map_or(true, |e| e != "almd") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read fixture");
        let (program, errors) = parse_source(&src);
        assert!(
            errors.is_empty(),
            "{} is an ACCEPTED fixture but parsed with errors: {errors:?}",
            path.display()
        );
        let n = count_error_nodes(&program);
        assert_eq!(
            n,
            0,
            "{} carries {n} recovery node(s) — recovery vocabulary must never appear in an accepted program",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 300, "the corpus glob went blind (only {checked} files) — the find-nothing class");
}

#[test]
fn broken_file_recovers_past_the_error() {
    // The first fn has a parse error in its body; the SECOND fn must still be
    // seen (recovery continued), and diagnostics must be non-empty.
    let src = "fn broken() -> Int = (1 +\n\nfn after() -> Int = 42\n";
    let (program, errors) = parse_source(src);
    assert!(
        !errors.is_empty(),
        "a broken file must report at least one diagnostic"
    );
    let names: Vec<String> = program
        .decls
        .iter()
        .filter_map(|d| match d {
            almide::ast::Decl::Fn { name, .. } => Some(name.as_str().to_string()),
            _ => None,
        })
        .collect();
    assert!(
        names.iter().any(|n| n == "after"),
        "recovery must keep parsing past the error — `after` not seen, got decls {names:?} with errors {errors:?}"
    );
}
