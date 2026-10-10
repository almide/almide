//! The #2117 growing-accumulator windows at a SELF TAIL CALL — the tail-call
//! twins of `stmts_append.rs`'s assign windows, split out for the file budget.
//!
//! `build(acc + s, n - 1)` is the accumulator `acc = acc + s` one position
//! over, but the generic argument path emitted a concat and let the exit plan
//! release the old block — which above the largest size class means a full
//! copy per iteration and an outgrown block the free list abandons. Both
//! windows route through the helper that moves ONE credit in and answers one
//! out, and both tell the exit PLAN that the parameter's credit is spent
//! (`tail_consumed`) rather than skipping the release behind the E083
//! validator's back.

use almide_ir::IrExpr;
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// The #2117 window: an argument of a LOOP-FORM self tail call that is
    /// `p + rhs` for the very parameter `p` it rebinds. `$str_append` takes
    /// `p`'s credit and answers the grown block, so the exit must not release
    /// `p` as well — `tail_consumed` tells the exit PLAN, which is what the
    /// E083 validator checks the emitted releases against.
    pub(crate) fn tail_str_append_arg(
        &mut self,
        k: usize,
        a: &IrExpr,
        want: SliceTy,
        self_tail: bool,
    ) -> Result<bool, EmitError> {
        if !self_tail || self.metered || !self.tail_release_allowed || want != STR {
            return Ok(false);
        }
        let Some((left, right)) = crate::stmts_append::concat_operands(a, almide_ir::BinOp::ConcatStr)
        else {
            return Ok(false);
        };
        let IrExprKind::Var { id } = &left.kind else { return Ok(false) };
        if self.cells.contains(id) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::Scalar(Scalar::Str))) = self.locals.get(id) else {
            return Ok(false);
        };
        // The loop-back writes this argument into local `k`: only the
        // parameter being rebound may have its credit spent here.
        if idx != k as u32 || !self.rc_frame_params.contains(&idx) {
            return Ok(false);
        }
        self.f.instructions().local_get(idx);
        self.lower(right, Some(STR))?;
        // An owned operand is borrowed by `$str_append`: released after it.
        let release = self.hold_owned_operand(right)?;
        self.f.instructions().call(F_STR_APPEND);
        self.release_owned_operand(release);
        // The credit MOVES through the helper — one in, one out — which is
        // what a moved param records, not a freshly born block.
        self.witness_arg_moved(left, want);
        self.tail_consumed.insert(idx);
        Ok(true)
    }

    /// The list twin of [`Self::tail_str_append_arg`] (#2117): `f(acc + [e], …)`
    /// for the parameter it rebinds. `$cow` + `$list_push_8` grows amortized
    /// in place where the generic path took `$concat`'s full copy and then
    /// released the outgrown block — at a size class the free list abandons,
    /// which is how a 200,000-element accumulator reached C-197. Any element
    /// the layout gives a slot (#2310): 8-byte scalars push through
    /// `$list_push_8`, 4-byte handles through `$list_push_4` with the
    /// literal builder's Dup discipline on the element.
    pub(crate) fn tail_list_append_arg(
        &mut self,
        k: usize,
        a: &IrExpr,
        want: SliceTy,
        self_tail: bool,
    ) -> Result<bool, EmitError> {
        if !self_tail || self.metered || !self.tail_release_allowed {
            return Ok(false);
        }
        let SliceTy::List(h) = want else { return Ok(false) };
        let Some((left, right)) = crate::stmts_append::concat_operands(a, almide_ir::BinOp::ConcatList)
        else {
            return Ok(false);
        };
        let IrExprKind::Var { id } = &left.kind else { return Ok(false) };
        if self.cells.contains(id) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::List(lh))) = self.locals.get(id) else { return Ok(false) };
        if lh != h || idx != k as u32 || !self.rc_frame_params.contains(&idx) {
            return Ok(false);
        }
        let IrExprKind::List { elements } = &right.kind else { return Ok(false) };
        let [elem] = &elements[..] else { return Ok(false) };
        let el = self.types.el(h);
        let cow = self.cow_fn_of(SliceTy::List(h));
        self.f.instructions().local_get(idx).call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        // The assign window's discipline, verbatim (#2310): a handle element
        // borrowed into the spine takes its +1, an owned one moves in.
        self.rc_share_guard(elem, el);
        self.witness_store(elem, el);
        let push = if el.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        self.f.instructions().call(push);
        self.witness_arg_moved(left, want);
        self.tail_consumed.insert(idx);
        Ok(true)
    }

    /// The prepend twin (#3519): `f([e] + acc, …)` for the parameter it
    /// rebinds. The generic path took `$concat`'s full copy every iteration —
    /// for a list of handles a +1 on every element of the copy and a -1 on
    /// every element of the released `acc`, and each outgrown block stranded
    /// between the elements allocated since, so 30,000 prepends of `[[i]]`
    /// took 4 s and 1.27 GB where native took 0.45 s. Here the block grows
    /// in place exactly as the append window does (`$cow`, `$list_push_*`
    /// with the same Dup discipline), and then the old elements shift one
    /// slot right — a `memory.copy`, which is overlap-safe — and the pushed
    /// element moves from the last slot to the first. The shift moves slots,
    /// not credits: no count changes, so the witness sees the append's events.
    pub(crate) fn tail_list_prepend_arg(
        &mut self,
        k: usize,
        a: &IrExpr,
        want: SliceTy,
        self_tail: bool,
    ) -> Result<bool, EmitError> {
        if !self_tail || self.metered || !self.tail_release_allowed {
            return Ok(false);
        }
        let SliceTy::List(h) = want else { return Ok(false) };
        let Some((left, right)) = prepend_operands(a) else { return Ok(false) };
        let IrExprKind::Var { id } = &right.kind else { return Ok(false) };
        if self.cells.contains(id) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::List(lh))) = self.locals.get(id) else { return Ok(false) };
        if lh != h || idx != k as u32 || !self.rc_frame_params.contains(&idx) {
            return Ok(false);
        }
        let IrExprKind::List { elements } = &left.kind else { return Ok(false) };
        let [elem] = &elements[..] else { return Ok(false) };
        let el = self.types.el(h);
        let stride = el.slot_size();
        let cow = self.cow_fn_of(SliceTy::List(h));
        self.f.instructions().local_get(idx).call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        self.rc_share_guard(elem, el);
        self.witness_store(elem, el);
        let push = if stride == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        self.f.instructions().call(push);
        self.shift_pushed_to_front(stride)?;
        self.witness_arg_moved(right, want);
        self.tail_consumed.insert(idx);
        Ok(true)
    }

    /// With the block a `$list_push_*` just answered on the stack, move the
    /// pushed element from the last slot to the first: the old slots shift
    /// one stride right (`memory.copy` is overlap-safe). Slots move, credits
    /// do not. Leaves the block on the stack. Shared by the tail-call window
    /// and the `acc = [e] + acc` assign window (#3519).
    pub(crate) fn shift_pushed_to_front(&mut self, stride: u32) -> Result<(), EmitError> {
        let blk = self.hold_i32()?;
        let last = self.hold_i32()?;
        let v = if stride == 8 { self.hold_i64()? } else { self.hold_i32()? };
        let payload = almide_layout::PAYLOAD as i32;
        let mut i = self.f.instructions();
        i.local_set(blk);
        // `last` = the byte offset of the pushed slot: old len.
        i.local_get(blk).i32_load(len_memarg()).i32_const(stride as i32).i32_sub().local_set(last);
        i.local_get(blk).local_get(last).i32_add();
        if stride == 8 { i.i64_load(slot_memarg(0)) } else { i.i32_load(slot_memarg(0)) };
        i.local_set(v);
        i.local_get(blk).i32_const(payload + stride as i32).i32_add();
        i.local_get(blk).i32_const(payload).i32_add();
        i.local_get(last);
        i.memory_copy(0, 0);
        i.local_get(blk).local_get(v);
        if stride == 8 { i.i64_store(slot_memarg(0)) } else { i.i32_store(slot_memarg(0)) };
        i.local_get(blk);
        if stride == 8 { self.release_i64() } else { self.release_i32() };
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}

/// `[e] + acc` as lowering hands it over: the bare `BinOp`, or the literal
/// bound to a temp first — `{ let t = [e]; t + acc }` — which is the shape a
/// self tail call's argument (and an assign's value) arrives in.
pub(crate) fn prepend_operands(a: &IrExpr) -> Option<(&IrExpr, &IrExpr)> {
    let op = almide_ir::BinOp::ConcatList;
    match &a.kind {
        IrExprKind::BinOp { op: o, left, right } if *o == op => Some((left, right)),
        IrExprKind::Block { stmts, expr: Some(tail) } if stmts.len() == 1 => {
            let almide_ir::IrStmtKind::Bind { var: t, value: bound, .. } = &stmts[0].kind else { return None };
            let IrExprKind::BinOp { op: o, left, right } = &tail.kind else { return None };
            if *o != op || !matches!(&left.kind, IrExprKind::Var { id } if id == t) {
                return None;
            }
            Some((bound, right))
        }
        _ => None,
    }
}
