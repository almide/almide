/// DISCARD AN EFFECT MAIN'S OK PAYLOAD (#2885, a pre-lowering program pass,
/// desugar-before-both). An `effect fn main` may declare any Ok type. The
/// entry wrapper unwraps the carrier and throws the payload away (check's E044
/// rule), and native and the structural leg do exactly that. This leg's
/// `main` convention is void (`_start` calls it and reads nothing), so an
/// `effect fn main() -> Int` reached the renderer returning an i64 into a
/// void `_start` (invalid wasm). A record-typed one made the value into the
/// die line (`Error: ` on stderr, exit 1).
///
/// The rewrite states the discard in the IR: the body's tail is bound to an unused local and
/// `main` returns Unit, so this leg lowers the same void `main` it lowers for
/// `-> Unit`. The value is dropped at the end of `main` like any other
/// unused local. A `main` that declares a `Result` or an `Option` is
/// not touched: it is a real fallible return, and the renderer already reads
/// it that way.
pub fn discard_effect_main_payload(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability};
    use almide_lang::types::constructor::TypeConstructorId;
    let almide_ir::IrProgram { functions, var_table, .. } = program;
    for f in functions.iter_mut() {
        if f.name.as_str() != "main" || !f.is_effect {
            continue;
        }
        if matches!(
            f.ret_ty,
            Ty::Unit | Ty::Applied(TypeConstructorId::Result | TypeConstructorId::Option, _)
        ) {
            continue;
        }
        let span = f.body.span;
        let body = std::mem::replace(
            &mut f.body,
            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span, def_id: None },
        );
        // The body's own statements stay at the fn-body level, where the
        // main-only desugars (a `let x = f()!` die line) recognise them; only
        // the tail becomes a statement.
        let (mut stmts, tail) = match body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.map(|t| *t)),
            kind => (Vec::new(), Some(IrExpr { kind, ty: body.ty, span: body.span, def_id: body.def_id })),
        };
        // Bound, not a bare statement: this leg lowers an expression
        // statement only when it is a call.
        if let Some(tail) = tail {
            let tail_span = tail.span;
            let var = var_table.alloc(almide_base::intern::sym("__main_payload"), tail.ty.clone(), Mutability::Let, tail_span);
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: tail.ty.clone(), value: tail },
                span: tail_span,
            });
        }
        f.body = IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span, def_id: None };
        f.ret_ty = Ty::Unit;
    }
}
