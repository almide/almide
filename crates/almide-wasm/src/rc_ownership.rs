//! RC-3 ownership machinery: the borrow/fresh classifier, the droppable
//! set, and the inc/share/arg guards — split from emitter.rs for the
//! file budget.

use almide_ir::{IrExpr, IrExprKind, VarId};
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

// ── RC-3 ownership helpers ──────────────────────────────────────────────

/// Expressions that certainly PRODUCE a fresh (or pool-static) block —
/// binding one transfers ownership, no inc. Everything else may borrow
/// (var reads, element/field reads, calls into native arms, if/match
/// funnels) and takes the +1; over-inc on a fresh value is a leak,
/// never a dangle.
pub(crate) fn rc_certainly_fresh(k: &almide_ir::IrExprKind) -> bool {
    use almide_ir::IrExprKind as K;
    matches!(
        k,
        K::LitStr { .. }
            | K::StringInterp { .. }
            | K::BinOp { .. }
            | K::List { .. }
            | K::MapLiteral { .. }
            | K::EmptyMap
            | K::Record { .. }
            | K::SpreadRecord { .. }
            | K::Tuple { .. }
            | K::Range { .. }
            | K::ResultOk { .. }
            | K::ResultErr { .. }
            | K::OptionSome { .. }
    )
}

/// The tail expression a fn body RETURNS (through block tails).
pub(crate) fn rc_tail(e: &almide_ir::IrExpr) -> &almide_ir::IrExpr {
    match &e.kind {
        almide_ir::IrExprKind::Block { expr: Some(t), .. } => rc_tail(t),
        _ => e,
    }
}

impl Emitter<'_> {
    /// The v1 droppable set: blocks with NO heap interiors (Str, Bytes,
    /// List of non-handle scalars). Everything else stays on the bump
    /// graveyard until its drop glue exists.
    pub(crate) fn rc_droppable(&self, t: SliceTy) -> bool {
        match t {
            SliceTy::Scalar(Scalar::Str | Scalar::Bytes) => true,
            SliceTy::List(h) => matches!(
                self.types.el(h),
                SliceTy::Scalar(s) if !matches!(s, Scalar::Str | Scalar::Bytes)
            ),
            _ => false,
        }
    }

    /// +1 the i32 block handle on top of the stack, leaving it there.
    pub(crate) fn rc_inc_top(&mut self) {
        let scr = self.scr_i32_local;
        let mut i = self.f.instructions();
        i.local_set(scr);
        i.local_get(scr).call(F_INC);
        i.local_get(scr);
    }
}

impl Emitter<'_> {
    /// RC-3 share guard for handle STORES into blocks: when the stored
    /// expression reads a droppable-owned LOCAL, the container becomes a
    /// co-owner (+1) — otherwise the local's release would free a block
    /// the container still holds. Fresh values and non-droppable
    /// sources pass through untouched (their owner never decs).
    pub(crate) fn rc_share_guard(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        if ty.val_type() != ValType::I32 {
            return;
        }
        // #1219 stage 1: a Map handle stored into a block witnesses a
        // second holder. Maps stay OFF the droppable set (never dec'd),
        // so the count is MONOTONE — "shared at some point" — which is
        // exactly what the in-place set window asks (rc == 1 ⇒ the
        // var's block is its alone). Binds/assigns copy, so a plain var
        // never shares; a fresh value has no other holder to witness.
        if matches!(ty, SliceTy::Map(..)) {
            if !rc_certainly_fresh(&e.kind) {
                self.rc_inc_top();
            }
            return;
        }
        match &e.kind {
            almide_ir::IrExprKind::Var { id } => {
                if self.cells.contains(id) {
                    return;
                }
                let Some(&(_, vt)) = self.locals.get(id) else { return };
                if self.rc_droppable(vt) {
                    self.rc_inc_top();
                }
            }
            // A control funnel can RETURN a var borrow through its arm
            // tails (`push(out, if c then a else b)`) — the O3 gap.
            // Conservative +1 when the stored type itself is droppable:
            // an over-inc on a fresh arm is a leak, never a dangle.
            almide_ir::IrExprKind::If { .. }
            | almide_ir::IrExprKind::Match { .. }
            | almide_ir::IrExprKind::Block { .. }
            | almide_ir::IrExprKind::Unwrap { .. }
            | almide_ir::IrExprKind::UnwrapOr { .. }
            | almide_ir::IrExprKind::Try { .. }
                if self.rc_droppable(ty) =>
            {
                self.rc_inc_top();
            }
            _ => {}
        }
    }
}

/// Does the expression read `var` anywhere? (The Assign dec-old
/// suppressor: a self-referential rhs means ownership moved through it.)
pub(crate) fn rc_mentions_var(e: &IrExpr, var: VarId) -> bool {
    struct Finder {
        var: VarId,
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind
                && *id == self.var
            {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, e);
            }
        }
    }
    let mut f = Finder { var, found: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut f, e);
    f.found
}

impl Emitter<'_> {
    /// RC-3 callee-owned argument guard: droppable args that are not
    /// certainly fresh get +1 at the call site (the callee's epilogue
    /// releases its params). Fresh temporaries transfer as-is — the
    /// callee's release is their consumption.
    pub(crate) fn rc_arg_guard(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        if self.rc_droppable(ty) && !rc_certainly_fresh(&e.kind) {
            self.rc_inc_top();
        }
    }
}

/// True when the body makes ANY `prim.*` module call — the raw-address
/// origin (`prim.handle`, raw loads). A fn whose body touches prim may
/// hand a tail callee a pointer into a param's block, so the tail-site
/// param release (calls.rs) is fenced off for it.
pub(crate) fn body_uses_prim(body: &almide_ir::IrExpr) -> bool {
    struct Scan {
        hit: bool,
    }
    impl almide_ir::visit::IrVisitor for Scan {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if self.hit {
                return;
            }
            if let almide_ir::IrExprKind::Call { target, .. }
            | almide_ir::IrExprKind::TailCall { target, .. } = &e.kind
            {
                if let almide_ir::CallTarget::Module { module, .. } = target
                    && module.as_str() == "prim"
                {
                    self.hit = true;
                    return;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut s = Scan { hit: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut s, body);
    s.hit
}
