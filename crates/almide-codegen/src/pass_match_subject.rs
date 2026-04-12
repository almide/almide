//! MatchSubject Nanopass: insert ownership transforms on match subjects.
//!
//! Target: Rust only.
//!
//! Rust's `match` on `String` requires `.as_str()` to match against `&str` literals.
//! `Option<String>` requires `.as_deref()` to match `Some("literal")` patterns.
//!
//! This pass inserts `Borrow { as_str: true }` or `Call { Method "as_deref" }` IR nodes
//! on match subjects, so the walker never needs to check types.

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MatchSubjectPass;

impl NanoPass for MatchSubjectPass {
    fn name(&self) -> &str { "MatchSubject" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for func in &mut program.functions {
            rewrite_expr(&mut func.body);
        }
        for tl in &mut program.top_lets {
            rewrite_expr(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                rewrite_expr(&mut func.body);
            }
            for tl in &mut module.top_lets {
                rewrite_expr(&mut tl.value);
            }
        }
        PassResult { program, changed: true }
    }
}

fn rewrite_expr(expr: &mut IrExpr) {
    // Recurse into children first (bottom-up)
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => { rewrite_expr(left); rewrite_expr(right); }
        IrExprKind::UnOp { operand, .. } => rewrite_expr(operand),
        IrExprKind::If { cond, then, else_ } => { rewrite_expr(cond); rewrite_expr(then); rewrite_expr(else_); }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { rewrite_stmt(s); }
            if let Some(e) = expr { rewrite_expr(e); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => rewrite_expr(object),
                _ => {}
            }
            for a in args { rewrite_expr(a); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { rewrite_expr(e); }
        }
        IrExprKind::Record { fields, .. } => { for (_, v) in fields { rewrite_expr(v); } }
        IrExprKind::SpreadRecord { base, fields } => {
            rewrite_expr(base);
            for (_, v) in fields { rewrite_expr(v); }
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { rewrite_expr(k); rewrite_expr(v); } }
        IrExprKind::Range { start, end, .. } => { rewrite_expr(start); rewrite_expr(end); }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => rewrite_expr(object),
        IrExprKind::IndexAccess { object, index } => { rewrite_expr(object); rewrite_expr(index); }
        IrExprKind::MapAccess { object, key } => { rewrite_expr(object); rewrite_expr(key); }
        IrExprKind::Lambda { body, .. } => rewrite_expr(body),
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { rewrite_expr(expr); } }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            rewrite_expr(iterable);
            for s in body { rewrite_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            rewrite_expr(cond);
            for s in body { rewrite_stmt(s); }
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Await { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => rewrite_expr(e),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => { rewrite_expr(e); rewrite_expr(f); }
        IrExprKind::RustMacro { args, .. } => { for a in args { rewrite_expr(a); } }
        // Match — handled below after recursion
        IrExprKind::Match { subject, arms } => {
            rewrite_expr(subject);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_expr(g); }
                rewrite_expr(&mut arm.body);
            }
        }
        _ => {}
    }

    // Transform match subjects after children are rewritten
    if let IrExprKind::Match { subject, arms } = &mut expr.kind {
        transform_match_subject(subject, arms);
    }
}

fn rewrite_stmt(stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => rewrite_expr(value),
        IrStmtKind::IndexAssign { index, value, .. } => { rewrite_expr(index); rewrite_expr(value); }
        IrStmtKind::MapInsert { key, value, .. } => { rewrite_expr(key); rewrite_expr(value); }
        IrStmtKind::ListSwap { a, b, .. } => { rewrite_expr(a); rewrite_expr(b); }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => { rewrite_expr(end); }
        IrStmtKind::ListCopySlice { len, .. } => { rewrite_expr(len); }
        IrStmtKind::Guard { cond, else_ } => { rewrite_expr(cond); rewrite_expr(else_); }
        IrStmtKind::Expr { expr } => rewrite_expr(expr),
        IrStmtKind::Comment { .. } => {}
    }
}

/// Insert `.as_str()` or `.as_deref()` on match subjects when needed.
fn transform_match_subject(subject: &mut Box<IrExpr>, arms: &[IrMatchArm]) {
    // String subject with string literal patterns → wrap with Borrow { as_str: true }
    if matches!(&subject.ty, Ty::String) {
        let has_str_pat = arms.iter().any(|a| {
            matches!(&a.pattern, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
        });
        if has_str_pat {
            let inner = std::mem::replace(
                subject.as_mut(),
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None },
            );
            **subject = IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(inner), as_str: true, mutable: false },
                ty: Ty::String, // type is still String for downstream
                span: subject.span,
            };
        }
    }

    // Option<String> subject with Some("literal") patterns → wrap with method call .as_deref()
    // Also trigger when inner type is Unknown/TypeVar — if patterns contain string literals,
    // the subject is Option<String> even if not fully resolved at this point.
    if let Ty::Applied(TypeConstructorId::Option, args) = &subject.ty {
        let inner_is_string = args.len() == 1 && matches!(&args[0], Ty::String | Ty::Unknown | Ty::TypeVar(_));
        if inner_is_string {
            let has_some_str_pat = arms.iter().any(|a| {
                if let IrPattern::Some { inner } = &a.pattern {
                    matches!(inner.as_ref(), IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
                } else { false }
            });
            if has_some_str_pat {
                let inner = std::mem::replace(
                    subject.as_mut(),
                    IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None },
                );
                let deref_ty = Ty::Applied(TypeConstructorId::Option, vec![Ty::String]);
                **subject = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Method {
                            object: Box::new(inner),
                            method: "as_deref".into(),
                        },
                        args: vec![],
                        type_args: vec![],
                    },
                    ty: deref_ty,
                    span: subject.span,
                };
            }
        }
    }
}
