//! Counted-while partial unroll — the wasm-field survey's float_math
//! decomposition made the loss mechanical: LLVM unrolls the tight loop 8×
//! and saves pure LOOP-CONTROL overhead (the fmul/fadd chain was proven
//! identical by wasm-dis). This pass claims the same headroom for the
//! structural leg.
//!
//! Scope is deliberately narrow and semantics-preserving by construction:
//!   - UNMETERED programs only. A region-bracketing program keeps the
//!     normative per-check charge count (ALS-DT2: n iterations = n+1
//!     checks), so its loops stay rolled — the meter never sees this pass.
//!   - The exact counted shape: `while i < LIT { …; i = i + 1 }` with the
//!     increment as the LAST statement, no other write to `i`, and no
//!     break/continue/lambda/nested-loop anywhere in the body (a lambda
//!     would lift once per copy; a nested loop's `continue` would need
//!     label bookkeeping this pass does not buy).
//!   - Literal bounds >= a trip floor, small bodies: the pass buys branch
//!     elision on hot tight loops; unrolling cold or fat loops only buys
//!     module size (the size ratchet holds the line).
//!
//! Shape: a FAST LANE `block { loop { i < LIT-(K-1) ? K×body : break } }`
//! followed by the ordinary rolled loop, which drains the remainder — so
//! every iteration still runs exactly once, in order, through the same
//! per-statement lowering as the rolled form.

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::EmitError;

/// The unroll factor — the survey's measured LLVM reference.
const UNROLL: usize = 8;
/// Bodies above this statement count keep the rolled form (size discipline).
const MAX_BODY_STMTS: usize = 8;
/// Literal bounds below this keep the rolled form (cold loops gain nothing).
const MIN_TRIP_LIT: i64 = 1024;

/// True when the statements disqualify the unroll: a write to the induction
/// var outside the tail increment, or any break/continue/lambda/nested loop.
struct Disqualify {
    ivar: VarId,
    hit: bool,
}

impl IrVisitor for Disqualify {
    fn visit_expr(&mut self, e: &IrExpr) {
        if self.hit {
            return;
        }
        match &e.kind {
            IrExprKind::Break
            | IrExprKind::Continue
            | IrExprKind::Lambda { .. }
            | IrExprKind::While { .. }
            | IrExprKind::ForIn { .. } => {
                self.hit = true;
                return;
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        if self.hit {
            return;
        }
        if let IrStmtKind::Assign { var, .. } = &s.kind
            && *var == self.ivar
        {
            self.hit = true;
            return;
        }
        walk_stmt(self, s);
    }
}

impl Emitter<'_> {
    /// Lower `while` through the unrolled fast lane when the shape allows;
    /// `false` means "not eligible — emit the rolled form only". On `true`
    /// the caller STILL emits the rolled loop after this: it drains the
    /// remainder iterations (fewer than UNROLL of them).
    pub(crate) fn try_unroll_while(
        &mut self,
        cond: &IrExpr,
        body: &[IrStmt],
    ) -> Result<bool, EmitError> {
        if self.metered {
            return Ok(false);
        }
        let IrExprKind::BinOp { op: BinOp::Lt, left, right } = &cond.kind else {
            return Ok(false);
        };
        let (IrExprKind::Var { id: ivar }, IrExprKind::LitInt { value: bound }) =
            (&left.kind, &right.kind)
        else {
            return Ok(false);
        };
        let (ivar, bound) = (*ivar, *bound);
        if bound < MIN_TRIP_LIT || body.is_empty() || body.len() > MAX_BODY_STMTS {
            return Ok(false);
        }
        let Some(guard) = bound.checked_sub(UNROLL as i64 - 1) else {
            return Ok(false);
        };
        let Some((last, rest)) = body.split_last() else {
            return Ok(false);
        };
        // Tail increment, exactly `i = i + 1`.
        let tail_is_incr = matches!(&last.kind, IrStmtKind::Assign { var, value }
            if *var == ivar
                && matches!(&value.kind, IrExprKind::BinOp { op: BinOp::AddInt, left: al, right: ar }
                    if matches!(&al.kind, IrExprKind::Var { id } if *id == ivar)
                        && matches!(&ar.kind, IrExprKind::LitInt { value: 1 })));
        if !tail_is_incr {
            return Ok(false);
        }
        if self.cells.contains(&ivar) {
            return Ok(false);
        }
        let Some(&(iidx, ity)) = self.locals.get(&ivar) else {
            return Ok(false);
        };
        if ity != crate::INT {
            return Ok(false);
        }
        let mut scan = Disqualify { ivar, hit: false };
        for s in rest {
            scan.visit_stmt(s);
        }
        if scan.hit {
            return Ok(false);
        }

        // Fast lane: while (i < bound - (K-1)) { K × body }.
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        {
            let mut i = self.f.instructions();
            i.local_get(iidx).i64_const(guard).i64_lt_s().i32_eqz().br_if(1);
        }
        for _ in 0..UNROLL {
            for st in body {
                self.lower_stmt(st)?;
            }
        }
        self.f.instructions().br(0).end().end();
        Ok(true)
    }
}
