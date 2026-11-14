// ── a straight-line reassignment remembers where its `var` was declared ──
// `var a = 0.0 … a = f(k)` outside a loop or arm binds `a` to a NEW value
// (a rebind, not a `SetLocal`). The value is the same source variable, so the
// native render declares its local where the `var` was declared: mutable
// locals then stand in source order, the order a backend sees them in when it
// lays out a loop's carried values. Render-only: no op is added or changed.

impl LowerCtx {
    pub(crate) fn lower_straight_rebind(&mut self, var: VarId, value: &IrExpr) -> Result<(), LowerError> {
        let before = self.value_of.get(&var).copied();
        self.lower_bind(var, &value.ty, value)?;
        let (Some(old), Some(new)) = (before, self.value_of.get(&var).copied()) else { return Ok(()) };
        if old != new && matches!(value.ty, Ty::Int | Ty::Float) {
            let origin = self.slot_origins.get(&old).copied().unwrap_or(old);
            self.slot_origins.insert(new, origin);
        }
        Ok(())
    }
}
