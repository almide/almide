//! Borrowed payload reads (#4006, no-perf-loss rule).
//!
//! A `LoadHandle` of a heap payload slot is a BORROW in the MIR: the block
//! still owns the payload. The native render realized it as an owned copy
//! (`(**f).clone()` for a boxed record), so a `match t { Node(l, _, r) => … }`
//! over a recursive enum deep-copied both subtrees at every node and a tree
//! walk went superlinear (25x on a churn program). Here a read whose every use
//! only borrows renders as a reference into the block instead.
//!
//! A read qualifies when, after it:
//! * the loaded value is only handed to a user call (a record param is taken
//!   by reference), copied with `Dup` (that copy is an owned clone, made only
//!   where the program asks for one), read through its own slots (its handle
//!   feeds address arithmetic and loads only), or dropped;
//! * nothing writes, rebinds or moves the block it was read from while the
//!   value is still used — the only ops that name the block's root up to the
//!   value's last use are address arithmetic over it and its loads.
//!
//! Anything else keeps the owned copy.

use crate::{CallArg, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The `LoadHandle` destinations of `func` that may render as a borrow.
pub(crate) fn borrow_reads(func: &MirFunction) -> BTreeSet<ValueId> {
    // Every address value's root block (`prim.handle(r)` plus offsets).
    let mut root: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    for op in &func.ops {
        match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => {
                root.insert(*d, args[0]);
            }
            Op::IntBinOp { dst, op: IntOp::Add, a, b } => {
                if let Some(r) = root.get(a).or_else(|| root.get(b)).copied() {
                    root.insert(*dst, r);
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(&base) = args.first().and_then(|a| root.get(a)) else { continue };
        if func.ret == Some(*d) {
            continue;
        }
        let Some(last) = last_borrowing_use(func, i, *d, &root) else { continue };
        if block_untouched(func, i, last, base, &root) {
            out.insert(*d);
        }
    }
    out
}

/// The index of the last op after `at` that uses `d`, when every such use only
/// borrows it; `None` when some use needs an owned value.
fn last_borrowing_use(func: &MirFunction, at: usize, d: ValueId, root: &BTreeMap<ValueId, ValueId>) -> Option<usize> {
    let mut last = at;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        let names_root = reads.iter().any(|v| root.get(v) == Some(&d));
        if !reads.contains(&d) && !names_root {
            continue;
        }
        let ok = crate::render_native_move::bookkeeping(op)
            || match op {
                Op::CallFn { args, .. } => args.iter().all(|a| !matches!(a, CallArg::Scalar(v) if *v == d)),
                Op::Dup { src, .. } => *src == d,
                Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [d],
                // An address into `d`'s own block, or a load through one.
                Op::IntBinOp { op: IntOp::Add, .. } => names_root,
                Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, args, .. } => {
                    args.len() == 1 && root.get(&args[0]) == Some(&d)
                }
                _ => false,
            };
        if !ok {
            return None;
        }
        last = j;
    }
    Some(last)
}

/// No op in `(from, to]` writes, rebinds or moves `base`'s block: the ops that
/// name it (or an address into it) are address arithmetic and loads only.
fn block_untouched(func: &MirFunction, from: usize, to: usize, base: ValueId, root: &BTreeMap<ValueId, ValueId>) -> bool {
    let mut reads = Vec::new();
    for op in &func.ops[from + 1..=to] {
        if matches!(op, Op::SetLocal { local, .. } if *local == base) {
            return false;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        let touches = reads.iter().any(|v| *v == base || root.get(v) == Some(&base));
        if !touches || crate::render_native_move::bookkeeping(op) {
            continue;
        }
        let ok = match op {
            Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [base],
            Op::IntBinOp { op: IntOp::Add, .. } => true,
            Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, .. } => true,
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}
