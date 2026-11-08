//! A map literal's entries built as Rust pairs, not as lists (#4006,
//! no-perf-loss rule).
//!
//! `["k": v]` lowers to a two-slot list per entry (the pair), a list of those
//! pairs, and a `map.from_list` call over it (`FloorParam::MapPairs`). On the
//! native render each pair was a `Vec` and the entries another `Vec`, copied
//! pair by pair into the runtime's pairs: three allocations per literal where
//! the standard codegen builds the map from an array of pairs.
//!
//! Here, when nothing else reads them, the pair blocks are never built: each
//! slot's value waits in a local (`<pair>_e0`, `<pair>_e1`), the entry list's
//! slot k binds the tuple of its pair's locals (`<list>_e<k>`), and the call
//! takes the array of those tuples. The MIR and its ownership events are
//! unchanged — each value is still stored once and handed to the map once.
//!
//! An entry list qualifies when it is a constant-length block whose every
//! slot is stored once with the handle of a pair block, and whose only other
//! read is the one `MapPairs` argument of a call, its last read. A pair block
//! qualifies when it is a two-slot block whose slots are each stored once and
//! whose only other read is that one store into the entry list.

use crate::lower::layout::{BLOCK_HEADER, SLOT_SIZE};
use crate::native_floor::FloorParam;
use crate::render_native::{var, NTy};
use crate::{CallArg, Init, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Entries {
    /// Entry lists that may stay unbuilt → their pair blocks.
    lists: BTreeMap<ValueId, Vec<ValueId>>,
    /// Pair blocks that may stay unbuilt.
    pairs: BTreeSet<ValueId>,
    /// Pairs left unbuilt by the render → the slots bound so far.
    deferred: BTreeMap<ValueId, usize>,
    /// Entry lists left unbuilt by the render → their element kind.
    unbuilt: BTreeMap<ValueId, NTy>,
}

thread_local! {
    static ENTRIES: RefCell<Entries> = RefCell::new(Entries::default());
}

/// Record the entry lists and pair blocks of the function about to render.
pub(crate) fn begin(func: &MirFunction) {
    let found = entries(func);
    ENTRIES.with(|e| *e.borrow_mut() = found);
}

/// At the alloc of the string list `v`: whether it is a pair left unbuilt.
pub(crate) fn defer_pair(v: ValueId) -> bool {
    ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let pair = e.pairs.contains(&v);
        if pair {
            e.deferred.insert(v, 0);
        }
        pair
    })
}

/// The slot number of the next store into the unbuilt pair `v`.
pub(crate) fn pair_slot(v: ValueId) -> Option<usize> {
    ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let k = e.deferred.get_mut(&v)?;
        *k += 1;
        Some(*k - 1)
    })
}

/// The Rust pair an unbuilt pair block `v` stands for.
pub(crate) fn pair_of(v: ValueId) -> Option<String> {
    let deferred = ENTRIES.with(|e| e.borrow().deferred.contains_key(&v));
    let p = var(v);
    deferred.then(|| format!("({p}_e0, {p}_e1)"))
}

/// At the alloc of the entry list `v` of element kind `elem`: whether it
/// stays unbuilt — its pairs are unbuilt string pairs, or two-field structs.
pub(crate) fn leave_unbuilt(v: ValueId, elem: Option<NTy>) -> bool {
    ENTRIES.with(|e| {
        let mut e = e.borrow_mut();
        let ok = e.lists.get(&v).is_some_and(|pairs| match elem {
            Some(NTy::VecStr) => pairs.iter().all(|p| e.deferred.contains_key(p)),
            Some(NTy::Rec(j)) => crate::render_native_rec::field_count(j) == 2,
            _ => false,
        });
        if let (true, Some(t)) = (ok, elem) {
            e.unbuilt.insert(v, t);
        }
        ok
    })
}

/// `v` is an entry list left unbuilt.
pub(crate) fn unbuilt(v: ValueId) -> bool {
    ENTRIES.with(|e| e.borrow().unbuilt.contains_key(&v))
}

/// The array of pairs the unbuilt entry list spelled `code` hands the map.
pub(crate) fn array_of(code: &str) -> Option<String> {
    ENTRIES.with(|e| {
        let e = e.borrow();
        let (v, t) = e.unbuilt.iter().find(|(v, _)| var(**v) == code)?;
        let n = e.lists.get(v)?.len();
        let pair = |k: usize| match t {
            NTy::VecStr => format!("{code}_e{k}"),
            _ => format!("({code}_e{k}.f0, {code}_e{k}.f1)"),
        };
        Some(format!("[{}]", (0..n).map(pair).collect::<Vec<_>>().join(", ")))
    })
}

fn entries(func: &MirFunction) -> Entries {
    let root = crate::render_native_borrow_read::roots(func);
    let consts: BTreeMap<ValueId, i64> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value } => Some((*dst, *value)),
            _ => None,
        })
        .collect();
    // A slot address: (block, slot number) for `handle + constant`.
    let mut slot_of: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    for op in &func.ops {
        if let Op::IntBinOp { dst, op: IntOp::Add, a, b } = op {
            if let (Some(base), Some(off)) = (root.get(a), consts.get(b)) {
                let k = (off - i64::from(BLOCK_HEADER)) / i64::from(SLOT_SIZE);
                slot_of.insert(*dst, (*base, k));
            }
        }
    }
    // Every slot store, per block: (op index, slot, the stored value's block).
    let mut stores: BTreeMap<ValueId, Vec<(usize, i64, ValueId)>> = BTreeMap::new();
    for (j, op) in func.ops.iter().enumerate() {
        if let Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } = op {
            if let ([_, v], Some((b, k))) = (args.as_slice(), args.first().and_then(|a| slot_of.get(a))) {
                stores.entry(*b).or_default().push((j, *k, root.get(v).copied().unwrap_or(*v)));
            }
        }
    }
    // A block's alloc: (op index, constant length).
    let alloc = |b: &ValueId| {
        func.ops.iter().enumerate().find_map(|(j, op)| match op {
            Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. } if dst == b => consts.get(len).map(|n| (j, *n)),
            _ => None,
        })
    };
    // The ops that read `b` other than as its own handle, address
    // arithmetic, slot stores and releases.
    let other_reads = |b: ValueId| -> Vec<usize> {
        let mut reads = Vec::new();
        let mut out = Vec::new();
        for (j, op) in func.ops.iter().enumerate() {
            reads.clear();
            crate::mir_ops::op_reads(op, &mut reads);
            if !reads.iter().any(|v| *v == b || root.get(v) == Some(&b)) {
                continue;
            }
            let own = match op {
                Op::Prim { kind: PrimKind::Handle, args, .. } => args.as_slice() == [b],
                Op::IntBinOp { .. } => true,
                Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } => {
                    args.first().and_then(|a| slot_of.get(a)).is_some_and(|(x, _)| *x == b)
                }
                Op::Consume { .. } | Op::MakeUnique { .. } => false,
                op => crate::render_native_move::bookkeeping(op),
            };
            if !own {
                out.push(j);
            }
        }
        out
    };
    // Its slots 0..n each stored once, in order.
    let slots_once = |b: ValueId, n: i64| {
        let ks: Vec<i64> = stores.get(&b).map(|s| s.iter().map(|(_, k, _)| *k).collect()).unwrap_or_default();
        ks == (0..n).collect::<Vec<_>>()
    };
    let flow = |op: &Op| {
        matches!(
            op,
            Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopBreakUnless { .. } | Op::LoopEnd
        )
    };
    let mut out = Entries::default();
    for (j, op) in func.ops.iter().enumerate() {
        let Op::CallFn { name, args, .. } = op else { continue };
        let Some(floor) = crate::native_floor::declared(name) else { continue };
        for (p, a) in floor.params.iter().zip(args) {
            let (FloorParam::MapPairs, CallArg::Handle(list)) = (p, a) else { continue };
            let Some((at, n)) = alloc(list).filter(|(_, n)| *n > 0 && slots_once(*list, *n)) else { continue };
            let held: Vec<(usize, ValueId)> = stores[list].iter().map(|(s, _, v)| (*s, *v)).collect();
            // A pair: two slots, read only by its one store into the list.
            let pair = |(s, pb): &(usize, ValueId)| {
                alloc(pb).filter(|(pa, n)| *n == 2 && *pa < at && slots_once(*pb, 2)).map(|(pa, _)| pa)
                    .filter(|_| other_reads(*pb) == [*s])
            };
            let pair_at: Option<Vec<usize>> = held.iter().map(pair).collect();
            let Some(first) = pair_at.and_then(|v| v.into_iter().chain([at]).min()) else { continue };
            let fixed = func.ret != Some(*list) && !func.ops[first..j].iter().any(flow);
            if fixed && other_reads(*list) == [j] {
                out.pairs.extend(held.iter().map(|(_, v)| *v));
                out.lists.insert(*list, held.into_iter().map(|(_, v)| v).collect());
            }
        }
    }
    out
}
