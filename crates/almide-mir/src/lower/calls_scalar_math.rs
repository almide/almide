// Scalar operators whose implementation is a pure library call. The call
// and numeric conversions remain explicit MIR operations on both targets.
impl LowerCtx {
    fn lower_scalar_binop_pow(
        &mut self, expr: &IrExpr, op: &almide_ir::BinOp, left: &IrExpr, right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        let callee = match op {
            BinOp::PowFloat => "math.__fpow",
            BinOp::ModFloat => "float.fmod",
            BinOp::PowInt => "int.pow",
            _ => return None,
        };
        let mut a = self.lower_scalar_value(left)?;
        let mut b = self.lower_scalar_value(right)?;
        let float = !matches!(op, BinOp::PowInt);
        // Float32 lives as low binary32 bits, while these library functions
        // take Float. Promote each input and round the result exactly once.
        if float && matches!(left.ty, Ty::Float32) {
            a = self.scalar_math_conversion(crate::PrimKind::F32Promote, a);
        }
        if float && matches!(right.ty, Ty::Float32) {
            b = self.scalar_math_conversion(crate::PrimKind::F32Promote, b);
        }
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst), name: callee.into(),
            args: vec![CallArg::Scalar(a), CallArg::Scalar(b)],
            result: Some(repr_of(if float { &Ty::Float } else { &expr.ty }).ok()?),
        });
        Some(if !float {
            self.wrap_to_declared_width(dst, &expr.ty)
        } else if matches!(expr.ty, Ty::Float32) {
            self.scalar_math_conversion(crate::PrimKind::F32Demote, dst)
        } else { dst })
    }

    fn scalar_math_conversion(&mut self, kind: crate::PrimKind, value: ValueId) -> ValueId {
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![value] });
        dst
    }
}
