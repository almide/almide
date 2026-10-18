//! Borrow joins (#3532): which if-as-value joins the native render may realize
//! as a BORROW of the value an arm selects, instead of an owned copy.
//!
//! `if c then x else x2` over two live heap values lowers to
//!
//! ```text
//! IfThen { cond, dst: d }
//!   Dup { dst: a, src: x }   Consume { a }   Else { val: a }
//!   Dup { dst: b, src: x2 }  Consume { b }   EndIf { val: b }
//! ```
//!
//! In MIR a `Dup` is a share; the native render owns every `Vec`/`String`
//! local, so it realized each `Dup` as a deep copy and the selection cost
//! O(len) per evaluation where wasm pays a count increment. When the join is
//! only READ and no arm's source is written while the join is live, the
//! share is exactly a Rust shared borrow: `let a: &[i64] = &x; d = a;`.
//!
//! The decision is made here, over the MIR alone, and the render only
//! realizes it. The MIR itself is unchanged, so the ownership certificate and
//! every witness keep judging the same ops: a borrow is the share the `Dup`
//! already names, held for no longer than the join it feeds.
//!
//! A join is a borrow join when ALL of these hold (each one is what rustc
//! needs to accept the borrow; a render rustc rejects is a failed build, not
//! a fallback, so the conditions are conservative):
//! 1. each arm yields the `dst` of a `Dup` inside that arm whose `src` is
//!    defined before the `IfThen` (a param or an earlier value), and that
//!    `dst` is touched by nothing but its `Consume` and the yield;
//! 2. every use of the join is a read the render passes by reference — a
//!    call argument, an element read, a `Borrow`, or its scope-end drop —
//!    so it is never moved, returned, stored, rebound or yielded on;
//! 3. no op from the `IfThen` to the join's last read (stretched to the end
//!    of any later loop that reads it) rebinds, writes, consumes, returns or
//!    makes unique an arm's source.

use crate::mir_ops::{defined_value, op_values};
use crate::{CallArg, MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The joins the native render realizes as borrows, and the arm `Dup`s that
/// feed them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct BorrowJoins {
    /// `IfThen` dsts rendered as a shared reference.
    pub joins: BTreeSet<ValueId>,
    /// Arm `Dup` dsts rendered as a borrow of their `src`.
    pub arms: BTreeSet<ValueId>,
}

/// Every borrow join in `func` (see the module docs for the conditions).
pub(crate) fn borrow_joins(func: &MirFunction) -> BorrowJoins {
    let facts = Facts::of(func);
    let mut out = BorrowJoins::default();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::IfThen { dst: Some(d), .. } = op else { continue };
        if let Some(arms) = facts.borrow_join_at(func, i, *d) {
            out.joins.insert(*d);
            out.arms.extend(arms);
        }
    }
    out
}

/// Per-function indexes the conditions read.
struct Facts {
    /// value → the op index that defines it (params are absent: defined before op 0).
    def_at: BTreeMap<ValueId, usize>,
    /// value → the indexes of every op that touches it.
    touched_at: BTreeMap<ValueId, Vec<usize>>,
    /// `(LoopStart, LoopEnd)` index pairs.
    loops: Vec<(usize, usize)>,
}

impl Facts {
    fn of(func: &MirFunction) -> Self {
        let mut def_at = BTreeMap::new();
        let mut touched_at: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
        let mut loops = Vec::new();
        let mut open = Vec::new();
        let mut vals = Vec::new();
        for (i, op) in func.ops.iter().enumerate() {
            if let Some(v) = defined_value(op) {
                def_at.entry(v).or_insert(i);
            }
            vals.clear();
            op_values(op, &mut vals);
            for v in &vals {
                let at = touched_at.entry(*v).or_default();
                if at.last() != Some(&i) {
                    at.push(i);
                }
            }
            match op {
                Op::LoopStart => open.push(i),
                Op::LoopEnd => {
                    if let Some(s) = open.pop() {
                        loops.push((s, i));
                    }
                }
                _ => {}
            }
        }
        Facts { def_at, touched_at, loops }
    }

    fn touches(&self, v: ValueId) -> &[usize] {
        self.touched_at.get(&v).map_or(&[], Vec::as_slice)
    }

    /// The arm `Dup` dsts when the `IfThen` at `i` (dst `d`) is a borrow join.
    fn borrow_join_at(&self, func: &MirFunction, i: usize, d: ValueId) -> Option<[ValueId; 2]> {
        let (else_at, end_at) = arm_bounds(&func.ops, i)?;
        let Op::Else { val: Some(a) } = func.ops[else_at] else { return None };
        let Op::EndIf { val: Some(b) } = func.ops[end_at] else { return None };
        let src_a = self.selected_source(func, a, i, (i, else_at))?;
        let src_b = self.selected_source(func, b, i, (else_at, end_at))?;
        let last = self.last_read_of_join(func, d, i, end_at)?;
        let written = |s: ValueId| func.ops[i..=last].iter().any(|op| writes(op, s));
        if written(src_a) || written(src_b) {
            return None;
        }
        Some([a, b])
    }

    /// Condition 1 for one arm: `y` is a `Dup` inside `(lo, hi)` of a source
    /// defined before the `IfThen` at `if_at`, touched only by its def, its
    /// `Consume` and the yield at `hi`. Returns that source.
    fn selected_source(&self, func: &MirFunction, y: ValueId, if_at: usize, (lo, hi): (usize, usize)) -> Option<ValueId> {
        if func.ret == Some(y) {
            return None;
        }
        let at = *self.def_at.get(&y)?;
        let Op::Dup { src, .. } = func.ops[at] else { return None };
        if at <= lo || at >= hi || self.def_at.get(&src).is_some_and(|&s| s >= if_at) {
            return None;
        }
        let only_consumed = self.touches(y).iter().all(|&k| {
            k == at || k == hi || matches!(func.ops[k], Op::Consume { v } if v == y)
        });
        only_consumed.then_some(src)
    }

    /// Condition 2: every use of the join `d` after its `EndIf` is a by-reference
    /// read. Returns the index the join must stay borrowable to — its last
    /// read, stretched to the end of every later loop that reads it.
    fn last_read_of_join(&self, func: &MirFunction, d: ValueId, if_at: usize, end_at: usize) -> Option<usize> {
        if func.ret == Some(d) {
            return None;
        }
        let mut last = end_at;
        for &k in self.touches(d) {
            if k == if_at {
                continue;
            }
            if k <= end_at || !reads_by_reference(&func.ops[k], d) {
                return None;
            }
            if !is_drop(&func.ops[k]) {
                last = last.max(k);
            }
        }
        // A read inside a loop that opens after the join runs again on the next
        // iteration, after every op later in that loop body.
        let mut stretched = last;
        for &(s, e) in &self.loops {
            let read_inside = self.touches(d).iter().any(|&k| s < k && k < e && !is_drop(&func.ops[k]));
            if s > end_at && read_inside {
                stretched = stretched.max(e);
            }
        }
        Some(stretched)
    }
}

/// The `Else` and `EndIf` indexes of the `IfThen` at `i`.
fn arm_bounds(ops: &[Op], i: usize) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut else_at = None;
    for (k, op) in ops.iter().enumerate().skip(i + 1) {
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::Else { .. } if depth == 0 => else_at = Some(k),
            Op::EndIf { .. } if depth == 0 => return else_at.map(|e| (e, k)),
            Op::EndIf { .. } => depth -= 1,
            _ => {}
        }
    }
    None
}

fn is_drop(op: &Op) -> bool {
    matches!(op, Op::Drop { .. } | Op::DropListStr { .. })
}

/// Condition 2 for one op: `op` reads `d` only where the native render passes
/// it by reference.
fn reads_by_reference(op: &Op, d: ValueId) -> bool {
    match op {
        Op::Drop { .. } | Op::DropListStr { .. } | Op::Borrow { .. } => true,
        Op::ListGetScalar { list, idx, dst } => *list == d && *idx != d && *dst != d,
        Op::CallFn { dst, args, .. } | Op::Call { dst, args, .. } => {
            *dst != Some(d)
                && args.iter().all(|a| !matches!(a, CallArg::Scalar(v) if *v == d))
        }
        _ => false,
    }
}

/// Condition 3 for one op: does `op` rebind, write, consume, return or make
/// unique the Rust local of `s`?
fn writes(op: &Op, s: ValueId) -> bool {
    match op {
        Op::SetLocal { local, .. } => *local == s,
        Op::ListSetScalar { list, .. } => *list == s,
        Op::Consume { v } | Op::MakeUnique { v } => *v == s,
        Op::Return { val } => *val == Some(s),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallArg, MirFunction, MirParam, Op, Repr, ValueId};

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }

    fn list_param(n: u32) -> MirParam {
        MirParam { value: v(n), repr: Repr::Ptr { layout: crate::LayoutId(0) } }
    }

    /// `d = if c then p0 else p1` over two params, read by `list.len`, with the
    /// ops in `tail` after the read.
    fn select(tail: Vec<Op>) -> MirFunction {
        let mut ops = vec![
            Op::ConstInt { dst: v(2), value: 1 },
            Op::IfThen { cond: v(2), dst: Some(v(3)) },
            Op::Dup { dst: v(4), src: v(0) },
            Op::Consume { v: v(4) },
            Op::Else { val: Some(v(4)) },
            Op::Dup { dst: v(5), src: v(1) },
            Op::Consume { v: v(5) },
            Op::EndIf { val: Some(v(5)) },
            Op::CallFn { dst: Some(v(6)), name: "list.len".into(), args: vec![CallArg::Handle(v(3))], result: None },
        ];
        ops.extend(tail);
        ops.push(Op::Drop { v: v(3) });
        MirFunction { name: "f".into(), params: vec![list_param(0), list_param(1)], ops, ..Default::default() }
    }

    #[test]
    fn a_read_only_selection_of_live_values_is_a_borrow_join() {
        let bj = borrow_joins(&select(vec![]));
        assert_eq!(bj.joins, BTreeSet::from([v(3)]));
        assert_eq!(bj.arms, BTreeSet::from([v(4), v(5)]));
    }

    #[test]
    fn a_join_that_is_moved_on_is_not_a_borrow_join() {
        let mut f = select(vec![Op::SetLocal { local: v(9), src: v(3) }]);
        assert!(borrow_joins(&f).joins.is_empty(), "stored");
        f = select(vec![]);
        f.ret = Some(v(3));
        assert!(borrow_joins(&f).joins.is_empty(), "returned");
        f = select(vec![Op::Dup { dst: v(9), src: v(3) }]);
        assert!(borrow_joins(&f).joins.is_empty(), "shared again");
    }

    #[test]
    fn a_source_written_while_the_join_is_live_is_not_a_borrow_join() {
        // The write sits between the join and a later read of it.
        let mut f = select(vec![]);
        let read = f.ops.len() - 1;
        f.ops.insert(read - 1, Op::ListSetScalar { list: v(0), idx: v(2), val: v(2) });
        assert!(borrow_joins(&f).joins.is_empty());
    }

    #[test]
    fn a_source_written_after_the_last_read_keeps_the_borrow_join() {
        let f = select(vec![Op::ListSetScalar { list: v(0), idx: v(2), val: v(2) }]);
        assert_eq!(borrow_joins(&f).joins, BTreeSet::from([v(3)]));
    }

    #[test]
    fn a_read_in_a_later_loop_covers_a_write_later_in_that_loop() {
        let f = select(vec![
            Op::LoopStart,
            Op::CallFn { dst: Some(v(7)), name: "list.len".into(), args: vec![CallArg::Handle(v(3))], result: None },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(2) },
            Op::LoopEnd,
        ]);
        assert!(borrow_joins(&f).joins.is_empty());
    }

    #[test]
    fn an_arm_that_allocates_is_not_a_borrow_join() {
        let mut f = select(vec![]);
        f.ops[2] = Op::ConstInt { dst: v(4), value: 0 };
        assert!(borrow_joins(&f).joins.is_empty());
    }
}
