// Included by calls.rs. A Bytes cell is owned storage, not a borrowed local.
impl LowerCtx {
    /// Snapshot the receiver into a normal owned binding, perform the existing
    /// checked mutator call, then publish through the existing cell assignment.
    /// The cell is not borrowed across argument evaluation or the call.
    fn try_cell_bytes_mutation(
        &mut self, module: &str, func: &str, args: &[IrExpr], result_ty: &Ty,
    ) -> Result<bool, LowerError> {
        if module != "bytes" || *result_ty != Ty::Unit || !is_inplace_mutator(module, func) {
            return Ok(false);
        }
        let Some(receiver) = args.first() else { return Ok(false) };
        let IrExprKind::Var { id } = receiver.kind else { return Ok(false) };
        let Some(cell) = self.cell_of.get(&id).copied() else { return Ok(false) };
        if receiver.ty != Ty::Bytes { return Ok(false); }
        let temporary = VarId(crate::lower::desugar_var_seed());
        self.lower_bind(temporary, &receiver.ty, receiver)?;
        let mut updated = args.to_vec();
        updated[0].kind = IrExprKind::Var { id: temporary };
        self.lower_effect_module_call(module, func, &updated, result_ty)?;
        self.lower_cell_assign(id, cell, &updated[0])?;
        Ok(true)
    }
}
