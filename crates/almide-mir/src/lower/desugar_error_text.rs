/// A typed error falling into a `String` channel (ADR-0021 D2, #2725): the text
/// `"${e}"` shows — its repr — as a one-part interpolation, so the lowering's own
/// Display desugar (and the corpus call counter, which reads the same node) renders it.
fn erased_err_text(err: IrExpr) -> IrExpr {
    if let Some(formatted) = crate::native_display::erased_error(&err) { return formatted; }
    IrExpr {
        span: err.span,
        kind: IrExprKind::StringInterp { parts: vec![almide_ir::IrStringPart::Expr { expr: err }] },
        ty: Ty::String,
        def_id: None,
    }
}

