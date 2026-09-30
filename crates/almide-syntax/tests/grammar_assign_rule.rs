//! docs/GRAMMAR.md's `assign` rule and the parser agree (#3064).
//!
//! The rule used to promise `postfix "." IDENT "=" expr` while the parser
//! took one identifier before the `.`, so `o.inner.xs = v` — a target the
//! document listed — died as "Assignments return Unit". This test reads the
//! rule's comment out of the document and parses every backticked example as
//! a statement: each listed target must parse as an assignment, and the one
//! the rule says is refused must be refused with its located message. Editing
//! the examples edits the test; a parser change that drops a listed shape
//! fails here.

use almide_syntax::ast::{Decl, ExprKind, Stmt};
use almide_syntax::lexer::Lexer;
use almide_syntax::parser::Parser;

/// The `assign` rule and its comment, up to the next rule.
fn assign_rule() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/GRAMMAR.md");
    let doc = std::fs::read_to_string(&path).expect("docs/GRAMMAR.md");
    let start = doc.find("\nassign ").expect("GRAMMAR.md has an `assign` rule");
    let rest = &doc[start + 1..];
    let end = rest.find("\nguard_stmt").expect("the rule after `assign` is `guard_stmt`");
    rest[..end].to_string()
}

/// Backticked spans in `text`, in order.
fn backticked(text: &str) -> Vec<String> {
    text.split('`').skip(1).step_by(2).map(str::to_string).collect()
}

/// Parse `stmt` as the first statement of a fn body.
fn parse_stmt(stmt: &str) -> Result<Stmt, String> {
    let src = format!("fn main() -> Unit = {{\n  {stmt}\n  ()\n}}\n");
    let mut parser = Parser::new(Lexer::tokenize(&src));
    let prog = parser.parse();
    if let Some(d) = parser.errors.first() {
        return Err(d.message.clone());
    }
    let prog = prog?;
    let Some(Decl::Fn { body: Some(body), .. }) = prog.decls.into_iter().next() else {
        return Err("no fn".into());
    };
    let ExprKind::Block { stmts, .. } = body.kind else { return Err("body is not a block".into()) };
    stmts.into_iter().next().ok_or_else(|| "no statement".into())
}

#[test]
fn every_target_the_grammar_lists_parses_as_an_assignment() {
    let rule = assign_rule();
    let (listed, refused) = rule.split_once("An index can only be the LAST step")
        .expect("the rule states the index restriction");
    let targets = backticked(listed);
    assert!(targets.len() >= 5, "the rule lists its target shapes: {targets:?}");
    for t in &targets {
        match parse_stmt(t) {
            Ok(Stmt::Assign { .. } | Stmt::FieldAssign { .. } | Stmt::IndexAssign { .. }) => {}
            other => panic!("GRAMMAR.md lists `{t}` as an assignment target, the parser gave {other:?}"),
        }
    }
    let refused_example = backticked(refused).into_iter().next().expect("the refused shape is spelled");
    let err = parse_stmt(&refused_example).expect_err("the refused shape must not parse");
    assert!(err.contains("an index can only be the last step"), "`{refused_example}` refused with: {err}");
}

#[test]
fn a_nested_target_keeps_its_path() {
    let Ok(Stmt::FieldAssign { path, field, .. }) = parse_stmt("o.inner.leaf.tag = v") else { panic!("not a field assign") };
    assert_eq!(path.iter().map(|p| p.as_str()).collect::<Vec<_>>(), ["inner", "leaf"]);
    assert_eq!(field.as_str(), "tag");
    let Ok(Stmt::IndexAssign { path, .. }) = parse_stmt("o.inner.m[k] = v") else { panic!("not an index assign") };
    assert_eq!(path.iter().map(|p| p.as_str()).collect::<Vec<_>>(), ["inner", "m"]);
}
