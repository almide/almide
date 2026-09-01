//! The growing-accumulator ASSIGN windows (`acc = acc + s`, `data =
//! data + [x]`) — split from stmts.rs for the 800-line file budget
//! (#1729 pushed it over); pure text move, same `impl Emitter` surface.

use almide_ir::IrExprKind;
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// The growing-accumulator window (`acc = acc + s`, Str): route
    /// through $str_append — in place under rc == 1 with class slack,
    /// else concat + release of the outgrown block. Without this the
    /// Assign dec-skip (rhs mentions the var) plus F_CONCAT's borrow
    /// semantics leaked every outgrown accumulator
    /// (spec/churn/string_accumulator_churn OOM'd at the commissioning).
    /// UNMETERED only — the metered ConcatStr path carries the T3-5
    /// dynamic charge and region programs keep that exact cost model.
    pub(crate) fn try_str_append_assign(
        &mut self,
        var: &almide_ir::VarId,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(var) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::Scalar(Scalar::Str))) = self.locals.get(var) else {
            return Ok(false);
        };
        let IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, left, right } = &value.kind
        else {
            return Ok(false);
        };
        if !matches!(&left.kind, IrExprKind::Var { id } if id == var) {
            return Ok(false);
        }
        self.f.instructions().local_get(idx);
        self.lower(right, Some(STR))?;
        self.f.instructions().call(F_STR_APPEND).local_set(idx);
        self.rc_owned.insert(idx);
        Ok(true)
    }

    /// The list twin of [`Self::try_str_append_assign`] (#1729):
    /// `data = data + [e]` — the canonical accumulator loop — routes
    /// through `$cow` + `$list_push_{8,4}` (amortized in-place growth,
    /// the outgrown block freed at rc==1) instead of `$concat`'s full
    /// copy per append. The COW judge keeps value semantics for a
    /// shared accumulator; the element is lowered AFTER the judge but
    /// read against the pre-assign block either way, so a
    /// self-referencing element (`data + [list.len(data)]`) observes
    /// the value before the mutation, exactly like the concat form.
    pub(crate) fn try_list_append_assign(
        &mut self,
        var: &almide_ir::VarId,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(var) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::List(h))) = self.locals.get(var) else {
            return Ok(false);
        };
        let IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, left, right } = &value.kind
        else {
            return Ok(false);
        };
        if !matches!(&left.kind, IrExprKind::Var { id } if id == var) {
            return Ok(false);
        }
        let IrExprKind::List { elements } = &right.kind else {
            return Ok(false);
        };
        let [elem] = &elements[..] else {
            return Ok(false);
        };
        let el = self.types.el(h);
        // SCALAR 8-byte elements only: a 4-byte HANDLE slot (record/str/
        // list element) needs the literal builder's Dup discipline — the
        // container owns the element, the load is a borrow, and pushing
        // the borrowed handle un-Dup'd double-owns it (the C-186 fixture
        // caught exactly that). Heap-element appends keep the concat
        // path, whose outgrown generations the assign dec now frees.
        if el != INT && el != FLOAT {
            return Ok(false);
        }
        self.f.instructions().local_get(idx).call(F_COW);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        self.f.instructions().call(F_LIST_PUSH_8).local_set(idx);
        self.rc_owned.insert(idx);
        Ok(true)
    }
}
