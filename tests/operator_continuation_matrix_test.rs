//! #3155: SPEC.md §2.1 / GRAMMAR.md promise that an operator may open a
//! continuation line. `??` could not, and neither could `<` or `>`: the
//! newline lookahead was a hand-kept list beside the binding-power table,
//! and the two had drifted apart.
//!
//! This gate asks the PARSER which spellings are binary operators instead of
//! keeping a third list. Every operator and keyword spelling the lexer knows
//! (scraped from its two tables, so a new operator is enrolled the moment it
//! gets a lexer row) is tried as `a OP b` on one line. Each spelling that
//! parses there as a two-operand node is a member of the family, and the same
//! expression with OP at the head of the next line must parse to the
//! IDENTICAL tree — same node, same nesting, same diagnostics. One `#[test]`
//! cell per member, so a red names its operator.
//!
//! The deliberate exception is `...`: a line-initial `...` is a spread inside a
//! multiline record or list literal, so an inclusive range cannot break before
//! it (Parser::INFIX_TOKENS). It is asserted NOT to continue, so the exception
//! cannot widen silently either.

use almide::ast::{Decl, ExprKind, Stmt};
use almide::lexer::Lexer;
use almide::parser::Parser;

/// Every spelling in the lexer's KEYWORDS and OPERATORS tables.
fn lexer_spellings() -> Vec<String> {
    let path = format!("{}/crates/almide-syntax/src/lexer.rs", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("read lexer.rs");
    let mut out = Vec::new();
    for (i, _) in text.match_indices("(\"") {
        let rest = &text[i + 2..];
        let Some(end) = rest.find('"') else { continue };
        let spelling = &rest[..end];
        // Only table rows: `("<spelling>", TokenType::…`.
        let is_row = rest[end + 1..].starts_with(", TokenType::");
        if is_row && !spelling.is_empty() && !out.iter().any(|s: &String| s == spelling) {
            out.push(spelling.to_string());
        }
    }
    out
}

/// Parse `src`, returning the `let v = …` value's tree as JSON (ids and spans
/// are not serialized) and the diagnostic codes, or `None` when the parse
/// failed or did not produce that let.
fn let_value(src: &str) -> Option<(serde_json::Value, Vec<String>)> {
    let mut parser = Parser::new(Lexer::tokenize(src)).with_file("matrix.almd");
    let prog = parser.parse().ok()?;
    let codes: Vec<String> = parser.errors.iter().map(|d| d.code.unwrap_or_default().to_string()).collect();
    let body = prog.decls.iter().find_map(|d| match d {
        Decl::Fn { body: Some(b), .. } => Some(b),
        _ => None,
    })?;
    let ExprKind::Block { stmts, expr } = &body.kind else { return None };
    // The shape is exactly `let v = …` then the tail `v`: a continuation that
    // failed to join leaves an extra statement behind.
    if stmts.len() != 1 || expr.is_none() {
        return None;
    }
    let Stmt::Let { value, .. } = &stmts[0] else { return None };
    Some((serde_json::to_value(value).ok()?, codes))
}

fn one_line(op: &str) -> String {
    format!("fn f() -> Int = {{\n  let v = a {op} b\n  v\n}}\n")
}

fn continued(op: &str) -> String {
    format!("fn f() -> Int = {{\n  let v = a\n    {op} b\n  v\n}}\n")
}

/// A two-operand node with `a` and `b` as its operands — the one-line parse of
/// `a OP b` for a binary (or postfix-binary, `??`) operator.
fn is_binary_family(tree: &serde_json::Value) -> bool {
    let kind = tree.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    let names: Vec<&str> = ["left", "right", "start", "end", "expr", "fallback"]
        .iter()
        .filter_map(|k| tree.get(*k))
        .filter_map(|e| e.get("name").and_then(|n| n.as_str()))
        .collect();
    !matches!(kind, "member" | "call" | "ident") && names == ["a", "b"]
}

/// The family, as the parser defines it.
fn binary_family() -> Vec<String> {
    lexer_spellings()
        .into_iter()
        .filter(|op| let_value(&one_line(op)).is_some_and(|(t, _)| is_binary_family(&t)))
        .collect()
}

/// The spellings the matrix below has a cell for. A new binary operator must
/// get a cell here — `the_matrix_names_every_binary_operator` fails until it
/// does, so the family can only grow WITH its continuation check.
const CELLS: &[&str] = &[
    "or", "and", "==", "!=", "<", "<=", ">", ">=", "|>", ">>", "..<", "...", "..", "..=", "+", "-",
    "++", "*", "/", "%", "^", "**", "??",
];

/// The one family member that may NOT open a line, and why.
const NO_CONTINUATION: &[(&str, &str)] =
    &[("...", "a line-initial `...` is a spread in a multiline record/list literal")];

#[test]
fn the_matrix_names_every_binary_operator() {
    let family = binary_family();
    assert!(family.len() >= 20, "family discovery found too little: {family:?}");
    for op in &family {
        assert!(CELLS.contains(&op.as_str()), "binary operator `{op}` has no continuation cell");
    }
    for cell in CELLS {
        assert!(family.iter().any(|f| f == cell), "cell `{cell}` is not a binary operator any more");
    }
}

fn assert_continues(op: &str) {
    let (flat, flat_codes) = let_value(&one_line(op)).expect("one-line form parses");
    let joined = let_value(&continued(op));
    if let Some((_, why)) = NO_CONTINUATION.iter().find(|(o, _)| *o == op) {
        assert!(
            joined.as_ref().map(|(t, _)| t != &flat).unwrap_or(true),
            "`{op}` was not supposed to open a line ({why})"
        );
        return;
    }
    let (tree, codes) = joined.unwrap_or_else(|| panic!("`a\\n  {op} b` did not continue the line above"));
    assert_eq!(tree, flat, "`{op}` at the head of a line parsed to a different tree");
    assert_eq!(codes, flat_codes, "`{op}` at the head of a line changed the diagnostics");
}

macro_rules! cells {
    ($($name:ident => $op:literal,)*) => {
        $( #[test] fn $name() { assert_continues($op); } )*
        #[test]
        fn every_cell_has_a_test() {
            let tested: &[&str] = &[$($op),*];
            for cell in CELLS {
                assert!(tested.contains(cell), "cell `{cell}` has no #[test]");
            }
        }
    };
}

cells! {
    or_continues => "or",
    and_continues => "and",
    eq_continues => "==",
    ne_continues => "!=",
    lt_continues => "<",
    le_continues => "<=",
    gt_continues => ">",
    ge_continues => ">=",
    pipe_continues => "|>",
    compose_continues => ">>",
    range_exclusive_continues => "..<",
    range_inclusive_does_not_continue => "...",
    retired_range_continues => "..",
    retired_inclusive_range_continues => "..=",
    plus_continues => "+",
    minus_continues => "-",
    plus_plus_continues => "++",
    star_continues => "*",
    slash_continues => "/",
    percent_continues => "%",
    caret_continues => "^",
    star_star_continues => "**",
    unwrap_or_continues => "??",
}

/// `??`'s own line rule (E038, #1112) is untouched: the FALLBACK must still
/// start on the `??` line, or the next statement would be swallowed.
#[test]
fn a_leading_qq_still_needs_its_fallback_on_its_line() {
    let src = "fn f() -> Int = {\n  let v = a\n    ??\n  b\n  v\n}\n";
    let mut parser = Parser::new(Lexer::tokenize(src)).with_file("matrix.almd");
    let _ = parser.parse();
    assert!(
        parser.errors.iter().any(|d| d.code == Some("E038")),
        "a fallback on the line after a leading `??` must stay E038"
    );
}

/// A pipe chain closed by a leading `??` is the issue's own shape: `??` binds
/// the last stage, exactly as it does written on one line.
#[test]
fn a_leading_qq_closes_a_multiline_pipe_chain() {
    let src = |body: &str| format!("fn f() -> Int = {{\n  let v = {body}\n  v\n}}\n");
    let flat = let_value(&src("xs |> g ?? 0")).expect("flat parses");
    let multi = let_value(&src("xs\n    |> g\n    ?? 0")).expect("multiline parses");
    assert_eq!(multi.0, flat.0);
}
