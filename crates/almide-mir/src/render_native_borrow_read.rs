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
//!   value's last use are address arithmetic over it, its loads and its
//!   scalar slot reads.
//!
//! Anything else keeps the owned copy.

use crate::{CallArg, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The `LoadHandle` destinations of `func` that may render as a borrow.
pub(crate) fn borrow_reads(func: &MirFunction) -> BTreeSet<ValueId> {
    let root = roots(func);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(&base) = args.first().and_then(|a| root.get(a)) else { continue };
        if func.ret == Some(*d) {
            continue;
        }
        let Some(last) = last_borrowing_use(func, i, *d, &root) else { continue };
        if block_untouched(func, i, through_enclosing_loops(func, i, last), base, &root) {
            out.insert(*d);
        }
    }
    out
}

/// Every address value's root block (`prim.handle(r)` plus offsets).
pub(crate) fn roots(func: &MirFunction) -> BTreeMap<ValueId, ValueId> {
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
            // A bounds-checked element address of a list block.
            Op::Prim { kind: PrimKind::ElemAddr, dst: Some(d), args } if args.len() == 2 => {
                if let Some(r) = root.get(&args[0]).copied() {
                    root.insert(*d, r);
                }
            }
            _ => {}
        }
    }
    root
}

/// The index of the last op after `at` that uses `d`, when every such use only
/// borrows it; `None` when some use needs an owned value.
pub(crate) fn last_borrowing_use(func: &MirFunction, at: usize, d: ValueId, root: &BTreeMap<ValueId, ValueId>) -> Option<usize> {
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
                Op::Prim { kind: PrimKind::ElemAddr, args, .. } => {
                    args.len() == 2 && root.get(&args[0]) == Some(&d) && args[1] != d
                }
                Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, args, .. } => {
                    args.len() == 1 && root.get(&args[0]) == Some(&d)
                }
                // A scalar slot read (a variant's tag, a field) only reads.
                Op::ListGetScalar { list, idx, .. } => *list == d && *idx != d,
                _ => false,
            };
        if !ok {
            return None;
        }
        last = j;
    }
    Some(last)
}

/// `last` pushed out to the end of the outermost loop that holds it but not
/// `from`: that loop's next round reads the value again after every op of its
/// body, so the block must stay untouched through the whole loop, not only up
/// to the value's last use in op order (a loop over a field whose body rebinds
/// the record).
fn through_enclosing_loops(func: &MirFunction, from: usize, last: usize) -> usize {
    let mut stack: Vec<usize> = Vec::new();
    let mut from_loops: Vec<usize> = Vec::new();
    let mut outer: Option<usize> = None;
    for (i, op) in func.ops.iter().enumerate() {
        if i == from {
            from_loops = stack.clone();
        }
        if i == last {
            outer = stack.iter().copied().find(|s| !from_loops.contains(s));
        }
        match op {
            Op::LoopStart => stack.push(i),
            Op::LoopEnd => {
                let start = stack.pop();
                if outer.is_some() && start == outer {
                    return i;
                }
            }
            _ => {}
        }
    }
    last
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
            Op::Prim { kind: PrimKind::ElemAddr | PrimKind::Load { .. } | PrimKind::LoadHandle, .. } => true,
            Op::ListGetScalar { list, .. } => *list == base,
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// The `Result[Value, String]` floor results of `func` that render through
/// the runtime's borrowing twin (`FloorFn::borrow_twin`) — the copy v0's
/// derived decode never makes (`value_field_ref`). A result qualifies when:
/// * every Value it is read from is a param the function borrows and never
///   rebinds (a `&` binding nothing moves or writes while the result lives);
/// * it is not the function's result, and its every use is its tag word, an
///   address into it, or a payload read that renders as a borrow
///   ([`borrow_reads`]) — nothing copies, moves, stores or hands it on.
pub(crate) fn borrowed_results(func: &MirFunction) -> BTreeSet<ValueId> {
    let root = roots(func);
    let reads = borrow_reads(func);
    let rebound: BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        })
        .collect();
    let borrowed_param = |v: ValueId| {
        func.params.iter().any(|p| p.value == v) && !func.owned_params.contains(&v) && !rebound.contains(&v)
    };
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::CallFn { dst: Some(d), name, args, .. } = op else { continue };
        let Some(floor) = crate::native_floor::declared(name) else { continue };
        if floor.borrow_twin.is_none() || func.ret == Some(*d) || args.len() != floor.params.len() {
            continue;
        }
        let sources_borrowed = floor.params.iter().zip(args).all(|(p, a)| match (p, a) {
            (crate::native_floor::FloorParam::Value(_), a) => matches!(a, CallArg::Handle(v) if borrowed_param(*v)),
            _ => true,
        });
        if sources_borrowed && only_borrowed_reads(func, i, *d, &root, &reads) {
            out.insert(*d);
        }
    }
    out
}

/// Every use of the carrier `d` after `ops[at]` reads its words or borrows a
/// payload ([`borrowed_results`]).
fn only_borrowed_reads(func: &MirFunction, at: usize, d: ValueId, root: &BTreeMap<ValueId, ValueId>, reads: &BTreeSet<ValueId>) -> bool {
    let mut vs = Vec::new();
    for op in &func.ops[at + 1..] {
        vs.clear();
        crate::mir_ops::op_reads(op, &mut vs);
        let names_root = vs.iter().any(|v| root.get(v) == Some(&d));
        if !vs.contains(&d) && !names_root {
            continue;
        }
        let ok = crate::render_native_move::bookkeeping(op)
            || match op {
                Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [d],
                Op::IntBinOp { op: IntOp::Add, a, b, .. } => names_root && *a != d && *b != d,
                Op::Prim { kind: PrimKind::Load { .. }, args, .. } => args.len() == 1 && root.get(&args[0]) == Some(&d),
                Op::Prim { kind: PrimKind::LoadHandle, dst: Some(x), args } => {
                    args.len() == 1 && root.get(&args[0]) == Some(&d) && reads.contains(x)
                }
                _ => false,
            };
        if !ok {
            return false;
        }
    }
    true
}

thread_local! {
    static BORROWED_RESULTS: std::cell::RefCell<BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(BTreeSet::new()) };
}

/// Record the [`borrowed_results`] of the function about to render.
pub(crate) fn set_borrowed_results(func: &MirFunction) {
    let set = borrowed_results(func);
    BORROWED_RESULTS.with(|m| *m.borrow_mut() = set);
}

/// `v` is a floor result of the function being rendered that holds a
/// reference into its Value arg (its Ok side is `&AlmideValue`).
pub(crate) fn borrowed_result(v: ValueId) -> bool {
    BORROWED_RESULTS.with(|m| m.borrow().contains(&v))
}
