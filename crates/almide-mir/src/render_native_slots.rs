//! #4006: the kinds of a function's untyped slot blocks, read from what is
//! stored into them and from the declarations that consume them.
//!
//! A DynList block the declarations do not type (a local tuple, a record or
//! list built in place and never passed through a typed signature) gets a
//! kind from the values its slots receive. One fixpoint closes the joins
//! together, each step reading only declarations and stored values:
//!
//! - down: a block stored into a typed parent's slot takes the slot's
//!   declared kind (a list's element, a record's field, an enum slot's one
//!   record kind, a carrier slot);
//! - the kinds of values: an allocation's own kind, a call's declared result
//!   (a user signature, a shim row, the declared floor), a parameter's
//!   declared kind, a copy's source, a joined `if`'s two arms when they agree,
//!   a slot read off a typed parent;
//! - up: an untyped block whose stores all have one heap kind is a list of
//!   that kind; one whose constant slots each receive one kind, scalars and
//!   heap values mixed, is a generated struct of those slot kinds
//!   (`render_native_rec::slots_type_of`). A block whose stores disagree, or
//!   that is also indexed, stays untyped and walls as before;
//! - a typed drop names a list type (`list_<T>`, the drop route the lowering
//!   names from the declared element type `T`): the block is a list of `T`.

use super::carrier;
use crate::lower::layout::{BLOCK_HEADER, SLOT_SIZE};
use crate::render_native::{NTy, NativeSigKind, NativeSigs};
use crate::render_native_rec as rec;
use crate::{Init, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// The address facts the prepass already holds: a `Handle` result's block,
/// an address's (block, constant offset), the constants.
pub(super) struct Facts<'a> {
    pub handle_of: &'a BTreeMap<ValueId, ValueId>,
    pub base_of: &'a BTreeMap<ValueId, (ValueId, Option<i64>)>,
    pub consts: &'a BTreeMap<ValueId, i64>,
}

/// The slot a constant byte offset names, past the header.
fn slot_of(off: Option<i64>) -> Option<usize> {
    let (h, w) = (i64::from(BLOCK_HEADER), i64::from(SLOT_SIZE));
    off.and_then(|o| (o >= h && (o - h) % w == 0).then(|| ((o - h) / w) as usize))
}

/// The owned kind a declared signature kind binds as (heap kinds only).
fn heap_of_sig(k: NativeSigKind) -> Option<NTy> {
    Some(match k {
        NativeSigKind::Str => NTy::Str,
        NativeSigKind::ListI64 => NTy::Vec,
        NativeSigKind::ListStr => NTy::VecStr,
        NativeSigKind::Rec(i) => NTy::Rec(i),
        _ => return None,
    })
}

/// The owned spelling of a kind a slot holds.
fn owned(t: NTy) -> NTy {
    match t {
        NTy::StrRef => NTy::Str,
        NTy::VecRef => NTy::Vec,
        NTy::VecStrRef => NTy::VecStr,
        NTy::RecRef(i) => NTy::Rec(i),
        NTy::F64 => NTy::I64,
        t => t,
    }
}

/// A call's declared heap result: the callee's signature, its shim row, or
/// the declared floor entry its stdlib declaration gives.
fn call_result(name: &str, sigs: &NativeSigs) -> Option<NTy> {
    if let Some((_, r)) = sigs.get(name) {
        return r.and_then(heap_of_sig);
    }
    if let Some((_, r, _)) = crate::render_native_shims::shim(name) {
        return r.map(owned).filter(|t| *t != NTy::I64);
    }
    use crate::native_floor::FloorRet;
    match crate::native_floor::declared(name)?.ret {
        FloorRet::Str => Some(NTy::Str),
        FloorRet::ListInt => Some(NTy::Vec),
        FloorRet::ListStr => Some(NTy::VecStr),
        FloorRet::Bytes => Some(NTy::Rec(rec::bytes_index())),
        FloorRet::Value => Some(NTy::Rec(rec::value_index())),
        FloorRet::Matrix => Some(NTy::Rec(rec::matrix_index())),
        _ => None,
    }
}

/// The declared kind a typed parent's slot gives the value stored or read there.
fn parent_slot_kind(i: u16, slot: Option<usize>) -> Option<NTy> {
    if let Some(e) = rec::list_elem(i) {
        return Some(e);
    }
    let k = slot?;
    if carrier::is_carrier(i) {
        return carrier::slot_kind(i, k);
    }
    rec::rec_field(i, k).or_else(|| {
        let ks: Vec<NTy> = rec::enum_slot_kinds(i, k).into_iter().filter(|t| matches!(t, NTy::Rec(_))).collect();
        (ks.len() == 1).then(|| ks[0])
    })
}

/// The heap kind a read of a typed parent's slot binds as: the declared kind,
/// or an enum slot's one heap kind.
fn slot_read_kind(i: u16, slot: Option<usize>) -> Option<NTy> {
    if rec::enum_cases(i).is_some() {
        let heap: Vec<NTy> = rec::enum_slot_kinds(i, slot?).into_iter().filter(|t| *t != NTy::I64).collect();
        return (heap.len() == 1).then(|| heap[0]);
    }
    parent_slot_kind(i, slot).filter(|t| *t != NTy::I64)
}

/// Type the untyped slot blocks of `func` into `alloc_rec` (block → generated
/// type) and return the blocks that are `List[String]`.
pub(super) fn type_blocks(
    func: &MirFunction,
    sigs: &NativeSigs,
    alloc_rec: &mut BTreeMap<ValueId, u16>,
    f: &Facts<'_>,
) -> BTreeSet<ValueId> {
    // A block the lowering built as an all-scalar tuple, record or variant
    // takes its declared type's by-value struct or enum.
    for (v, key) in &func.aggregate_blocks {
        if let Some(i) = rec::rec_index(key).filter(|i| rec::all_scalar_struct(*i) || rec::enum_cases(*i).is_some()) {
            alloc_rec.entry(*v).or_insert(i);
        }
    }
    // The kinds values have before any block is typed.
    let mut known: BTreeMap<ValueId, NTy> = BTreeMap::new();
    if let Some((ps, _)) = sigs.get(func.name.as_str()).filter(|(ps, _)| ps.len() == func.params.len()) {
        for (p, k) in func.params.iter().zip(ps) {
            if let Some(t) = heap_of_sig(*k) {
                known.insert(p.value, t);
            }
        }
    }
    // DynList blocks (and their declared length), the joins, the copies.
    let mut blocks: Vec<(ValueId, Option<i64>)> = Vec::new();
    let mut joins: Vec<(ValueId, ValueId, ValueId)> = Vec::new();
    let mut open: Vec<(Option<ValueId>, Option<ValueId>)> = Vec::new();
    let mut dups: Vec<(ValueId, ValueId)> = Vec::new();
    for op in &func.ops {
        match op {
            Op::Alloc { dst, init: Init::Str(_) | Init::DynStr { .. }, .. } => {
                known.insert(*dst, NTy::Str);
            }
            Op::Prim { kind: PrimKind::OptMakeSome | PrimKind::OptMakeNone | PrimKind::OptCopy, dst: Some(d), .. } => {
                known.insert(*d, NTy::Opt);
            }
            Op::Alloc { dst, init: Init::IntList(_), .. } | Op::ListLit { dst, .. } if !alloc_rec.contains_key(dst) => {
                known.insert(*dst, NTy::Vec);
            }
            Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. } => {
                blocks.push((*dst, f.consts.get(len).copied()));
            }
            Op::CallFn { dst: Some(d), name, .. } => {
                if let Some(t) = call_result(name, sigs) {
                    known.insert(*d, t);
                }
            }
            Op::Dup { dst, src } => dups.push((*dst, *src)),
            Op::IfThen { dst, .. } => open.push((*dst, None)),
            Op::Else { val } => {
                if let Some(top) = open.last_mut() {
                    top.1 = *val;
                }
            }
            Op::EndIf { val } => {
                if let Some((Some(d), Some(t))) = open.pop() {
                    if let Some(e) = val {
                        joins.push((d, t, *e));
                    }
                }
            }
            // A typed drop that names a list of a generated type `T`.
            Op::DropVariant { v, ty } if !alloc_rec.contains_key(v) => {
                if let Some(j) = ty.strip_prefix("list_").and_then(rec::rec_index) {
                    alloc_rec.insert(*v, rec::list_type_of(NTy::Rec(j)));
                }
            }
            _ => {}
        }
    }
    let kind_of = |v: &ValueId, known: &BTreeMap<ValueId, NTy>, alloc_rec: &BTreeMap<ValueId, u16>, str_lists: &BTreeSet<ValueId>, scalar_blocks: &BTreeSet<ValueId>| {
            if let Some(i) = alloc_rec.get(v) {
                Some(NTy::Rec(*i))
            } else if str_lists.contains(v) {
                Some(NTy::VecStr)
            } else if scalar_blocks.contains(v) {
                Some(NTy::Vec)
            } else {
                known.get(v).copied()
            }
        };
    let mut str_lists: BTreeSet<ValueId> = BTreeSet::new();
    // Untyped blocks every store of which is a scalar: the `Vec<i64>` default.
    let mut scalar_blocks: BTreeSet<ValueId> = BTreeSet::new();
    loop {
        let before = (alloc_rec.len(), str_lists.len(), scalar_blocks.len(), known.len());
        // Down and across first, to their own fixpoint: a declared kind
        // reaches every block it can before any block is typed from its
        // stores, so a store-typed struct never stands in for a declared type.
        loop {
        let inner = (alloc_rec.len(), known.len());
        // Down: a block stored into a typed parent's slot.
        for op in &func.ops {
            let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op else { continue };
            let (Some(&(r, off)), Some(&vsrc)) = (f.base_of.get(&args[0]), f.handle_of.get(&args[1])) else { continue };
            let Some(&i) = alloc_rec.get(&r) else { continue };
            if let Some(NTy::Rec(j)) = parent_slot_kind(i, slot_of(off)) {
                alloc_rec.entry(vsrc).or_insert(j);
            }
        }
        // A declared block type crosses a copy or a join, in either direction:
        // both arms of an `if` and its result are one value's type.
        for (d, s) in &dups {
            for (x, y) in [(d, s), (s, d)] {
                if let Some(&i) = alloc_rec.get(x) {
                    alloc_rec.entry(*y).or_insert(i);
                }
            }
        }
        for (d, a, b) in &joins {
            if let Some(&i) = [d, a, b].iter().find_map(|v| alloc_rec.get(*v)) {
                for v in [d, a, b] {
                    alloc_rec.entry(*v).or_insert(i);
                }
            }
        }
        // The kinds of copies, joins and slot reads.
        for (d, s) in &dups {
            if let Some(t) = kind_of(s, &known, alloc_rec, &str_lists, &scalar_blocks) {
                known.entry(*d).or_insert(t);
            }
        }
        for (d, a, b) in &joins {
            let ka = kind_of(a, &known, alloc_rec, &str_lists, &scalar_blocks);
            if ka.is_some() && ka == kind_of(b, &known, alloc_rec, &str_lists, &scalar_blocks) {
                known.entry(*d).or_insert(ka.unwrap_or(NTy::I64));
            }
        }
        for op in &func.ops {
            let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
            let Some(&(r, off)) = args.first().and_then(|a| f.base_of.get(a)) else { continue };
            let t = match kind_of(&r, &known, alloc_rec, &str_lists, &scalar_blocks) {
                Some(NTy::Rec(i) | NTy::RecRef(i)) => slot_read_kind(i, slot_of(off)),
                Some(NTy::VecStr | NTy::VecStrRef) => Some(NTy::Str),
                _ => None,
            };
            if let Some(t) = t {
                known.entry(*d).or_insert(t);
            }
        }
        // A list concatenation is a list of its left operand's kind.
        for op in &func.ops {
            let Op::CallFn { dst: Some(d), name, args, .. } = op else { continue };
            let Some(crate::CallArg::Handle(l)) = args.first() else { continue };
            if !crate::native_list_ops::is_list_concat(name) {
                continue;
            }
            let t = kind_of(l, &known, alloc_rec, &str_lists, &scalar_blocks).map(owned);
            if let Some(t @ (NTy::Vec | NTy::VecStr)) = t {
                known.entry(*d).or_insert(t);
            } else if let Some(NTy::Rec(i)) = t.filter(|t| matches!(t, NTy::Rec(i) if rec::list_elem(*i).is_some())) {
                alloc_rec.entry(*d).or_insert(i);
            }
        }
        if (alloc_rec.len(), known.len()) == inner {
            break;
        }
        }
        // Up: an untyped block from the kinds its slots receive.
        for &(r, len) in &blocks {
            if alloc_rec.contains_key(&r) || str_lists.contains(&r) || scalar_blocks.contains(&r) {
                continue;
            }
            let mut stores: Vec<(Option<usize>, NTy)> = Vec::new();
            let mut complete = true;
            for op in &func.ops {
                let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op else { continue };
                let Some(&(root, off)) = f.base_of.get(&args[0]) else { continue };
                if root != r {
                    continue;
                }
                let k = match f.handle_of.get(&args[1]) {
                    // A scalar Option stored by value (`native_option_rewrite`).
                    None if known.get(&args[1]) == Some(&NTy::Opt) => Some(NTy::Opt),
                    None => Some(NTy::I64),
                    Some(src) => kind_of(src, &known, alloc_rec, &str_lists, &scalar_blocks).map(owned),
                };
                match k {
                    Some(k) => stores.push((slot_of(off), k)),
                    None => complete = false,
                }
            }
            if !complete {
                continue;
            }
            let mut kinds: Vec<NTy> = stores.iter().map(|s| s.1).collect();
            kinds.dedup();
            kinds.sort_by_key(|k| format!("{k:?}"));
            kinds.dedup();
            match kinds.as_slice() {
                [] | [NTy::I64] => {
                    scalar_blocks.insert(r);
                }
                // A block filled with strings is a List[String] block.
                [NTy::Str] => {
                    str_lists.insert(r);
                }
                [k] => {
                    alloc_rec.insert(r, rec::list_type_of(*k));
                }
                // Scalars and heap values at constant slots: a struct of the
                // slot kinds, when every slot of the block receives one kind.
                _ => {
                    if let Some(fields) = slot_kinds(&stores, len, func.heap_slot_masks.get(&r)) {
                        alloc_rec.insert(r, rec::slots_type_of(&fields));
                    }
                }
            }
        }
        if (alloc_rec.len(), str_lists.len(), scalar_blocks.len(), known.len()) == before {
            break;
        }
    }
    str_lists
}

/// The kind of each slot `0..len` of a block, when every store names a
/// constant slot, every slot is stored, each slot receives one kind, and the
/// lowering's heap-slot mask (when it declares one) names exactly the heap slots.
fn slot_kinds(stores: &[(Option<usize>, NTy)], len: Option<i64>, mask: Option<&Vec<usize>>) -> Option<Vec<NTy>> {
    let n = usize::try_from(len?).ok()?;
    let mut fields: Vec<Option<NTy>> = vec![None; n];
    for (slot, k) in stores {
        let cell = fields.get_mut((*slot)?)?;
        match cell {
            Some(prev) if prev != k => return None,
            _ => *cell = Some(*k),
        }
    }
    let fields: Vec<NTy> = fields.into_iter().collect::<Option<_>>()?;
    if let Some(m) = mask {
        let heap: Vec<usize> = (0..n).filter(|k| fields[*k] != NTy::I64).collect();
        let mut m = m.clone();
        m.sort_unstable();
        if heap != m {
            return None;
        }
    }
    Some(fields)
}

/// The heap slot reads of an owned block that can MOVE the value out instead
/// of copying it: outside every loop, and after the read the block is only
/// dropped, addressed, or read at its other slots. Nothing reads the slot
/// again, so taking it leaves every later observation as the copy would.
/// The second set holds the loads after which nothing reads the block at all
/// (no other slot either): a carrier whose payload moves out reads as empty
/// after it, so only those may move a carrier's payload.
pub(super) fn movable_loads(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> (BTreeSet<ValueId>, BTreeSet<ValueId>) {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    let mut last = BTreeSet::new();
    // The open loops' starts, and where each value is defined.
    let mut loops: Vec<usize> = Vec::new();
    let mut def_at: BTreeMap<ValueId, usize> = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => loops.push(i),
            Op::LoopEnd => {
                loops.pop();
            }
            _ => {}
        }
        if let Some(v) = crate::mir_ops::defined_value(op) {
            def_at.entry(v).or_insert(i);
        }
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(&(b, Some(off))) = args.first().and_then(|a| base_of.get(a)) else { continue };
        let Some(k) = slot_of(Some(off)) else { continue };
        // Inside a loop the block must be born in the same iteration (defined
        // after the innermost loop's start, never rebound): a block from
        // before the loop would be read again by the next iteration.
        if let Some(&ls) = loops.last() {
            let fresh = def_at.get(&b).is_some_and(|d| *d > ls)
                && !func.ops.iter().any(|o| matches!(o, Op::SetLocal { local, .. } if *local == b));
            if !fresh {
                continue;
            }
        }
        let mut movable = true;
        let mut other_slot = false;
        for later in &func.ops[i + 1..] {
            let fine = match later {
                Op::Drop { v } | Op::DropListStr { v } | Op::DropVariant { v, .. } | Op::DropWrapperRec { v, .. } => *v == b,
                Op::Prim { kind: PrimKind::Handle, args, .. } => args.first() == Some(&b),
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } => rooted(a, b) || rooted(y, b),
                Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, args, .. } => args.first().is_some_and(|a| {
                    base_of.get(a).is_some_and(|(r, o)| *r == b && o.is_some() && slot_of(*o).is_some_and(|s| s != k))
                }),
                _ => false,
            };
            if fine {
                other_slot |= matches!(later, Op::Prim { kind: PrimKind::Load { .. } | PrimKind::LoadHandle, .. });
                continue;
            }
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            if reads.iter().any(|v| *v == b || rooted(v, b)) {
                movable = false;
                break;
            }
        }
        if movable {
            out.insert(*d);
            if !other_slot {
                last.insert(*d);
            }
        }
    }
    (out, last)
}

/// The field count of a generated plain struct (not a list, an enum or a carrier).
pub(super) fn struct_width(i: u16) -> Option<usize> {
    if rec::list_elem(i).is_some() || rec::enum_cases(i).is_some() || carrier::is_carrier(i) {
        return None;
    }
    let n = (0..).take_while(|k| rec::rec_field(i, *k).is_some()).count();
    (n > 0).then_some(n)
}

/// The struct blocks the render builds whole, `Struct { f0, … }`, instead of
/// a default value whose fields are then assigned: each field is stored
/// exactly once at a constant slot, in the alloc's own straight-line run
/// (no `if` or loop marker before the last store), and nothing reads the
/// block before then but its own address arithmetic.
/// List blocks filled densely by one loop: allocated with length `n`, then a
/// loop whose counter starts at 0, steps by 1 and exits only on `c < n` failing
/// stores slot `c` exactly once per iteration (at the loop's own level, not
/// under an `if`) and reads nothing else of the block. Every slot is written in
/// order, so the block is reserved and pushed (`list.map`'s defunctionalized
/// fill), as the standard codegen collects it.
pub(super) fn dense_loop_fills(func: &MirFunction, consts: &BTreeMap<ValueId, i64>) -> BTreeSet<ValueId> {
    let ops = &func.ops;
    let mut out = BTreeSet::new();
    for (i, op) in ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { len } | Init::DynListStr { len }, .. } = op else { continue };
        let b = *b;
        let len = *len;
        // Up to the loop: only the block's handle is taken.
        let mut j = i + 1;
        let mut handle: Option<ValueId> = None;
        let mut ok = true;
        while j < ops.len() && !matches!(ops[j], Op::LoopStart) {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(&ops[j], &mut reads);
            match &ops[j] {
                Op::Prim { kind: PrimKind::Handle, dst: Some(h), args } if args.as_slice() == [b] => handle = Some(*h),
                Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => ok = false,
                _ if reads.contains(&b) || handle.is_some_and(|h| reads.contains(&h)) => ok = false,
                _ => {}
            }
            if !ok {
                break;
            }
            j += 1;
        }
        let (Some(h), true, Some(Op::LoopStart)) = (handle, ok, ops.get(j)) else { continue };
        // The loop: `c < len`, break unless, body, `c = c + 1`, LoopEnd.
        let Some(Op::IntBinOp { dst: t, op: crate::IntOp::Lt, a: c, b: n }) = ops.get(j + 1) else { continue };
        let c = *c;
        if *n != len || !matches!(ops.get(j + 2), Some(Op::LoopBreakUnless { cond }) if cond == t) {
            continue;
        }
        if !ops[..j].iter().any(|o| matches!(o, Op::ConstInt { dst, value: 0 } if *dst == c)) {
            continue;
        }
        let mut depth = 0i32;
        let mut end = None;
        for (k, o) in ops.iter().enumerate().skip(j + 3) {
            match o {
                Op::LoopStart | Op::IfThen { .. } => depth += 1,
                Op::EndIf { .. } => depth -= 1,
                Op::LoopEnd if depth == 0 => {
                    end = Some(k);
                    break;
                }
                Op::LoopEnd => depth -= 1,
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        let body = &ops[j + 3..end];
        // The step: `c' = c + 1; c = c'` right before LoopEnd.
        let step = body.len() >= 2
            && matches!(body[body.len() - 1], Op::SetLocal { local, src } if local == c && matches!(body[body.len() - 2], Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: one } if dst == src && a == c && consts.get(&one) == Some(&1)));
        if !step {
            continue;
        }
        // Addresses into the block: h + const, + c * 8.
        let mut scaled: BTreeSet<ValueId> = BTreeSet::new();
        let mut base12: BTreeSet<ValueId> = BTreeSet::new();
        let mut slot_c: BTreeSet<ValueId> = BTreeSet::new();
        let mut stores = 0usize;
        let mut fine = true;
        let mut d = 0i32;
        for o in body {
            match o {
                Op::IfThen { .. } | Op::LoopStart => d += 1,
                Op::EndIf { .. } | Op::LoopEnd => d -= 1,
                _ => {}
            }
            match o {
                Op::LoopBreakUnless { .. } | Op::Return { .. } => fine = false,
                Op::SetLocal { local, .. } if *local == c && !matches!(body.last(), Some(l) if std::ptr::eq(l, o)) => fine = false,
                Op::IntBinOp { dst, op: crate::IntOp::Mul, a, b: w } if *a == c && consts.get(w) == Some(&8) => {
                    scaled.insert(*dst);
                }
                Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: k } if *a == h && consts.get(k) == Some(&12) => {
                    base12.insert(*dst);
                }
                Op::IntBinOp { dst, op: crate::IntOp::Add, a, b: x } if base12.contains(a) && scaled.contains(x) => {
                    slot_c.insert(*dst);
                }
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| slot_c.contains(a)) => {
                    if d != 0 {
                        fine = false;
                    }
                    stores += 1;
                }
                other => {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(other, &mut reads);
                    if reads.contains(&b) || reads.iter().any(|v| *v == h || base12.contains(v) || slot_c.contains(v)) {
                        fine = false;
                    }
                }
            }
            if !fine {
                break;
            }
        }
        if fine && stores == 1 {
            out.insert(b);
        }
    }
    out
}

/// List blocks of a constant length whose every slot is stored once, in slot
/// order, right after the allocation (only address arithmetic between, no
/// read of the block, no control flow): reserved and pushed, rather than
/// filled with defaults first and each default replaced. Slots past the
/// header are the elements, so the k-th store is the k-th push.
pub(super) fn filled_in_order(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { len } | Init::DynListStr { len }, .. } = op else { continue };
        let Some(n) = consts.get(len).and_then(|n| usize::try_from(*n).ok()).filter(|n| *n > 0) else { continue };
        let mut next = 0usize;
        for later in &func.ops[i + 1..] {
            let ok = match later {
                Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => false,
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } if rooted(a, *b) || rooted(y, *b) => true,
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    let k = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o));
                    if k == Some(next) && !rooted(&args[1], *b) {
                        next += 1;
                        true
                    } else {
                        false
                    }
                }
                other => {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(other, &mut reads);
                    !reads.iter().any(|v| v == b || rooted(v, *b))
                }
            };
            if !ok {
                break;
            }
            if next == n {
                out.insert(*b);
                break;
            }
        }
    }
    out
}

/// `List[String]` blocks that are a constant: a constant length, every slot
/// stored once in order right after the allocation (`filled_in_order`) with a
/// string literal, and afterwards only read — handed to a floor call's
/// borrowed list param, or released. Such a block is the same value on every
/// run of its function (a top-level `let` table the lowering re-materializes
/// at each use), so the render builds it once, as v0 keeps a top-level `let`.
/// Returns each block's literals in slot order.
pub(super) fn const_str_lists(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeMap<ValueId, Vec<String>> {
    let filled = filled_in_order(func, base_of, consts);
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let lits: BTreeMap<ValueId, &String> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: Init::Str(s), .. } => Some((*dst, s)),
            _ => None,
        })
        .collect();
    let handle_of: BTreeMap<ValueId, ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 => Some((*d, args[0])),
            _ => None,
        })
        .collect();
    let mut out = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynListStr { len }, .. } = op else { continue };
        if !filled.contains(b) {
            continue;
        }
        let Some(n) = consts.get(len).and_then(|n| usize::try_from(*n).ok()) else { continue };
        let mut elems: Vec<String> = Vec::new();
        let mut ok = true;
        for later in &func.ops[i + 1..] {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            let touches = reads.iter().any(|v| v == b || rooted(v, *b));
            let fine = match later {
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    match handle_of.get(&args[1]).and_then(|s| lits.get(s)) {
                        Some(s) if elems.len() < n => {
                            elems.push((*s).clone());
                            true
                        }
                        _ => false,
                    }
                }
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, .. } if touches => true,
                Op::DropListStr { v } if v == b => true,
                Op::CallFn { name, args, .. } if touches => args.iter().enumerate().all(|(k, a)| match a {
                    crate::CallArg::Handle(v) if v == b => crate::native_floor::borrows_list(name, k),
                    crate::CallArg::Handle(v) | crate::CallArg::Scalar(v) => !rooted(v, *b),
                    _ => true,
                }),
                _ => !touches,
            };
            if !fine {
                ok = false;
                break;
            }
        }
        if ok && elems.len() == n {
            out.insert(*b, elems);
        }
    }
    out
}

pub(super) fn built_whole(
    func: &MirFunction,
    alloc_rec: &BTreeMap<ValueId, u16>,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { .. }, .. } = op else { continue };
        let Some(n) = alloc_rec.get(b).and_then(|r| struct_width(*r)) else { continue };
        let mut stored = vec![false; n];
        let mut whole = false;
        for later in &func.ops[i + 1..] {
            let ok = match later {
                Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => false,
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } if rooted(a, *b) || rooted(y, *b) => true,
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    match base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o)).filter(|k| *k < n) {
                        Some(k) if !stored[k] && !rooted(&args[1], *b) => {
                            stored[k] = true;
                            true
                        }
                        _ => false,
                    }
                }
                other => {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(other, &mut reads);
                    !reads.iter().any(|v| v == b || rooted(v, *b))
                }
            };
            if !ok {
                break;
            }
            if stored.iter().all(|s| *s) {
                whole = true;
                break;
            }
        }
        if whole {
            out.insert(*b);
        }
    }
    out
}

/// Enum blocks built whole: the tag stored first as a constant naming one of
/// the enum's declared cases, then each payload slot of that case stored
/// exactly once, with nothing reading the block in between. The render then
/// builds the case in one expression (`T::C{t}(f1, Box::new(f2), ..)`) where
/// it would otherwise construct the case with default payloads and replace
/// each one (a boxed default allocated and freed per heap slot). The value
/// is the tag and the case's payload width.
pub(super) fn enum_built_whole(
    func: &MirFunction,
    alloc_rec: &BTreeMap<ValueId, u16>,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeMap<ValueId, (u32, usize)> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynList { .. }, .. } = op else { continue };
        let Some(cases) = alloc_rec.get(b).and_then(|r| rec::enum_cases(*r)) else { continue };
        let mut case: Option<(u32, Vec<bool>)> = None;
        let mut whole = false;
        for later in &func.ops[i + 1..] {
            let ok = match later {
                Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => false,
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } if rooted(a, *b) || rooted(y, *b) => true,
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    let k = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o));
                    match (k, case.as_mut()) {
                        // The tag, first: a constant that names a declared case.
                        (Some(0), None) => match consts.get(&args[1]).and_then(|t| u32::try_from(*t).ok()) {
                            Some(t) => match cases.iter().find(|(c, _)| *c == t) {
                                Some((_, fs)) => {
                                    case = Some((t, vec![false; fs.len()]));
                                    true
                                }
                                None => false,
                            },
                            None => false,
                        },
                        (Some(k), Some((_, stored))) if k >= 1 && k <= stored.len() && !stored[k - 1] && !rooted(&args[1], *b) => {
                            stored[k - 1] = true;
                            true
                        }
                        _ => false,
                    }
                }
                other => {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(other, &mut reads);
                    !reads.iter().any(|v| v == b || rooted(v, *b))
                }
            };
            if !ok {
                break;
            }
            if case.as_ref().is_some_and(|(_, s)| s.iter().all(|x| *x)) {
                whole = true;
                break;
            }
        }
        if let (true, Some((t, s))) = (whole, case) {
            out.insert(*b, (t, s.len()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::slot_kinds;
    use crate::render_native::NTy;

    #[test]
    fn a_block_of_mixed_constant_slots_is_a_struct_of_its_slot_kinds() {
        let stores = [(Some(0), NTy::I64), (Some(1), NTy::Str)];
        assert_eq!(slot_kinds(&stores, Some(2), None), Some(vec![NTy::I64, NTy::Str]));
        assert_eq!(slot_kinds(&stores, Some(2), Some(&vec![1])), Some(vec![NTy::I64, NTy::Str]));
    }

    /// A block the stores do not type whole stays untyped (it walls): two
    /// kinds into one slot, an unstored slot, an indexed store, a store past
    /// the length, an unknown length, a heap-slot mask that disagrees.
    #[test]
    fn a_block_whose_stores_disagree_stays_untyped() {
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(0), NTy::Str), (Some(1), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(1), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (None, NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(2), NTy::Str)], Some(2), None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(1), NTy::Str)], None, None), None);
        assert_eq!(slot_kinds(&[(Some(0), NTy::I64), (Some(1), NTy::Str)], Some(2), Some(&vec![0])), None);
    }
}

/// One-element string lists that exist only to be appended (#4006, the heap
/// twin of `render_native_append::list_push`): `acc + [x]` lowers to a
/// `DynListStr` block of length 1, its slot 0 stored once, then the
/// self-append rebind `d = acc + t; drop acc; acc = d`. The render keeps the
/// element in a local and pushes it (`acc.push(e)`), where it would allocate
/// a one-element Vec per append and free it after. Nothing else may read the
/// block: between the alloc and the append only its own address arithmetic
/// and the one store name it, and after the append only its drop.
pub(super) fn single_elem_lists(
    func: &MirFunction,
    base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
    consts: &BTreeMap<ValueId, i64>,
) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Alloc { dst: b, init: Init::DynListStr { len }, .. } = op else { continue };
        if consts.get(len) != Some(&1) {
            continue;
        }
        let mut stored = false;
        // (the op that takes the element, the first op after it)
        let mut window = None;
        for (j, later) in func.ops.iter().enumerate().skip(i + 1) {
            if let Some((_, x, _)) = crate::render_native_append::self_append_window(func, j) {
                if x == *b {
                    window = stored.then_some((j, j + 3));
                    break;
                }
            }
            // The right side of a list `+`: the element is pushed onto the copy.
            if let Op::CallFn { name, args, .. } = later {
                if crate::native_list_ops::is_list_concat(name)
                    && matches!(args.as_slice(), [crate::CallArg::Handle(a), crate::CallArg::Handle(r)] if r == b && a != b && !rooted(a, *b))
                {
                    window = stored.then_some((j, j + 1));
                    break;
                }
            }
            let ok = match later {
                Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. } => false,
                Op::Prim { kind: PrimKind::Handle, args, .. } if args.first() == Some(b) => true,
                Op::IntBinOp { op: crate::IntOp::Add, a, b: y, .. } if rooted(a, *b) || rooted(y, *b) => true,
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.first().is_some_and(|a| rooted(a, *b)) => {
                    let slot0 = base_of.get(&args[0]).and_then(|(_, o)| slot_of(*o)) == Some(0);
                    let fine = slot0 && !stored && !rooted(&args[1], *b) && args[1] != *b;
                    stored = true;
                    fine
                }
                other => {
                    let mut reads = Vec::new();
                    crate::mir_ops::op_reads(other, &mut reads);
                    !reads.iter().any(|v| v == b || rooted(v, *b))
                }
            };
            if !ok {
                break;
            }
        }
        let Some((_, after)) = window else { continue };
        // After the window, only bookkeeping names the block.
        let clean_after = func.ops[after..].iter().all(|later| {
            if crate::render_native_move::bookkeeping(later) {
                return true;
            }
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(later, &mut reads);
            !reads.iter().any(|v| v == b || rooted(v, *b))
        });
        if clean_after && func.ret != Some(*b) {
            out.insert(*b);
        }
    }
    out
}

/// Loop exits written as a flag (#4006): the tail-recursion loop the lowering
/// emits is `loop { if done != 0 { break; } if base { done = 1 } else { step } }`
/// (`done` a local, its test the loop's first ops). The `SetLocal` that raises
/// the flag is the last thing that arm does before the loop comes round and
/// breaks, so the render can break right there — the value of every local is
/// the same — and the loop reads as the counted loop it is (LLVM folds a
/// counted sum where it keeps a flag loop running). Returns the op indices of
/// those `SetLocal`s.
///
/// Taken when the loop opens with exactly `c = 0; t = (flag == c); break
/// unless t` over a flag nothing else in the loop reads, the `SetLocal`
/// writes a nonzero constant into the flag, and from it to the loop's end the
/// ops are only bookkeeping, the closing of statement `if`s, and other arms
/// (skipped whole).
pub(crate) fn flag_exits(func: &MirFunction) -> BTreeSet<usize> {
    let ops = &func.ops;
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    for op in ops {
        if let Op::ConstInt { dst, value } = op {
            consts.insert(*dst, *value);
        }
    }
    let mut out = BTreeSet::new();
    for (l, op) in ops.iter().enumerate() {
        if !matches!(op, Op::LoopStart) {
            continue;
        }
        let (Some(Op::ConstInt { dst: c, value: 0 }), Some(Op::IntBinOp { dst: t, op: crate::IntOp::Eq, a, b }), Some(Op::LoopBreakUnless { cond })) =
            (ops.get(l + 1), ops.get(l + 2), ops.get(l + 3))
        else {
            continue;
        };
        let flag = match (a == c, b == c) {
            (false, true) => *a,
            (true, false) => *b,
            _ => continue,
        };
        if cond != t {
            continue;
        }
        // The loop's extent.
        let mut depth = 0usize;
        let mut end = None;
        for (j, o) in ops.iter().enumerate().skip(l + 1) {
            match o {
                Op::LoopStart => depth += 1,
                Op::LoopEnd if depth == 0 => {
                    end = Some(j);
                    break;
                }
                Op::LoopEnd => depth -= 1,
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        // Nothing in the body reads the flag but its own test; the header's
        // values are read nowhere else.
        let mut reads = Vec::new();
        let mut clean = true;
        for (j, o) in ops.iter().enumerate() {
            // The header's own three ops.
            if (l + 1..=l + 3).contains(&j) {
                continue;
            }
            reads.clear();
            crate::mir_ops::op_reads(o, &mut reads);
            // A write of the flag names it as its slot, not as a read.
            if let Op::SetLocal { local, src } = o {
                if *local == flag && *src != flag {
                    reads.retain(|v| *v != flag);
                }
            }
            if (j > l + 3 && j < end && reads.contains(&flag)) || reads.contains(t) || reads.contains(c) {
                clean = false;
                break;
            }
        }
        if !clean {
            continue;
        }
        for i in l + 4..end {
            let Op::SetLocal { local, src } = &ops[i] else { continue };
            if *local != flag || consts.get(src).is_none_or(|v| *v == 0) {
                continue;
            }
            if exits_to_loop_end(ops, i + 1, end) {
                out.insert(i);
            }
        }
    }
    out
}

/// From `from` to the loop end at `end`, control reaches the end doing
/// nothing: bookkeeping, the close of a statement `if` it is in, and the
/// other arm of one (skipped whole, it does not run).
fn exits_to_loop_end(ops: &[Op], from: usize, end: usize) -> bool {
    let mut k = from;
    while k < end {
        match &ops[k] {
            o if crate::render_native_move::bookkeeping(o) => k += 1,
            Op::EndIf { val: None } => k += 1,
            Op::Else { val: None } => {
                // Skip the else arm to its matching EndIf.
                let mut depth = 0usize;
                k += 1;
                loop {
                    if k >= end {
                        return false;
                    }
                    match &ops[k] {
                        Op::IfThen { .. } => depth += 1,
                        Op::EndIf { .. } if depth == 0 => break,
                        Op::EndIf { .. } => depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                // ops[k] is that EndIf.
                if !matches!(ops[k], Op::EndIf { val: None }) {
                    return false;
                }
                k += 1;
            }
            _ => return false,
        }
    }
    true
}

