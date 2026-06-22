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
//! - **Lower**: implemented for `map`, `filter`, `fold`, slot
//!   references, and the two fusion markers. `compose` / `and-pred`
//!   are beta-reduced into real `IrExprKind::Lambda` nodes with fresh
//!   VarIds allocated from a caller-supplied `VarTable`. Non-combinator
//!   subtrees flow back verbatim from the slot table.
//!
//! ## Why slot-based opaque references
//!
//! egg's e-graph requires every leaf to be an `AlmideExpr` node. Real
//! IrExpr includes `LitStr`, arbitrary user calls, `BinOp`, `Match`,
//! records — expressing each as an egg variant would bloat the
//! Language enum and slow saturation. A side table keeps the egg
//! representation small and lets us round-trip opaque subtrees
//! verbatim (modulo the fusion rewrites at the structural level).
//!
//! ## Where lambda substitution happens
//!
//! Saturation keeps `compose g f` and `and-pred p q` as zero-cost
//! markers — e-graphs do not represent binders natively, and doing
//! honest beta-reduction inside saturation requires alpha-renaming
//! machinery (see egg's `examples/lambda.rs`) that would bloat the
//! PoC without changing the verified properties.
//!
//! Instead the extracted `RecExpr` is single-shot beta-reduced during
//! `lower`: each surviving marker becomes a fresh `IrExprKind::Lambda`
//! whose param VarId is allocated through the caller's `VarTable`.
//! Because we only touch the one extracted form (not the full
//! e-graph), the fresh-id allocation is bounded by output size and
//! `use_count` stays consistent with the surrounding IR.

use almide_base::intern::{sym, Sym};
use almide_ir::{
    substitute_var_in_expr, BinOp, CallTarget, IrExpr, IrExprKind, IrStmtKind, Mutability,
    VarId, VarTable,
};
use almide_lang::types::Ty;
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
    ///
    /// Pre-processing: `let`-split chains of matrix ops are inlined
    /// into the trailing expression when every binding is used
    /// exactly once downstream. This lets equality saturation fuse
    /// patterns the imperative `MatrixFusionPass` would otherwise
    /// have to pick up via `fuse_let_split_chain`.
    pub fn lift(&mut self, expr: &IrExpr) -> (RecExpr<AlmideExpr>, Id) {
        let inlined = inline_let_split_matrix_chain(expr);
        let subject = inlined.as_ref().unwrap_or(expr);
        let mut rec = RecExpr::default();
        let root = self.lift_node(subject, &mut rec);
        (rec, root)
    }

    fn lift_node(&mut self, expr: &IrExpr, rec: &mut RecExpr<AlmideExpr>) -> Id {
        if let IrExprKind::Call { target, args, .. } = &expr.kind {
            if let CallTarget::Module { module, func, .. } = target {
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
                // list.flat_map(xs, f)
                if m == "list" && f == "flat_map" && args.len() == 2 {
                    let xs = self.lift_node(&args[0], rec);
                    let f_id = self.lift_lambda(&args[1], rec);
                    return rec.add(AlmideExpr::FlatMap([xs, f_id]));
                }
                // list.filter_map(xs, f)
                if m == "list" && f == "filter_map" && args.len() == 2 {
                    let xs = self.lift_node(&args[0], rec);
                    let f_id = self.lift_lambda(&args[1], rec);
                    return rec.add(AlmideExpr::FilterMap([xs, f_id]));
                }

                // Matrix ops — atomic forms. Children recurse so nested
                // matrix subtrees fuse in the same e-graph. Non-matrix
                // args (scalars, bindings) fall through to `opaque`.
                if m == "matrix" {
                    if let Some(node) = self.lift_matrix(f, args, rec) {
                        return node;
                    }
                }
            }
        }
        self.opaque(expr, rec)
    }

    /// Map the stdlib `matrix.<func>` name + arg list onto an
    /// `AlmideExpr` enum variant. Returns `None` when the name or
    /// arity is not one of the fusion-relevant shapes — the caller
    /// falls back to storing the whole call as an opaque slot.
    fn lift_matrix(
        &mut self,
        func: &str,
        args: &[IrExpr],
        rec: &mut RecExpr<AlmideExpr>,
    ) -> Option<Id> {
        match (func, args.len()) {
            ("mul", 2) => {
                let a = self.lift_node(&args[0], rec);
                let b = self.lift_node(&args[1], rec);
                Some(rec.add(AlmideExpr::MatrixMul([a, b])))
            }
            ("add", 2) => {
                let a = self.lift_node(&args[0], rec);
                let b = self.lift_node(&args[1], rec);
                Some(rec.add(AlmideExpr::MatrixAdd([a, b])))
            }
            ("scale", 2) => {
                let m = self.lift_node(&args[0], rec);
                let s = self.lift_node(&args[1], rec);
                Some(rec.add(AlmideExpr::MatrixScale([m, s])))
            }
            ("gelu", 1) => {
                let m = self.lift_node(&args[0], rec);
                Some(rec.add(AlmideExpr::MatrixGelu([m])))
            }
            ("softmax_rows", 1) => {
                let m = self.lift_node(&args[0], rec);
                Some(rec.add(AlmideExpr::MatrixSoftmaxRows([m])))
            }
            ("linear_row", 3) => {
                let x = self.lift_node(&args[0], rec);
                let w = self.lift_node(&args[1], rec);
                let b = self.lift_node(&args[2], rec);
                Some(rec.add(AlmideExpr::MatrixLinearRow([x, w, b])))
            }
            ("layer_norm_rows", 4) => {
                let x = self.lift_node(&args[0], rec);
                let gamma = self.lift_node(&args[1], rec);
                let beta = self.lift_node(&args[2], rec);
                let eps = self.lift_node(&args[3], rec);
                Some(rec.add(AlmideExpr::MatrixLayerNormRows([x, gamma, beta, eps])))
            }

            // Fused forms — normally produced only by rewrites, but we
            // also accept them as input (e.g. if the user called the
            // intrinsic directly) so that lift is idempotent.
            ("fused_gemm_bias_scale_gelu", 4) => {
                let a = self.lift_node(&args[0], rec);
                let b = self.lift_node(&args[1], rec);
                let bias = self.lift_node(&args[2], rec);
                let alpha = self.lift_node(&args[3], rec);
                Some(rec.add(AlmideExpr::MatrixFusedGemmBiasScaleGelu([a, b, bias, alpha])))
            }
            ("attention_weights", 3) => {
                let q = self.lift_node(&args[0], rec);
                let kt = self.lift_node(&args[1], rec);
                let s = self.lift_node(&args[2], rec);
                Some(rec.add(AlmideExpr::MatrixAttentionWeights([q, kt, s])))
            }
            ("scaled_dot_product_attention", 4) => {
                let q = self.lift_node(&args[0], rec);
                let kt = self.lift_node(&args[1], rec);
                let v = self.lift_node(&args[2], rec);
                let s = self.lift_node(&args[3], rec);
                Some(rec.add(AlmideExpr::MatrixScaledDotProductAttention([q, kt, v, s])))
            }
            ("pre_norm_linear", 6) => {
                let x = self.lift_node(&args[0], rec);
                let gamma = self.lift_node(&args[1], rec);
                let beta = self.lift_node(&args[2], rec);
                let eps = self.lift_node(&args[3], rec);
                let w = self.lift_node(&args[4], rec);
                let b = self.lift_node(&args[5], rec);
                Some(rec.add(AlmideExpr::MatrixPreNormLinear([x, gamma, beta, eps, w, b])))
            }
            ("linear_row_gelu", 3) => {
                let x = self.lift_node(&args[0], rec);
                let w = self.lift_node(&args[1], rec);
                let b = self.lift_node(&args[2], rec);
                Some(rec.add(AlmideExpr::MatrixLinearRowGelu([x, w, b])))
            }
            ("mul_scaled", 3) => {
                let a = self.lift_node(&args[0], rec);
                let s = self.lift_node(&args[1], rec);
                let b = self.lift_node(&args[2], rec);
                Some(rec.add(AlmideExpr::MatrixMulScaled([a, s, b])))
            }
            _ => None,
        }
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

// ── Lower: RecExpr → IrExpr ─────────────────────────────────────────

/// Errors that can arise during `lower`.
#[derive(Debug)]
pub enum LowerError {
    /// A node appeared in a position where the lower pass cannot
    /// reconstruct a well-typed IrExpr (e.g. `compose` outside a
    /// lambda slot, an unknown bare symbol).
    UnexpectedNode(String),
    /// A `_slot_N` symbol referenced a slot index that is out of
    /// range for the bridge's slot table.
    SlotOutOfRange(usize),
    /// A slot expected to hold a unary lambda did not.
    NotUnaryLambda,
    /// The element type of a combinator's list argument could not be
    /// resolved from the IR; lowering cannot synthesize a well-typed
    /// lambda param.
    MissingElementType,
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedNode(msg) => write!(f, "unexpected node: {msg}"),
            Self::SlotOutOfRange(idx) => write!(f, "slot index {idx} out of range"),
            Self::NotUnaryLambda => write!(f, "slot is not a unary lambda"),
            Self::MissingElementType => write!(f, "could not resolve list element type"),
        }
    }
}

impl std::error::Error for LowerError {}

include!("bridge_p2.rs");
include!("bridge_p3.rs");
