//! The dialect-16 regrouping of `??` (#4531): where a program written for
//! the pre-16 grammar would change meaning, and how `almide fix` keeps it.
//!
//! Through dialect 15 `??` was a POSTFIX: it bound the postfix chain right
//! before it and read its fallback with `parse_unary`, so `x ?? 0 - 1` was
//! `(x ?? 0) - 1` and `s + b ?? 0` was `s + (b ?? 0)`. From dialect 16 every
//! binary operator follows one widely used precedence order and `??` is a
//! right-associative infix level between `|>` and the comparisons:
//! `x ?? 0 - 1` is `x ?? (0 - 1)`. No other operator moved relative to any
//! other, so a grouping that changed always involves a `??`.
//!
//! The change is silent — both readings type-check in the common cases — so
//! it is never left to chance:
//! - [`regrouped_sites`] parses a file a second time with the pre-16 grammar
//!   and names every `??` whose operands differ between the two trees. The
//!   checker refuses each one with E120 unless the file is stamped
//!   `@dialect(16)` or later (`almide check --stamp` writes the stamp only
//!   after the file checks clean, so a stamped file was verified against the
//!   new grammar).
//! - [`parenthesize_for_current`] is the migration: given the PRE-16 tree, it
//!   adds the parentheses under which the current grammar reads the same
//!   tree. `almide fix` prints the result and keeps it only if the current
//!   grammar's parse of the printed text equals the pre-16 tree
//!   (`almide::fmt::regroup`).

use std::collections::{HashMap, HashSet};

use super::Parser;
use crate::ast::{self, Expr, ExprId, ExprKind, Program};
use crate::lexer::{Lexer, Token, TokenType};
use crate::span::Span;

/// The dialect whose grammar groups `??` as an infix level. A file stamped
/// with this epoch or a later one is read under it without the E120 check.
pub const REGROUPING_EPOCH: u32 = 16;

/// Whether a file with this stamp may have been written for the pre-16
/// grouping — an unstamped file may, since stamps are optional.
pub fn predates_regrouping(stamp: Option<u32>) -> bool {
    stamp.is_none_or(|n| n < REGROUPING_EPOCH)
}

/// Whether the token stream can hold a `??` — a `??` token, or one inside an
/// interpolation hole (a hole is lexed by its own sub-parse).
pub fn mentions_coalesce(tokens: &[Token]) -> bool {
    tokens.iter().any(|t| match t.token_type {
        TokenType::QuestionQuestion => true,
        TokenType::InterpolatedString => {
            t.value.contains("??") || t.raw.as_deref().is_some_and(|r| r.contains("??"))
        }
        _ => false,
    })
}

/// The pre-16 parse of `tokens`, with the ADR-0005 D6 re-association the
/// pre-16 checker applied (see [`reassociate_pipe_coalesce`]) — the tree a
/// pre-16 compiler gave meaning to. `None` when that grammar reports an
/// error. The second value is the parser's next free expression id.
pub fn parse_legacy_tokens(tokens: Vec<Token>) -> Option<(Program, u32)> {
    let mut parser = Parser::new(tokens).with_legacy_grouping(true);
    let mut program = parser.parse().ok()?;
    if !parser.errors.is_empty() {
        return None;
    }
    reassociate_pipe_coalesce(&mut program);
    Some((program, parser.expr_id_counter()))
}

/// [`parse_legacy_tokens`] over source text.
pub fn parse_legacy(source: &str) -> Option<(Program, u32)> {
    parse_legacy_tokens(Lexer::tokenize(source))
}

/// ADR-0005 D6 as the pre-16 checker and lowering applied it: `??` bound
/// tighter than `|>`, so `xs |> f(a) ?? d` parsed as
/// `Pipe(xs, UnwrapOr(f(a), d))`, and the checker re-associated it to
/// `UnwrapOr(Pipe(xs, f(a)), d)` — the fallback of the PIPE's result. The
/// current grammar parses that tree directly. A parenthesised stage
/// (`xs |> (f ?? g)`) keeps its `Paren` node and was never re-associated.
pub fn reassociate_pipe_coalesce(program: &mut Program) {
    ast::visit_exprs_mut(program, &mut |e: &mut Expr| {
        let ExprKind::Pipe { right, .. } = &e.kind else { return };
        if !matches!(right.kind, ExprKind::UnwrapOr { .. }) {
            return;
        }
        let ExprKind::Pipe { left, right } = std::mem::replace(&mut e.kind, ExprKind::Unit) else {
            unreachable!("matched a Pipe above")
        };
        let right = *right;
        let ExprKind::UnwrapOr { expr: stage, fallback } = right.kind else {
            unreachable!("matched an UnwrapOr stage above")
        };
        let pipe = Expr::new(e.id, e.span, ExprKind::Pipe { left, right: stage });
        e.id = right.id;
        e.span = right.span;
        e.kind = ExprKind::UnwrapOr { expr: Box::new(pipe), fallback };
    });
}

/// The `??` operators of `current` (a current-grammar parse) whose operands
/// differ from the pre-16 parse of the same `tokens`, by the `??` token's
/// position. Only the innermost differing `??` of a nest is named: an outer
/// one whose own operands did not move differs only through it.
pub fn regrouped_sites(current: &Program, tokens: Vec<Token>) -> Vec<Span> {
    let Some((legacy, _)) = parse_legacy_tokens(tokens) else { return Vec::new() };
    diff_sites(current, &legacy)
}

/// [`regrouped_sites`] between two parses already in hand.
pub fn diff_sites(current: &Program, legacy: &Program) -> Vec<Span> {
    let now = coalesce_shapes(current);
    let then = coalesce_shapes(legacy);
    let differs: HashSet<(usize, usize)> = now
        .iter()
        .filter(|(at, (shape, _, _))| then.get(*at).is_none_or(|(old, _, _)| old != shape))
        .map(|(at, _)| *at)
        .collect();
    let mut sites: Vec<Span> = now
        .iter()
        .filter(|(at, (_, nested, _))| differs.contains(*at) && !nested.iter().any(|n| differs.contains(n)))
        .map(|(_, (_, _, span))| *span)
        .collect();
    // A safety rail, not a path: every grouping that moved involves a `??`
    // (the module header), so two trees that differ with every `??` alike
    // would be a hole in that argument. Name the first `??` rather than
    // let the file through.
    if sites.is_empty() && grouping_value(current) != grouping_value(legacy) {
        if let Some((_, (_, _, span))) = now.iter().min_by_key(|(at, _)| **at) {
            sites.push(*span);
        }
    }
    sites.sort_by_key(|s| (s.line, s.col));
    sites
}

type Shapes = HashMap<(usize, usize), (serde_json::Value, Vec<(usize, usize)>, Span)>;

/// Every `??` node by its operator position: its grouping (the subtree with
/// `Paren` nodes dropped) and the positions of the `??` nodes inside it.
fn coalesce_shapes(program: &Program) -> Shapes {
    let mut program = program.clone();
    let mut out = Shapes::new();
    ast::visit_exprs_mut(&mut program, &mut |e: &mut Expr| {
        let (ExprKind::UnwrapOr { .. }, Some(span)) = (&e.kind, e.span) else { return };
        let mut nested = Vec::new();
        ast::visit_expr(e, &mut |inner: &Expr| {
            if inner.id != e.id && matches!(inner.kind, ExprKind::UnwrapOr { .. }) {
                if let Some(s) = inner.span {
                    nested.push((s.line, s.col));
                }
            }
        });
        out.insert((span.line, span.col), (expr_grouping(e), nested, span));
    });
    out
}

/// A program's structure with every `Paren` node dropped: two parses with
/// the same value group every operator alike. `Program`'s `Serialize` skips
/// ids, positions and comments already.
pub fn grouping_value(program: &Program) -> serde_json::Value {
    let mut v = serde_json::to_value((&program.module, &program.imports, &program.decls))
        .unwrap_or(serde_json::Value::Null);
    strip_parens(&mut v);
    v
}

fn expr_grouping(e: &Expr) -> serde_json::Value {
    let mut v = serde_json::to_value(e).unwrap_or(serde_json::Value::Null);
    strip_parens(&mut v);
    v
}

/// Replace every `{"kind": "paren", "expr": X}` with `X`.
fn strip_parens(v: &mut serde_json::Value) {
    loop {
        let inner = match v {
            serde_json::Value::Object(map) if map.get("kind").and_then(|k| k.as_str()) == Some("paren") => {
                map.remove("expr")
            }
            _ => None,
        };
        match inner {
            Some(x) => *v = x,
            None => break,
        }
    }
    match v {
        serde_json::Value::Object(map) => map.values_mut().for_each(strip_parens),
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_parens),
        _ => {}
    }
}

/// The migration: add to a PRE-16 tree (from [`parse_legacy`]) the
/// parentheses under which the current grammar reads the same tree. Only a
/// `??` node and the operands of one can need them — no other pair of
/// operators changed order — and an operand that runs to the right with no
/// closing token (`if`, `match`, a lambda) reads alike under both grammars,
/// so it is left as written. `fresh` is the first unused expression id;
/// returns how many parentheses were added.
///
/// An interpolated string whose hole gained parentheses drops its cached
/// verbatim spelling, which the formatter would otherwise reprint (#1263).
pub fn parenthesize_for_current(program: &mut Program, fresh: u32) -> usize {
    use super::grouping::{is_open, needs_parens, operand_slots_mut};
    let mut next = fresh;
    let mut added = 0;
    ast::visit_exprs_mut(program, &mut |expr: &mut Expr| {
        let parent_is_coalesce = matches!(expr.kind, ExprKind::UnwrapOr { .. });
        for (child, slot) in operand_slots_mut(expr) {
            let child_is_coalesce = matches!(child.kind, ExprKind::UnwrapOr { .. });
            if (parent_is_coalesce || child_is_coalesce) && !is_open(child) && needs_parens(child, slot) {
                let inner = std::mem::replace(child, Expr::new(ExprId(next), child.span, ExprKind::Unit));
                next += 1;
                child.kind = ExprKind::Paren { expr: Box::new(inner) };
                added += 1;
            }
        }
    });
    if added > 0 {
        ast::visit_exprs_mut(program, &mut |e: &mut Expr| {
            if let ExprKind::InterpolatedString { raw, parts, .. } = &mut e.kind {
                let has_paren_hole = parts.iter().any(|p| match p {
                    ast::StringPart::Expr { expr } => {
                        let mut found = false;
                        ast::visit_expr(expr, &mut |x: &Expr| {
                            found |= matches!(x.kind, ExprKind::Paren { .. });
                        });
                        found
                    }
                    ast::StringPart::Lit { .. } => false,
                });
                if has_paren_hole {
                    *raw = None;
                }
            }
        });
    }
    added
}
