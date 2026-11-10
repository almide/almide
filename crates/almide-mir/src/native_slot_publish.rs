//! Native value cells publish a borrowed COW receiver after its plain mutator.
//! The linear-memory form publishes its pointer before mutating the pointee.
//! Moving only that store across pure argument preparation and the declared
//! runtime mutation preserves values: intervening reads see the same value,
//! and no callback, control edge or other write can observe an early update.
use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn delay(f: &mut MirFunction) -> Result<(), crate::lower::LowerError> {
    let mutable: BTreeSet<_> = f.ops.iter().filter_map(|o| match o {
        Op::SetLocal { local, .. } => Some(*local), _ => None,
    }).collect();
    let consts: BTreeMap<_, _> = f.ops.iter().filter_map(|o| match o {
        Op::ConstInt { dst, value } if !mutable.contains(dst) => Some((*dst, *value)), _ => None,
    }).collect();
    let mut reads = BTreeMap::new();
    let mut handles = BTreeMap::new();
    let mut move_to = BTreeMap::new();
    let mut occupied = BTreeSet::new();
    for (at, op) in f.ops.iter().enumerate() {
        match op {
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 => {
                if let Some(a) = super::slot_addr(&consts, &args[0]) {
                    reads.insert(*d, a);
                }
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => { handles.insert(*d, args[0]); }
            Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.len() == 2 => {
                let Some(&v) = handles.get(&args[1]) else { continue };
                if reads.get(&v).is_none() || reads.get(&v) != consts.get(&args[0]) { continue; }
                if let Some(end) = consumer(&f.ops, at, v) {
                    if (at..=end).all(|i| !occupied.contains(&i)) {
                        occupied.extend(at..=end);
                        move_to.insert(at, end);
                    }
                } else if f.ops[at + 1..].iter().any(|o| uses(o, v)) {
                    return Err(crate::render_native::wall("native: a published global borrow used beyond a plain mutation window"));
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::with_capacity(f.ops.len());
    for (at, op) in f.ops.iter().enumerate() {
        if !move_to.contains_key(&at) { out.push(op.clone()); }
        for (from, to) in &move_to {
            if *to == at { out.push(f.ops[*from].clone()); }
        }
    }
    f.ops = out;
    Ok(())
}

fn uses(op: &Op, v: ValueId) -> bool {
    let mut vs = Vec::new(); crate::mir_ops::op_values(op, &mut vs); vs.contains(&v)
}

fn consumer(ops: &[Op], at: usize, v: ValueId) -> Option<usize> {
    for (end, op) in ops.iter().enumerate().skip(at + 1) {
        match op {
            Op::CallFn { name, args, .. } => {
                if !super::QUIET.with(|q| q.borrow().as_ref().is_some_and(|q| q.call(name))) { return None; }
                let k = args.iter().position(|a| matches!(a, CallArg::Handle(x) if *x == v))?;
                if args.iter().filter(|a| matches!(a, CallArg::Handle(x) if *x == v)).count() != 1 { return None; }
                if !crate::native_hof::plain_mutator(name, k) { return None; }
                // A later use of this borrow would require a persistent place
                // representation. This small window only publishes its final use.
                if ops[end + 1..].iter().any(|o| uses(o, v)) { return None; }
                return Some(end);
            }
            Op::ConstInt { .. } | Op::IntBinOp { .. } | Op::Dup { .. }
            | Op::Prim { kind: PrimKind::LoadHandle | PrimKind::Load { .. } | PrimKind::Handle, .. }
                if !uses(op, v) => {}
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_cannot_cross_observers_control_edges_or_retained_aliases() {
        super::super::set_quiet(&[MirFunction { name: "main".into(), ..Default::default() }]);
        let v = ValueId(0);
        let call = Op::CallFn { dst: None, name: "bytes.copy_from".into(),
            args: vec![CallArg::Handle(v), CallArg::Handle(ValueId(1)), CallArg::Imm(0), CallArg::Imm(0), CallArg::Imm(1)], result: None };
        let start = Op::ConstInt { dst: ValueId(9), value: 0 };
        assert_eq!(consumer(&[start.clone(), call.clone()], 0, v), Some(1));
        for observer in [
            Op::CallFn { dst: None, name: "user_observer".into(), args: vec![], result: None },
            Op::IfThen { cond: ValueId(9), dst: None },
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![ValueId(9), ValueId(1)] },
            Op::Dup { dst: ValueId(8), src: v },
        ] {
            assert_eq!(consumer(&[start.clone(), observer, call.clone()], 0, v), None);
        }
        assert_eq!(consumer(&[start, call, Op::Dup { dst: ValueId(8), src: v }], 0, v), None);
        super::super::set_quiet(&[]);
    }
}
