//! `var (x, y) = e` / `var { a, b } = r` (#3149, ruling A): `var` takes the
//! patterns `let` takes, and every name the pattern binds is a `var`.
//!
//! The form is a desugar to what the author would write by hand, so nothing
//! downstream of the checker (lowering, the ownership passes, native, wasm,
//! interp) sees anything new:
//!
//! ```text
//! var (x, (y, _)) = e     ==>   let __var_destructure_N = e
//!                               var x = __var_destructure_N.0
//!                               var y = (__var_destructure_N.1).0
//! ```
//!
//! When the value is already a plain name, the projections read it directly
//! (`var x = p.0`, `var y = p.1`) — exactly the hand-written spelling, so no
//! copy of the whole value is introduced. The one exception is a pattern that
//! rebinds that same name (`var (p, q) = p`): the later projections would read
//! the new `p`, so the value is named once first.

use crate::ast::{Expr, ExprId, ExprKind, Pattern, Span, Stmt};
use crate::intern::{sym, Sym};

/// One step from the destructured value to a bound name.
#[derive(Clone, Copy)]
enum Access {
    Index(usize),
    Field(Sym),
}

/// Expand `var <pattern> = value` into one hidden `let` (when the value is not
/// already a name) and one `var` per bound name. `next_id` supplies fresh ids
/// for the synthesized projection expressions.
///
/// A pattern that binds no name (`var (_, _) = e`) stays the `let` form — the
/// two are the same statement when nothing is bound.
pub fn desugar_var_destructure(
    pattern: Pattern,
    value: Expr,
    span: Option<Span>,
    next_id: &mut dyn FnMut() -> ExprId,
) -> Vec<Stmt> {
    let mut bindings = Vec::new();
    collect_bindings(&pattern, &mut Vec::new(), &mut bindings);
    if bindings.is_empty() {
        return vec![Stmt::LetDestructure { pattern, value, mutable: false, span }];
    }
    let mut stmts = Vec::with_capacity(bindings.len() + 1);
    let base = match &value.kind {
        ExprKind::Ident { name } if !bindings.iter().any(|(n, _)| n == name) => *name,
        _ => {
            let tmp = sym(&format!("__var_destructure_{}", value.id.0));
            stmts.push(Stmt::Let { name: tmp, ty: None, value, span });
            tmp
        }
    };
    for (name, path) in bindings {
        let mut e = Expr::new(next_id(), span, ExprKind::Ident { name: base });
        for step in path {
            let kind = match step {
                Access::Index(index) => ExprKind::TupleIndex { object: Box::new(e), index },
                Access::Field(field) => ExprKind::Member { object: Box::new(e), field },
            };
            e = Expr::new(next_id(), span, kind);
        }
        stmts.push(Stmt::Var { name, ty: None, value: e, span });
    }
    stmts
}

/// Replace every `var <pattern>` statement of one statement list by its
/// desugaring, in place. Nested statement lists are the caller's to reach
/// (the checker expands each list as it enters it).
pub fn expand_var_destructures(stmts: &mut Vec<Stmt>, next_id: &mut dyn FnMut() -> ExprId) {
    if !stmts.iter().any(|s| matches!(s, Stmt::LetDestructure { mutable: true, .. })) {
        return;
    }
    let old = std::mem::take(stmts);
    for stmt in old {
        match stmt {
            Stmt::LetDestructure { pattern, value, mutable: true, span } => {
                stmts.extend(desugar_var_destructure(pattern, value, span, next_id));
            }
            other => stmts.push(other),
        }
    }
}

fn collect_bindings(pat: &Pattern, path: &mut Vec<Access>, out: &mut Vec<(Sym, Vec<Access>)>) {
    match pat {
        Pattern::Ident { name } => out.push((*name, path.clone())),
        Pattern::Tuple { elements } => {
            for (i, e) in elements.iter().enumerate() {
                path.push(Access::Index(i));
                collect_bindings(e, path, out);
                path.pop();
            }
        }
        Pattern::RecordPattern { fields, .. } => {
            for f in fields {
                path.push(Access::Field(f.name));
                match &f.pattern {
                    Some(p) => collect_bindings(p, path, out),
                    None => out.push((f.name, path.clone())),
                }
                path.pop();
            }
        }
        // `let` destructuring parses tuples (nested), `_` and record
        // shorthand only; `_` binds nothing.
        _ => {}
    }
}
