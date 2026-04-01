/// Shared Result expression analysis for both Rust and TS codegen.
///
/// Determines whether an IR expression produces a Result value (Ok/Err).
/// Used for Ok-wrapping in effect fn bodies and guard handling.
///
/// Key rule: `Try` (`?`) unwraps Result to T — it is NOT Result-producing.
/// This holds for both Rust (`?` operator) and TS (Result object unwrap).

use super::*;

/// Check if an IR expression produces a Result value (Ok/Err), including through
/// if/match/block where all branches are Result-producing.
pub fn is_ir_result_expr(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => true,
        IrExprKind::If { then, else_, .. } => is_ir_result_expr(then) && is_ir_result_expr(else_),
        IrExprKind::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| is_ir_result_expr(&a.body)),
        IrExprKind::Block { expr: Some(tail), .. } => is_ir_result_expr(tail),
        // Try unwraps Result to T — NOT Result-producing
        IrExprKind::Try { .. } => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_lang::types::Ty;

    fn mk(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Unknown, span: None }
    }

    fn mk_int(n: i64) -> IrExpr {
        mk(IrExprKind::LitInt { value: n })
    }

    #[test]
    fn result_ok_is_result() {
        assert!(is_ir_result_expr(&mk(IrExprKind::ResultOk { expr: Box::new(mk_int(1)) })));
    }

    #[test]
    fn result_err_is_result() {
        assert!(is_ir_result_expr(&mk(IrExprKind::ResultErr { expr: Box::new(mk_int(1)) })));
    }

    #[test]
    fn try_is_not_result() {
        let inner = mk(IrExprKind::ResultOk { expr: Box::new(mk_int(1)) });
        assert!(!is_ir_result_expr(&mk(IrExprKind::Try { expr: Box::new(inner) })));
    }

    #[test]
    fn plain_value_is_not_result() {
        assert!(!is_ir_result_expr(&mk_int(42)));
        assert!(!is_ir_result_expr(&mk(IrExprKind::Unit)));
    }

    #[test]
    fn if_both_branches_result() {
        let e = mk(IrExprKind::If {
            cond: Box::new(mk(IrExprKind::LitBool { value: true })),
            then: Box::new(mk(IrExprKind::ResultOk { expr: Box::new(mk_int(1)) })),
            else_: Box::new(mk(IrExprKind::ResultErr { expr: Box::new(mk_int(0)) })),
        });
        assert!(is_ir_result_expr(&e));
    }

    #[test]
    fn if_one_branch_not_result() {
        let e = mk(IrExprKind::If {
            cond: Box::new(mk(IrExprKind::LitBool { value: true })),
            then: Box::new(mk(IrExprKind::ResultOk { expr: Box::new(mk_int(1)) })),
            else_: Box::new(mk_int(0)),
        });
        assert!(!is_ir_result_expr(&e));
    }

    #[test]
    fn block_with_result_tail() {
        let e = mk(IrExprKind::Block {
            stmts: vec![],
            expr: Some(Box::new(mk(IrExprKind::ResultOk { expr: Box::new(mk_int(1)) }))),
        });
        assert!(is_ir_result_expr(&e));
    }

    #[test]
    fn block_with_plain_tail() {
        let e = mk(IrExprKind::Block {
            stmts: vec![],
            expr: Some(Box::new(mk_int(1))),
        });
        assert!(!is_ir_result_expr(&e));
    }
}
