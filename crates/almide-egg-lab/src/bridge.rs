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
    substitute_var_in_expr, BinOp, CallTarget, IrExpr, IrExprKind, Mutability, VarId, VarTable,
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
        }
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
            AlmideExpr::Map(_) | AlmideExpr::Filter(_) | AlmideExpr::Fold(_) | AlmideExpr::Num(_) => {
                Err(LowerError::UnexpectedNode(
                    "non-lambda node in lambda position".into(),
                ))
            }
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
