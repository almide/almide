// A comparison reads two Result payloads only when BOTH selected tags hold.
// Native carrier reads require an explicit guard for each owner. Nested
// branches retain those facts without guessing a payload's type or value.
impl LowerCtx {
    fn begin_tag_pair_gate(&mut self, combined: ValueId, left: ValueId, right: ValueId)
        -> (ValueId, Option<ValueId>) {
        let outer = self.fresh_value();
        if crate::lower::owned_tco_slots() {
            self.ops.push(Op::IfThen { cond: left, dst: Some(outer) });
            let inner = self.fresh_value();
            self.ops.push(Op::IfThen { cond: right, dst: Some(inner) });
            (outer, Some(inner))
        } else {
            self.ops.push(Op::IfThen { cond: combined, dst: Some(outer) });
            (outer, None)
        }
    }

    fn end_tag_pair_gate(&mut self, gate: (ValueId, Option<ValueId>), value: ValueId) {
        self.end_bool_gate(value);
        if let Some(inner) = gate.1 { self.end_bool_gate(inner); }
    }

    fn end_bool_gate(&mut self, value: ValueId) {
        self.ops.push(Op::Else { val: Some(value) });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::EndIf { val: Some(zero) });
    }
}
