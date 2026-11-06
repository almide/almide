//! #4006 C1 class D: a module-level binding that holds a heap value.
//!
//! The shared lowering keeps such a binding in a storage slot of linear
//! memory: it reads it as `LoadHandle(mg_slot_addr(i))` (a borrow, `Dup`ed by
//! the reader that keeps it), assigns it as `__mg_take(addr)` (the old value,
//! owned) followed by `Store(addr, Handle(new))` and the move-out of `new`,
//! and `__mg_init` stores its first value the same way, without the take.
//!
//! The native render has no linear memory. A slot whose declared type has a
//! native kind (`native_slot_rewrite::heap_slot`) is a thread-local
//! `RefCell` of an `Rc` of that type, holding the value: the borrow reads a
//! copy of it, or, when every use of the read only borrows it, a snapshot of
//! the `Rc` (#4562: one reference count where the copy cloned the whole
//! value at every read-only call); the take moves it out (its type's default
//! left behind, which nothing reads: the store follows), copying it only
//! while a snapshot still shares it, and the store moves the new value in.
//! The MIR
//! and its certificates are the ones the wasm leg runs; only their storage
//! is spelled differently. A program with a `fan` keeps the wall: its arms
//! run on threads of their own, and a thread-local cell is per thread.

use super::{AddrState, NTy};
use crate::lower::LowerError;
use crate::render_native::{var, wall};
use crate::{CallArg, Op, PrimKind, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The value moved out of a slot's cell (`s`): its `Rc`'s own block when no
/// snapshot shares it, which is every take but one a borrow-only read of the
/// slot still holds; a copy of it then, the copy that read no longer makes.
const UNWRAP_TAKE: &str = "std::rc::Rc::try_unwrap(std::mem::take(&mut *s.borrow_mut())).unwrap_or_else(|r| (*r).clone())";

/// Render `op` when it reads, takes or stores a heap slot. `None` = not one.
pub(super) fn render_heap_slot_op(
    op: &Op,
    st: &mut AddrState,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Option<Result<bool, LowerError>> {
    let slot_of = |st: &AddrState, v: &ValueId| st.consts.get(v).and_then(|a| crate::native_slot_rewrite::heap_slot_at(*a));
    let pad = "    ".repeat(indent);
    match op {
        Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 => {
            let (slot, t) = slot_of(st, &args[0])?;
            if st.slot_reads.rebuilds.contains(d) {
                return Some(Err(wall("native: a heap slot read again before its write-back (each read copies the cell's value)")));
            }
            // Every use only borrows it: a snapshot of the cell's `Rc`, read
            // through a reference (the kind a borrowed param of the type has).
            // (A string's copy shares its buffer already, ADR-0013.)
            if let (true, Some((rt, spelled, whole))) =
                (st.slot_reads.borrows.contains(d) && t != NTy::Str, super::borrowed_kind(t))
            {
                tys.insert(*d, rt);
                let _ = writeln!(
                    out,
                    "{pad}let {v}_rc: std::rc::Rc<{ty}> = ALMD_HSLOT_{slot}.with(|s| std::rc::Rc::clone(&*s.borrow())); let mut {v}: {spelled} = &(*{v}_rc){whole};",
                    v = var(*d),
                    ty = crate::render_native_rec::owned_ty(t)
                );
                return Some(Ok(true));
            }
            tys.insert(*d, t);
            // The slot's own take follows, and nothing in between sees it.
            let read = if st.slot_reads.takes.contains(d) && crate::render_native_rec::has_default(t) {
                UNWRAP_TAKE
            } else {
                "(**s.borrow()).clone()"
            };
            let _ = writeln!(
                out,
                "{pad}let mut {}: {} = ALMD_HSLOT_{slot}.with(|s| {read});",
                var(*d),
                crate::render_native_rec::owned_ty(t)
            );
            Some(Ok(true))
        }
        Op::CallFn { dst: Some(d), name, args, .. } if name == "__mg_take" => {
            let [CallArg::Scalar(a)] = args.as_slice() else {
                return Some(Err(wall("native: a slot take whose address is not one scalar")));
            };
            let (slot, t) = slot_of(st, a)?;
            if !crate::render_native_rec::has_default(t) {
                return Some(Err(wall("native: a heap slot whose type has no default to leave behind")));
            }
            tys.insert(*d, t);
            let _ = writeln!(
                out,
                "{pad}let mut {}: {} = ALMD_HSLOT_{slot}.with(|s| {UNWRAP_TAKE});",
                var(*d),
                crate::render_native_rec::owned_ty(t)
            );
            Some(Ok(true))
        }
        Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
            let (slot, t) = slot_of(st, &args[0])?;
            let Some(h) = st.addrs.get(&args[1]).copied().filter(|h| h.off == 0 && h.idx.is_none()) else {
                return Some(Err(wall("native: a heap slot stored with something other than a block's handle")));
            };
            let got = tys.get(&h.base).copied();
            let rhs = if got == Some(t) {
                crate::render_native_move::take_expr(&var(h.base))
            } else if got.is_some() && st.empty_lists.contains(&h.base) {
                // An empty list literal is every list type's empty list.
                format!("<{}>::default()", crate::render_native_rec::owned_ty(t))
            } else {
                return Some(Err(wall(format!("native: a {got:?} value stored into a {t:?} heap slot"))));
            };
            let _ = writeln!(out, "{pad}ALMD_HSLOT_{slot}.with(|s| *s.borrow_mut() = std::rc::Rc::new({rhs}));");
            Some(Ok(true))
        }
        _ => None,
    }
}

/// A block stored into a heap slot holds the slot's declared type (a record
/// rebuilt with one field changed, `g.n = g.n + 1`): a type-bearing op, like
/// a typed return or argument.
pub(super) fn note_heap_slot_recs(func: &crate::MirFunction, alloc_rec: &mut BTreeMap<ValueId, u16>) {
    let base = i64::from(crate::mg_slot_addr(0));
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    let mut handles: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    for op in &func.ops {
        match op {
            Op::ConstInt { dst, value } => {
                consts.insert(*dst, *value);
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), args } if args.len() == 1 => {
                handles.insert(*h, args[0]);
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
                let slot = consts.get(&args[0]).copied().filter(|a| *a >= base && (*a - base) % 8 == 0);
                let Some(slot) = slot.and_then(|a| u32::try_from((a - base) / 8).ok()) else { continue };
                if let (Some(NTy::Rec(i)), Some(x)) = (crate::native_slot_rewrite::heap_slot(slot), handles.get(&args[1])) {
                    alloc_rec.entry(*x).or_insert(i);
                }
            }
            _ => {}
        }
    }
}
