//! #4006 C1 class D: a heap slot read that the slot's own take follows.
//!
//! The shared lowering assigns a module-level heap binding as a read of it
//! (`LoadHandle`, a borrow its reader `Dup`s), the new value computed from
//! that copy, then `__mg_take` (the old value, dropped) and the `Store` of
//! the new one. The native render keeps the slot's value in a cell, so the
//! read is a copy of the whole value, and the take throws the original away.
//!
//! When nothing between the read and the take can see the slot, the read
//! moves the value out instead, and the take finds the cell holding its
//! type's default. The ops in between run in straight line (no branch, loop
//! edge or return that could leave the slot empty), touch no other copy of
//! that slot, and call only functions that touch no heap slot at all, so the
//! empty cell is never observed; the take's own value is only dropped. The
//! MIR and its certificates do not change: only the read is spelled as a
//! move.

use crate::{CallArg, MirFunction, Op, PrimKind, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// The calls of a program that touch no heap slot: its functions that touch
/// none, directly or through any call, and the builtins the render spells
/// itself (a runtime body has no slot to reach), except the slot builtins
/// (`__mg_take`, `__mg_init`).
struct Quiet {
    noisy: BTreeSet<String>,
}

impl Quiet {
    fn call(&self, name: &str) -> bool {
        !name.starts_with("__mg_") && !self.noisy.contains(name)
    }
}

thread_local! {
    /// The quiet calls of the program being rendered; `None` = none known.
    static QUIET: RefCell<Option<Quiet>> = const { RefCell::new(None) };
}

/// Note which calls of the program touch no heap slot; an empty `functions`
/// forgets the last program's.
pub(crate) fn set_quiet(functions: &[MirFunction]) {
    if functions.is_empty() {
        QUIET.with(|q| *q.borrow_mut() = None);
        return;
    }
    let mut q = Quiet { noisy: BTreeSet::new() };
    loop {
        let noisy: Vec<String> =
            functions.iter().filter(|f| !q.noisy.contains(&f.name) && touches(f, &q)).map(|f| f.name.clone()).collect();
        if noisy.is_empty() {
            break;
        }
        q.noisy.extend(noisy);
    }
    QUIET.with(|c| *c.borrow_mut() = Some(q));
}

fn consts_of(func: &MirFunction) -> BTreeMap<ValueId, i64> {
    func.ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value } => Some((*dst, *value)),
            _ => None,
        })
        .collect()
}

/// The heap slot address `v` holds, when it is one.
fn slot_addr(consts: &BTreeMap<ValueId, i64>, v: &ValueId) -> Option<i64> {
    let base = i64::from(crate::mg_slot_addr(0));
    let a = consts.get(v).copied().filter(|a| *a >= base && (*a - base) % 8 == 0)?;
    crate::native_slot_rewrite::heap_slot(u32::try_from((a - base) / 8).ok()?).map(|_| a)
}

/// The heap slot address an op reads or writes through (a load, a store, a take).
fn accessed(op: &Op, consts: &BTreeMap<ValueId, i64>) -> Option<i64> {
    match op {
        Op::Prim { kind: PrimKind::LoadHandle | PrimKind::Load { .. } | PrimKind::Store { .. }, args, .. } => {
            args.first().and_then(|a| slot_addr(consts, a))
        }
        Op::CallFn { name, args, .. } if name == "__mg_take" => match args.as_slice() {
            [CallArg::Scalar(a)] => slot_addr(consts, a),
            _ => None,
        },
        _ => None,
    }
}

fn touches(func: &MirFunction, quiet: &Quiet) -> bool {
    let consts = consts_of(func);
    func.ops.iter().any(|op| match op {
        Op::CallIndirect { .. } | Op::CallImport { .. } => true,
        Op::CallFn { name, .. } => !quiet.call(name),
        _ => accessed(op, &consts).is_some(),
    })
}

/// An op between the read and the take that could see the slot at `a`, or
/// leave it empty (another read of it aside).
fn blocks(op: &Op, a: i64, consts: &BTreeMap<ValueId, i64>, quiet: &Quiet) -> bool {
    match op {
        Op::IfThen { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd
        | Op::Return { .. }
        | Op::CallIndirect { .. }
        | Op::CallImport { .. } => true,
        // A take of another slot is no read of this one.
        Op::CallFn { name, .. } if name == "__mg_take" => accessed(op, consts).is_none_or(|b| b == a),
        Op::CallFn { name, .. } => !quiet.call(name),
        _ => accessed(op, consts) == Some(a),
    }
}

fn is_drop_of(op: &Op, t: ValueId) -> bool {
    match op {
        Op::Drop { v } | Op::DropListStr { v } | Op::DropVariant { v, .. } | Op::DropValue { v } => *v == t,
        _ => false,
    }
}

/// The value of the take at `j`, when nothing but a drop reads it.
fn only_dropped(ops: &[Op], j: usize) -> bool {
    let Op::CallFn { dst, .. } = &ops[j] else { return false };
    let Some(t) = *dst else { return true };
    ops.iter().enumerate().all(|(k, op)| {
        if k == j || is_drop_of(op, t) {
            return true;
        }
        let mut vs = Vec::new();
        crate::mir_ops::op_values(op, &mut vs);
        !vs.contains(&t)
    })
}

/// The heap slot reads of a function: the ones that move the value out of
/// the slot, the ones another read of the slot follows before its take
/// (a write rebuilding the value from more than one read of it), and the
/// ones every use of which only borrows (#4562).
///
/// A borrow-only read is a snapshot of the cell's `Rc`, one reference count,
/// where a copy of the value would clone the whole map or list on every read
/// of a module-level `var` handed to a read-only call. Nothing has to be
/// proved about the ops between the read and its last use: a write of the
/// slot meanwhile replaces the cell's `Rc` (a store) or takes it (its take
/// finds the snapshot sharing the block and copies it then), so the snapshot
/// keeps the value it had at the read, as the copy did.
///
/// The lowering reads a heap global as that borrow and a `Dup` of it (the
/// reader's own reference, an `rc_inc` on the wasm leg). Natively that `Dup`
/// is the copy, so a `Dup` of a borrow-only read whose every use only borrows
/// too, and which nothing moves into an owned param, is an `alias`: it reads
/// through the snapshot's reference as well.
#[derive(Default)]
pub(crate) struct SlotReads {
    pub(crate) takes: BTreeSet<ValueId>,
    pub(crate) rebuilds: BTreeSet<ValueId>,
    pub(crate) borrows: BTreeSet<ValueId>,
    pub(crate) aliases: BTreeSet<ValueId>,
}

/// The read `d` at `ops[i]` is only borrowed: every later use of it borrows
/// (render_native_borrow_read's rule), it is not the result, and no
/// `SetLocal` rebinds it (its binding is a reference, not an owned value).
fn borrow_only(func: &MirFunction, i: usize, d: ValueId, root: &BTreeMap<ValueId, ValueId>) -> bool {
    func.ret != Some(d)
        && !func.params.iter().any(|p| p.value == d)
        && !func.ops.iter().any(|o| matches!(o, Op::SetLocal { local, .. } if *local == d))
        && crate::render_native_borrow_read::last_borrowing_use(func, i, d, root).is_some()
}

/// The slot's take that ends the straight line after the read at `i` of the
/// slot at `a`, and whether another read of it comes first.
fn take_after(ops: &[Op], i: usize, a: i64, consts: &BTreeMap<ValueId, i64>, quiet: &Quiet) -> Option<(usize, bool)> {
    let mut reread = false;
    for (j, o) in ops.iter().enumerate().skip(i + 1) {
        match o {
            Op::CallFn { name, .. } if name == "__mg_take" && accessed(o, consts) == Some(a) => return Some((j, reread)),
            Op::Prim { kind: PrimKind::LoadHandle, .. } if accessed(o, consts) == Some(a) => reread = true,
            _ if blocks(o, a, consts, quiet) => return None,
            _ => {}
        }
    }
    None
}

/// The heap slot reads of `func`, sorted.
pub(crate) fn slot_reads(func: &MirFunction) -> SlotReads {
    let consts = consts_of(func);
    QUIET.with(|q| {
        let q = q.borrow();
        let mut out = SlotReads::default();
        let Some(quiet) = q.as_ref() else { return out };
        let ops = &func.ops;
        let root = crate::render_native_borrow_read::roots(func);
        for (i, op) in ops.iter().enumerate() {
            let Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
            let Some(a) = args.first().and_then(|v| slot_addr(&consts, v)) else { continue };
            match take_after(ops, i, a, &consts, quiet) {
                Some((_, true)) => {
                    out.rebuilds.insert(*d);
                }
                Some((j, false)) if only_dropped(ops, j) => {
                    out.takes.insert(*d);
                }
                _ if borrow_only(func, i, *d, &root) => {
                    out.borrows.insert(*d);
                }
                _ => {}
            }
        }
        for (j, op) in ops.iter().enumerate() {
            let Op::Dup { dst, src } = op else { continue };
            let moved = ops.iter().any(|o| matches!(o, Op::Consume { v } if v == dst));
            if out.borrows.contains(src) && !moved && borrow_only(func, j, *dst, &root) {
                out.aliases.insert(*dst);
            }
        }
        out
    })
}
