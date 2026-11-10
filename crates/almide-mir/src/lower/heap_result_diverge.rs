impl LowerCtx {
    /// Some(Some(value)) yields an owned value; Some(None) ends the process;
    /// None is a refusal. No object or ownership credit is invented for an
    /// aborting branch. The native gate keeps other consumers unchanged.
    pub(crate) fn lower_heap_result_branch(&mut self, arm: &IrExpr, ty: &Ty)
        -> Option<Option<ValueId>>
    {
        if !crate::lower::owned_tco_slots() || !almide_ir::diverge::diverges(arm) {
            return self.lower_heap_result_arm(arm, ty).map(Some);
        }
        self.speculate(|ctx| {
            let live = ctx.live_heap_handles.clone();
            let bindings = ctx.value_of.clone();
            ctx.in_frame += 1;
            ctx.unit_arm_depth += 1;
            let result = ctx.lower_tail(Some(arm));
            ctx.unit_arm_depth -= 1;
            ctx.in_frame -= 1;
            let ended = matches!(result, Ok(None)) && matches!(ctx.ops.last(), Some(Op::Prim {
                kind: crate::PrimKind::Die | crate::PrimKind::ProcExit, dst: None, ..
            }));
            // Locals created on the ending path cannot escape into its sibling.
            // Their live references at process termination are covered by t.
            ctx.live_heap_handles = live;
            ctx.value_of = bindings;
            ended.then_some(None)
        })
    }
}
