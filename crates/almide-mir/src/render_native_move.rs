//! Last-use moves on the native render (#4006, no-perf-loss rule).
//!
//! The MIR copies a heap value with `Dup` and rebinds a slot with `SetLocal`;
//! the native render realizes both as `.clone()`, which keeps every alias its
//! own value. When the source is never read again that copy is pure cost — a
//! swap-carried list (`cur = merged` at a loop's end) copied its whole buffer
//! twice per round. Here such a copy becomes `std::mem::take(&mut src)`: the
//! destination gets the buffer and the source is left empty, which nothing
//! observes, because:
//!
//! * no op after the copy reads the source (a drop is not a read: drops erase
//!   to scope end on this render), nor is it the function's result;
//! * the source is defined in the same loop as the copy, so a later iteration
//!   re-defines it before any read — a value from outside the loop is never
//!   emptied for the next round;
//! * no borrow join (`crate::borrow_join`) holds a reference to it;
//! * its type has a `Default` to leave behind.
//!
//! Any doubt keeps the clone.

use crate::render_native::NTy;
use crate::{MirFunction, Op, ValueId};

/// Whether `op` only does ownership bookkeeping on this render (no code reads
/// its operand).
pub(crate) fn bookkeeping(op: &Op) -> bool {
    matches!(
        op,
        Op::Drop { .. }
            | Op::DropListStr { .. }
            | Op::DropVariant { .. }
            | Op::DropWrapperRec { .. }
            | Op::Consume { .. }
            | Op::Borrow { .. }
            | Op::MakeUnique { .. }
    )
}

/// The innermost open `LoopStart` index at each op (`None` = no loop).
fn loop_of(func: &MirFunction) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    func.ops
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let here = stack.last().copied();
            match op {
                Op::LoopStart => stack.push(i),
                Op::LoopEnd => {
                    stack.pop();
                }
                _ => {}
            }
            here
        })
        .collect()
}

/// Whether the copy of `src` at `ops[at]` may move it out instead.
pub(crate) fn last_use(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    t: NTy,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if !matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_)) || !crate::render_native_rec::has_default(t) {
        return false;
    }
    if func.ret == Some(src) || func.params.iter().any(|p| p.value == src) {
        return false;
    }
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        if bookkeeping(op) {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&src) {
            return false;
        }
        if matches!(op, Op::SetLocal { local, .. } if *local == src) {
            return false;
        }
    }
    let joined = func
        .ops
        .iter()
        .any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    if joined {
        return false;
    }
    let loops = loop_of(func);
    let defs: Vec<usize> = func
        .ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            crate::mir_ops::defined_value(op) == Some(src) || matches!(op, Op::SetLocal { local, .. } if *local == src)
        })
        .map(|(i, _)| i)
        .collect();
    // Every definition precedes the copy, inside its loop (or a loop nested in
    // it), and one is in that loop itself: each round re-defines the value.
    let within = |d: usize| {
        let mut l = loops[d];
        loop {
            if l == loops[at] {
                return true;
            }
            match l {
                Some(start) => l = loops[start],
                None => return false,
            }
        }
    };
    defs.iter().any(|&d| d < at && loops[d] == loops[at]) && defs.iter().all(|&d| d < at && within(d))
}
