//! Loop statements and their `break` / `continue` wiring (#2745) — split
//! from stmts.rs for the file budget.
//!
//! `loop_ctl` counts the labels a statement-position `if` / `guard` / match
//! arm opens between a jump and its loop's continue target. Ownership needs
//! nothing on the jump edge: the body's heap locals are FRAME credits (the
//! next pass's rebind releases the previous occupant, the epilogue the last),
//! so leaving the body early leaves them exactly where a fall-through pass
//! does. The one exception is a named list rest of an arm the jump leaves:
//! no rebind releases it, so the jump edge does (#3377, arm_rests.rs).

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `while`: block { loop { !cond → br out; body; br loop } }.
    /// `continue` brs to the loop head (the next cond CHECK, which
    /// charges — the interp's per-check meter), `break` to the block.
    pub(crate) fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), EmitError> {
        // #2150: one copy-on-write judge per loop entry for a list the loop
        // reaches only element-wise — cleared before the unrolled lane too,
        // which runs copies of this same condition and body.
        let flags = self.hoist_cow_flags(Some(cond), body)?;
        // #3345: the pre-judge, guarded by one extra evaluation of an INERT
        // condition (cow_hoist.rs) — before the unrolled lane, whose bodies
        // run only where the condition holds.
        let pre = if crate::cow_hoist::inert_cond(cond) {
            self.prejudge_first_stores(Some(cond), body, &|e: &mut Self| e.lower(cond, Some(BOOL)).map(|_| ()))?
        } else {
            Vec::new()
        };
        // #3345: address-stable lists address through a payload pointer.
        let ptrs = self.hoist_payload_ptrs(Some(cond), body, &pre)?;
        // Counted-shape fast lane (unroll.rs): on `true` the rolled loop
        // below drains the remainder iterations.
        let _ = self.try_unroll_while(cond, body)?;
        // #2319: element counts this loop cannot change are loaded once,
        // before the loop — the bounds checks inside read the local.
        let hoisted = self.hoist_invariant_counts(Some(cond), body)?;
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        // Deterministic meter: one loop-head charge per condition
        // CHECK (n iterations = n+1 checks), ALS-DT2.
        self.emit_det_charge_const(1);
        // The witness (#2757): an iteration runs the condition, then either
        // leaves (the failing check) or runs the body.
        self.witness_loop_open();
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().i32_eqz().br_if(1);
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_loop_jump();
        self.witness_branch_arm();
        self.lower_loop_body(body, false)?;
        self.witness_branch_close();
        self.witness_loop_close();
        self.f.instructions().br(0).end().end();
        self.drop_hoisted_counts(hoisted);
        self.drop_payload_ptrs(ptrs);
        self.drop_prejudged(pre);
        self.drop_cow_flags(flags);
        Ok(())
    }

    /// A loop body with break/continue wired. For-in bodies sit in an
    /// extra block so `continue` still reaches the STEP code after it;
    /// a while `continue` brs straight to the loop head (the next cond
    /// check). break_delta = labels from the continue target up to the
    /// exit block (while: 1; for-in: 2 — the inner block adds one).
    pub(crate) fn lower_loop_body(&mut self, body: &[IrStmt], for_in: bool) -> Result<(), EmitError> {
        let saved = self.loop_ctl.take();
        let rest_floor = self.open_loop_rests();
        if for_in {
            self.f.instructions().block(BlockType::Empty);
            self.loop_ctl = Some((0, 2));
        } else {
            self.loop_ctl = Some((0, 1));
        }
        // #3345: each iteration starts with no bounds facts (bounds_facts.rs).
        let outer_facts = self.bounds_facts.replace(Vec::new());
        self.lower_stmts_moving(body, None, Self::lower_stmt_with_facts)?;
        self.bounds_facts = outer_facts;
        if for_in {
            self.f.instructions().end();
        }
        self.loop_ctl = saved;
        self.close_loop_rests(rest_floor);
        Ok(())
    }

    /// `continue` / `break` in statement position: a branch to the loop
    /// context's continue label (`break` adds the depth to its exit). The
    /// arms the jump leaves release their list rests first (#3377).
    pub(crate) fn lower_loop_jump(&mut self, brk: bool) -> Result<(), EmitError> {
        let Some((extra, delta)) = self.loop_ctl else {
            return unsup(if brk { "expr:Break" } else { "expr:Continue" });
        };
        self.release_jumped_rests();
        self.f.instructions().br(if brk { extra + delta } else { extra });
        self.witness_loop_jump();
        Ok(())
    }

    /// A statement body inside one freshly opened label (an `if_` the
    /// caller wrote): break/continue targets shift one deeper for it.
    pub(crate) fn lower_stmt_in_label(&mut self, e: &IrExpr) -> Result<(), EmitError> {
        self.shift_loop_labels(1);
        self.branch_depth += 1;
        let r = self.lower_stmt_expr(e);
        self.branch_depth -= 1;
        self.shift_loop_labels(-1);
        r
    }

    /// Labels opened (+) or closed (-) between the loop's continue target
    /// and the statement being lowered. No-op outside a loop body.
    pub(crate) fn shift_loop_labels(&mut self, by: i32) {
        if let Some((extra, _)) = self.loop_ctl.as_mut() {
            *extra = extra.checked_add_signed(by).expect("loop label depth underflow");
        }
    }

    /// `guard c else break` / `else continue` (#2745): the else leaves
    /// the loop body, not the frame — no return, no exit plan. Returns
    /// false (nothing emitted) for any other guard.
    pub(crate) fn try_lower_guard_loop_ctl(&mut self, cond: &IrExpr, else_: &IrExpr) -> Result<bool, EmitError> {
        if self.loop_ctl.is_none() || !ends_in_loop_ctl(else_) {
            return Ok(false);
        }
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        // The witness (#2757): a one-arm site whose arm leaves the iteration.
        self.witness_branch_open();
        self.witness_branch_arm();
        self.lower_stmt_in_label(else_)?;
        self.witness_branch_arm();
        self.witness_branch_close();
        self.f.instructions().end();
        Ok(true)
    }
}

/// Does this guard else leave the enclosing LOOP body (`break` /
/// `continue`, possibly after statements in a block) rather than the
/// frame? Such an else is not the function's return (#2745).
fn ends_in_loop_ctl(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Break | IrExprKind::Continue => true,
        IrExprKind::Block { expr: Some(tail), .. } => ends_in_loop_ctl(tail),
        IrExprKind::Block { stmts, expr: None } => matches!(
            stmts.last().map(|s| &s.kind),
            Some(IrStmtKind::Expr { expr }) if ends_in_loop_ctl(expr)
        ),
        _ => false,
    }
}
