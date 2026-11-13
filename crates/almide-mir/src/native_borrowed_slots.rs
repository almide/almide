//! Borrowed loop slots on the native render (#3550 carries, no perf loss).
//!
//! The loop conversion of a self tail call carries a heap parameter in an
//! owned slot: `Dup s = p` on entry, a rebind `Dup n = child; Drop s;
//! SetLocal s = n` per round, and a release after the loop. The standard
//! codegen recurses on `&p` instead. When the slot only ever takes payloads
//! read out of itself and everything else only reads it, it needs no
//! reference of its own: each value it takes is a part of the caller's
//! argument, which the caller keeps alive and unchanged for the whole call.
//! Such a slot becomes the borrowed parameter itself, re-pointed at the
//! payload — no copy on entry (so the caller copies nothing either), no copy
//! per round, no release.
//!
//! A slot qualifies when:
//! * its parameter is a record or variant, named by one op only: its copy
//!   into the slot, at the top level of the body;
//! * every other op naming the slot reads it — its handle, a scalar slot
//!   read, an argument a call only borrows — or releases it;
//! * every rebind takes a copy chain of a payload read through the slot's own
//!   handle in an arm whose tag test proves the variant (so the payload is a
//!   real part of the block, never a fallback), and the chain's copies are
//!   named by the chain and their releases only.
//!
//! The rewrite drops the entry copy (the slot value IS the parameter), the
//! chain copies with their releases, and every release of the slot. The
//! ownership certificate sees a borrowed parameter that is re-pointed at its
//! own payloads and never released, acquired or moved.

use crate::render_native::{NativeSigKind, NativeSigs};
use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    /// fn name → its borrowed slots (the parameters re-pointed in place).
    static SLOTS: std::cell::RefCell<BTreeMap<String, BTreeSet<ValueId>>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// The borrowed slots of `f` (empty unless [`rewrite`] made some).
pub(crate) fn of(f: &MirFunction) -> BTreeSet<ValueId> {
    SLOTS.with(|m| m.borrow().get(&f.name).cloned().unwrap_or_default())
}

/// Rewrite every qualifying slot of every function.
pub(crate) fn rewrite(functions: &mut [MirFunction], sigs: &NativeSigs) {
    SLOTS.with(|m| m.borrow_mut().clear());
    for f in functions.iter_mut() {
        let Some((kinds, _)) = sigs.get(&f.name) else { continue };
        if !f.owned_params.is_empty() || kinds.len() != f.params.len() {
            continue;
        }
        for (k, kind) in kinds.iter().enumerate() {
            if !matches!(kind, NativeSigKind::Rec(_)) {
                continue;
            }
            let Some((g, s, payloads)) = borrow_slot(f, k) else { continue };
            let before = of(f);
            let mut slots = before.clone();
            slots.insert(s);
            SLOTS.with(|m| m.borrow_mut().insert(f.name.clone(), slots));
            // Each payload the slot takes must render as a reference into
            // the caller's block, or the rebind would change the slot's type.
            let reads = crate::render_native_borrow_read::borrow_reads(&g);
            if payloads.iter().all(|p| reads.contains(p)) {
                *f = g;
            } else {
                SLOTS.with(|m| m.borrow_mut().insert(f.name.clone(), before));
            }
        }
    }
}

fn values(op: &Op) -> Vec<ValueId> {
    let mut out = Vec::new();
    crate::mir_ops::op_values(op, &mut out);
    out
}

/// Nesting depth before each op.
fn depths(f: &MirFunction) -> Vec<i32> {
    let mut d = 0i32;
    f.ops
        .iter()
        .map(|op| {
            if matches!(op, Op::EndIf { .. } | Op::LoopEnd) {
                d -= 1;
            }
            let here = if matches!(op, Op::Else { .. }) { d - 1 } else { d };
            if matches!(op, Op::IfThen { .. } | Op::LoopStart) {
                d += 1;
            }
            here
        })
        .collect()
}

/// `f` with parameter `k`'s slot rewritten, when it qualifies: the new
/// function, the slot's value and the payloads its rebinds take.
fn borrow_slot(f: &MirFunction, k: usize) -> Option<(MirFunction, ValueId, Vec<ValueId>)> {
    let p = f.params[k].value;
    let named: Vec<usize> = (0..f.ops.len()).filter(|&i| values(&f.ops[i]).contains(&p)).collect();
    let [entry] = named.as_slice() else { return None };
    let Op::Dup { dst: s, src } = f.ops[*entry] else { return None };
    if src != p || depths(f)[*entry] != 0 || f.ret == Some(s) {
        return None;
    }
    let plan = plan(f, s)?;
    let mut remove: BTreeSet<usize> = plan.removed;
    remove.insert(*entry);
    let mut ops = Vec::with_capacity(f.ops.len());
    for (i, op) in f.ops.iter().enumerate() {
        if remove.contains(&i) {
            continue;
        }
        match (op, plan.rebinds.get(&i)) {
            (Op::SetLocal { local, .. }, Some(&payload)) => ops.push(Op::SetLocal { local: *local, src: payload }),
            _ => ops.push(op.clone()),
        }
    }
    let mut g = f.clone();
    g.ops = ops;
    g.params[k].value = s;
    Some((g, s, plan.rebinds.values().copied().collect()))
}

/// What the rewrite of one slot removes and re-points.
struct Plan {
    /// Op indices that go: the slot's releases, the chain copies and theirs.
    removed: BTreeSet<usize>,
    /// A rebind's op index → the payload read the slot now points at.
    rebinds: BTreeMap<usize, ValueId>,
}

fn plan(f: &MirFunction, s: ValueId) -> Option<Plan> {
    let roots = crate::render_native_borrow_read::roots(f);
    let guarded = crate::native_borrowed_tco::slot_guarded_reads(f, &BTreeSet::from([s]), &BTreeMap::new());
    let defs: BTreeMap<ValueId, usize> = f
        .ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| crate::mir_ops::defined_value(op).map(|v| (v, i)))
        .collect();
    let mut removed = BTreeSet::new();
    let mut rebinds = BTreeMap::new();
    let mut chain_values = BTreeSet::new();
    for (i, op) in f.ops.iter().enumerate() {
        if !values(op).contains(&s) {
            continue;
        }
        match op {
            Op::Dup { dst, .. } if *dst == s => {}
            Op::Prim { kind: PrimKind::Handle, args, .. } if args.as_slice() == [s] => {}
            Op::ListGetScalar { list, idx, .. } if *list == s && *idx != s => {}
            Op::CallFn { name, args, .. } => {
                if !call_only_lends(name, args, s) {
                    return None;
                }
            }
            Op::SetLocal { local, src } if *local == s => {
                let (payload, chain) = payload_chain(f, *src, &defs)?;
                let Op::Prim { args, .. } = &f.ops[defs[&payload]] else { return None };
                if args.first().and_then(|a| roots.get(a)) != Some(&s) || !guarded.contains_key(&payload) {
                    return None;
                }
                rebinds.insert(i, payload);
                for v in chain {
                    removed.insert(defs[&v]);
                    chain_values.insert(v);
                }
            }
            op if crate::native_move_params::is_release(op, s) => {
                removed.insert(i);
            }
            _ => return None,
        }
    }
    remove_chain_releases(f, &chain_values, &rebinds, &mut removed)?;
    (!rebinds.is_empty()).then_some(Plan { removed, rebinds })
}

/// The chain copies are named by the chain, the rebind and their releases:
/// adds each release to `removed`, or `None` when another op names a copy.
fn remove_chain_releases(
    f: &MirFunction,
    chain_values: &BTreeSet<ValueId>,
    rebinds: &BTreeMap<usize, ValueId>,
    removed: &mut BTreeSet<usize>,
) -> Option<()> {
    for (i, op) in f.ops.iter().enumerate() {
        let named: Vec<ValueId> = values(op).into_iter().filter(|v| chain_values.contains(v)).collect();
        if named.is_empty() || removed.contains(&i) || rebinds.contains_key(&i) {
            continue;
        }
        if named.len() == 1 && crate::native_move_params::is_release(op, named[0]) {
            removed.insert(i);
            continue;
        }
        return None;
    }
    Some(())
}

/// A call that names `s` only lends it: never as a scalar, and as a handle
/// only in a position the callee does not write through.
fn call_only_lends(name: &str, args: &[CallArg], s: ValueId) -> bool {
    args.iter().enumerate().all(|(k, a)| match a {
        CallArg::Scalar(v) => *v != s,
        CallArg::Handle(v) => *v != s || !crate::native_hof::mutable_argument(name, k),
        CallArg::Imm(_) | CallArg::Label(_) => true,
    })
}

/// `v` as a chain of copies (`Dup`) of a payload read (`LoadHandle`): the
/// payload read and the copies.
fn payload_chain(f: &MirFunction, mut v: ValueId, defs: &BTreeMap<ValueId, usize>) -> Option<(ValueId, Vec<ValueId>)> {
    let mut chain = Vec::new();
    loop {
        match &f.ops[*defs.get(&v)?] {
            Op::Dup { dst, src } if *dst == v => {
                chain.push(v);
                v = *src;
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if *d == v && args.len() == 1 => {
                return (!chain.is_empty()).then_some((v, chain));
            }
            _ => return None,
        }
    }
}
