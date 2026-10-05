//! #3406: a var's LAST read hands its credit over instead of sharing it.
//!
//! `b = add(b, i)` with `fn add(b: Box, i: Int) -> Box = { ...b, m:
//! map.set(b.m, k, v), n: b.n + 1 }` copied the map on every call: the
//! spread copied `b` (the map at rc 2), the field read shared `b.m` into
//! `map.set` (rc 3), and the functional set copied any receiver it did not
//! own — 3.8 s for 20 000 updates where native, after #3404, takes 2 ms.
//! Three rules, each the wasm twin of an ownership rule native applies:
//!
//! * **A dying var moves.** At the rhs of its own reassignment (`x = V`)
//!   and at the frame's tail, a var the frame holds a credit on is never
//!   read again before it is rebound or released. Where `V` is a consuming
//!   op over it — `map.set(x, …)`, or a spread `{ ...x, … }` — the op
//!   takes the credit and the local is emptied (the later release is a
//!   release of NULL), so the op meets the block at its true count. The
//!   reassignment form of a program-fn call (`b = add(b, i)`) is
//!   writeback_move.rs's move-in, widened to owned non-`mut` params.
//! * **A dying spread base is rebuilt in place.** The base is made unique
//!   (its block when the credit is its only one, else a copy and the
//!   credit on the shared original goes), every field value is computed
//!   while the base still reads as before, and only then are the old slots
//!   released and the new ones stored. A field read `b.f` of an overridden
//!   handle field that occurs once (outside loops) MOVES the slot's credit
//!   out — the slot is nulled, so the release of the old slot is a release
//!   of NULL — and is an owned value to its consumer.
//! * **The functional `map.set` judges an owned receiver.** A receiver
//!   whose credit the op holds (an owned temporary, a moved slot, a dying
//!   var) is written in place when that credit is the block's only one,
//!   exactly the window's judge (map_inplace.rs); a shared one is copied
//!   and loses the op's credit. A borrowed receiver keeps the copy.
//!
//! Value semantics hold by count, not by syntax: a block anything else
//! still holds — `let old = b` before the call, an alias, a container —
//! is at rc > 1 when the judge reads it, so it is copied and the other
//! holder keeps the old map (spec/wasm_cross/record_map_field_update.almd).

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::{BlockType, MemArg};

use crate::arm::{ArgMode, Lowered};
use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

/// The Var read a consuming `value` would move, if it is one: the
/// receiver of `map.set`, or the base of a spread.
fn consumed_read(value: &IrExpr) -> Option<&IrExpr> {
    let read = match &value.kind {
        IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "map" && func.as_str() == "set" && args.len() == 3 =>
        {
            &args[0]
        }
        IrExprKind::SpreadRecord { base, .. } => base.as_ref(),
        _ => return None,
    };
    matches!(read.kind, IrExprKind::Var { .. }).then_some(read)
}

/// How the spread's field values read the base `b`: `None` when anything
/// but a plain `b.f` read mentions it (a whole-`b` use, a closure, a
/// rebind); else, per field name, how many reads of `b.f` occur — a read
/// under a loop counts as many.
fn base_reads(fields: &[(almide_base::intern::Sym, IrExpr)], b: VarId) -> Option<Vec<(almide_base::intern::Sym, u32)>> {
    struct Reads {
        b: VarId,
        loops: u32,
        bad: bool,
        seen: Vec<(almide_base::intern::Sym, u32)>,
    }
    impl IrVisitor for Reads {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Member { object, field } if matches!(&object.kind, IrExprKind::Var { id } if *id == self.b) => {
                    let n = if self.loops > 0 { 2 } else { 1 };
                    match self.seen.iter_mut().find(|(f, _)| f == field) {
                        Some((_, c)) => *c += n,
                        None => self.seen.push((*field, n)),
                    }
                }
                IrExprKind::Var { id } if *id == self.b => self.bad = true,
                IrExprKind::Lambda { .. } => self.bad |= crate::rc_ownership::rc_mentions_var(e, self.b),
                IrExprKind::While { .. } | IrExprKind::ForIn { .. } => {
                    self.loops += 1;
                    walk_expr(self, e);
                    self.loops -= 1;
                }
                _ => walk_expr(self, e),
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            let target = match &s.kind {
                IrStmtKind::Assign { var, .. }
                | IrStmtKind::IndexAssign { target: var, .. }
                | IrStmtKind::MapInsert { target: var, .. }
                | IrStmtKind::FieldAssign { target: var, .. }
                | IrStmtKind::ListSwap { target: var, .. }
                | IrStmtKind::ListReverse { target: var, .. }
                | IrStmtKind::ListRotateLeft { target: var, .. }
                | IrStmtKind::RcInc { var }
                | IrStmtKind::RcDec { var } => Some(*var),
                IrStmtKind::ListCopySlice { dst, src, .. } => [*dst, *src].into_iter().find(|v| *v == self.b),
                _ => None,
            };
            self.bad |= target == Some(self.b);
            walk_stmt(self, s);
        }
    }
    let mut r = Reads { b, loops: 0, bad: false, seen: Vec::new() };
    for (_, e) in fields {
        r.visit_expr(e);
    }
    (!r.bad).then_some(r.seen)
}

/// The address of every `b.f` node of the field values (the slot-take
/// key, node_marks.rs).
fn member_reads(fields: &[(almide_base::intern::Sym, IrExpr)], b: VarId, f: almide_base::intern::Sym) -> Vec<usize> {
    struct Find {
        b: VarId,
        f: almide_base::intern::Sym,
        out: Vec<usize>,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Member { object, field }
                    if *field == self.f && matches!(&object.kind, IrExprKind::Var { id } if *id == self.b) =>
                {
                    self.out.push(e as *const IrExpr as usize);
                }
                _ => walk_expr(self, e),
            }
        }
    }
    let mut fd = Find { b, f, out: Vec::new() };
    for (_, e) in fields {
        fd.visit_expr(e);
    }
    fd.out
}

impl Emitter<'_> {
    /// Note the read `value` would move as dying (`on`), or withdraw the
    /// note (`!on`) — around the lowering of `var = value` (`Some(var)`),
    /// or once for the frame's tail (`None`: any var the frame holds).
    pub(crate) fn note_dying(&mut self, value: &IrExpr, var: Option<VarId>, on: bool) {
        let Some(read) = consumed_read(value) else { return };
        let IrExprKind::Var { id } = &read.kind else { return };
        if var.is_none_or(|v| v == *id) {
            self.owned_call_marks.set_dying(read, on);
        }
    }

    /// The local of the var `read` names when the read was noted dying and
    /// the frame holds the var's credit in a plain local — consuming the
    /// note. `None`: the read shares as before.
    fn dying_local(&mut self, read: &IrExpr) -> Option<(VarId, u32, SliceTy)> {
        if !self.owned_call_marks.take_dying(read) || self.metered {
            return None;
        }
        let IrExprKind::Var { id } = &read.kind else { return None };
        let &(idx, ty) = self.locals.get(id)?;
        (!self.cells.contains(id) && self.holds_credit(idx) && self.rc_droppable(ty)).then_some((*id, idx, ty))
    }

    /// The dying var's credit has moved out: empty its local.
    fn empty_dying(&mut self, id: VarId, idx: u32) {
        self.f.instructions().i32_const(0).local_set(idx);
        self.witness_move_and_empty(id, false);
    }

    /// `map.set(m, k, v)` over a receiver whose credit the op may take
    /// (see the module doc); `None` keeps the borrowed-receiver copy.
    pub(crate) fn try_map_set_owned(&mut self, m: &IrExpr, key: &IrExpr, value: &IrExpr) -> Result<Option<Lowered>, EmitError> {
        if self.metered {
            return Ok(None);
        }
        let dying = if self.owned_call_marks.is_dying(m) {
            let dying = self.dying_local(m);
            let mentioned = dying.is_some_and(|(id, ..)| {
                crate::rc_ownership::rc_mentions_var(key, id) || crate::rc_ownership::rc_mentions_var(value, id)
            });
            if mentioned { None } else { dying }
        } else {
            None
        };
        // An owned temporary, or a moved slot (owned once lowered).
        if dying.is_none() && !self.rc_owned_result(m) && !self.owned_call_marks.has_take(m) {
            return Ok(None);
        }
        self.owned_call_marks.set_moving(m, dying.is_some());
        let got = self.lower_arg(m, None, ArgMode::Retain)?;
        self.owned_call_marks.set_moving(m, false);
        let SliceTy::Map(kt, vt) = got else {
            return unsup(&format!("map-op-of:{got:?}"));
        };
        let (k, v) = (self.types.el(kt), self.types.el(vt));
        let mh = self.hold_i32()?;
        self.f.instructions().local_set(mh);
        if let Some((id, idx, _)) = dying {
            self.empty_dying(id, idx);
        }
        let fns = self.map_set_fns(k, v)?;
        let kh = self.hold_for(k)?;
        self.lower_arg(key, Some(k), ArgMode::Retain)?;
        self.f.instructions().local_set(kh);
        let vh = self.hold_for(v)?;
        self.lower_arg(value, Some(v), ArgMode::Retain)?;
        self.f.instructions().local_set(vh);
        self.emit_map_set_judged(mh, kh, vh, (k, v), fns)?;
        self.f.instructions().local_get(mh);
        self.release_for(v);
        self.release_for(k);
        self.release_i32(); // mh
        Ok(Some(Lowered::owned(got)))
    }

    /// `{ ...b, f: e, … }` where `b` dies here: the rebuild in place of the
    /// module doc. `None` keeps the copy-then-overwrite route.
    pub(crate) fn try_spread_dying(
        &mut self,
        base: &IrExpr,
        fields: &[(almide_base::intern::Sym, IrExpr)],
    ) -> Result<Option<SliceTy>, EmitError> {
        if !self.owned_call_marks.is_dying(base) {
            return Ok(None);
        }
        let Some((id, idx, ty)) = self.dying_local(base) else { return Ok(None) };
        let SliceTy::Named(ti) = ty else { return Ok(None) };
        let NamedDef::Record(def) = &self.types.def(ti) else { return Ok(None) };
        let mut slots = Vec::new();
        for (fname, _) in fields {
            match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                Some(fi) => slots.push((fi.ty, fi.offset)),
                None => return Ok(None),
            }
        }
        let Some(reads) = base_reads(fields, id) else { return Ok(None) };
        // The overridden handle fields read exactly once: their slot moves.
        let mut takes = Vec::new();
        for ((fname, _), &(fty, off)) in fields.iter().zip(&slots) {
            if self.elem_is_handle(fty) && reads.iter().any(|(f, n)| f == fname && *n == 1) {
                takes.extend(member_reads(fields, id, *fname).into_iter().map(|e| (e, off)));
            }
        }
        // Unique: the base's block when its credit is the only one, else a
        // copy (the copy takes its own slot credits) and the credit on the
        // shared original goes. The base's local names the unique block
        // while the field values read it.
        let (copy, dec) = (self.copy_fn_of(ty), self.dec_fn_of(ty));
        let hold = self.hold_i32()?;
        let rc = MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 };
        {
            let mut i = self.f.instructions();
            i.local_get(idx).local_tee(hold).global_get(G_LINE_END).i32_ge_u();
            i.local_get(hold).i32_load(rc).i32_const(1).i32_eq();
            i.i32_and().i32_eqz().if_(BlockType::Empty);
            i.local_get(hold).call(copy).local_get(hold).call(dec).local_set(hold);
            i.end();
            i.local_get(hold).local_set(idx);
        }
        for &(e, off) in &takes {
            self.owned_call_marks.set_take(e, Some(off));
        }
        let mut vals = Vec::new();
        for ((_, fexpr), &(fty, _)) in fields.iter().zip(&slots) {
            let vh = self.hold_for(fty)?;
            self.lower(fexpr, Some(fty))?;
            self.rc_share_guard(fexpr, fty);
            self.witness_store(fexpr, fty);
            self.f.instructions().local_set(vh);
            vals.push(vh);
        }
        for &(e, _) in &takes {
            self.owned_call_marks.set_take(e, None);
        }
        for (&(fty, off), &vh) in slots.iter().zip(&vals) {
            // The overwritten field's credit goes with it (a moved slot is
            // NULL: a release of nothing).
            if self.elem_is_handle(fty) {
                let dec = self.dec_fn_of(fty);
                self.f.instructions().local_get(hold).i32_load(slot_memarg(off)).call(dec);
            }
            self.f.instructions().local_get(hold);
            self.f.instructions().local_get(vh);
            self.store_ty_slot(fty, off);
        }
        self.empty_dying(id, idx);
        self.f.instructions().local_get(hold);
        for &(fty, _) in slots.iter().rev() {
            self.release_for(fty);
        }
        self.release_i32(); // hold
        Ok(Some(ty))
    }

    /// The field read `e` (its record's address on the stack) when it was
    /// noted a slot take: load the slot, null it, and hand the value on
    /// owned. `false`: not a take — nothing emitted.
    pub(crate) fn try_take_slot(&mut self, e: &IrExpr, fty: SliceTy) -> Result<bool, EmitError> {
        let Some(off) = self.owned_call_marks.take_slot(e) else { return Ok(false) };
        let t = self.hold_i32()?;
        self.f.instructions().local_tee(t);
        self.load_ty_slot(fty, off);
        self.f.instructions().local_get(t).i32_const(0).i32_store(slot_memarg(off));
        self.release_i32();
        self.owned_call_marks.mark(e);
        Ok(true)
    }
}
