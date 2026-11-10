// Handle liveness for the symbolic native address algebra.
/// The `Handle`s of reassigned locals that could go stale: some address
/// derived from the handle (by an offset or `ElemAddr`) is read at or after a
/// `SetLocal` of the local, or a loop edge can carry such a reassignment to a read
/// of it. Every other handle of a reassigned local reads the one value the
/// local holds over the handle's whole live range, as its rendered variable does.
fn stale_handles(func: &MirFunction) -> BTreeSet<ValueId> {
    let paths = address_branch_paths(&func.ops);
    let compatible = |a: usize, b: usize| paths[a].iter().all(|(id, side)|
        paths[b].iter().all(|(other, branch)| id != other || side == branch));
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
        let mut reads_at = Vec::new();
        for (j, later) in func.ops.iter().enumerate().skip(i + 1) {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            if !reads.iter().any(|v| derived.contains(v)) {
                continue;
            }
            last = j;
            reads_at.push(j);
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
        let crosses = set_at.iter().any(|(k, l)| *l == x && *k > i
            && compatible(i, *k)
            && reads_at.iter().any(|j| *k <= *j && compatible(*k, *j)))
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

/// Else's value is read in the then arm; EndIf's value in the else arm.
/// Keep their operand paths before switching/closing the branch. A write in
/// one arm cannot invalidate a handle used only by its sibling in the same
/// iteration. Loop-edge checks stay conservative across iterations.
fn address_branch_paths(ops: &[Op]) -> Vec<Vec<(usize, bool)>> {
    let mut active = Vec::new();
    let mut paths = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        paths.push(active.clone());
        match op {
            Op::IfThen { .. } => active.push((i, true)),
            Op::Else { .. } => { if let Some((_, side)) = active.last_mut() { *side = false; } }
            Op::EndIf { .. } => { active.pop(); }
            _ => {}
        }
    }
    paths
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
    fn sibling_branch_writes_do_not_reach_each_others_reads() {
        let branch = || Op::IfThen { cond: ValueId(9), dst: None };
        assert!(!stale(vec![handle(), branch(), assign(), Op::Else { val: None }, read(), Op::EndIf { val: None }]));
        assert!(stale(vec![handle(), branch(), assign(), Op::Else { val: None }, read(), Op::EndIf { val: None }, read()]));
        assert!(stale(vec![handle(), branch(), assign(), read(), Op::Else { val: None }, Op::EndIf { val: None }]));
        // Re-entering the loop may select a different arm. A handle taken
        // before the loop must retain the conservative back-edge rejection.
        assert!(stale(vec![handle(), Op::LoopStart, branch(), assign(), Op::Else { val: None }, read(), Op::EndIf { val: None }, Op::LoopEnd]));
        assert!(!stale(vec![Op::LoopStart, handle(), branch(), assign(), Op::Else { val: None }, read(), Op::EndIf { val: None }, Op::LoopEnd]));
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
