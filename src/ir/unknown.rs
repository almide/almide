// ── Unknown type detection (post-pass) ──────────────────────────

use super::*;
use super::visit::{IrVisitor, walk_expr, walk_stmt};

/// A warning about Ty::Unknown surviving into the IR.
#[derive(Debug)]
pub struct UnknownTypeWarning {
    pub fn_name: String,
    pub span: Option<Span>,
    pub ty: Ty,
    pub context: &'static str,
}

struct UnknownChecker {
    fn_name: String,
    warnings: Vec<UnknownTypeWarning>,
}

impl IrVisitor for UnknownChecker {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if expr.ty.contains_unknown() {
            self.warnings.push(UnknownTypeWarning {
                fn_name: self.fn_name.clone(),
                span: expr.span,
                ty: expr.ty.clone(),
                context: "expression",
            });
            // Don't recurse into children — one warning per subtree is enough
            return;
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &stmt.kind {
            if ty.contains_unknown() {
                self.warnings.push(UnknownTypeWarning {
                    fn_name: self.fn_name.clone(),
                    span: stmt.span,
                    ty: ty.clone(),
                    context: "let binding",
                });
            }
        }
        walk_stmt(self, stmt);
    }
}

/// Scan an IR program for any Ty::Unknown that survived lowering.
/// Returns a list of warnings (not errors) for diagnostic reporting.
pub fn collect_unknown_warnings(program: &IrProgram) -> Vec<UnknownTypeWarning> {
    let mut warnings = Vec::new();
    for f in &program.functions {
        let mut checker = UnknownChecker {
            fn_name: f.name.to_string(),
            warnings: Vec::new(),
        };
        checker.visit_expr(&f.body);
        warnings.append(&mut checker.warnings);

        for p in &f.params {
            if p.ty.contains_unknown() {
                warnings.push(UnknownTypeWarning {
                    fn_name: f.name.to_string(),
                    span: None,
                    ty: p.ty.clone(),
                    context: "function parameter",
                });
            }
        }
        if f.ret_ty.contains_unknown() {
            warnings.push(UnknownTypeWarning {
                fn_name: f.name.to_string(),
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
