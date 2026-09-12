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
        self.f.instructions().call(F_STR_APPEND);
        // The credit MOVES through the helper — one in, one out — which is
        // what a moved param records, not a freshly born block.
        self.witness_arg(left, want);
        self.tail_consumed.insert(idx);
        Ok(true)
    }

    /// The list twin of [`Self::tail_str_append_arg`] (#2117): `f(acc + [e], …)`
    /// for the parameter it rebinds. `$cow` + `$list_push_8` grows amortized
    /// in place where the generic path took `$concat`'s full copy and then
    /// released the outgrown block — at a size class the free list abandons,
    /// which is how a 200,000-element accumulator reached C-197. Scalar
    /// 8-byte elements only, the same bound the assign window carries: a
    /// 4-byte handle slot needs the literal builder's Dup discipline.
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
        if el != INT && el != FLOAT {
            return Ok(false);
        }
        let cow = self.cow_fn_of(SliceTy::List(h));
        self.f.instructions().local_get(idx).call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        self.f.instructions().call(F_LIST_PUSH_8);
        self.witness_arg(left, want);
        self.tail_consumed.insert(idx);
        Ok(true)
    }

}
