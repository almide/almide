// Set equality uses the same declared runtime operations as the typed
// stdlib body: equal cardinality and containment. The source-type admission
// excludes float lanes and other representations with different equality.
impl LowerCtx {
    fn native_set_content_eq(&mut self, left: ValueId, right: ValueId) -> Option<ValueId> {
        let mut lengths = Vec::new();
        for value in [left, right] {
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst), name: "set.len".into(),
                args: vec![self.eq_handle_arg(value)], result: Some(repr_of(&Ty::Int).ok()?),
            });
            lengths.push(dst);
        }
        let same_len = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: same_len, op: IntOp::Eq, a: lengths[0], b: lengths[1] });
        let subset = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(subset), name: "set.is_subset".into(),
            args: vec![self.eq_handle_arg(left), self.eq_handle_arg(right)],
            result: Some(repr_of(&Ty::Bool).ok()?),
        });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::And, a: same_len, b: subset });
        Some(dst)
    }
}
