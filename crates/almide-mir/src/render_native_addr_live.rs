// Handle liveness for the symbolic native address algebra.
/// The `Handle`s of reassigned locals that could go stale: some address
/// derived from the handle (by an offset or `ElemAddr`) is read at or after a
/// `SetLocal` of the local, or a loop edge can carry such a reassignment to a read
/// of it. Every other handle of a reassigned local reads the one value the
/// local holds over the handle's whole live range, as its rendered variable does.
fn stale_handles(func: &MirFunction) -> BTreeSet<ValueId> {
    let set_at: Vec<(usize, ValueId)> = func
        .ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| match op {
            Op::SetLocal { local, .. } => Some((i, *local)),
            _ => None,
        })
        .collect();
    // Only loops that can reassign this base matter. A completed accumulator
    // loop before Handle cannot invalidate a handle used by a later read loop.
    let mut stack = Vec::new();
    let mut loops = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => stack.push(i),
            Op::LoopEnd => {
                if let Some(start) = stack.pop() {
                    loops.push((start, i));
                }
            }
            _ => {}
        }
    }
    let mut stale = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Some((d, x)) = handle_of(op) else {
            continue;
        };
        let d = &d;
        if !set_at.iter().any(|(_, l)| *l == x) {
            continue;
        }
        // The handle and every address derived from it, and where each is last read.
        let mut derived: BTreeSet<ValueId> = BTreeSet::from([*d]);
        let mut last = i;
        for (j, later) in func.ops.iter().enumerate().skip(i + 1) {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            if !reads.iter().any(|v| derived.contains(v)) {
                continue;
            }
            last = j;
            if let Op::IntBinOp {
                dst,
                op: IntOp::Add,
                ..
            }
            | Op::Prim {
                kind: PrimKind::ElemAddr,
                dst: Some(dst),
                ..
            } = later
            {
                derived.insert(*dst);
            }
        }
        let crosses = set_at.iter().any(|(k, l)| *l == x && *k > i && *k <= last)
            || loops.iter().any(|(start, end)| {
                ((*start >= i && *start <= last) || (*end >= i && *end <= last))
                    && set_at
                        .iter()
                        .any(|(k, l)| *l == x && *k > *start && *k < *end)
            });
        stale.extend(crosses.then_some(*d));
    }
    stale
}

#[cfg(test)]
mod live_tests {
    use super::*;
    fn handle() -> Op {
        Op::Prim {
            kind: PrimKind::Handle,
            dst: Some(ValueId(1)),
            args: vec![ValueId(0)],
        }
    }
    fn read() -> Op {
        Op::Prim {
            kind: PrimKind::Load { width: 4 },
            dst: Some(ValueId(2)),
            args: vec![ValueId(1)],
        }
    }
    fn assign() -> Op {
        Op::SetLocal {
            local: ValueId(0),
            src: ValueId(3),
        }
    }
    fn stale(ops: Vec<Op>) -> bool {
        stale_handles(&MirFunction {
            ops,
            ..Default::default()
        })
        .contains(&ValueId(1))
    }
    #[test]
    fn finished_accumulation_does_not_invalidate_a_later_handle() {
        assert!(!stale(vec![
            Op::LoopStart,
            assign(),
            Op::LoopEnd,
            handle(),
            Op::LoopStart,
            read(),
            Op::LoopEnd
        ]));
        assert!(!stale(vec![
            Op::LoopStart,
            assign(),
            handle(),
            Op::LoopStart,
            read(),
            Op::LoopEnd,
            Op::LoopEnd
        ]));
    }
    #[test]
    fn assignment_between_handle_and_use_still_refuses() {
        assert!(stale(vec![handle(), assign(), read()]));
        assert!(stale(vec![
            handle(),
            Op::LoopStart,
            assign(),
            read(),
            Op::LoopEnd
        ]));
    }
    #[test]
    fn assignment_after_read_can_reach_next_iteration() {
        assert!(stale(vec![
            handle(),
            Op::LoopStart,
            read(),
            assign(),
            Op::LoopEnd
        ]));
        assert!(stale(vec![
            handle(),
            Op::LoopStart,
            Op::LoopStart,
            read(),
            Op::LoopEnd,
            assign(),
            Op::LoopEnd
        ]));
        assert!(!stale(vec![
            Op::LoopStart,
            handle(),
            read(),
            assign(),
            Op::LoopEnd
        ]));
    }
}
