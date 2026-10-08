//! The String-channel conversions at a `!` propagation: a typed error
//! carried as its repr text (ADR-0021 D2, #2725) and a `List[String]`
//! error joined with `", "` (#2748), and the braced-raise peel the guard's
//! propagation recognizer uses — split from data.rs for the file budget.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// A braced raise (`guard c else { err(m)! }`) is the bare propagation
/// inside a block with no statements, which adds no scope (#2748): the
/// guard's early-return recognizer sees through it.
pub(crate) fn through_empty_blocks(mut e: &IrExpr) -> &IrExpr {
    while let IrExprKind::Block { stmts, expr: Some(inner) } = &e.kind
        && stmts.is_empty()
    {
        e = inner;
    }
    e
}

impl Emitter<'_> {
    /// `expr!` whose operand fails with a `List[String]` inside a String
    /// channel (#2748): native's `map_err_join` — the propagated block is a
    /// FRESH `err(errs.join(", "))`. Read, never moved, exactly as
    /// [`Self::propagate_err_as_repr`] reads its carrier. Emits the
    /// propagation's `return`.
    pub(crate) fn propagate_err_joined(
        &mut self,
        carrier_ty: SliceTy,
        ert: SliceTy,
        owned_carrier: bool,
    ) -> Result<(), EmitError> {
        let car = self.hold_i32()?;
        let blk = self.hold_i32()?;
        let sep = self.pool.intern(", ");
        self.f.instructions().local_get(self.scr_i32_local).local_tee(car);
        self.load_ty_slot(ert, almide_layout::SUM_FIELD);
        // $list_join borrows both and hands back a fresh String.
        self.f
            .instructions()
            .i32_const(sep as i32)
            .call(F_LIST_JOIN)
            .local_set(self.tmp_i32_local)
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(blk)
            .i32_const(1)
            .i32_store(slot_memarg(almide_layout::SUM_TAG))
            .local_get(blk)
            .local_get(self.tmp_i32_local)
            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
        if owned_carrier {
            let dec = self.dec_fn_of(carrier_ty);
            self.f.instructions().local_get(car).call(dec);
        }
        let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
        self.emit_exit(&plan);
        self.f.instructions().local_get(blk).return_();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// ADR-0021 D2 (#2725): `expr!` whose operand fails with a typed `E`
    /// inside a String channel (a `-> T!` fn, or a lambda whose `!`s did
    /// not agree on one `E`). Native converts at the `!` with
    /// `almide_repr` — the text `"${e}"` shows — so the propagated block is
    /// a FRESH `err(<repr of e>)`, not the operand's. The carrier (in
    /// `scr_i32_local`, tag already known non-zero) is read, never moved:
    /// an owned temporary is released here, a borrowed one stays with the
    /// route that owns it. Emits the propagation's `return`.
    pub(crate) fn propagate_err_as_repr(
        &mut self,
        carrier_ty: SliceTy,
        ert: SliceTy,
        err_ir: Option<&Ty>,
        owned_carrier: bool,
        wc: crate::witness_unwrap::WCarrier,
    ) -> Result<(), EmitError> {
        let car = self.hold_i32()?;
        self.f.instructions().local_get(self.scr_i32_local).local_set(car);
        self.emit_repr_message(ert, err_ir, |s| {
            s.f.instructions().local_get(car);
            s.load_ty_slot(ert, almide_layout::SUM_FIELD);
        })?;
        let msg = self.hold_i32()?;
        let start = self.hold_i32()?;
        self.f.instructions().local_set(msg);
        // err(msg): the same 16-byte String-channel err block `none` builds.
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(start)
            .i32_const(1)
            .i32_store(slot_memarg(almide_layout::SUM_TAG))
            .local_get(start)
            .local_get(msg)
            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
        if owned_carrier {
            let dec = self.dec_fn_of(carrier_ty);
            self.f.instructions().local_get(car).call(dec);
        }
        self.witness_repr_built(wc);
        let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
        self.emit_exit(&plan);
        self.f.instructions().local_get(start).return_();
        self.witness_unwrap_exit(wc, crate::witness_unwrap::Leaves::Fresh);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// The repr text of an error value (native's `almide_repr`, the text
    /// `"${e}"` shows) as a FRESH String block, left on the stack. A
    /// one-part `"${e}"` build (the StringInterp capture, emitter.rs): start
    /// at the published cursor, append the display, capture, and hand the
    /// line buffer back. `push` puts the value on the stack.
    pub(crate) fn emit_repr_message(
        &mut self,
        ert: SliceTy,
        err_ir: Option<&Ty>,
        push: impl FnOnce(&mut Self),
    ) -> Result<(), EmitError> {
        let start = self.hold_i32()?;
        self.f
            .instructions()
            .global_get(G_LINE_CURSOR)
            .local_tee(start)
            .local_set(self.cursor_local);
        push(self);
        self.build_depth += 1;
        let shown = self.emit_display_value(ert, false, err_ir);
        self.build_depth -= 1;
        shown?;
        self.f
            .instructions()
            .local_get(start)
            .local_get(self.cursor_local)
            .call(F_BUF_TO_BLOCK)
            .local_get(start)
            .global_set(G_LINE_CURSOR)
            .local_get(start)
            .local_set(self.cursor_local);
        self.release_i32();
        Ok(())
    }

    /// The message `main`'s abort prints for an error value of type `ert`
    /// (#3470, C-035), left on the stack — native's rendering of the error at
    /// the `fn main` wrapper: a `String` as is, a `List[String]` joined with
    /// `", "` (`map_err_join`), any other type its repr. `push` puts the
    /// value on the stack. Returns whether the message is a FRESH block (the
    /// abort takes it with it).
    pub(crate) fn emit_abort_message(
        &mut self,
        ert: SliceTy,
        err_ir: Option<&Ty>,
        push: impl FnOnce(&mut Self),
    ) -> Result<bool, EmitError> {
        if ert == STR {
            push(self);
            return Ok(false);
        }
        if self.is_str_list(ert) {
            push(self);
            let sep = self.pool.intern(", ");
            self.f.instructions().i32_const(sep as i32).call(F_LIST_JOIN);
            return Ok(true);
        }
        self.emit_repr_message(ert, err_ir, push)?;
        Ok(true)
    }
}

impl Emitter<'_> {
    /// main's err channel (#1734, the pre-existing structural hole): a
    /// Result-typed expression in MAIN's statement/tail position is the
    /// effect carrier, not a discardable value — the native/interp
    /// contract is `Error: {msg}` on stderr + exit 1 on err, plain
    /// fallthrough on ok. Discarding it swallowed the err (silent exit
    /// 0 — `effect fn main() -> Unit = err("boom")` on the released
    /// 0.61.0). Returns Ok(true) when this handled the expression.
    /// A non-String err payload (#3474) aborts with the message main's `!`
    /// abort renders from it (`emit_abort_message`): its repr, or a
    /// `List[String]` joined — native's wrapper prints the same.
    pub(crate) fn try_lower_main_err_carrier(&mut self, e: &IrExpr) -> Result<bool, EmitError> {
        use almide_types::types::{Ty, TypeConstructorId};
        if !self.in_main {
            return Ok(false);
        }
        let Ty::Applied(TypeConstructorId::Result, a) = &e.ty else {
            return Ok(false);
        };
        if a.len() != 2 {
            return Ok(false);
        }
        let got = self.lower(e, None)?;
        let SliceTy::Result(_, eh) = got else {
            // Effect-ABI transparency already unwrapped it — nothing to route.
            self.f.instructions().drop();
            return Ok(true);
        };
        let ert = self.types.el(eh);
        let err_ir = crate::display::ir_arg(Some(&e.ty), 1).cloned();
        // #2969: an OWNED ok carrier (`effect fn main() -> Result[Unit,
        // String] = { … }` ends in a fresh `ok(())`) is main's last use of
        // it — released on the ok path, where the err path aborts.
        let owned_dec = (self.rc_droppable(got) && self.rc_owned_result(e)).then(|| self.dec_fn_of(got));
        // #2758: the err arm aborts (the checker's terminal), the ok arm
        // releases an owned carrier.
        // A message rendered from a non-String error is a FRESH block the
        // abort takes with it (`emit_abort_message`).
        self.witness_main_carrier(owned_dec.is_some(), ert != STR);
        let hb = self.scr_i32_local;
        self.f
            .instructions()
            .local_set(hb)
            .local_get(hb)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .if_(BlockType::Empty);
        self.emit_abort_message(ert, err_ir.as_ref(), |s| {
            s.f.instructions().local_get(hb);
            s.load_ty_slot(ert, almide_layout::SUM_FIELD);
        })?;
        self.emit_error_frame_abort();
        if let Some(dec) = owned_dec {
            self.f.instructions().else_().local_get(hb).call(dec);
        }
        self.f.instructions().end();
        Ok(true)
    }
}
