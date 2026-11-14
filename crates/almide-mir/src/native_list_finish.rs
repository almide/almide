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
            .filter(|_| !self.mutable.contains(&handle))
            .copied()
    }

    /// A builder slot rebound only AFTER its commitment (`list.push` onto
    /// the filtered prefix) is still the fresh block at the commitment, as
    /// long as no address taken from the old block is read once it moved.
    fn rebound_after(&self, ops: &[Op], store: usize, base: ValueId) -> bool {
        let rebinds = |op: &Op| matches!(op, Op::SetLocal { local, .. } if *local == base);
        let Some(first) = ops.iter().position(rebinds) else { return true };
        if first <= store {
            return false;
        }
        let old: BTreeSet<ValueId> = ops[..first]
            .iter()
            .filter_map(crate::mir_ops::defined_value)
            .filter(|v| *v != base && self.block_of(*v, 4) == Some(base))
            .collect();
        ops[first..].iter().all(|op| {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(op, &mut reads);
            reads.iter().all(|v| !old.contains(v))
        })
    }

    /// The block an address is derived from: its handle, moved by sums.
    fn block_of(&self, address: ValueId, fuel: u8) -> Option<ValueId> {
        if self.mutable.contains(&address) || fuel == 0 {
            return None;
        }
        if let Some(base) = self.handles.get(&address) {
            return Some(*base);
        }
        let &(_, a, b) = self.sums.get(&address)?;
        self.block_of(a, fuel - 1).or_else(|| self.block_of(b, fuel - 1))
    }

    fn valid(&self, ops: &[Op], store: usize, base: ValueId, count: ValueId) -> bool {
        let Some(&(alloc, len)) = self.allocated.get(&base) else {
            return false;
        };
        if !self.rebound_after(ops, store, base) {
            return false;
        }
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
        let Some(start) = Self::matching_loop_start(ops, alloc, end) else {
            return false;
        };
        // A rebound builder: no loop may start between the block's birth and
        // the filter loop (an enclosing one would carry the rebound slot back).
        if self.mutable.contains(&base) && ops[alloc + 1..start].iter().any(|op| matches!(op, Op::LoopStart | Op::LoopEnd)) {
            return false;
        }
        let Some((test, cond)) = Self::outer_exit(ops, start, end) else {
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
            || Self::nested_rebinds(&ops[start + 1..end], &[index, count, base])
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

    /// The `LoopStart` that the `LoopEnd` at `end` closes (an inner loop's pair is
    /// skipped), strictly after `alloc`.
    fn matching_loop_start(ops: &[Op], alloc: usize, end: usize) -> Option<usize> {
        let mut inner = 0usize;
        for i in (alloc + 1..end).rev() {
            match ops[i] {
                Op::LoopEnd => inner += 1,
                Op::LoopStart if inner == 0 => return Some(i),
                Op::LoopStart => inner -= 1,
                _ => {}
            }
        }
        None
    }

    /// The filter loop's own exit: its first `LoopBreakUnless` outside any `if`
    /// and outside any loop nested in it. A nested loop's breaks leave only that
    /// loop; a second outer break, or a `return` anywhere, refuses.
    fn outer_exit(ops: &[Op], start: usize, end: usize) -> Option<(usize, ValueId)> {
        let (mut depth, mut loops) = (0i32, 0i32);
        let mut exit = None;
        for (i, op) in ops.iter().enumerate().take(end).skip(start + 1) {
            match op {
                Op::IfThen { .. } => depth += 1,
                Op::EndIf { .. } => depth -= 1,
                Op::LoopStart => loops += 1,
                Op::LoopEnd => loops -= 1,
                Op::LoopBreakUnless { .. } if loops > 0 => {}
                Op::LoopBreakUnless { cond } if depth == 0 && exit.is_none() => exit = Some((i, *cond)),
                Op::LoopBreakUnless { .. } | Op::Return { .. } => return None,
                _ => {}
            }
        }
        exit
    }

    /// Does a loop nested in the filter body rebind one of `values`? Its body runs
    /// any number of times per outer iteration, so a counter (or the builder)
    /// rebound there would no longer advance at most once per iteration.
    fn nested_rebinds(body: &[Op], values: &[ValueId]) -> bool {
        let mut loops = 0i32;
        body.iter().any(|op| {
            match op {
                Op::LoopStart => loops += 1,
                Op::LoopEnd => loops -= 1,
                _ => {}
            }
            loops > 0 && matches!(op, Op::SetLocal { local, .. } if values.contains(local))
        })
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
