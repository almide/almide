//! A fresh list builder may commit its filtered prefix after a bounded loop.
//! Recognize the counter argument structurally, never by a stdlib function name.
//! Arbitrary header writes, growth and aliases are outside this rule.
use crate::{Init, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn finalizers(f: &MirFunction) -> BTreeMap<ValueId, ValueId> {
    let facts = Facts::of(&f.ops);
    f.ops
        .iter()
        .enumerate()
        .filter_map(|(at, op)| {
            let Op::Prim {
                kind: PrimKind::Store { width: 4 },
                args,
                ..
            } = op
            else {
                return None;
            };
            let [address, count] = args.as_slice() else {
                return None;
            };
            let base = facts.length_base(*address)?;
            facts
                .valid(&f.ops, at, base, *count)
                .then_some((*address, base))
        })
        .collect()
}

#[derive(Default)]
struct Facts {
    constants: BTreeMap<ValueId, (usize, i64)>,
    handles: BTreeMap<ValueId, ValueId>,
    sums: BTreeMap<ValueId, (usize, ValueId, ValueId)>,
    less: BTreeMap<ValueId, (usize, ValueId, ValueId)>,
    mutable: BTreeSet<ValueId>,
    allocated: BTreeMap<ValueId, (usize, ValueId)>,
}

impl Facts {
    fn of(ops: &[Op]) -> Self {
        let mut out = Self::default();
        for (i, op) in ops.iter().enumerate() {
            match op {
                Op::ConstInt { dst, value } => {
                    out.constants.insert(*dst, (i, *value));
                }
                Op::Prim {
                    kind: PrimKind::Handle,
                    dst: Some(dst),
                    args,
                } if args.len() == 1 => {
                    out.handles.insert(*dst, args[0]);
                }
                Op::IntBinOp {
                    dst,
                    op: IntOp::Add,
                    a,
                    b,
                } => {
                    out.sums.insert(*dst, (i, *a, *b));
                }
                Op::IntBinOp {
                    dst,
                    op: IntOp::Lt,
                    a,
                    b,
                } => {
                    out.less.insert(*dst, (i, *a, *b));
                }
                Op::Alloc {
                    dst,
                    init: Init::DynList { len } | Init::DynListStr { len },
                    ..
                } => {
                    out.allocated.insert(*dst, (i, *len));
                }
                Op::SetLocal { local, .. } => {
                    out.mutable.insert(*local);
                }
                _ => {}
            }
        }
        out
    }

    fn plus(&self, value: ValueId, constant: i64) -> Option<ValueId> {
        if self.mutable.contains(&value) {
            return None;
        }
        let &(_, a, b) = self.sums.get(&value)?;
        [(a, b), (b, a)].into_iter().find_map(|(v, c)| {
            self.constants
                .get(&c)
                .filter(|(_, n)| *n == constant && !self.mutable.contains(&c))
                .map(|_| v)
        })
    }

    fn length_base(&self, address: ValueId) -> Option<ValueId> {
        let handle = self.plus(address, i64::from(crate::lower::layout::BLOCK_LEN_OFFSET))?;
        self.handles
            .get(&handle)
            .filter(|base| !self.mutable.contains(&handle) && !self.mutable.contains(base))
            .copied()
    }

    fn valid(&self, ops: &[Op], store: usize, base: ValueId, count: ValueId) -> bool {
        let Some(&(alloc, len)) = self.allocated.get(&base) else {
            return false;
        };
        // Only address construction may separate the loop exit and commitment.
        let Some(end) = (alloc + 1..store).rev().find(|&i| {
            !matches!(
                ops[i],
                Op::ConstInt { .. }
                    | Op::IntBinOp { op: IntOp::Add, .. }
                    | Op::Prim {
                        kind: PrimKind::Handle,
                        ..
                    }
            )
        }) else {
            return false;
        };
        if !matches!(ops[end], Op::LoopEnd) {
            return false;
        }
        let Some(start) = (alloc + 1..end)
            .rev()
            .find(|&i| matches!(ops[i], Op::LoopStart))
        else {
            return false;
        };
        // No nested loop: both counters advance at most once per iteration.
        if ops[start + 1..end]
            .iter()
            .any(|op| matches!(op, Op::LoopStart | Op::LoopEnd))
        {
            return false;
        }
        let mut depth = 0;
        let mut exit = None;
        for (i, op) in ops.iter().enumerate().take(end).skip(start + 1) {
            match op {
                Op::IfThen { .. } => depth += 1,
                Op::EndIf { .. } => depth -= 1,
                Op::LoopBreakUnless { cond } if depth == 0 && exit.is_none() => {
                    exit = Some((i, *cond))
                }
                Op::LoopBreakUnless { .. } | Op::Return { .. } => return false,
                _ => {}
            }
        }
        let Some((test, cond)) = exit else {
            return false;
        };
        if ops[start + 1..test]
            .iter()
            .any(|op| !matches!(op, Op::ConstInt { .. } | Op::IntBinOp { .. }))
        {
            return false;
        }
        let Some(&(comparison, index, bound)) = self.less.get(&cond) else {
            return false;
        };
        if comparison <= start
            || comparison >= test
            || self.mutable.contains(&cond)
            || bound != len
            || index == count
        {
            return false;
        }
        if !self.counter(ops, index, start, test, end, true)
            || !self.counter(ops, count, start, test, end, false)
        {
            return false;
        }
        if ops
            .iter()
            .any(|op| matches!(op, Op::SetLocal { local, .. } if *local == len))
        {
            return false;
        }
        // The fresh result cannot escape or acquire aliases before commitment.
        for op in &ops[alloc + 1..store] {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(op, &mut reads);
            if reads.contains(&base)
                && !matches!(
                    op,
                    Op::Prim {
                        kind: PrimKind::Handle,
                        ..
                    }
                )
            {
                return false;
            }
        }
        // A second length write could expose values a Rust truncate dropped.
        ops.iter()
            .filter(|op| {
                matches!(op,
            Op::Prim { kind: PrimKind::Store { width: 4 }, args, .. }
            if args.first().and_then(|a| self.length_base(*a)) == Some(base))
            })
            .count()
            == 1
    }

    fn counter(
        &self,
        ops: &[Op],
        value: ValueId,
        start: usize,
        test: usize,
        end: usize,
        unconditional: bool,
    ) -> bool {
        if !self
            .constants
            .get(&value)
            .is_some_and(|&(at, n)| at < start && n == 0)
        {
            return false;
        }
        let mut depth = 0;
        let mut updates = 0;
        for (i, op) in ops.iter().enumerate() {
            if i == start {
                depth = 0;
            }
            match op {
                Op::IfThen { .. } if i > start && i < end => depth += 1,
                Op::EndIf { .. } if i > start && i < end => depth -= 1,
                Op::SetLocal { local, src } if *local == value => {
                    if i <= test
                        || i >= end
                        || self.plus(*src, 1) != Some(value)
                        || !self
                            .sums
                            .get(src)
                            .is_some_and(|&(at, _, _)| at > test && at + 1 == i)
                        || (unconditional && depth != 0)
                    {
                        return false;
                    }
                    updates += 1;
                }
                _ => {}
            }
        }
        if unconditional { updates == 1 } else { Self::max_updates(&ops[start + 1..end], value) == Some(1) }
    }

    /// Alternative filter arms may each increment the write cursor. Count
    /// along execution paths: mutually exclusive arms take their maximum,
    /// while successive conditions add. More than one increment still refuses.
    fn max_updates(ops: &[Op], value: ValueId) -> Option<usize> {
        let mut current = 0;
        let mut stack = Vec::new();
        for op in ops {
            match op {
                Op::IfThen { .. } => { stack.push((current, None)); current = 0; }
                Op::Else { .. } => { stack.last_mut()?.1 = Some(current); current = 0; }
                Op::EndIf { .. } => {
                    let (before, then) = stack.pop()?;
                    current = before + current.max(then.unwrap_or(0));
                }
                Op::SetLocal { local, .. } if *local == value => current += 1,
                _ => {}
            }
        }
        stack.is_empty().then_some(current)
    }
}

#[cfg(test)]
#[path = "native_list_finish_test.rs"]
mod tests;
