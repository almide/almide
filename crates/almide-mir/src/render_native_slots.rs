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
pub(super) fn movable_loads(func: &MirFunction, base_of: &BTreeMap<ValueId, (ValueId, Option<i64>)>) -> BTreeSet<ValueId> {
    let rooted = |v: &ValueId, b: ValueId| base_of.get(v).is_some_and(|(r, _)| *r == b);
    let mut out = BTreeSet::new();
    let mut depth = 0usize;
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => depth += 1,
            Op::LoopEnd => depth = depth.saturating_sub(1),
            _ => {}
        }
        let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(&(b, Some(off))) = args.first().and_then(|a| base_of.get(a)) else { continue };
        let Some(k) = slot_of(Some(off)) else { continue };
        if depth != 0 {
            continue;
        }
        let mut movable = true;
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
        }
    }
    out
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
