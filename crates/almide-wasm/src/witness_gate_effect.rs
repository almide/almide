//! The witness gate's effect-body rule (split from witness_gate.rs for the
//! file budget): an effect fn's body is judged as a straight-line one, after
//! the transparent `ok(v)` carriers of its raw positions.

use almide_ir::{IrExpr, IrExprKind};

use super::{scalar_ty, straightline_subset};

/// #2758: the gate for an EFFECT frame. Its body lowers at the raw ok type
/// and func.rs wraps the result in the ok carrier (both recorded there), so
/// the straight-line rules apply unchanged — except to a constructor of the
/// carrier itself. Inside an effect body, where the raw type is expected:
///
/// - `err(e)` RAISES through the frame's error exit (data.rs
///   `lower_err_raise`): the err block is built (its payload store is
///   `witness_store`), the exit plan's releases are recorded like a `!`
///   propagation's, and the block leaves (`witness_err_raise`). Admitted.
/// - `ok(v)` is transparent — the value IS `v` — while every consumer reads
///   the node as a fresh construction (`rc_owned_result`). That agrees when
///   `v` (through block tails) is a scalar, certainly fresh, or a call's
///   handed-over credit; a borrowed Var in a RAW tail position takes its
///   share at the node (data.rs `lower_err_raise`, the carrier's share-out on
///   the Var's line). Any other `ok(v)` declines as
///   `effect:carrier:ok-borrowed`.
pub fn effect_subset(body: &IrExpr, raw_is_heap: bool) -> Option<String> {
    // The `ok(v)` nodes in the body's RAW tail positions (through block tails
    // and branch arms): the only ones `lower_err_raise` lowers transparently,
    // and so the only ones whose borrowed Var it shares. An `ok(v)` anywhere
    // else (`some(ok(s))`) is a real construction this rule does not widen.
    fn raw_oks(e: &IrExpr, out: &mut Vec<*const IrExpr>) {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => raw_oks(t, out),
            IrExprKind::If { then, else_, .. } => {
                raw_oks(then, out);
                raw_oks(else_, out);
            }
            IrExprKind::Match { arms, .. } => arms.iter().for_each(|a| raw_oks(&a.body, out)),
            IrExprKind::ResultOk { .. } => out.push(e as *const IrExpr),
            _ => {}
        }
    }
    struct Carrier(bool, Vec<*const IrExpr>);
    impl almide_ir::visit::IrVisitor for Carrier {
        fn visit_expr(&mut self, e: &IrExpr) {
            // A call's droppable result is its callee's handed-over credit
            // (#1986); one an arm declares a View declines at emission
            // (data.rs `lower_err_raise`).
            // A block's value is its tail's (`{ let t = f(x); "w" + t }`,
            // arg_temps.rs): its binds are the body's own sites, judged
            // with the body below.
            let raw = self.1.contains(&(e as *const IrExpr));
            if let IrExprKind::ResultOk { expr } = &e.kind
                && !scalar_ty(&expr.ty)
                && !crate::rc_ownership::rc_certainly_fresh(&crate::rc_ownership::rc_tail(expr).kind)
                && !matches!(crate::rc_ownership::rc_tail(expr).kind, IrExprKind::Call { .. })
                && !(raw && matches!(crate::rc_ownership::rc_tail(expr).kind, IrExprKind::Var { .. }))
            {
                self.0 = true;
            } else if !self.0 {
                almide_ir::visit::walk_expr(self, e);
            }
        }
    }
    let mut oks = Vec::new();
    raw_oks(body, &mut oks);
    let mut c = Carrier(false, oks);
    almide_ir::visit::IrVisitor::visit_expr(&mut c, body);
    if c.0 {
        return Some("effect:carrier:ok-borrowed".into());
    }
    straightline_subset(body, raw_is_heap)
}
