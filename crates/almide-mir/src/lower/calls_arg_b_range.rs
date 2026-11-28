// ── part of calls_arg_b.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    /// Materialize the Int range `s..e` (or `s..=e`) as an owned list (#4355).
    ///
    /// Exclusive: `list.range(s, e)`. Inclusive: NOT `list.range(s, e + 1)`,
    /// which wraps to an empty list at `e == Int max`. The count comes from
    /// [`Self::int_range_count`] — saturated to Int max when the true span does
    /// not fit (so the allocation aborts out of memory, as an exclusive range
    /// that large does) — `list.range(0, count)` allocates it, and a fill loop
    /// writes `s + k` into slot `k`.
    pub(crate) fn materialize_int_range(&mut self, s: ValueId, e: ValueId, inclusive: bool, repr: crate::Repr) -> ValueId {
        let konst = |me: &mut Self, value: i64| {
            let v = me.fresh_value();
            me.ops.push(Op::ConstInt { dst: v, value });
            v
        };
        let bin = |me: &mut Self, op: crate::IntOp, a: ValueId, b: ValueId| {
            let v = me.fresh_value();
            me.ops.push(Op::IntBinOp { dst: v, op, a, b });
            v
        };
        let dst = self.fresh_value();
        if !inclusive {
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: "list.range".to_string(),
                args: vec![CallArg::Scalar(s), CallArg::Scalar(e)],
                result: Some(repr),
            });
            return dst;
        }
        let (zero, one, count) = self.int_range_count(s, e, true);
        let one = one.expect("an inclusive count makes the constant 1");
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "list.range".to_string(),
            args: vec![CallArg::Scalar(zero), CallArg::Scalar(count)],
            result: Some(repr),
        });
        // Fill: slot k holds s + k.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![dst] });
        let k = konst(self, 0);
        let slot = konst(self, i64::from(crate::lower::layout::SLOT_SIZE));
        let header = konst(self, i64::from(crate::lower::layout::BLOCK_HEADER));
        self.ops.push(Op::LoopStart);
        let more = bin(self, crate::IntOp::Lt, k, count);
        self.ops.push(Op::LoopBreakUnless { cond: more });
        let off = bin(self, crate::IntOp::Mul, k, slot);
        let base = bin(self, crate::IntOp::Add, h, header);
        let at = bin(self, crate::IntOp::Add, base, off);
        let val = bin(self, crate::IntOp::Add, s, k);
        self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![at, val] });
        let k1 = bin(self, crate::IntOp::Add, k, one);
        self.ops.push(Op::SetLocal { local: k, src: k1 });
        self.ops.push(Op::LoopEnd);
        dst
    }

    /// The element count of the Int range `s..e` (or `s..=e`), never wrapped:
    /// `e - s` (`+ 1` inclusive) when the range is not empty, saturated to Int
    /// max when that true count does not fit in an Int, and 0 when it is empty.
    /// `0...i64::MAX` holds 2^63 elements and `i64::MIN..<i64::MAX` 2^64 - 1;
    /// computed by plain subtraction they wrapped negative and read as empty
    /// (#4538). This count sizes a list, and saturating loses nothing there: a
    /// list that long cannot be allocated, so it aborts out of memory before
    /// any element is read. A loop that only walks the range takes
    /// [`Self::int_range_count_unsigned`] instead. Returns `(zero, one, count)` — the constants
    /// callers reuse (`one` only for an inclusive range).
    pub(crate) fn int_range_count(&mut self, s: ValueId, e: ValueId, inclusive: bool) -> (ValueId, Option<ValueId>, ValueId) {
        let konst = |me: &mut Self, value: i64| {
            let v = me.fresh_value();
            me.ops.push(Op::ConstInt { dst: v, value });
            v
        };
        let bin = |me: &mut Self, op: crate::IntOp, a: ValueId, b: ValueId| {
            let v = me.fresh_value();
            me.ops.push(Op::IntBinOp { dst: v, op, a, b });
            v
        };
        let zero = konst(self, 0);
        let one = inclusive.then(|| konst(self, 1));
        let max = konst(self, i64::MAX);
        let ok = bin(self, if inclusive { crate::IntOp::Ge } else { crate::IntOp::Gt }, e, s);
        let d = bin(self, crate::IntOp::Sub, e, s);
        // The true span e - s overflowed iff e and s differ in sign and d's
        // sign differs from e's.
        let x1 = bin(self, crate::IntOp::Xor, e, s);
        let x2 = bin(self, crate::IntOp::Xor, e, d);
        let x3 = bin(self, crate::IntOp::And, x1, x2);
        let ovf = bin(self, crate::IntOp::Lt, x3, zero);
        // Inclusive also saturates a span of exactly Int max, whose +1 wraps.
        let (sat, fit) = match one {
            Some(one) => {
                let dmax = bin(self, crate::IntOp::Eq, d, max);
                let sat = bin(self, crate::IntOp::Or, ovf, dmax);
                (sat, bin(self, crate::IntOp::Add, d, one))
            }
            None => (ovf, d),
        };
        let span = self.fresh_value();
        self.ops.push(Op::IfThen { cond: sat, dst: Some(span) });
        self.ops.push(Op::Else { val: Some(max) });
        self.ops.push(Op::EndIf { val: Some(fit) });
        let count = self.fresh_value();
        self.ops.push(Op::IfThen { cond: ok, dst: Some(count) });
        self.ops.push(Op::Else { val: Some(span) });
        self.ops.push(Op::EndIf { val: Some(zero) });
        (zero, one, count)
    }

    /// The element count of the Int range `s..e` (or `s..=e`) as an UNSIGNED
    /// 64-bit value in an i64 slot, for a loop that compares it with `LtU`:
    /// `e - s` (`+ 1` inclusive) wrapping, which is exact modulo 2^64 and so
    /// exact for every exclusive range (at most 2^64 - 1 elements), and 0 when
    /// the range is empty. The one count that does not fit, the 2^64 of
    /// `i64::MIN...i64::MAX`, stays at 2^64 - 1 rather than wrapping to 0.
    pub(crate) fn int_range_count_unsigned(&mut self, s: ValueId, e: ValueId, inclusive: bool) -> ValueId {
        let konst = |me: &mut Self, value: i64| {
            let v = me.fresh_value();
            me.ops.push(Op::ConstInt { dst: v, value });
            v
        };
        let bin = |me: &mut Self, op: crate::IntOp, a: ValueId, b: ValueId| {
            let v = me.fresh_value();
            me.ops.push(Op::IntBinOp { dst: v, op, a, b });
            v
        };
        let zero = konst(self, 0);
        let ok = bin(self, if inclusive { crate::IntOp::Ge } else { crate::IntOp::Gt }, e, s);
        let d = bin(self, crate::IntOp::Sub, e, s);
        let span = if inclusive {
            let one = konst(self, 1);
            let all = konst(self, -1);
            let full = bin(self, crate::IntOp::Eq, d, all);
            let d1 = bin(self, crate::IntOp::Add, d, one);
            let span = self.fresh_value();
            self.ops.push(Op::IfThen { cond: full, dst: Some(span) });
            self.ops.push(Op::Else { val: Some(d) });
            self.ops.push(Op::EndIf { val: Some(d1) });
            span
        } else {
            d
        };
        let count = self.fresh_value();
        self.ops.push(Op::IfThen { cond: ok, dst: Some(count) });
        self.ops.push(Op::Else { val: Some(span) });
        self.ops.push(Op::EndIf { val: Some(zero) });
        count
    }
}
