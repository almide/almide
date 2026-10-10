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

/// The var read a one-element push onto it would consume: `x + [e]` or
/// `[e] + x`, bare or temp-bound. Read by `dying_move::consumed_read`, so
/// the fold body, the frame tail and the var's own reassignment note it.
pub(crate) fn concat_read(value: &IrExpr) -> Option<&IrExpr> {
    let is_var = |e: &IrExpr| matches!(e.kind, IrExprKind::Var { .. });
    match crate::stmts_append::concat_operands(value, almide_ir::BinOp::ConcatList) {
        Some((l, r)) if is_var(l) && single_literal(r).is_some() => Some(l),
        _ => match crate::tail_append::prepend_operands(value) {
            Some((l, r)) if is_var(r) && single_literal(l).is_some() => Some(r),
            _ => None,
        },
    }
}
