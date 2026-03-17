// ── Unknown type detection (post-pass) ──────────────────────────

use super::*;

/// A warning about Ty::Unknown surviving into the IR.
#[derive(Debug)]
pub struct UnknownTypeWarning {
    pub fn_name: String,
    pub span: Option<Span>,
    pub ty: Ty,
    pub context: &'static str,
}

/// Scan an IR program for any Ty::Unknown that survived lowering.
/// Returns a list of warnings (not errors) for diagnostic reporting.
pub fn collect_unknown_warnings(program: &IrProgram) -> Vec<UnknownTypeWarning> {
    let mut warnings = Vec::new();
    for f in &program.functions {
        check_expr_for_unknown(&f.body, &f.name, &mut warnings);
        for p in &f.params {
            if p.ty.contains_unknown() {
                warnings.push(UnknownTypeWarning {
                    fn_name: f.name.clone(),
                    span: None,
                    ty: p.ty.clone(),
                    context: "function parameter",
                });
            }
        }
        if f.ret_ty.contains_unknown() {
            warnings.push(UnknownTypeWarning {
                fn_name: f.name.clone(),
                span: None,
                ty: f.ret_ty.clone(),
                context: "function return type",
            });
        }
    }
    for tl in &program.top_lets {
        if tl.ty.contains_unknown() {
            warnings.push(UnknownTypeWarning {
                fn_name: "<top-level>".to_string(),
                span: None,
                ty: tl.ty.clone(),
                context: "top-level let binding",
            });
        }
    }
    warnings
}

fn check_expr_for_unknown(expr: &IrExpr, fn_name: &str, warnings: &mut Vec<UnknownTypeWarning>) {
    if expr.ty.contains_unknown() {
        warnings.push(UnknownTypeWarning {
            fn_name: fn_name.to_string(),
            span: expr.span,
            ty: expr.ty.clone(),
            context: "expression",
        });
        // Don't recurse into children — one warning per subtree is enough
        return;
    }
    // Recurse into children
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { check_stmt_for_unknown(s, fn_name, warnings); }
            if let Some(t) = tail { check_expr_for_unknown(t, fn_name, warnings); }
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Computed { callee } = target {
                check_expr_for_unknown(callee, fn_name, warnings);
            }
            if let CallTarget::Method { object, .. } = target {
                check_expr_for_unknown(object, fn_name, warnings);
            }
            for a in args { check_expr_for_unknown(a, fn_name, warnings); }
        }
        IrExprKind::If { cond, then, else_ } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            check_expr_for_unknown(then, fn_name, warnings);
            check_expr_for_unknown(else_, fn_name, warnings);
        }
        IrExprKind::BinOp { left, right, .. } => {
            check_expr_for_unknown(left, fn_name, warnings);
            check_expr_for_unknown(right, fn_name, warnings);
        }
        IrExprKind::UnOp { operand, .. } => {
            check_expr_for_unknown(operand, fn_name, warnings);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { check_expr_for_unknown(e, fn_name, warnings); }
        }
        IrExprKind::Lambda { body, .. } => {
            check_expr_for_unknown(body, fn_name, warnings);
        }
        IrExprKind::Match { subject, arms } => {
            check_expr_for_unknown(subject, fn_name, warnings);
            for a in arms { check_expr_for_unknown(&a.body, fn_name, warnings); }
        }
        IrExprKind::IndexAccess { object, index } => {
            check_expr_for_unknown(object, fn_name, warnings);
            check_expr_for_unknown(index, fn_name, warnings);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_expr_for_unknown(object, fn_name, warnings);
        }
        IrExprKind::Try { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Await { expr } => {
            check_expr_for_unknown(expr, fn_name, warnings);
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { check_expr_for_unknown(v, fn_name, warnings); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            check_expr_for_unknown(base, fn_name, warnings);
            for (_, v) in fields { check_expr_for_unknown(v, fn_name, warnings); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                check_expr_for_unknown(k, fn_name, warnings);
                check_expr_for_unknown(v, fn_name, warnings);
            }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                match p {
                    IrStringPart::Expr { expr } => check_expr_for_unknown(expr, fn_name, warnings),
                    IrStringPart::Lit { .. } => {}
                }
            }
        }
        IrExprKind::Range { start, end, .. } => {
            check_expr_for_unknown(start, fn_name, warnings);
            check_expr_for_unknown(end, fn_name, warnings);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            check_expr_for_unknown(iterable, fn_name, warnings);
            for s in body { check_stmt_for_unknown(s, fn_name, warnings); }
        }
        IrExprKind::While { cond, body } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            for s in body { check_stmt_for_unknown(s, fn_name, warnings); }
        }
        // Leaf nodes — no children
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::Break
        | IrExprKind::Continue | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
        // Codegen-specific nodes (not present in type-checked IR)
        IrExprKind::Clone { expr } | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::ToVec { expr } => {
            check_expr_for_unknown(expr, fn_name, warnings);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { check_expr_for_unknown(a, fn_name, warnings); }
        }
        IrExprKind::RenderedCall { .. } => {}
    }
}

fn check_stmt_for_unknown(stmt: &IrStmt, fn_name: &str, warnings: &mut Vec<UnknownTypeWarning>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            if ty.contains_unknown() {
                warnings.push(UnknownTypeWarning {
                    fn_name: fn_name.to_string(),
                    span: stmt.span,
                    ty: ty.clone(),
                    context: "let binding",
                });
            }
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::BindDestructure { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::Assign { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            check_expr_for_unknown(index, fn_name, warnings);
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            check_expr_for_unknown(value, fn_name, warnings);
        }
        IrStmtKind::Guard { cond, else_ } => {
            check_expr_for_unknown(cond, fn_name, warnings);
            check_expr_for_unknown(else_, fn_name, warnings);
        }
        IrStmtKind::Expr { expr } => {
            check_expr_for_unknown(expr, fn_name, warnings);
        }
        IrStmtKind::Comment { .. } => {}
    }
}
