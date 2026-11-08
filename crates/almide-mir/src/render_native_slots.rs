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
        NativeSigKind::Opt => NTy::Opt,
        NativeSigKind::Res => NTy::Res,
        NativeSigKind::ResStr => NTy::ResStr,
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
        FloorRet::Res(_) => Some(NTy::Res),
        FloorRet::ResStr => Some(NTy::ResStr),
        FloorRet::Opt(_) => Some(NTy::Opt),
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
    // A shared Result slot has two possible heap types. Without the tag,
    // neither is a fact about this read; the render chooses under the arm.
    if rec::result_payloads(i).is_some_and(|(a, b)| a != b) {
        return None;
    }
    if rec::enum_cases(i).is_some() {
        let heap: Vec<NTy> = rec::enum_slot_kinds(i, slot?).into_iter().filter(|t| *t != NTy::I64).collect();
        return (heap.len() == 1).then(|| heap[0]);
    }
    parent_slot_kind(i, slot).filter(|t| *t != NTy::I64)
}

/// Seed declarations before call-site inference. A linked self-host body may
/// spell a heap-generic payload as String; that signature must not overwrite
/// the actual caller's List/Option/Result representation before routing.
pub(super) fn declared_blocks(func: &MirFunction) -> BTreeMap<ValueId, u16> {
    let mut alloc_rec = BTreeMap::new();
    // A block the lowering built as a typed aggregate or inlined Value
    // takes its declared type's native representation.
    for (v, key) in &func.aggregate_blocks {
        if key == rec::VALUE_KEY {
            alloc_rec.entry(*v).or_insert_with(rec::value_index);
        } else if let Some(i) = rec::rec_index(key) {
            alloc_rec.entry(*v).or_insert(i);
        }
    }
    alloc_rec
}

/// Type the untyped slot blocks of `func` into `alloc_rec` (block → generated
/// type) and return the blocks that are `List[String]`.
pub(super) fn type_blocks(
    func: &MirFunction,
    sigs: &NativeSigs,
    alloc_rec: &mut BTreeMap<ValueId, u16>,
    f: &Facts<'_>,
) -> BTreeSet<ValueId> {
    let mut tb = BlockTyping {
        func,
        sigs,
        alloc_rec,
        f,
        known: BTreeMap::new(),
        blocks: Vec::new(),
        joins: Vec::new(),
        concats: Vec::new(),
        dups: Vec::new(),
        str_lists: BTreeSet::new(),
        scalar_blocks: BTreeSet::new(),
    };
    // The kinds values have before any block is typed.
    tb.seed_params();
    // DynList blocks (and their declared length), the joins, the copies.
    tb.scan_ops();
    let blocks = tb.blocks.clone();
    loop {
        let before = (tb.alloc_rec.len(), tb.str_lists.len(), tb.scalar_blocks.len(), tb.known.len());
        // Down and across first, to their own fixpoint: a declared kind
        // reaches every block it can before any block is typed from its
        // stores, so a store-typed struct never stands in for a declared type.
        tb.down_and_across();
        // Up: an untyped block from the kinds its slots receive.
        for &(r, len) in &blocks {
            tb.type_from_stores(r, len);
        }
        if (tb.alloc_rec.len(), tb.str_lists.len(), tb.scalar_blocks.len(), tb.known.len()) == before {
            break;
        }
    }
    tb.str_lists
}

/// The state of [`type_blocks`]' fixpoint over one function.
struct BlockTyping<'a, 'f> {
    func: &'a MirFunction,
    sigs: &'a NativeSigs,
    alloc_rec: &'a mut BTreeMap<ValueId, u16>,
    f: &'a Facts<'f>,
    known: BTreeMap<ValueId, NTy>,
    blocks: Vec<(ValueId, Option<i64>)>,
    joins: Vec<(ValueId, ValueId, ValueId)>,
    /// List concatenations `dst = a + b`: one Almide list type, so one
    /// native kind for all three.
    concats: Vec<(ValueId, ValueId, ValueId)>,
    dups: Vec<(ValueId, ValueId)>,
    str_lists: BTreeSet<ValueId>,
    /// Untyped blocks every store of which is a scalar: the `Vec<i64>` default.
    scalar_blocks: BTreeSet<ValueId>,
}

impl BlockTyping<'_, '_> {
    fn seed_params(&mut self) {
        let func = self.func;
        if let Some((ps, _)) = self.sigs.get(func.name.as_str()).filter(|(ps, _)| ps.len() == func.params.len()) {
            for (p, k) in func.params.iter().zip(ps) {
                if let Some(t) = heap_of_sig(*k) {
                    self.known.insert(p.value, t);
                }
            }
        }
    }

    fn scan_ops(&mut self) {
        let func = self.func;
        let mut open: Vec<(Option<ValueId>, Option<ValueId>)> = Vec::new();
        for op in &func.ops {
            self.scan_op(op, &mut open);
        }
    }

    fn scan_op(&mut self, op: &Op, open: &mut Vec<(Option<ValueId>, Option<ValueId>)>) {
        match op {
            Op::Alloc { dst, init: Init::Str(_) | Init::DynStr { .. }, .. } => {
                self.known.insert(*dst, NTy::Str);
            }
            Op::Prim { kind: PrimKind::OptMakeSome | PrimKind::OptMakeNone | PrimKind::OptCopy, dst: Some(d), .. } => {
                self.known.insert(*d, NTy::Opt);
            }
            Op::Prim { kind: PrimKind::ResMakeOk | PrimKind::ResMakeErrStr, dst: Some(d), .. } => {
                self.known.insert(*d, NTy::Res);
            }
            Op::Prim { kind: PrimKind::ResStrMake, dst: Some(d), .. } => {
                self.known.insert(*d, NTy::ResStr);
            }
            Op::Prim { kind: PrimKind::ResCopy, dst: Some(d), args } if args.len() == 1 => self.dups.push((*d, args[0])),
            Op::Alloc { dst, init: Init::IntList(_), .. } | Op::ListLit { dst, .. } if !self.alloc_rec.contains_key(dst) => {
                self.known.insert(*dst, NTy::Vec);
            }
            Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. } => {
                self.blocks.push((*dst, self.f.consts.get(len).copied()));
            }
            Op::CallFn { dst: Some(d), name, args, .. } => {
                if let Some(t) = call_result(name, self.sigs) {
                    self.known.insert(*d, t);
                }
                if let (true, [crate::CallArg::Handle(a), crate::CallArg::Handle(b)]) =
                    (crate::native_list_ops::is_list_concat(name), args.as_slice())
                {
                    self.concats.push((*d, *a, *b));
                }
            }
            Op::Dup { dst, src } | Op::SetLocal { local: dst, src } => self.dups.push((*dst, *src)),
            Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } => self.scan_join(op, open),
            _ => {}
        }
    }

    /// An `if` value join: its result and its two arms' values.
    fn scan_join(&mut self, op: &Op, open: &mut Vec<(Option<ValueId>, Option<ValueId>)>) {
        match op {
            Op::IfThen { dst, .. } => open.push((*dst, None)),
            Op::Else { val } => {
                if let Some(top) = open.last_mut() {
                    top.1 = *val;
                }
            }
            Op::EndIf { val } => {
                if let Some((Some(d), Some(t))) = open.pop() {
                    if let Some(e) = val {
                        self.joins.push((d, t, *e));
                    }
                }
            }
            _ => {}
        }
    }

    fn kind_of(&self, v: &ValueId) -> Option<NTy> {
        if let Some(i) = self.alloc_rec.get(v) {
            Some(NTy::Rec(*i))
        } else if self.str_lists.contains(v) {
            Some(NTy::VecStr)
        } else if self.scalar_blocks.contains(v) {
            Some(NTy::Vec)
        } else {
            self.known.get(v).copied()
        }
    }

    fn down_and_across(&mut self) {
        loop {
            let inner = (self.alloc_rec.len(), self.known.len());
            self.down();
            self.across();
            self.copy_kinds();
            self.read_kinds();
            if (self.alloc_rec.len(), self.known.len()) == inner {
                break;
            }
        }
    }

    /// Down: a block stored into a typed parent's slot.
    fn down(&mut self) {
        let (func, f) = (self.func, self.f);
        let empty = super::empty_list_blocks(func);
        for op in &func.ops {
            let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op else { continue };
            let (Some(&(r, off)), Some(&vsrc)) = (f.base_of.get(&args[0]), f.handle_of.get(&args[1])) else { continue };
            let Some(&i) = self.alloc_rec.get(&r) else { continue };
            match parent_slot_kind(i, slot_of(off)) {
                Some(NTy::Rec(j)) => { self.alloc_rec.entry(vsrc).or_insert(j); }
                // An empty literal has no element store to establish String.
                // Preserve the parent's declared list type at allocation;
                // never discard or reinterpret an already populated list.
                Some(NTy::VecStr) if empty.contains(&vsrc) => {
                    self.alloc_rec.entry(vsrc).or_insert_with(|| rec::list_type_of(NTy::Str));
                }
                _ => {}
            }
        }
    }

    /// A declared block type crosses a copy or a join, in either direction:
    /// both arms of an `if` and its result are one value's type.
    fn across(&mut self) {
        // Preserve nominal types before inferring anonymous slot structs.
        for (v, t) in &self.known {
            if let NTy::Rec(i) = t { self.alloc_rec.entry(*v).or_insert(*i); }
        }
        for (d, s) in &self.dups {
            for (x, y) in [(d, s), (s, d)] {
                if let Some(&i) = self.alloc_rec.get(x) {
                    self.alloc_rec.entry(*y).or_insert(i);
                }
            }
        }
        for (d, a, b) in &self.joins {
            if let Some(&i) = [d, a, b].iter().find_map(|v| self.alloc_rec.get(*v)) {
                for v in [d, a, b] {
                    self.alloc_rec.entry(*v).or_insert(i);
                }
            }
        }
        self.across_concats();
        self.empty_string_lists();
    }

    fn empty_string_lists(&mut self) {
        let empty = super::empty_list_blocks(self.func);
        let edges = self.dups.iter().copied().chain(self.joins.iter().flat_map(|(d, a, b)| [(*d, *a), (*d, *b), (*a, *b)]));
        let mut pending = Vec::new();
        for (a, b) in edges {
            for (src, dst) in [(a, b), (b, a)] {
                if self.kind_of(&src) == Some(NTy::VecStr) && empty.contains(&dst) && !self.alloc_rec.contains_key(&dst) {
                    pending.push(dst);
                }
            }
        }
        for v in pending { self.scalar_blocks.remove(&v); self.str_lists.insert(v); self.known.insert(v, NTy::VecStr); }
    }

    /// A list concatenation's operands and result are one list type: a
    /// generated list kind on one of them reaches an operand that is an
    /// empty, never-stored block (`var xs: List[List[Int]] = []` before
    /// `xs = xs + [[…]]`), which no store can type. A block typed any other
    /// way keeps its kind, so a real disagreement still walls at the render.
    fn across_concats(&mut self) {
        let concats = self.concats.clone();
        let empty = super::empty_list_blocks(self.func);
        for (d, a, b) in concats {
            let Some(t) = [d, a, b].iter().find_map(|v| match self.kind_of(v) {
                Some(NTy::VecStr | NTy::VecStrRef) => Some(NTy::VecStr),
                Some(NTy::Rec(i) | NTy::RecRef(i)) if rec::list_elem(i).is_some() => {
                    Some(if rec::list_elem(i) == Some(NTy::Str) { NTy::VecStr } else { NTy::Rec(i) })
                }
                _ => None,
            }) else { continue };
            for v in [a, b] {
                if self.alloc_rec.contains_key(&v) { continue; }
                if (empty.contains(&v) || self.blocks.iter().any(|(r, _)| *r == v))
                    && self.block_stores(v).is_some_and(|s| s.is_empty()) {
                    self.scalar_blocks.remove(&v);
                    if let NTy::Rec(i) = t {
                        self.str_lists.remove(&v);
                        self.alloc_rec.insert(v, i);
                    } else {
                        self.str_lists.insert(v);
                        self.known.insert(v, t);
                    }
                }
            }
            // Keep the existing scalar/string-list representation. Generated
            // lists carry their element declaration through later concats too.
            if let NTy::Rec(i) = t { self.alloc_rec.entry(d).or_insert(i); }
            else { self.known.entry(d).or_insert(t); }
        }
    }

    /// The kinds of copies and joins.
    fn copy_kinds(&mut self) {
        for (d, s) in &self.dups {
            if let Some(t) = self.kind_of(s) {
                self.known.entry(*d).or_insert(t);
            }
        }
        for (d, a, b) in &self.joins {
            let ka = self.kind_of(a);
            if ka.is_some() && ka == self.kind_of(b) {
                self.known.entry(*d).or_insert(ka.unwrap_or(NTy::I64));
            }
        }
    }

    /// The kinds of slot reads.
    fn read_kinds(&mut self) {
        let (func, f) = (self.func, self.f);
        for op in &func.ops {
            let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
            let Some(&(r, off)) = args.first().and_then(|a| f.base_of.get(a)) else { continue };
            let t = match self.kind_of(&r) {
                Some(NTy::Rec(i) | NTy::RecRef(i)) => slot_read_kind(i, slot_of(off)),
                Some(NTy::VecStr | NTy::VecStrRef) => Some(NTy::Str),
                _ => None,
            };
            if let Some(t) = t {
                self.known.entry(*d).or_insert(t);
            }
        }
        // A list concatenation is a list of its left operand's kind.
        for op in &func.ops {
            let Op::CallFn { dst: Some(d), name, args, .. } = op else { continue };
            let Some(crate::CallArg::Handle(l)) = args.first() else { continue };
            if !crate::native_list_ops::is_list_concat(name) {
                continue;
            }
            let t = self.kind_of(l).map(owned);
            if let Some(t @ (NTy::Vec | NTy::VecStr)) = t {
                self.known.entry(*d).or_insert(t);
            } else if let Some(NTy::Rec(i)) = t.filter(|t| matches!(t, NTy::Rec(i) if rec::list_elem(*i).is_some())) {
                self.alloc_rec.entry(*d).or_insert(i);
            }
        }
    }

    /// The (slot, kind) of every store into block `r`; `None` when some
    /// stored value has no kind yet.
    fn block_stores(&self, r: ValueId) -> Option<Vec<(Option<usize>, NTy)>> {
        let mut stores: Vec<(Option<usize>, NTy)> = Vec::new();
        let mut complete = true;
        for op in &self.func.ops {
            let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op else { continue };
            let Some(&(root, off)) = self.f.base_of.get(&args[0]) else { continue };
            if root != r {
                continue;
            }
            let k = match self.f.handle_of.get(&args[1]) {
                // A scalar Option stored by value (`native_option_rewrite`).
                None if self.known.get(&args[1]) == Some(&NTy::Opt) => Some(NTy::Opt),
                None => Some(NTy::I64),
                Some(src) => self.kind_of(src).map(owned),
            };
            match k {
                Some(k) => stores.push((slot_of(off), k)),
                None => complete = false,
            }
        }
        complete.then_some(stores)
    }

    /// Up: untyped block `r` from the kinds its slots receive.
    fn type_from_stores(&mut self, r: ValueId, len: Option<i64>) {
        if self.alloc_rec.contains_key(&r) || self.str_lists.contains(&r) || self.scalar_blocks.contains(&r) {
            return;
        }
        let Some(stores) = self.block_stores(r) else { return };
        let mut kinds: Vec<NTy> = stores.iter().map(|s| s.1).collect();
        kinds.dedup();
        kinds.sort_by_key(|k| format!("{k:?}"));
        kinds.dedup();
        match kinds.as_slice() {
            [] | [NTy::I64] => {
                self.scalar_blocks.insert(r);
            }
            // A block filled with strings is a List[String] block.
            [NTy::Str] => {
                self.str_lists.insert(r);
            }
            [k] => {
                self.alloc_rec.insert(r, rec::list_type_of(*k));
            }
            // Scalars and heap values at constant slots: a struct of the
            // slot kinds, when every slot of the block receives one kind.
            _ => {
                if let Some(fields) = slot_kinds(&stores, len, self.func.heap_slot_masks.get(&r)) {
                    self.alloc_rec.insert(r, rec::slots_type_of(&fields));
                }
            }
        }
    }
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

include!("render_native_slots_b.rs");

/// A borrow-view stores existing handles without consuming them. Native Vec
/// elements own their values, so those stores copy; an owned constructor's
/// Store/Consume pair can still move. Any less direct ownership shape copies.
pub(super) fn borrowed_stores(
    func: &MirFunction,
    handles: &BTreeMap<ValueId, ValueId>,
    bases: &BTreeMap<ValueId, (ValueId, Option<i64>)>,
) -> BTreeSet<(ValueId, ValueId)> {
    func.ops.iter().enumerate().filter_map(|(at, op)| {
        let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op else { return None };
        let [address, value] = args.as_slice() else { return None };
        let src = handles.get(value)?;
        let (base, _) = bases.get(address)?;
        let consumed = matches!(func.ops.get(at + 1), Some(Op::Consume { v }) if v == src) || handed_to_store(func, at, *src, *value);
        (!consumed).then_some((*base, *value))
    }).collect()
}

/// The value `src` was handed over (`Consume`) before the store at `ops[at]`
/// and nothing but the store's own handle names it from that hand-over on: a
/// call result consumed where it is bound (a `map` callback's value) and then
/// stored. The store is the consumer, so the element takes the value itself.
fn handed_to_store(func: &MirFunction, at: usize, src: ValueId, handle: ValueId) -> bool {
    let Some(c) = func.ops[..at].iter().rposition(|op| matches!(op, Op::Consume { v } if *v == src)) else { return false };
    let mut vs = Vec::new();
    func.ops.iter().enumerate().skip(c + 1).all(|(j, op)| {
        if j == at {
            return true;
        }
        vs.clear();
        crate::mir_ops::op_values(op, &mut vs);
        let handle_def = matches!(op, Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if *d == handle && args.as_slice() == [src]);
        handle_def || (!vs.contains(&src) && (j > at || !vs.contains(&handle)))
    })
}

#[cfg(test)]
mod store_ownership_tests {
    use super::*;

    #[test]
    fn repeated_borrowed_elements_copy_and_explicit_handoffs_move() {
        let store = |address, value| Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None,
            args: vec![ValueId(address), ValueId(value)] };
        let f = MirFunction { ops: vec![
            store(10, 20), store(11, 21), Op::Consume { v: ValueId(1) },
            store(12, 22), Op::Consume { v: ValueId(9) },
        ], ..Default::default() };
        let handles = BTreeMap::from([(ValueId(20), ValueId(1)), (ValueId(21), ValueId(1)), (ValueId(22), ValueId(2))]);
        let bases = BTreeMap::from([(ValueId(10), (ValueId(3), Some(12))), (ValueId(11), (ValueId(3), Some(20))), (ValueId(12), (ValueId(3), Some(28)))]);
        assert_eq!(borrowed_stores(&f, &handles, &bases), BTreeSet::from([(ValueId(3), ValueId(20)), (ValueId(3), ValueId(22))]));
    }
}
