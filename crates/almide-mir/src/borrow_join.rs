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
//!    of any later loop that reads it, and over the `SetLocal` of a
//!    self-append window the read opens, which renders as an in-place
//!    `push_str`) rebinds, writes, consumes, returns or makes unique an arm's
//!    source.
//!
//! Two shapes extend these (#3453 on this render):
//! * a join read through its own slots — a record's fields (`(a ?? b).f`) —
//!   is still only read: the address of the join feeds loads and nothing
//!   else, and the join stays borrowable to the last read of what those
//!   loads lend in turn;
//! * the present arm of an element Option's `?? fallback` whose carrier is
//!   held as the lent element (`Option<&A>`,
//!   `render_native_borrow_read::borrowed_options`) yields the payload read
//!   it makes under the carrier's own tag test: that read is a reference
//!   into a list param the function borrows, alive for the whole function;
//! * the other arm of a join whose first arm is one of the above may yield a
//!   string literal it builds itself: a literal is immortal text, so the join
//!   holds it as a `&'static str` and there is no source to write.

use crate::lower::LowerError;
use crate::mir_ops::{defined_value, op_values};
use crate::render_native::{render_dup, var, NTy};
use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

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
pub(crate) fn borrow_joins(func: &MirFunction, lent: &BTreeSet<ValueId>) -> BorrowJoins {
    let facts = Facts::of(func, lent);
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
    /// address value → the block it points into.
    root: BTreeMap<ValueId, ValueId>,
    /// The payload reads of the lent element Options (`lent`) that render as
    /// the lent reference: read → its carrier.
    lent_reads: BTreeMap<ValueId, ValueId>,
}

impl Facts {
    fn of(func: &MirFunction, lent: &BTreeSet<ValueId>) -> Self {
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
        let root = crate::render_native_borrow_read::roots(func);
        let mut lent_reads = BTreeMap::new();
        if !lent.is_empty() {
            let reads = crate::render_native_borrow_read::borrow_reads(func);
            for op in &func.ops {
                let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(p), args } = op else { continue };
                match args.first().and_then(|a| root.get(a)) {
                    Some(c) if lent.contains(c) && reads.contains(p) => lent_reads.insert(*p, *c),
                    _ => None,
                };
            }
        }
        Facts { def_at, touched_at, loops, root, lent_reads }
    }

    fn touches(&self, v: ValueId) -> &[usize] {
        self.touched_at.get(&v).map_or(&[], Vec::as_slice)
    }

    /// The arm `Dup` dsts when the `IfThen` at `i` (dst `d`) is a borrow join.
    fn borrow_join_at(&self, func: &MirFunction, i: usize, d: ValueId) -> Option<[ValueId; 2]> {
        let (else_at, end_at) = arm_bounds(&func.ops, i)?;
        let Op::Else { val: Some(a) } = func.ops[else_at] else { return None };
        let Op::EndIf { val: Some(b) } = func.ops[end_at] else { return None };
        let src_a = self.selected_source(func, a, i, (i, else_at)).or_else(|| self.lent_source(func, a, i, else_at))?;
        let src_b = self.selected_source(func, b, i, (else_at, end_at));
        if src_b.is_none() {
            self.literal_arm(func, b, (else_at, end_at))?;
        }
        let last = self.last_read_of_join(func, d, i, end_at)?;
        let written = |s: ValueId| func.ops[i..=last].iter().any(|op| writes(op, s) || self.stores_into(op, s));
        if written(src_a) || src_b.is_some_and(written) {
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

    /// Condition 1 for an arm that yields a string literal: `y` is the literal
    /// built inside `(lo, hi)`, touched only by its def, its `Consume` and the
    /// yield at `hi`. A literal is immortal text with no source to write.
    fn literal_arm(&self, func: &MirFunction, y: ValueId, (lo, hi): (usize, usize)) -> Option<()> {
        let at = *self.def_at.get(&y)?;
        let only_consumed = self.touches(y).iter().all(|&k| {
            k == at || k == hi || matches!(func.ops[k], Op::Consume { v } if v == y)
        });
        let literal = matches!(func.ops[at], Op::Alloc { init: crate::Init::Str(_), .. });
        (literal && lo < at && at < hi && only_consumed && func.ret != Some(y)).then_some(())
    }

    /// Condition 1 for the present arm of an element Option's `?? fallback`
    /// whose carrier is held as the lent element (`Option<&A>`,
    /// render_native_borrow_read::borrowed_options): `y` is a `Dup` of the
    /// payload read the arm makes under the carrier's own tag test. That read
    /// renders as the lent reference, which the list param keeps alive for
    /// the whole function, so the join may hold it. Returns the read.
    fn lent_source(&self, func: &MirFunction, y: ValueId, if_at: usize, else_at: usize) -> Option<ValueId> {
        let at = *self.def_at.get(&y)?;
        let Op::Dup { src, .. } = func.ops[at] else { return None };
        let Op::IfThen { cond, .. } = func.ops[if_at] else { return None };
        let carrier = self.lent_reads.get(&src)?;
        let read_at = *self.def_at.get(&src)?;
        // The arm is the Some side: the `if` tests this carrier's tag word.
        let tag_of = match &func.ops[*self.def_at.get(&cond)?] {
            Op::Prim { kind: PrimKind::Load { width: 4 }, args, .. } => args.first().and_then(|a| self.root.get(a)),
            _ => None,
        };
        let only_consumed = self.touches(y).iter().all(|&k| {
            k == at || k == else_at || matches!(func.ops[k], Op::Consume { v } if v == y)
        });
        let placed = if_at < read_at && read_at < at && at < else_at && func.ret != Some(y);
        (placed && tag_of == Some(carrier) && only_consumed).then_some(src)
    }

    /// `op` writes a slot of the block `s` through an address into it.
    fn stores_into(&self, op: &Op, s: ValueId) -> bool {
        matches!(op, Op::Prim { kind: PrimKind::Store { .. }, args, .. } if args.first().is_some_and(|a| self.root.get(a) == Some(&s)))
    }

    /// Every read through an address into the join `d` only loads: a field
    /// or element of a borrowed value reads through the reference.
    fn only_loads_through(&self, func: &MirFunction, d: ValueId, end_at: usize) -> bool {
        let mut vs = Vec::new();
        func.ops[end_at + 1..].iter().all(|op| {
            vs.clear();
            crate::mir_ops::op_reads(op, &mut vs);
            !vs.iter().any(|v| self.root.get(v) == Some(&d))
                || matches!(op, Op::IntBinOp { op: crate::IntOp::Add, .. }
                    | Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle | PrimKind::ElemAddr, .. })
        })
    }

    /// Condition 2: every use of the join `d` after its `EndIf` is a by-reference
    /// read. Returns the index the join must stay borrowable to — its last
    /// read, stretched to the end of every later loop that reads it.
    fn last_read_of_join(&self, func: &MirFunction, d: ValueId, if_at: usize, end_at: usize) -> Option<usize> {
        if func.ret == Some(d) {
            return None;
        }
        let mut last = end_at;
        // The join is read through its own slots (a record's fields).
        let mut through = false;
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
            through |= matches!(func.ops[k], Op::Prim { .. });
            // The self-append window renders a read here as `s.push_str(d)`,
            // which writes `s` at the read itself: count the join live through
            // the window's `SetLocal` of `s`.
            if crate::render_native_append::self_append_window(func, k).is_some() {
                last = last.max(k + 2);
            }
        }
        if through {
            if !self.only_loads_through(func, d, end_at) {
                return None;
            }
            // What is read through the join borrows from it in turn.
            last = last.max(crate::render_native_borrow_read::borrow_span(func, end_at, d).0);
        }
        // A read inside a loop that opens after the join runs again on the next
        // iteration, after every op later in that loop body.
        let mut stretched = last;
        for &(s, e) in &self.loops {
            let read_inside = self.touches(d).iter().any(|&k| s < k && k < e && !is_drop(&func.ops[k]))
                || (through && s < last && last < e);
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
    matches!(op, Op::Drop { .. } | Op::DropListStr { .. } | Op::DropVariant { .. })
}

/// Condition 2 for one op: `op` reads `d` only where the native render passes
/// it by reference.
fn reads_by_reference(op: &Op, d: ValueId) -> bool {
    match op {
        Op::Drop { .. } | Op::DropListStr { .. } | Op::DropVariant { .. } | Op::Borrow { .. } => true,
        // The join's own address: its slots are read through it
        // (`Facts::only_loads_through`).
        Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [d],
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

/// The native spelling of an arm `Dup` that feeds a borrow join: the share is
/// a Rust shared borrow of `src`. A source the render does not hold as a list
/// or String keeps the ordinary [`render_dup`] (its join then renders owned
/// as well).
pub(crate) fn render_arm(
    dst: &ValueId,
    src: &ValueId,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<(), LowerError> {
    let rec_ref = |i: u16| format!("&{}", crate::render_native_rec::struct_name(i));
    let (t, ty_name, amp) = match tys.get(src) {
        Some(NTy::Vec) => (NTy::VecRef, "&[i64]".to_string(), "&"),
        Some(NTy::VecRef) => (NTy::VecRef, "&[i64]".to_string(), ""),
        Some(NTy::Str) => (NTy::StrRef, "&str".to_string(), "&"),
        Some(NTy::StrRef) => (NTy::StrRef, "&str".to_string(), ""),
        // A generated record (an all-scalar one is a by-value struct): the
        // share is a shared borrow of the struct, as for a list or a string.
        Some(NTy::Rec(i)) => (NTy::RecRef(*i), rec_ref(*i), "&"),
        Some(NTy::RecRef(i)) => (NTy::RecRef(*i), rec_ref(*i), ""),
        _ => return render_dup(dst, src, tys, out, indent),
    };
    tys.insert(*dst, t);
    out.push_str(&"    ".repeat(indent));
    let _ = writeln!(out, "let {}: {ty_name} = {amp}{};", var(*dst), var(*src));
    Ok(())
}

/// The native spelling of an arm's string literal that feeds a borrow join:
/// the immortal text itself.
pub(crate) fn render_literal_arm(dst: &ValueId, text: &str, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) {
    tys.insert(*dst, NTy::StrRef);
    out.push_str(&"    ".repeat(indent));
    let _ = writeln!(out, "let {}: &str = \"{}\";", var(*dst), almide_base::rust_lit::escape_rust_str(text));
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
        let bj = borrow_joins(&select(vec![]), &BTreeSet::new());
        assert_eq!(bj.joins, BTreeSet::from([v(3)]));
        assert_eq!(bj.arms, BTreeSet::from([v(4), v(5)]));
    }

    #[test]
    fn a_join_that_is_moved_on_is_not_a_borrow_join() {
        let mut f = select(vec![Op::SetLocal { local: v(9), src: v(3) }]);
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty(), "stored");
        f = select(vec![]);
        f.ret = Some(v(3));
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty(), "returned");
        f = select(vec![Op::Dup { dst: v(9), src: v(3) }]);
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty(), "shared again");
    }

    #[test]
    fn a_source_written_while_the_join_is_live_is_not_a_borrow_join() {
        // The write sits between the join and a later read of it.
        let mut f = select(vec![]);
        let read = f.ops.len() - 1;
        f.ops.insert(read - 1, Op::ListSetScalar { list: v(0), idx: v(2), val: v(2) });
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty());
    }

    #[test]
    fn a_source_written_after_the_last_read_keeps_the_borrow_join() {
        let f = select(vec![Op::ListSetScalar { list: v(0), idx: v(2), val: v(2) }]);
        assert_eq!(borrow_joins(&f, &BTreeSet::new()).joins, BTreeSet::from([v(3)]));
    }

    #[test]
    fn a_self_append_of_a_source_reading_the_join_is_not_a_borrow_join() {
        // `p0 = p0 + d` renders as `v0.push_str(v3)`: the write of the source
        // happens at the join's last read.
        let f = select(vec![
            Op::CallFn { dst: Some(v(9)), name: "__str_concat".into(), args: vec![CallArg::Handle(v(0)), CallArg::Handle(v(3))], result: None },
            Op::Drop { v: v(0) },
            Op::SetLocal { local: v(0), src: v(9) },
        ]);
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty());
    }

    #[test]
    fn a_read_in_a_later_loop_covers_a_write_later_in_that_loop() {
        let f = select(vec![
            Op::LoopStart,
            Op::CallFn { dst: Some(v(7)), name: "list.len".into(), args: vec![CallArg::Handle(v(3))], result: None },
            Op::ListSetScalar { list: v(1), idx: v(2), val: v(2) },
            Op::LoopEnd,
        ]);
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty());
    }

    /// `d = list.first(p0) ?? p1`, then `d.field` read: the carrier `v2`,
    /// its tag `v6`, the payload read `v9` in the present arm.
    fn fallback(lent: bool, write: bool) -> (MirFunction, BTreeSet<ValueId>) {
        use crate::{IntOp, PrimKind};
        let add = |dst: u32, a: u32, b: u32| Op::IntBinOp { dst: v(dst), op: IntOp::Add, a: v(a), b: v(b) };
        let prim = |kind: PrimKind, dst: u32, args: Vec<u32>| Op::Prim { kind, dst: Some(v(dst)), args: args.into_iter().map(v).collect() };
        let mut ops = vec![
            Op::CallFn { dst: Some(v(2)), name: "head".into(), args: vec![CallArg::Handle(v(0))], result: None },
            prim(PrimKind::Handle, 3, vec![2]),
            Op::ConstInt { dst: v(4), value: 4 },
            add(5, 3, 4),
            prim(PrimKind::Load { width: 4 }, 6, vec![5]),
            Op::ConstInt { dst: v(7), value: 12 },
            add(8, 3, 7),
            Op::IfThen { cond: v(6), dst: Some(v(10)) },
            prim(PrimKind::LoadHandle, 9, vec![8]),
            Op::Dup { dst: v(11), src: v(9) },
            Op::Consume { v: v(11) },
            Op::Else { val: Some(v(11)) },
            Op::Dup { dst: v(12), src: v(1) },
            Op::Consume { v: v(12) },
            Op::EndIf { val: Some(v(12)) },
            prim(PrimKind::Handle, 13, vec![10]),
            add(14, 13, 7),
            prim(PrimKind::LoadHandle, 15, vec![14]),
            Op::CallFn { dst: Some(v(16)), name: "list.len".into(), args: vec![CallArg::Handle(v(15))], result: None },
        ];
        if write {
            ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![v(14), v(16)] });
        }
        ops.push(Op::DropVariant { v: v(10), ty: "R".into() });
        let f = MirFunction { name: "f".into(), params: vec![list_param(0), list_param(1)], ops, ..Default::default() };
        (f, if lent { BTreeSet::from([v(2)]) } else { BTreeSet::new() })
    }

    #[test]
    fn a_fallback_over_a_lent_element_read_through_its_fields_is_a_borrow_join() {
        let (f, lent) = fallback(true, false);
        let bj = borrow_joins(&f, &lent);
        assert_eq!(bj.joins, BTreeSet::from([v(10)]));
        assert_eq!(bj.arms, BTreeSet::from([v(11), v(12)]));
    }

    #[test]
    fn a_fallback_over_an_owned_carrier_or_a_written_join_stays_owned() {
        // The payload read of an owned carrier is local to its arm.
        let (f, lent) = fallback(false, false);
        assert!(borrow_joins(&f, &lent).joins.is_empty(), "owned carrier");
        // A slot of the join written through its address.
        let (f, lent) = fallback(true, true);
        assert!(borrow_joins(&f, &lent).joins.is_empty(), "written join");
    }

    #[test]
    fn an_arm_that_allocates_is_not_a_borrow_join() {
        let mut f = select(vec![]);
        f.ops[2] = Op::ConstInt { dst: v(4), value: 0 };
        assert!(borrow_joins(&f, &BTreeSet::new()).joins.is_empty());
    }
}
