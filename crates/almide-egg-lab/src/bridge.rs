//! Bridge: `almide_ir::IrExpr` ↔ `egg::RecExpr<AlmideExpr>`.
//!
//! This is Stage 1 foundation work: the egg rewriter has to operate
//! on real Almide IR, not just string-parsed toy expressions. The
//! bridge lifts list-combinator subtrees (`list.map / filter / fold`
//! with their lambda args) into egg nodes while storing
//! non-combinator leaves in a side table (`slots`). Saturation then
//! runs over a mixed graph of structural nodes + opaque references.
//!
//! ## Scope of this PoC
//!
//! - **Lift**: IrExpr → RecExpr. Handles `list.map / filter / fold`
//!   nested arbitrarily; everything else becomes an opaque slot.
//! - **Lower**: not implemented in this PoC. Reconstruction to a
//!   well-typed IrExpr requires either (a) beta-reducing compose /
//!   and-pred markers into fresh lambdas (needs VarTable access) or
//!   (b) keeping them as Named-call pseudo-ops for a later pass.
//!   Both are Phase C work. For now the bridge proves **lift +
//!   saturation** work on real IR; round-tripping lands next.
//!
//! ## Why slot-based opaque references
//!
//! egg's e-graph requires every leaf to be an `AlmideExpr` node. Real
//! IrExpr includes `LitStr`, arbitrary user calls, `BinOp`, `Match`,
//! records — expressing each as an egg variant would bloat the
//! Language enum and slow saturation. A side table keeps the egg
//! representation small and lets us round-trip opaque subtrees
//! verbatim (modulo the fusion rewrites at the structural level).

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use egg::{Id, RecExpr, Symbol as EggSym};

use crate::AlmideExpr;

/// Lifter for `IrExpr` → `RecExpr<AlmideExpr>`.
///
/// Stateful: accumulates opaque leaves in `slots` so that the caller
/// (or a future lower pass) can recover the original IrExpr fragments
/// referenced from the e-graph.
pub struct Bridge {
    slots: Vec<IrExpr>,
}

impl Bridge {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Opaque subtrees captured during lift, indexed by the `_slot_N`
    /// suffix embedded in their placeholder symbols.
    pub fn slots(&self) -> &[IrExpr] {
        &self.slots
    }

    /// Lift an `IrExpr` into an e-graph representation. Returns the
    /// full `RecExpr` plus the root id. Callers hand the `RecExpr`
    /// (plus `self.slots()`) to egg's `Runner`.
    pub fn lift(&mut self, expr: &IrExpr) -> (RecExpr<AlmideExpr>, Id) {
        let mut rec = RecExpr::default();
        let root = self.lift_node(expr, &mut rec);
        (rec, root)
    }

    fn lift_node(&mut self, expr: &IrExpr, rec: &mut RecExpr<AlmideExpr>) -> Id {
        if let IrExprKind::Call { target, args, .. } = &expr.kind {
            if let CallTarget::Module { module, func } = target {
                let m = module.as_str();
                let f = func.as_str();

                // list.map(xs, f)
                if m == "list" && f == "map" && args.len() == 2 {
                    let xs = self.lift_node(&args[0], rec);
                    let f_id = self.lift_lambda(&args[1], rec);
                    return rec.add(AlmideExpr::Map([xs, f_id]));
                }
                // list.filter(xs, p)
                if m == "list" && f == "filter" && args.len() == 2 {
                    let xs = self.lift_node(&args[0], rec);
                    let p = self.lift_lambda(&args[1], rec);
                    return rec.add(AlmideExpr::Filter([xs, p]));
                }
                // list.fold(xs, init, f)
                if m == "list" && f == "fold" && args.len() == 3 {
                    let xs = self.lift_node(&args[0], rec);
                    let init = self.lift_node(&args[1], rec);
                    let f_id = self.lift_lambda(&args[2], rec);
                    return rec.add(AlmideExpr::Fold([xs, init, f_id]));
                }
            }
        }
        self.opaque(expr, rec)
    }

    /// Lift a lambda argument. Identity lambdas (`(x) => x`) map to
    /// the dedicated `identity` symbol so the fusion rule can fire.
    /// Everything else is stored opaquely and wrapped in `(lam _slot_N)`.
    fn lift_lambda(&mut self, expr: &IrExpr, rec: &mut RecExpr<AlmideExpr>) -> Id {
        if is_identity_lambda(expr) {
            return rec.add(AlmideExpr::Symbol(EggSym::from("identity")));
        }
        let slot_id = self.opaque(expr, rec);
        rec.add(AlmideExpr::Lam([slot_id]))
    }

    fn opaque(&mut self, expr: &IrExpr, rec: &mut RecExpr<AlmideExpr>) -> Id {
        let idx = self.slots.len();
        self.slots.push(expr.clone());
        let name = format!("_slot_{}", idx);
        rec.add(AlmideExpr::Symbol(EggSym::from(name.as_str())))
    }
}

/// Detect the shape `(x) => x`. Real Almide lowering represents this
/// as `IrExprKind::Lambda { params: [(var, ty)], body: Var { id: var } }`.
fn is_identity_lambda(expr: &IrExpr) -> bool {
    let IrExprKind::Lambda { params, body, .. } = &expr.kind else {
        return false;
    };
    let [(param_var, _)] = params.as_slice() else {
        return false;
    };
    matches!(&body.kind, IrExprKind::Var { id } if id == param_var)
}
