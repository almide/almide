//! Interpret a fresh Value's Null initialization, never an arbitrary header
//! mutation. The declared type distinguishes its tag from a list's length.

use super::*;

/// The contiguous allocation/initialization window has no intervening use or
/// branch. All operands must match, and the allocation must be declared Value.
/// Its native default is Null (render_native_rec), so only this first tag store
/// can be erased. Ownership, call and bounds verification still see every op.
pub(super) fn null_initializers(func: &MirFunction, recs: &BTreeMap<ValueId, u16>) -> BTreeSet<ValueId> {
    func.ops.windows(7).filter_map(|ops| {
        let [
            Op::ConstInt { dst: count, value: 1 },
            Op::Alloc { dst: block, init: Init::DynList { len }, .. },
            Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: handle_args },
            Op::ConstInt { dst: offset, value: off },
            Op::IntBinOp { dst: address, op: IntOp::Add, a, b },
            Op::ConstInt { dst: zero, value: 0 },
            Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args },
        ] = ops else { return None };
        (len == count && handle_args == &[*block] && *off == i64::from(BLOCK_LEN_OFFSET)
            && a == handle && b == offset && args == &[*address, *zero]
            && recs.get(block).is_some_and(|i| crate::render_native_rec::is_value(NTy::Rec(*i))))
            .then_some(*address)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_native::{render_native_items, NativeSigKind as S};
    use crate::{MirProgram, Repr, PLACEHOLDER_LAYOUT};

    fn constructor() -> MirFunction {
        let v = ValueId;
        MirFunction {
            name: "fresh".into(),
            ops: vec![
                Op::ConstInt { dst: v(0), value: 1 },
                Op::Alloc { dst: v(1), repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::DynList { len: v(0) } },
                Op::Prim { kind: PrimKind::Handle, dst: Some(v(2)), args: vec![v(1)] },
                Op::ConstInt { dst: v(3), value: i64::from(BLOCK_LEN_OFFSET) },
                Op::IntBinOp { dst: v(4), op: IntOp::Add, a: v(2), b: v(3) },
                Op::ConstInt { dst: v(5), value: 0 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![v(4), v(5)] },
            ],
            ret: Some(v(1)),
            ..Default::default()
        }
    }

    fn render(f: MirFunction, ret: S) -> Result<String, LowerError> {
        let sigs = [(f.name.clone(), (vec![], Some(ret)))].into();
        render_native_items(&MirProgram { functions: vec![f], ..Default::default() }, &sigs)
    }

    #[test]
    fn only_the_fresh_declared_null_tag_is_accepted() {
        let _guard = crate::render_native_rec::install(vec![]);
        let value = S::Rec(crate::render_native_rec::value_index());
        assert!(render(constructor(), value).is_ok());
        // Identical bytes on a list describe a length mutation, not a Null.
        assert!(render(constructor(), S::ListI64).unwrap_err().to_string().contains("Store{4}"));
        // Neither another tag nor an unexpected block size is a Null init.
        for (at, n) in [(5, 1), (0, 2), (3, 8)] {
            let mut f = constructor();
            let Op::ConstInt { value: literal, .. } = &mut f.ops[at] else { unreachable!() };
            *literal = n;
            assert!(render(f, value).is_err());
        }
        // Even another zero written through the same address is not the
        // initialization. A changed tag after it must not be swallowed either.
        for tag in [0, 1] {
            let mut f = constructor();
            f.ops.push(Op::ConstInt { dst: ValueId(6), value: tag });
            f.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(4), ValueId(6)] });
            assert!(render(f, value).unwrap_err().to_string().contains("Store{4}"));
        }
    }
}
