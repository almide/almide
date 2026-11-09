//! Option builders reserve one payload slot, clear their length, then store
//! the selected payload before committing length one. Rust's Option carries
//! that presence in its variant; only these checked constructor stores vanish.
use super::*;

/// Only immutable scalar None constructors can be specialized at a typed
/// payload boundary. A mutable local or arbitrary Option value cannot.
pub(super) fn known_none(f: &MirFunction) -> BTreeSet<ValueId> {
    let mutable: BTreeSet<_> = f.ops.iter().filter_map(|op| match op {
        Op::SetLocal { local, .. } => Some(*local), _ => None,
    }).collect();
    let mut none = BTreeSet::new();
    for op in &f.ops {
        let dst = match op {
            Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(d), .. } => Some(*d),
            Op::Prim { kind: PrimKind::OptCopy, dst: Some(d), args }
                if args.len() == 1 && none.contains(&args[0]) => Some(*d),
            Op::Dup { dst, src } if none.contains(src) => Some(*dst),
            _ => None,
        };
        if let Some(d) = dst.filter(|d| !mutable.contains(d)) { none.insert(d); }
    }
    none
}

pub(super) fn sites(
    f: &MirFunction,
    kinds: &BTreeMap<ValueId, u16>,
    bases: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeMap<ValueId, bool> {
    let mut out = BTreeMap::new();
    let mut repeated = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for op in &f.ops {
        if let Op::Prim { kind: PrimKind::Store { .. }, args, .. } = op {
            if let Some(a) = args.first() { if !seen.insert(*a) { repeated.insert(*a); } }
        }
    }
    for (at, op) in f.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::Store { width: 4 }, args, .. } = op else { continue };
        let [address, value] = args.as_slice() else { continue };
        let Some(&(base, Some(off))) = bases.get(address) else { continue };
        if off != i64::from(BLOCK_LEN_OFFSET) || repeated.contains(address)
            || !kinds.get(&base).is_some_and(|i| crate::render_native_rec::option_payload(*i).is_some()) {
            continue;
        }
        let Some(&tag) = consts.get(value) else { continue };
        if !matches!(tag, 0 | 1) { continue; }
        let mut accepted = false;
        for prev in f.ops[..at].iter().rev() {
            match prev {
                Op::ConstInt { .. } | Op::IntBinOp { op: IntOp::Add, .. } => {}
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.as_slice() == [base] => {}
                Op::Consume { v } if tag == 1 && *v != base => {}
                Op::Alloc { dst, init: Init::DynList { len }, .. } if tag == 0 && *dst == base => {
                    accepted = consts.get(len) == Some(&1);
                    break;
                }
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if tag == 1 => {
                    accepted = args.first().and_then(|a| bases.get(a)) == Some(&(base, Some(i64::from(BLOCK_HEADER))));
                    break;
                }
                _ => break,
            }
        }
        if accepted { out.insert(*address, tag == 1); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn none_specialization_excludes_mutable_values_and_their_copies() {
        let v = ValueId;
        let f = MirFunction { ops: vec![
            Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(v(0)), args: vec![] },
            Op::Prim { kind: PrimKind::OptCopy, dst: Some(v(1)), args: vec![v(0)] },
            Op::Prim { kind: PrimKind::OptMakeNone, dst: Some(v(2)), args: vec![] },
            Op::Prim { kind: PrimKind::OptCopy, dst: Some(v(3)), args: vec![v(2)] },
            Op::SetLocal { local: v(2), src: v(4) },
        ], ..Default::default() };
        assert_eq!(known_none(&f), BTreeSet::from([v(0), v(1)]));
    }

    #[test]
    fn only_fresh_clear_and_adjacent_payload_commit_are_constructors() {
        let v = ValueId;
        let _guard = crate::render_native_rec::install(vec![crate::render_native_rec::RecDecl {
            name: "opt-str".into(), option_of: Some(NTy::Str), ..Default::default()
        }]);
        let clear = Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![v(2), v(3)] };
        let commit = Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![v(4), v(1)] };
        let mut f = MirFunction { ops: vec![
            Op::Alloc { dst: v(0), repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }, init: Init::DynList { len: v(1) } },
            clear.clone(),
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![v(5), v(6)] },
            commit.clone(),
        ], ..Default::default() };
        let kinds = BTreeMap::from([(v(0), 0)]);
        let bases = BTreeMap::from([(v(2), (v(0), Some(4))), (v(4), (v(0), Some(4))), (v(5), (v(0), Some(12)))]);
        let consts = BTreeMap::from([(v(1), 1), (v(3), 0)]);
        assert_eq!(sites(&f, &kinds, &bases, &consts), BTreeMap::from([(v(2), false), (v(4), true)]));
        f.ops.insert(3, Op::IfThen { cond: v(9), dst: None });
        assert!(!sites(&f, &kinds, &bases, &consts).contains_key(&v(4)));
        f.ops.insert(1, Op::Dup { dst: v(8), src: v(0) });
        assert!(sites(&f, &kinds, &bases, &consts).is_empty());
        f.ops.push(clear);
        assert!(sites(&f, &kinds, &bases, &consts).is_empty());
    }
}
