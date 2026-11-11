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

/// The borrow-only reads of `func` taken at an index that is rebound while
/// the read is still used (`let w = ws[j]; j = j + 1; … w …`). The reference
/// such a read holds stays on the element it was taken from, but its place,
/// spelled again over the index variable, names another element: a copy of
/// the read must not be made by reading the place again (#4661).
pub(crate) fn moved_index_reads(func: &MirFunction) -> BTreeSet<ValueId> {
    let root = roots(func);
    let defs: BTreeMap<ValueId, &Op> = func.ops.iter().filter_map(|o| match o {
        Op::Prim { dst: Some(d), .. } | Op::IntBinOp { dst: d, .. } => Some((*d, o)),
        _ => None,
    }).collect();
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(addr) = args.first() else { continue };
        let indices = index_values(&defs, *addr, &root);
        let end = through_enclosing_loops(func, i, borrow_span(func, i, *d).0);
        if func.ops[i + 1..=end].iter().any(|o| matches!(o, Op::SetLocal { local, .. } if indices.contains(local))) {
            out.insert(*d);
        }
    }
    out
}

/// Every value the address `addr` is computed from other than its root
/// block: an element index, a byte offset, and what those were computed from
/// (`defs`: the arithmetic and prim ops of the function by result).
fn index_values(defs: &BTreeMap<ValueId, &Op>, addr: ValueId, root: &BTreeMap<ValueId, ValueId>) -> BTreeSet<ValueId> {
    let mut out = BTreeSet::new();
    let mut todo = vec![addr];
    while let Some(v) = todo.pop() {
        // The block's own handle is the base, not an index.
        let read: &[ValueId] = match defs.get(&v) {
            Some(Op::Prim { kind: PrimKind::ElemAddr, args, .. }) => args,
            Some(Op::IntBinOp { a, b, .. }) => &[*a, *b],
            _ => &[],
        };
        for x in read {
            let base = matches!(defs.get(x), Some(Op::Prim { kind: PrimKind::Handle, .. }));
            if !base && (root.contains_key(x) || out.insert(*x)) {
                todo.push(*x);
            }
        }
    }
    out
}

/// The copies (`Dup`) of borrow-only reads that may stay references: every
/// use of the copy only borrows too, it is never rebound, returned or
/// handed over, and
/// the block the read borrows from is untouched up to the last read of
/// anything borrowed from it ([`borrow_span`]). A copy rendered owned cloned
/// the whole value (a captured map read for one `map.get`).
pub(crate) fn ref_dups(func: &MirFunction) -> BTreeSet<ValueId> {
    let root = roots(func);
    let reads = borrow_reads(func);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let (true, Some(&base)) = (reads.contains(d), args.first().and_then(|a| root.get(a))) else { continue };
        let (last, dups) = borrow_span(func, i, *d);
        // Never rebound, returned, or handed over (a call-site move, a
        // uniqueness claim): the copy exists to be owned there.
        let fixed = |v: &ValueId| {
            func.ret != Some(*v)
                && !func.ops.iter().any(|o| matches!(o, Op::SetLocal { local: x, .. } | Op::Consume { v: x } | Op::MakeUnique { v: x } if x == v))
        };
        let borrowing = |(j, v): &(usize, ValueId)| fixed(v) && last_borrowing_use(func, *j, *v, &root).is_some();
        if dups.iter().all(borrowing) && block_untouched(func, i, last, base, &root) {
            out.extend(dups.iter().map(|(_, v)| *v));
        }
    }
    out
}

/// The last op after `at` that reads, other than to release it, `d` or
/// anything borrowed from it — its copies, addresses into it and into them,
/// and the reads through those — and the copies (`Dup`, with their ops).
/// A release only ends a borrow; a call's result is its own.
pub(crate) fn borrow_span(func: &MirFunction, at: usize, d: ValueId) -> (usize, Vec<(usize, ValueId)>) {
    let mut taint = BTreeSet::from([d]);
    let mut dups = Vec::new();
    let mut last = at;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if crate::render_native_move::bookkeeping(op) || !reads.iter().any(|v| taint.contains(v)) {
            continue;
        }
        last = j;
        let borrowed = match op {
            Op::Dup { dst, .. } => {
                dups.push((j, *dst));
                Some(*dst)
            }
            Op::Prim { kind: PrimKind::Handle | PrimKind::ElemAddr | PrimKind::LoadHandle, dst, .. } => *dst,
            Op::IntBinOp { dst, op: IntOp::Add, .. } => Some(*dst),
            _ => None,
        };
        taint.extend(borrowed);
    }
    (last, dups)
}

thread_local! {
    static REF_DUPS: std::cell::RefCell<BTreeSet<ValueId>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

/// `v` is a [`ref_dups`] copy of the function being rendered.
pub(crate) fn ref_dup(v: ValueId) -> bool {
    REF_DUPS.with(|m| m.borrow().contains(&v))
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
                Op::CallFn { name, args, .. } => args.iter().enumerate().all(|(k, a)| match a {
                    CallArg::Scalar(v) if *v == d => false,
                    // A declared runtime mutator needs the owned temporary
                    // that the lowering publishes back into its source place.
                    CallArg::Handle(v) if *v == d => !crate::native_hof::mutable_argument(name, k),
                    _ => true,
                }),
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

#[cfg(test)]
mod mutation_tests {
    use super::*;
    #[test]
    fn a_mutating_runtime_receiver_is_not_a_read_only_payload_view() {
        let v = ValueId;
        let mut f = MirFunction { ops: vec![
            Op::Prim { kind: PrimKind::Handle, dst: Some(v(1)), args: vec![v(0)] },
            Op::ConstInt { dst: v(2), value: 12 },
            Op::IntBinOp { dst: v(3), op: IntOp::Add, a: v(1), b: v(2) },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(4)), args: vec![v(3)] },
            Op::CallFn { dst: None, name: "read_only".into(), args: vec![CallArg::Handle(v(4))], result: None },
        ], ..Default::default() };
        assert!(borrow_reads(&f).contains(&v(4)));
        let Op::CallFn { name, .. } = &mut f.ops[4] else { unreachable!() };
        *name = "list_pop".into();
        assert!(!borrow_reads(&f).contains(&v(4)));
    }
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
        let value_twin = floor.borrow_twin.is_some() && floor.ret == crate::native_floor::FloorRet::ResValue;
        if !value_twin || func.ret == Some(*d) || args.len() != floor.params.len() {
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

/// The element-Option floor results of `func` (`list.get(xs, i)`,
/// `list.first(xs)`: the `A?` of a `List[A]` param) that render through the
/// runtime's borrowing twin — `Option<&A>`, the element lent from the list,
/// where the owned body copies it whole (a head read of a wide record cost
/// the record's size per read; v0 lends it, #3453). A result qualifies when:
/// * the list it reads is a param the function borrows and never rebinds (a
///   `&` binding, so the element outlives every use in the function), whose
///   element kind has a reference form (`param_tys`: the params' kinds);
/// * it is not the function's result, and its every use is its tag word, an
///   address into it, a payload read, or a combinator call that lends it on
///   — nothing copies, moves or stores the carrier itself.
///
/// A payload read of such a carrier is a reference when its own uses only
/// borrow ([`borrow_reads`]), and a copy of the one element otherwise.
pub(crate) fn borrowed_options(
    func: &MirFunction,
    param_tys: &BTreeMap<ValueId, crate::render_native::NTy>,
    linked: &dyn Fn(&str) -> bool,
) -> BTreeSet<ValueId> {
    use crate::native_floor::{ElemRet, FloorParam, FloorRet, ListElem};
    use crate::render_native::NTy;
    let root = roots(func);
    let rebound: BTreeSet<ValueId> =
        func.ops.iter().filter_map(|op| if let Op::SetLocal { local, .. } = op { Some(*local) } else { None }).collect();
    let lent_elem = |v: ValueId| {
        let borrowed = func.params.iter().any(|p| p.value == v) && !func.owned_params.contains(&v) && !rebound.contains(&v);
        let elem = match param_tys.get(&v) {
            Some(NTy::VecStrRef) => Some(NTy::Str),
            Some(NTy::RecRef(i)) => crate::render_native_rec::list_elem(*i),
            _ => None,
        };
        borrowed && matches!(elem, Some(NTy::Rec(_) | NTy::Str | NTy::Vec | NTy::VecStr))
    };
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::CallFn { dst: Some(d), name, args, .. } = op else { continue };
        let Some(floor) = crate::native_floor::declared(name) else { continue };
        if floor.borrow_twin.is_none() || floor.ret != FloorRet::Elem(ElemRet::Opt) || func.ret == Some(*d) || args.len() != floor.params.len() {
            continue;
        }
        let mut lists = 0;
        let sources_lent = floor.params.iter().zip(args).all(|(p, a)| match (p, a) {
            (FloorParam::List(ListElem::Any), CallArg::Handle(v)) => {
                lists += 1;
                lent_elem(*v)
            }
            (FloorParam::Int, _) => true,
            _ => false,
        });
        if sources_lent && lists == 1 && only_word_and_payload_reads(func, i, *d, &root, linked) {
            out.insert(*d);
        }
    }
    out
}

/// Every use of the Option carrier `d` after `ops[at]` reads its tag word or
/// its payload through an address into it ([`borrowed_options`]).
///
/// Or it is handed to a stdlib combinator whose callback alone takes the
/// payload (`option.map(list.get(xs, i), f)`), served by the runtime body
/// (`linked`: the name is a linked body's instead): the Option goes lent.
fn only_word_and_payload_reads(
    func: &MirFunction,
    at: usize,
    d: ValueId,
    root: &BTreeMap<ValueId, ValueId>,
    linked: &dyn Fn(&str) -> bool,
) -> bool {
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
                Op::Prim { kind: PrimKind::Load { width: 4 } | PrimKind::LoadHandle, args, .. } => {
                    args.len() == 1 && root.get(&args[0]) == Some(&d)
                }
                Op::CallFn { name, args, .. } => {
                    !linked(name)
                        && args.iter().enumerate().all(|(k, a)| match a {
                            CallArg::Handle(v) if *v == d => crate::native_hof::lends_receiver(name, k),
                            CallArg::Scalar(v) => *v != d,
                            _ => true,
                        })
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
    static BORROWED_OPTIONS: std::cell::RefCell<BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(BTreeSet::new()) };
    static LENT_STR_READS: std::cell::RefCell<BTreeMap<ValueId, String>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The string read `v` is the lent element of the carrier `carrier` (an
/// `Option<&AlmideString>`): an owned copy of it shares the element's handle.
pub(crate) fn note_lent_str_read(v: ValueId, carrier: &str) {
    LENT_STR_READS.with(|m| m.borrow_mut().insert(v, carrier.to_string()));
}

/// The owned copy of a lent string read ([`note_lent_str_read`]): the
/// element's own handle shared by its count, never its bytes copied into a
/// new block. The absent side is the empty text the read itself shows.
pub(crate) fn lent_str_copy(v: ValueId) -> Option<String> {
    let carrier = LENT_STR_READS.with(|m| m.borrow().get(&v).cloned())?;
    Some(format!("match {carrier} {{ Some(v) => v.clone(), None => AlmideString::new() }}"))
}

/// `v` is an element-Option floor result of the function being rendered
/// held through the borrowing twin ([`borrowed_options`]): an `Option<&A>`.
pub(crate) fn borrowed_option(v: ValueId) -> bool {
    BORROWED_OPTIONS.with(|m| m.borrow().contains(&v))
}

/// Record the [`borrowed_results`] of the function about to render, and its
/// [`borrowed_options`] (`options`).
pub(crate) fn set_borrowed_results(func: &MirFunction, options: BTreeSet<ValueId>) {
    BORROWED_OPTIONS.with(|m| *m.borrow_mut() = options);
    LENT_STR_READS.with(|m| m.borrow_mut().clear());
    let set = borrowed_results(func);
    crate::native_decode_hint::set(func, &set);
    BORROWED_RESULTS.with(|m| *m.borrow_mut() = set);
    let dups = ref_dups(func);
    REF_DUPS.with(|m| *m.borrow_mut() = dups);
}

/// `v` is a floor result of the function being rendered that holds a
/// reference into its Value arg (its Ok side is `&AlmideValue`).
pub(crate) fn borrowed_result(v: ValueId) -> bool {
    BORROWED_RESULTS.with(|m| m.borrow().contains(&v))
}
