//! Bound-range deferral (#1400 / C-238): a `let`-bound range whose EVERY
//! use is a `for-in` head never materializes — the loop counts between
//! two i64 locals. The load-bearing cell is 4294967295 iterations
//! (range_bind_huge.almd): a materializing leg needs ~32 GB; the
//! counting leg needs two locals. Any other reading of the var — an
//! index, a `list.len`, a reassignment, a capture into a lambda —
//! DISQUALIFIES the bind, and the value-position `Range` arm in
//! emitter.rs materializes the real `List[Int]` block instead (with the
//! C-197 `out of memory` die past the wasm leg's own structural bound —
//! success between the two legs' bounds is the contracted divergence,
//! runtime/rs list.rs ratified A 2026-08-17).

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

#[derive(Default)]
struct RangeScan {
    /// Bind var → inclusive flag, for binds whose initializer is a Range.
    candidates: HashMap<VarId, bool>,
    /// Var-expression occurrences, all positions.
    total: HashMap<VarId, usize>,
    /// Occurrences that are exactly a `for-in` head (outside lambdas).
    heads: HashMap<VarId, usize>,
    /// Reassigned or rebound vars — never deferred.
    tainted: HashSet<VarId>,
    lambda_depth: usize,
}

impl IrVisitor for RangeScan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Var { id } => {
                *self.total.entry(*id).or_default() += 1;
                if self.lambda_depth > 0 {
                    // A capture would need a real block to cross the
                    // closure boundary.
                    self.tainted.insert(*id);
                }
            }
            IrExprKind::Lambda { .. } => {
                self.lambda_depth += 1;
                walk_expr(self, e);
                self.lambda_depth -= 1;
                return;
            }
            IrExprKind::ForIn { iterable, .. } => {
                if self.lambda_depth == 0
                    && let IrExprKind::Var { id } = &iterable.kind
                {
                    *self.heads.entry(*id).or_default() += 1;
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                if let IrExprKind::Range { inclusive, .. } = &value.kind {
                    if self.candidates.insert(*var, *inclusive).is_some() {
                        // Rebound (shadowing shares the VarId only if the
                        // front end reuses it — either way, not deferrable).
                        self.tainted.insert(*var);
                    }
                } else {
                    self.tainted.insert(*var);
                }
            }
            IrStmtKind::Assign { var, .. } => {
                self.tainted.insert(*var);
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

/// Vars deferrable to the counting path: bound ONCE to a syntactic
/// range, never assigned, and every occurrence is a for-in head.
pub(crate) fn deferred_ranges_of(body: &IrExpr) -> HashMap<VarId, bool> {
    let mut scan = RangeScan::default();
    scan.visit_expr(body);
    scan.candidates
        .into_iter()
        .filter(|(v, _)| {
            !scan.tainted.contains(v)
                && scan.total.get(v).copied().unwrap_or(0)
                    == scan.heads.get(v).copied().unwrap_or(0)
        })
        .collect()
}

use crate::*;
use wasm_encoder::{BlockType, MemArg};

impl crate::emitter::Emitter<'_> {
    /// Range in VALUE position (see the comments below) — extracted from
    /// the emitter dispatcher for the complexity budget.
            // Range in VALUE position materializes the real List[Int]
            // (the front end types it Applied(List, [Int])). Span follows
            // native list_range: end.saturating_sub(start).max(0) — the
            // saturation is real i64-overflow detection, so (i64::MIN, 3)
            // is the C-197 die, not an empty list. Past the wasm leg's own
            // structural bound the same "Error: out of memory" + exit 1
            // fires BEFORE the allocator (success between the two legs'
            // bounds is the contracted divergence, runtime/rs list.rs).
    pub(crate) fn lower_range_value(
        &mut self,
        start: &IrExpr,
        end: &IrExpr,
        inclusive: bool,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let inclusive = &inclusive;
        Ok({

                let hty = SliceTy::List(self.types.intern(INT));
                if let Some(w) = want
                    && w != hty
                {
                    return unsup(&format!("ty-mismatch:range-vs-{w:?}"));
                }
                self.lower(start, Some(INT))?;
                let hs = self.hold_i64()?;
                self.f.instructions().local_set(hs);
                self.lower(end, Some(INT))?;
                let he = self.hold_i64()?;
                let hd = self.hold_i64()?;
                let hb = self.hold_i32()?;
                let hc = self.hold_i32()?;
                let msg = self.pool.intern("out of memory");
                // Block bytes must fit a positive i32: span*8 + header.
                const RANGE_CAP: i64 = ((i32::MAX - 16) / 8) as i64;
                {
                    let mut i = self.f.instructions();
                    i.local_set(he);
                    if *inclusive {
                        i.local_get(he).i64_const(1).i64_add().local_set(he);
                    }
                    // d = he - hs (wrapping); true span positive iff
                    // he > hs; positive overflow iff sign(he)!=sign(hs)
                    // and sign(d)!=sign(he) — then any past-cap value
                    // stands in for the saturated span.
                    i.local_get(he).local_get(hs).i64_sub().local_set(hd);
                    i.i64_const(RANGE_CAP + 1);
                    i.local_get(hd);
                    i.local_get(he).local_get(hs).i64_xor();
                    i.local_get(he).local_get(hd).i64_xor();
                    i.i64_and().i64_const(0).i64_lt_s();
                    i.select();
                    i.i64_const(0);
                    i.local_get(he).local_get(hs).i64_gt_s();
                    i.select();
                    i.local_set(hd);
                    i.local_get(hd).i64_const(RANGE_CAP).i64_gt_s();
                    i.if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                {
                    let mut i = self.f.instructions();
                    i.end();
                    i.local_get(hd)
                        .i64_const(8)
                        .i64_mul()
                        .i32_wrap_i64()
                        .call(F_ALLOC)
                        .local_set(hb);
                    // fill ascending: payload[k] = start + k
                    i.local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(hc);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hd).i64_const(0).i64_le_s().br_if(1);
                    i.local_get(hc).local_get(hs).i64_store(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    });
                    i.local_get(hs).i64_const(1).i64_add().local_set(hs);
                    i.local_get(hc).i32_const(8).i32_add().local_set(hc);
                    i.local_get(hd).i64_const(1).i64_sub().local_set(hd);
                    i.br(0);
                    i.end();
                    i.end();
                    i.local_get(hb);
                }
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                hty
        })
    }
}
