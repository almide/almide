//! A symbolic address formed before a copy still reads its original owner.
use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeSet;

pub(super) fn read_after(f: &MirFunction, at: usize, source: ValueId) -> bool {
    let mut addresses = BTreeSet::new();
    for op in &f.ops[..at] {
        match op {
            Op::Prim { kind: PrimKind::Handle | PrimKind::ElemAddr, dst: Some(d), args }
                if args.first().is_some_and(|v| *v == source || addresses.contains(v)) => { addresses.insert(*d); }
            Op::IntBinOp { dst, op: IntOp::Add, a, b }
                if addresses.contains(a) || addresses.contains(b) => { addresses.insert(*dst); }
            _ => {}
        }
    }
    if addresses.is_empty() { return false; }
    let mut reads = Vec::new();
    for op in &f.ops[at + 1..] {
        if super::bookkeeping(op) { continue; }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.iter().any(|v| addresses.contains(v)) { return true; }
    }
    f.ret.is_some_and(|v| addresses.contains(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_preexisting_slot_address_keeps_its_owner_live() {
        let mut f = MirFunction { ops: vec![
            Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(1)), args: vec![ValueId(0)] },
            Op::ConstInt { dst: ValueId(2), value: 12 },
            Op::IntBinOp { dst: ValueId(3), op: IntOp::Add, a: ValueId(1), b: ValueId(2) },
            Op::SetLocal { local: ValueId(4), src: ValueId(0) },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(ValueId(5)), args: vec![ValueId(3)] },
        ], ..Default::default() };
        assert!(read_after(&f, 3, ValueId(0)));
        assert!(!read_after(&f, 3, ValueId(8)));
        f.ops.truncate(4);
        assert!(!read_after(&f, 3, ValueId(0)));
    }
}
