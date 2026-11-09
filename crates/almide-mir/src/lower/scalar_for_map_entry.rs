// C-013: an undestructured map loop binds the whole entry, not its key.
// Build that checked tuple from the same key/value reads used by `(k, v)`.
// A tuple escaping the loop owns its fields through the usual MIR copies.
impl LowerCtx {
    fn native_map_entry_binding(&mut self, var: VarId, iterable: &IrExpr, body: &[IrStmt])
        -> Option<(Vec<VarId>, Vec<IrStmt>)> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Map, types) = &iterable.ty else { return None };
        if types.len() != 2 { return None; }
        let vars = vec![self.fresh_synth_var(), self.fresh_synth_var()];
        let elements = vars.iter().zip(types).map(|(v, ty)| IrExpr {
            kind: IrExprKind::Var { id: *v }, ty: ty.clone(), span: iterable.span, def_id: None,
        }).collect();
        let ty = Ty::Tuple(types.clone());
        let value = IrExpr { kind: IrExprKind::Tuple { elements }, ty: ty.clone(), span: iterable.span, def_id: None };
        let mut stmts = vec![IrStmt { kind: IrStmtKind::Bind {
            var, mutability: almide_ir::Mutability::Let, ty, value,
        }, span: iterable.span }];
        stmts.extend_from_slice(body);
        Some((vars, stmts))
    }
}
