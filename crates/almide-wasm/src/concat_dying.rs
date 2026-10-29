//! #3530: a DYING list var's `+ [e]` / `[e] +` grows in place.
//!
//! `list.fold([], (a, i) => [[i]] + a)` took `$concat`'s full copy on every
//! step: the callback's `a` is not rebound by its own expression — the fold
//! rebinds its accumulator after the body — so neither the assign window
//! (`stmts_append.rs`) nor the tail-call windows (`tail_append.rs`) could see
//! it. On a list of handles that was a +1 per element of the copy and a -1 per
//! element of the released accumulator, and every outgrown block was left
//! between the inner lists allocated since: 30,000 steps took 3.7 s and
//! 1.28 GB on wasm, 0.3 s on native.
//!
//! The fold body's last read of its accumulator is already noted DYING
//! (`lower_fold_body`, dying_move.rs): the fold releases the replaced
//! accumulator right after, so the body may take its credit. The same note is
//! set at a frame's tail and at the rhs of the var's own reassignment. Where
//! such a read is one operand of a list concat and the other is a
//! one-element literal, the concat takes the credit — the local is emptied,
//! so the later release is a release of NULL — and grows the block exactly as
//! the append window does: `$cow` (in place at rc 1, a copy that drops the
//! moved credit otherwise, so value semantics hold by count), then
//! `$list_push_*` with the literal builder's Dup discipline on the element,
//! and for a prepend the shift of `shift_pushed_to_front`.
//!
//! #4261 widens the same hand-off to the two other owned left operands an
//! append meets in a builder: a dying STRING var (`list.fold("", (a, s) =>
//! a + s)` copied the whole accumulator per step) goes through
//! `$str_append`, which consumes the credit and writes in place under rc 1;
//! and a field read a dying spread rebuild moved out of its slot
//! (`{ ...r, xs: r.xs + [i] }`, `{ ...r, s: r.s + "x" }`, dying_move.rs
//! `member_reads`) is an owned operand the append consumes the same way.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

fn single_literal(e: &IrExpr) -> Option<&IrExpr> {
    match &e.kind {
        IrExprKind::List { elements } if elements.len() == 1 => elements.first(),
        _ => None,
    }
}

impl Emitter<'_> {
    /// `x + [e]` or `[e] + x` (also in the temp-bound shapes lowering hands
    /// over) for a list var `x` whose read was noted dying. `None`: not that
    /// shape, or the read shares as before — the generic concat lowers it.
    pub(crate) fn try_concat_dying(&mut self, e: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        if self.metered || !matches!(e.kind, IrExprKind::BinOp { .. } | IrExprKind::Block { .. }) {
            return Ok(None);
        }
        if let Some(t) = self.try_concat_taken(e)? {
            return Ok(Some(t));
        }
        if let Some(t) = self.try_str_concat_dying(e)? {
            return Ok(Some(t));
        }
        let is_var = |e: &IrExpr| matches!(e.kind, IrExprKind::Var { .. });
        let shape = match crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatList) {
            Some((l, r)) if is_var(l) && single_literal(r).is_some() => Some((l, r, false)),
            _ => match crate::tail_append::prepend_operands(e) {
                Some((l, r)) if is_var(r) && single_literal(l).is_some() => Some((r, l, true)),
                _ => None,
            },
        };
        let Some((read, lit, front)) = shape else { return Ok(None) };
        let Some(elem) = single_literal(lit) else { return Ok(None) };
        if !self.owned_call_marks.is_dying(read) {
            return Ok(None);
        }
        let Some((id, idx, ty)) = self.dying_local(read) else { return Ok(None) };
        // The element is lowered after the local is emptied: one that reads
        // the var keeps the generic concat.
        let SliceTy::List(h) = ty else { return Ok(None) };
        if crate::rc_ownership::rc_mentions_var(elem, id) {
            return Ok(None);
        }
        self.in_tail = false;
        let el = self.types.el(h);
        let cow = self.cow_fn_of(ty);
        self.f.instructions().local_get(idx);
        self.empty_dying(id, idx);
        self.f.instructions().call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        self.rc_share_guard(elem, el);
        self.witness_store(elem, el);
        let push = if el.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        self.f.instructions().call(push);
        if front {
            self.shift_pushed_to_front(el.slot_size())?;
        }
        Ok(Some(ty))
    }
}

impl Emitter<'_> {
    /// #4261: `x + s` over a STRING var `x` whose read was noted dying (a
    /// fold accumulator, a frame tail): `$str_append` takes the credit and
    /// extends the block in place under rc 1 (a shared block is copied, so
    /// value semantics hold by count). `None`: not that shape, or the read
    /// shares as before.
    fn try_str_concat_dying(&mut self, e: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let Some((read, right)) = crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatStr) else {
            return Ok(None);
        };
        if !matches!(read.kind, IrExprKind::Var { .. }) || !self.owned_call_marks.is_dying(read) {
            return Ok(None);
        }
        let IrExprKind::Var { id: vid } = &read.kind else { return Ok(None) };
        // The right side is lowered after the local is emptied: one that
        // reads the var keeps the generic concat (the note stays unread).
        if crate::rc_ownership::rc_mentions_var(right, *vid) {
            return Ok(None);
        }
        let Some((id, idx, ty)) = self.dying_local(read) else { return Ok(None) };
        if ty != STR {
            return Ok(None);
        }
        self.in_tail = false;
        self.f.instructions().local_get(idx);
        self.empty_dying(id, idx);
        self.append_owned_str(right)?;
        Ok(Some(STR))
    }

    /// #4261: an append whose left operand is a field read a dying spread
    /// rebuild moved out of its slot (`dying_move::member_reads` admits it
    /// only in these shapes): the moved value is owned, so the append
    /// consumes it in place instead of copying it — `$str_append` for a
    /// String, `$cow` + `$list_push` for a one-element list literal.
    fn try_concat_taken(&mut self, e: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        if let Some((left, right)) = crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatStr)
            && self.owned_call_marks.has_take(left)
        {
            self.in_tail = false;
            let got = self.lower(left, Some(STR))?;
            if got != STR {
                return unsup("concat-taken:str");
            }
            self.append_owned_str(right)?;
            return Ok(Some(STR));
        }
        let Some((left, lit)) = crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatList) else {
            return Ok(None);
        };
        let Some(elem) = single_literal(lit) else { return Ok(None) };
        if !self.owned_call_marks.has_take(left) {
            return Ok(None);
        }
        self.in_tail = false;
        let ty = self.lower(left, None)?;
        let SliceTy::List(h) = ty else { return unsup("concat-taken:list") };
        let el = self.types.el(h);
        let cow = self.cow_fn_of(ty);
        self.f.instructions().call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        self.rc_share_guard(elem, el);
        self.witness_store(elem, el);
        let push = if el.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        self.f.instructions().call(push);
        Ok(Some(ty))
    }

    /// An OWNED string on the stack: append `right` through `$str_append`
    /// (which consumes the left credit and BORROWS its operand; an owned
    /// operand is released right after, as `try_str_append_assign` does).
    fn append_owned_str(&mut self, right: &IrExpr) -> Result<(), EmitError> {
        self.lower(right, Some(STR))?;
        let release = self.hold_owned_operand(right)?;
        self.f.instructions().call(F_STR_APPEND);
        self.release_owned_operand(release);
        Ok(())
    }
}

/// The var read a one-element push onto it would consume: `x + [e]` or
/// `[e] + x`, bare or temp-bound — or a string append `x + s` (#4261). Read
/// by `dying_move::consumed_read`, so the fold body, the frame tail and the
/// var's own reassignment note it.
pub(crate) fn concat_read(value: &IrExpr) -> Option<&IrExpr> {
    let is_var = |e: &IrExpr| matches!(e.kind, IrExprKind::Var { .. });
    match crate::stmts_append::concat_operands(value, almide_ir::BinOp::ConcatList) {
        Some((l, r)) if is_var(l) && single_literal(r).is_some() => Some(l),
        _ => match crate::tail_append::prepend_operands(value) {
            Some((l, r)) if is_var(r) && single_literal(l).is_some() => Some(r),
            _ => match crate::stmts_append::concat_operands(value, almide_ir::BinOp::ConcatStr) {
                Some((l, _)) if is_var(l) => Some(l),
                _ => None,
            },
        },
    }
}

/// #4261: is `e` an append the spread rebuild may hand a moved slot `b.f`
/// as its left operand — `b.f + s` (String, `s` not reading `b`) or
/// `b.f + [x]` (`x` not reading `b`)? Exactly the shapes
/// [`Emitter::try_concat_taken`] consumes, so a moved slot never reaches a
/// concat that would only borrow it (#3441's leak).
pub(crate) fn taken_append_left(e: &IrExpr, b: almide_ir::VarId) -> Option<&IrExpr> {
    let is_field_of_b = |l: &IrExpr| matches!(&l.kind, IrExprKind::Member { object, .. } if matches!(object.kind, IrExprKind::Var { id } if id == b));
    if let Some((l, r)) = crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatStr)
        && is_field_of_b(l)
        && !crate::rc_ownership::rc_mentions_var(r, b)
    {
        return Some(l);
    }
    let (l, r) = crate::stmts_append::concat_operands(e, almide_ir::BinOp::ConcatList)?;
    let x = single_literal(r)?;
    (is_field_of_b(l) && !crate::rc_ownership::rc_mentions_var(x, b)).then_some(l)
}
