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

impl Bridge {
    /// Lower an `egg::RecExpr` back into `IrExpr`, allocating fresh
    /// VarIds through `vt` for any `compose` / `and-pred` markers that
    /// need to become real `IrExprKind::Lambda` nodes.
    ///
    /// The root of the `RecExpr` is taken to be its last node, which
    /// matches egg's post-order numbering after extraction.
    pub fn lower(
        &self,
        rec: &RecExpr<AlmideExpr>,
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        let nodes = rec.as_ref();
        assert!(!nodes.is_empty(), "cannot lower an empty RecExpr");
        let root = Id::from(nodes.len() - 1);
        self.lower_expr(rec, root, vt)
    }

    fn lower_expr(
        &self,
        rec: &RecExpr<AlmideExpr>,
        id: Id,
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        match &rec[id] {
            AlmideExpr::Symbol(s) => self.resolve_symbol_expr(s.as_str()),
            AlmideExpr::Num(n) => Ok(IrExpr {
                kind: IrExprKind::LitInt { value: *n },
                ty: Ty::Int,
                span: None,
            }),
            AlmideExpr::Map([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let ret_elem = lambda_ret_ty(&f).unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("map"),
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_elem),
                    span: None,
                })
            }
            AlmideExpr::Filter([xs_id, p_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let p = self.lower_lambda_arg(rec, *p_id, &[elem_ty.clone()], vt)?;
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("filter"),
                        },
                        args: vec![xs, p],
                        type_args: vec![],
                    },
                    ty: Ty::list(elem_ty),
                    span: None,
                })
            }
            AlmideExpr::Fold([xs_id, init_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let init = self.lower_expr(rec, *init_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let acc_ty = init.ty.clone();
                let f =
                    self.lower_lambda_arg(rec, *f_id, &[acc_ty.clone(), elem_ty], vt)?;
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("fold"),
                        },
                        args: vec![xs, init, f],
                        type_args: vec![],
                    },
                    ty: acc_ty,
                    span: None,
                })
            }
            AlmideExpr::Lam(_)
            | AlmideExpr::Compose(_)
            | AlmideExpr::AndPred(_) => Err(LowerError::UnexpectedNode(
                "lambda-position marker in expression position".into(),
            )),

            AlmideExpr::MatrixMul([a, b]) =>
                self.lower_matrix_call(rec, "mul", &[*a, *b], vt),
            AlmideExpr::MatrixAdd([a, b]) =>
                self.lower_matrix_call(rec, "add", &[*a, *b], vt),
            AlmideExpr::MatrixScale([m, s]) =>
                self.lower_matrix_call(rec, "scale", &[*m, *s], vt),
            AlmideExpr::MatrixGelu([m]) =>
                self.lower_matrix_call(rec, "gelu", &[*m], vt),
            AlmideExpr::MatrixSoftmaxRows([m]) =>
                self.lower_matrix_call(rec, "softmax_rows", &[*m], vt),
            AlmideExpr::MatrixLinearRow([x, w, b]) =>
                self.lower_matrix_call(rec, "linear_row", &[*x, *w, *b], vt),
            AlmideExpr::MatrixLayerNormRows([x, g, be, e]) =>
                self.lower_matrix_call(rec, "layer_norm_rows", &[*x, *g, *be, *e], vt),

            AlmideExpr::MatrixFusedGemmBiasScaleGelu([a, b, bi, al]) =>
                self.lower_matrix_call(rec, "fused_gemm_bias_scale_gelu", &[*a, *b, *bi, *al], vt),
            AlmideExpr::MatrixAttentionWeights([q, kt, s]) =>
                self.lower_matrix_call(rec, "attention_weights", &[*q, *kt, *s], vt),
            AlmideExpr::MatrixScaledDotProductAttention([q, kt, v, s]) =>
                self.lower_matrix_call(rec, "scaled_dot_product_attention", &[*q, *kt, *v, *s], vt),
            AlmideExpr::MatrixPreNormLinear([x, g, be, e, w, b]) =>
                self.lower_matrix_call(rec, "pre_norm_linear", &[*x, *g, *be, *e, *w, *b], vt),
            AlmideExpr::MatrixLinearRowGelu([x, w, b]) =>
                self.lower_matrix_call(rec, "linear_row_gelu", &[*x, *w, *b], vt),
            AlmideExpr::MatrixMulScaled([a, s, b]) =>
                self.lower_matrix_call(rec, "mul_scaled", &[*a, *s, *b], vt),

            AlmideExpr::FlatMap([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                // flat_map's lambda returns List[U]; overall result is
                // List[U]. Try to recover U from the lambda's return
                // type; if that's unresolved, fall back to List[Int]
                // as a placeholder (saturation doesn't propagate types
                // through slot symbols).
                let ret_inner = lambda_ret_ty(&f)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("flat_map"),
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_inner),
                    span: None,
                })
            }
            AlmideExpr::FilterMap([xs_id, f_id]) => {
                let xs = self.lower_expr(rec, *xs_id, vt)?;
                let elem_ty = list_elem_ty(&xs.ty).ok_or(LowerError::MissingElementType)?;
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                // filter_map's lambda returns Option[U]; overall
                // result is List[U]. Same fallback as flat_map.
                let ret_inner = lambda_ret_ty(&f)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or(elem_ty);
                Ok(IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: sym("list"),
                            func: sym("filter_map"),
                        },
                        args: vec![xs, f],
                        type_args: vec![],
                    },
                    ty: Ty::list(ret_inner),
                    span: None,
                })
            }
            AlmideExpr::ComposeFold(_) | AlmideExpr::ComposeFlatmap(_)
            | AlmideExpr::ComposeMapFilter(_) | AlmideExpr::ComposeFmFold(_) => Err(LowerError::UnexpectedNode(
                "list-fusion marker in expression position".into(),
            )),
        }
    }

    /// Emit an `IrExprKind::Call` with `CallTarget::Module { matrix,
    /// <func> }`. Result type inherits from the first matrix-typed
    /// argument, which matches stdlib `matrix.<op>` signatures:
    /// every op takes a Matrix as arg[0] and returns a Matrix that
    /// shares its dtype. Children are lowered recursively so that
    /// fused RHS nodes reach back to their unfused leaves.
    fn lower_matrix_call(
        &self,
        rec: &RecExpr<AlmideExpr>,
        func: &str,
        children: &[Id],
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        let lowered: Result<Vec<IrExpr>, _> = children
            .iter()
            .map(|id| self.lower_expr(rec, *id, vt))
            .collect();
        let lowered = lowered?;
        let ty = lowered
            .iter()
            .map(|e| &e.ty)
            .find(|t| is_matrix_ty(t))
            .cloned()
            .unwrap_or(Ty::Matrix);
        Ok(IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("matrix"),
                    func: sym(func),
                },
                args: lowered,
                type_args: vec![],
            },
            ty,
            span: None,
        })
    }

    /// Lower a node that sits in lambda position — i.e. the second
    /// arg of `map`/`filter` or the third of `fold`. Accepts
    /// `(lam _slot)`, bare `identity`, `compose`, and `and-pred`.
    ///
    /// `param_tys` describes the types the lambda expects. For map /
    /// filter this is a single element type; for fold it is
    /// `[acc_ty, elem_ty]`. Compose / and-pred are only legal in the
    /// unary case (map / filter), not fold.
    fn lower_lambda_arg(
        &self,
        rec: &RecExpr<AlmideExpr>,
        id: Id,
        param_tys: &[Ty],
        vt: &mut VarTable,
    ) -> Result<IrExpr, LowerError> {
        match &rec[id] {
            AlmideExpr::Lam([slot_id]) => {
                let AlmideExpr::Symbol(s) = &rec[*slot_id] else {
                    return Err(LowerError::UnexpectedNode(
                        "(lam ...) child must be a slot symbol".into(),
                    ));
                };
                let slot_idx = parse_slot_index(s.as_str())?;
                let lam = self
                    .slots
                    .get(slot_idx)
                    .cloned()
                    .ok_or(LowerError::SlotOutOfRange(slot_idx))?;
                if !matches!(&lam.kind, IrExprKind::Lambda { .. }) {
                    return Err(LowerError::NotUnaryLambda);
                }
                Ok(lam)
            }
            AlmideExpr::Symbol(s) if s.as_str() == "identity" => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "identity marker requires exactly one param type".into(),
                    ));
                };
                Ok(build_identity_lambda(elem_ty.clone(), vt))
            }
            AlmideExpr::Symbol(s) => {
                // Bare slot symbol lifted as a lambda without the
                // `(lam ...)` wrapper — happens when the original IR
                // put a non-literal lambda-shaped slot straight into
                // the combinator. Fall back to slot lookup.
                let slot_idx = parse_slot_index(s.as_str())?;
                self.slots
                    .get(slot_idx)
                    .cloned()
                    .ok_or(LowerError::SlotOutOfRange(slot_idx))
            }
            AlmideExpr::Compose([g_id, f_id]) => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose marker only valid for unary lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[f_ret], vt)?;
                compose_lambdas_fresh(&f, &g, vt)
            }
            AlmideExpr::AndPred([p_id, q_id]) => {
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "and-pred marker only valid for unary lambda position".into(),
                    ));
                };
                let p = self.lower_lambda_arg(rec, *p_id, &[elem_ty.clone()], vt)?;
                let q = self.lower_lambda_arg(rec, *q_id, &[elem_ty.clone()], vt)?;
                compose_predicates_fresh(&p, &q, vt)
            }
            AlmideExpr::Map(_) | AlmideExpr::Filter(_) | AlmideExpr::Fold(_)
            | AlmideExpr::FlatMap(_) | AlmideExpr::FilterMap(_)
            | AlmideExpr::Num(_) => {
                Err(LowerError::UnexpectedNode(
                    "non-lambda node in lambda position".into(),
                ))
            }
            AlmideExpr::ComposeFold([g_id, f_id]) => {
                // fold position: param_tys = [acc_ty, elem_ty].
                // Build λ(acc, x). g(acc, f(x)).
                let [acc_ty, elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-fold marker only valid in fold's reducer position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[acc_ty.clone(), f_ret], vt)?;
                compose_map_into_fold_fresh(&f, &g, vt)
            }
            AlmideExpr::ComposeFlatmap([g_id, f_id]) => {
                // flat_map position: param_tys = [elem_ty]. Build
                // λx. list.flat_map(f(x), g).
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-flatmap marker only valid in flat_map's lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| Ty::list(elem_ty.clone()));
                let g_elem = f_ret.inner().cloned().unwrap_or_else(|| elem_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[g_elem], vt)?;
                compose_flatmaps_fresh(&f, &g, vt)
            }
            AlmideExpr::ComposeMapFilter([p_id, f_id]) => {
                // filter_map position: param_tys = [elem_ty]. Build
                // λx. if p(f(x)) then some(f(x)) else none.
                let [elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-map-filter marker only valid in filter_map's lambda position".into(),
                    ));
                };
                let f = self.lower_lambda_arg(rec, *f_id, &[elem_ty.clone()], vt)?;
                let f_ret = lambda_ret_ty(&f).unwrap_or_else(|| elem_ty.clone());
                let p = self.lower_lambda_arg(rec, *p_id, &[f_ret], vt)?;
                compose_map_filter_fresh(&f, &p, vt)
            }
            AlmideExpr::ComposeFmFold([g_id, fm_id]) => {
                // fold position: param_tys = [acc_ty, elem_ty]. Build
                // λ(acc, x). match fm(x) { some(y) ⇒ g(acc, y), none ⇒ acc }.
                let [acc_ty, elem_ty] = param_tys else {
                    return Err(LowerError::UnexpectedNode(
                        "compose-fm-fold marker only valid in fold's reducer position".into(),
                    ));
                };
                let fm = self.lower_lambda_arg(rec, *fm_id, &[elem_ty.clone()], vt)?;
                let fm_inner = lambda_ret_ty(&fm)
                    .and_then(|t| t.inner().cloned())
                    .unwrap_or_else(|| acc_ty.clone());
                let g = self.lower_lambda_arg(rec, *g_id, &[acc_ty.clone(), fm_inner], vt)?;
                compose_filter_map_into_fold_fresh(&fm, &g, vt)
            }
            AlmideExpr::MatrixMul(_) | AlmideExpr::MatrixAdd(_)
            | AlmideExpr::MatrixScale(_) | AlmideExpr::MatrixGelu(_)
            | AlmideExpr::MatrixSoftmaxRows(_) | AlmideExpr::MatrixLinearRow(_)
            | AlmideExpr::MatrixLayerNormRows(_)
            | AlmideExpr::MatrixFusedGemmBiasScaleGelu(_)
            | AlmideExpr::MatrixAttentionWeights(_)
            | AlmideExpr::MatrixScaledDotProductAttention(_)
            | AlmideExpr::MatrixPreNormLinear(_)
            | AlmideExpr::MatrixLinearRowGelu(_)
            | AlmideExpr::MatrixMulScaled(_) => Err(LowerError::UnexpectedNode(
                "matrix node in lambda position".into(),
            )),
        }
    }

    fn resolve_symbol_expr(&self, name: &str) -> Result<IrExpr, LowerError> {
        let slot_idx = parse_slot_index(name)?;
        self.slots
            .get(slot_idx)
            .cloned()
            .ok_or(LowerError::SlotOutOfRange(slot_idx))
    }
}

fn parse_slot_index(name: &str) -> Result<usize, LowerError> {
    name.strip_prefix("_slot_")
        .and_then(|rest| rest.parse::<usize>().ok())
        .ok_or_else(|| LowerError::UnexpectedNode(format!("unknown bare symbol `{name}`")))
}

fn list_elem_ty(ty: &Ty) -> Option<Ty> {
    ty.inner().cloned()
}

/// Whether `ty` is a Matrix type — either the bare `Ty::Matrix` alias
/// or a parametric `Matrix[T]` (post-dtype arc). Used when inheriting
/// the result type of a lowered matrix call from its arguments.
fn is_matrix_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(
        ty,
        Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)
    )
}

// ── Let-split chain inlining ────────────────────────────────────────
//
// `MatrixFusionPass` recognises both nested-call shapes
//   matrix.gelu(matrix.scale(matrix.add(matrix.mul(a, b), bias), alpha))
// and let-split shapes
//   let mul_ab    = matrix.mul(a, b)
//   let added     = matrix.add(mul_ab, bias)
//   let scaled    = matrix.scale(added, alpha)
//   matrix.gelu(scaled)
//
// The egg bridge sees only `IrExprKind::Call`, so let-split chains
// would lift as a single opaque `Block` slot and never enter
// saturation. To cover the same pattern surface, we pre-process: when
// every binding in a Block is a matrix-typed value used exactly once
// in the trailing expression, inline each `let x = v` into the
// trailing expression and lift the rewritten tree.
//
// The transform is conservative: any Bind whose variable is used 0
// or >1 times bails out and the original Block is lifted opaquely.
// The reasoning is that an inline that shares state across uses
// would change semantics; an inline that drops a binding would lose
// referential transparency for any side effect (the matrix ops
// considered here are pure, but we keep the rule simple). The
// imperative `MatrixFusionPass` is still run after egg, so anything
// not pulled into the inlined tree falls back to its existing
// matcher.

/// Try to fold a `Block { stmts; trailing }` whose tail is a matrix
/// call and whose stmts are `Bind { var, value: matrix_op }` with
/// each `var` used exactly once downstream. Returns the inlined
/// trailing expression on success.
pub(crate) fn inline_let_split_matrix_chain(expr: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(trailing) } = &expr.kind else {
        return None;
    };
    if stmts.is_empty() {
        return None;
    }
    if !is_matrix_callish(trailing) {
        return None;
    }

    // Walk stmts in source order, accumulating an inlined trailing
    // expression. Each Bind's value is itself recursively inlined so
    // multi-step chains compose. If any stmt isn't an inline-eligible
    // Bind, give up.
    let mut current: IrExpr = (**trailing).clone();
    let bind_vars: Vec<VarId> = collect_bind_vars(stmts)?;

    for stmt in stmts.iter().rev() {
        let IrStmtKind::Bind { var, value, .. } = &stmt.kind else {
            return None;
        };
        if !is_matrix_callish(value) {
            return None;
        }
        if !is_used_exactly_once(&current, *var) {
            return None;
        }
        // Substitute, then continue inlining inner Binds. Recursive
        // call handles `value` itself being a Block (rare but
        // possible after lowering of `do { ... }`-style sugar).
        let value_inlined = inline_let_split_matrix_chain(value)
            .unwrap_or_else(|| value.clone());
        current = substitute_var_in_expr(&current, *var, &value_inlined);
    }

    // Sanity: every bound var must now be gone — no later stmt may
    // have referenced it without participating in the chain.
    for v in bind_vars {
        if expr_references_var(&current, v) {
            return None;
        }
    }
    Some(current)
}

/// Collect all VarIds bound by a sequence of stmts. Bails on
/// non-Bind stmts so callers can rely on the chain being made of
/// pure let bindings.
fn collect_bind_vars(stmts: &[almide_ir::IrStmt]) -> Option<Vec<VarId>> {
    stmts.iter().map(|s| match &s.kind {
        IrStmtKind::Bind { var, .. } => Some(*var),
        _ => None,
    }).collect()
}

/// Whether `expr` is a `matrix.<op>(...)` Call. Restricting to this
/// shape keeps the inline transform from disturbing non-matrix
/// blocks (where it could subtly change ordering of side effects).
fn is_matrix_callish(expr: &IrExpr) -> bool {
    matches!(
        &expr.kind,
        IrExprKind::Call { target: CallTarget::Module { module, .. }, .. }
            if module.as_str() == "matrix"
    )
}

fn is_used_exactly_once(expr: &IrExpr, target: VarId) -> bool {
    count_var_refs(expr, target) == 1
}

fn expr_references_var(expr: &IrExpr, target: VarId) -> bool {
    count_var_refs(expr, target) > 0
}

/// Count references to `target` inside `expr`. Conservative: counts
/// each appearance whether or not it's in a tail position. Skips
/// nothing — every IrExprKind variant recurses via `walk` when it
/// has children.
fn count_var_refs(expr: &IrExpr, target: VarId) -> usize {
    use almide_ir::{walk_expr, IrVisitor, IrStmt};
    struct Counter { target: VarId, count: usize }
    impl IrVisitor for Counter {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.target { self.count += 1; }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            almide_ir::walk_stmt(self, s);
        }
    }
    let mut c = Counter { target, count: 0 };
    c.visit_expr(expr);
    c.count
}

fn lambda_ret_ty(expr: &IrExpr) -> Option<Ty> {
    match &expr.ty {
        Ty::Fn { ret, .. } => Some((**ret).clone()),
        _ => None,
    }
}

fn fresh_sym() -> Sym {
    sym("__egg_v")
}

fn build_identity_lambda(elem_ty: Ty, vt: &mut VarTable) -> IrExpr {
    let var = vt.alloc(fresh_sym(), elem_ty.clone(), Mutability::Let, None);
    IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(var, elem_ty.clone())],
            body: Box::new(IrExpr {
                kind: IrExprKind::Var { id: var },
                ty: elem_ty.clone(),
                span: None,
            }),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![elem_ty.clone()],
            ret: Box::new(elem_ty),
        },
        span: None,
    }
}

/// Beta-reduce `compose g f` into `λv. g(f(v))` with a fresh VarId.
/// Both f and g are expected to be unary `IrExprKind::Lambda`.
fn compose_lambdas_fresh(
    f: &IrExpr,
    g: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let (g_param_id, _g_param_ty, g_body) = unary_lambda_parts(g)?;

    let fresh = vt.alloc(fresh_sym(), f_param_ty.clone(), Mutability::Let, None);
    let fresh_var = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: f_param_ty.clone(),
        span: None,
    };
    // First rename f's own param to fresh so f_body references fresh,
    // then substitute g's param with the renamed f_body.
    let f_body_fresh = substitute_var_in_expr(f_body, f_param_id, &fresh_var);
    let composed_body = substitute_var_in_expr(g_body, g_param_id, &f_body_fresh);

    let ret_ty = lambda_ret_ty(g).unwrap_or_else(|| composed_body.ty.clone());

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(fresh, f_param_ty.clone())],
            body: Box::new(composed_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![f_param_ty],
            ret: Box::new(ret_ty),
        },
        span: None,
    })
}

/// Beta-reduce `and-pred p q` into `λv. p(v) && q(v)` with a fresh
/// VarId. Both p and q are expected to be unary predicates — i.e.
/// `IrExprKind::Lambda` whose body has type `Bool`.
fn compose_predicates_fresh(
    p: &IrExpr,
    q: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (p_param_id, p_param_ty, p_body) = unary_lambda_parts(p)?;
    let (q_param_id, _q_param_ty, q_body) = unary_lambda_parts(q)?;

    let fresh = vt.alloc(fresh_sym(), p_param_ty.clone(), Mutability::Let, None);
    let fresh_var = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: p_param_ty.clone(),
        span: None,
    };
    let p_body_fresh = substitute_var_in_expr(p_body, p_param_id, &fresh_var);
    let q_body_fresh = substitute_var_in_expr(q_body, q_param_id, &fresh_var);

    let and_body = IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::And,
            left: Box::new(p_body_fresh),
            right: Box::new(q_body_fresh),
        },
        ty: Ty::Bool,
        span: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(fresh, p_param_ty.clone())],
            body: Box::new(and_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![p_param_ty],
            ret: Box::new(Ty::Bool),
        },
        span: None,
    })
}

fn unary_lambda_parts(expr: &IrExpr) -> Result<(VarId, Ty, &IrExpr), LowerError> {
    let IrExprKind::Lambda { params, body, .. } = &expr.kind else {
        return Err(LowerError::NotUnaryLambda);
    };
    let [(id, ty)] = params.as_slice() else {
        return Err(LowerError::NotUnaryLambda);
    };
    Ok((*id, ty.clone(), body.as_ref()))
}

/// Like `unary_lambda_parts` but expects two parameters (for fold /
/// reducers). Returns (acc_id, acc_ty, elem_id, elem_ty, body).
fn binary_lambda_parts(
    expr: &IrExpr,
) -> Result<(VarId, Ty, VarId, Ty, &IrExpr), LowerError> {
    let IrExprKind::Lambda { params, body, .. } = &expr.kind else {
        return Err(LowerError::NotUnaryLambda);
    };
    let [(a_id, a_ty), (b_id, b_ty)] = params.as_slice() else {
        return Err(LowerError::NotUnaryLambda);
    };
    Ok((*a_id, a_ty.clone(), *b_id, b_ty.clone(), body.as_ref()))
}

/// Compose map f into fold reducer g: λ(acc, x). g(acc, f(x)) with
/// fresh VarIds. `f` is a unary lambda λy. f_body(y), `g` is a
/// binary lambda λ(acc, elem). g_body(acc, elem).
fn compose_map_into_fold_fresh(
    f: &IrExpr,
    g: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let (g_acc_id, g_acc_ty, g_elem_id, _g_elem_ty, g_body) = binary_lambda_parts(g)?;

    let fresh_acc = vt.alloc(fresh_sym(), g_acc_ty.clone(), Mutability::Let, None);
    let fresh_elem = vt.alloc(fresh_sym(), f_param_ty.clone(), Mutability::Let, None);
    let fresh_elem_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh_elem },
        ty: f_param_ty.clone(),
        span: None,
    };
    let fresh_acc_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh_acc },
        ty: g_acc_ty.clone(),
        span: None,
    };
    // Substitute f's param to fresh_elem in f_body.
    let f_body_fresh = substitute_var_in_expr(f_body, f_param_id, &fresh_elem_ref);
    // Substitute g's elem param with f_body_fresh, and g's acc param
    // with fresh_acc_ref.
    let g_body_fresh = substitute_var_in_expr(g_body, g_acc_id, &fresh_acc_ref);
    let composed_body = substitute_var_in_expr(&g_body_fresh, g_elem_id, &f_body_fresh);

    let ret_ty = composed_body.ty.clone();
    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![
                (fresh_acc, g_acc_ty.clone()),
                (fresh_elem, f_param_ty.clone()),
            ],
            body: Box::new(composed_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![g_acc_ty, f_param_ty],
            ret: Box::new(ret_ty),
        },
        span: None,
    })
}

/// Compose two flat_map functions: λx. list.flat_map(f(x), g). `f`
/// is unary (x → List[U]), `g` is unary (U → List[V]); the composed
/// lambda returns List[V].
fn compose_flatmaps_fresh(
    f: &IrExpr,
    g: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let g_ty = g.ty.clone();
    let g_ret = lambda_ret_ty(g).unwrap_or_else(|| g_ty.clone());

    let fresh = vt.alloc(fresh_sym(), f_param_ty.clone(), Mutability::Let, None);
    let fresh_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: f_param_ty.clone(),
        span: None,
    };
    let f_body_fresh = substitute_var_in_expr(f_body, f_param_id, &fresh_ref);

    let inner_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: sym("list"),
                func: sym("flat_map"),
            },
            args: vec![f_body_fresh, g.clone()],
            type_args: vec![],
        },
        ty: g_ret.clone(),
        span: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(fresh, f_param_ty.clone())],
            body: Box::new(inner_call),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![f_param_ty],
            ret: Box::new(g_ret),
        },
        span: None,
    })
}

/// Compose map f and filter p into a filter_map lambda:
///   λx. if p(f(x)) then some(f(x)) else none
fn compose_map_filter_fresh(
    f: &IrExpr,
    p: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let (p_param_id, _p_param_ty, p_body) = unary_lambda_parts(p)?;

    let fresh = vt.alloc(fresh_sym(), f_param_ty.clone(), Mutability::Let, None);
    let fresh_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: f_param_ty.clone(),
        span: None,
    };
    let f_body_fresh = substitute_var_in_expr(f_body, f_param_id, &fresh_ref);
    let p_applied = substitute_var_in_expr(p_body, p_param_id, &f_body_fresh);

    let result_ty = f_body_fresh.ty.clone();
    let composed_body = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(p_applied),
            then: Box::new(IrExpr {
                kind: IrExprKind::OptionSome { expr: Box::new(f_body_fresh.clone()) },
                ty: Ty::option(result_ty.clone()),
                span: None,
            }),
            else_: Box::new(IrExpr {
                kind: IrExprKind::OptionNone,
                ty: Ty::option(result_ty.clone()),
                span: None,
            }),
        },
        ty: Ty::option(result_ty.clone()),
        span: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(fresh, f_param_ty.clone())],
            body: Box::new(composed_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![f_param_ty],
            ret: Box::new(Ty::option(result_ty)),
        },
        span: None,
    })
}

/// Compose filter_map lambda into fold reducer: produce
///   λ(acc, x). match fm(x) { some(y) ⇒ g(acc, y), none ⇒ acc }
/// `fm` is unary (x → Option[U]), `g` is binary (acc, U → acc').
fn compose_filter_map_into_fold_fresh(
    fm: &IrExpr,
    g: &IrExpr,
    vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (fm_param_id, fm_param_ty, fm_body) = unary_lambda_parts(fm)?;
    let (g_acc_id, g_acc_ty, g_elem_id, g_elem_ty, g_body) = binary_lambda_parts(g)?;

    // Fresh VarIds for the composed reducer params + the pattern
    // bind in the `some` arm.
    let fresh_acc = vt.alloc(fresh_sym(), g_acc_ty.clone(), Mutability::Let, None);
    let fresh_elem = vt.alloc(fresh_sym(), fm_param_ty.clone(), Mutability::Let, None);
    let fresh_y = vt.alloc(fresh_sym(), g_elem_ty.clone(), Mutability::Let, None);

    let fresh_acc_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh_acc },
        ty: g_acc_ty.clone(),
        span: None,
    };
    let fresh_elem_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh_elem },
        ty: fm_param_ty.clone(),
        span: None,
    };
    let fresh_y_ref = IrExpr {
        kind: IrExprKind::Var { id: fresh_y },
        ty: g_elem_ty.clone(),
        span: None,
    };

    // fm(fresh_elem) — subject of the match.
    let fm_body_fresh = substitute_var_in_expr(fm_body, fm_param_id, &fresh_elem_ref);

    // some(y) arm body = g(fresh_acc, fresh_y)
    let g_with_acc = substitute_var_in_expr(g_body, g_acc_id, &fresh_acc_ref);
    let some_arm_body = substitute_var_in_expr(&g_with_acc, g_elem_id, &fresh_y_ref);

    use almide_ir::{IrMatchArm, IrPattern};
    let some_arm = IrMatchArm {
        pattern: IrPattern::Some {
            inner: Box::new(IrPattern::Bind { var: fresh_y, ty: g_elem_ty.clone() }),
        },
        guard: None,
        body: some_arm_body,
    };
    let none_arm = IrMatchArm {
        pattern: IrPattern::None,
        guard: None,
        body: fresh_acc_ref.clone(),
    };
    let match_expr = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(fm_body_fresh),
            arms: vec![some_arm, none_arm],
        },
        ty: g_acc_ty.clone(),
        span: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![
                (fresh_acc, g_acc_ty.clone()),
                (fresh_elem, fm_param_ty.clone()),
            ],
            body: Box::new(match_expr),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![g_acc_ty.clone(), fm_param_ty],
            ret: Box::new(g_acc_ty),
        },
        span: None,
    })
}
