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

/// Produce a uniquely-named `Sym` for lambda parameters synthesised
/// during lower-time beta reduction. The codegen walker identifies
/// variables by name (`var_table.get(id).name`), so two distinct
/// `VarId`s with the same name collide into the same Rust binding —
/// the Stage-1 `snapshot_pipe_chain` regression was caused by
/// `compose_map_into_fold_fresh` allocating two params with the
/// previous fixed name `__egg_v`.
///
/// The suffix is `vt.len()`, which equals the `VarId` the imminent
/// `vt.alloc` will assign (`VarTable::alloc` sets `id = entries.len()`):
/// unique within the table AND a pure function of allocation order, so it
/// resets per compile and never depends on process history. A previous
/// process-global `AtomicU64` drifted across compiles in a long-lived
/// process (e.g. the in-browser playground compiling repeatedly without
/// reload), making the SAME input emit `__egg_v0` then `__egg_v1` — a
/// same-input-different-output determinism bug on the Rust target. Raw
/// atomics are also forbidden in the compile path by the Determinism Belt
/// (they don't exist on wasm32-unknown-unknown). See
/// docs/roadmap/active/determinism-belt.md.
fn fresh_sym(vt: &VarTable) -> Sym {
    sym(&format!("__egg_v{}", vt.len()))
}

fn build_identity_lambda(elem_ty: Ty, vt: &mut VarTable) -> IrExpr {
    let name = fresh_sym(vt);
    let var = vt.alloc(name, elem_ty.clone(), Mutability::Let, None);
    IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(var, elem_ty.clone())],
            body: Box::new(IrExpr {
                kind: IrExprKind::Var { id: var },
                ty: elem_ty.clone(),
                span: None, def_id: None,
            }),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![elem_ty.clone()],
            ret: Box::new(elem_ty),
        },
        span: None, def_id: None,
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

    let name = fresh_sym(vt);
    let fresh = vt.alloc(name, f_param_ty.clone(), Mutability::Let, None);
    let fresh_var = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: f_param_ty.clone(),
        span: None, def_id: None,
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
        span: None, def_id: None,
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

    let name = fresh_sym(vt);
    let fresh = vt.alloc(name, p_param_ty.clone(), Mutability::Let, None);
    let fresh_var = IrExpr {
        kind: IrExprKind::Var { id: fresh },
        ty: p_param_ty.clone(),
        span: None, def_id: None,
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
        span: None, def_id: None,
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
        span: None, def_id: None,
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

/// Compose map f into fold reducer g: λ(acc, x). g(acc, f(x)).
/// Reuses the original `g`'s `acc` param VarId and `f`'s param VarId
/// so that existing variable names (`acc`, `x`, …) round-trip
/// through codegen. Substitutes `g`'s elem param with `f`'s body
/// (re-written to use `f_param_id`). `f` is unary, `g` is binary.
fn compose_map_into_fold_fresh(
    f: &IrExpr,
    g: &IrExpr,
    _vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let (g_acc_id, g_acc_ty, g_elem_id, _g_elem_ty, g_body) = binary_lambda_parts(g)?;

    let composed_body = substitute_var_in_expr(g_body, g_elem_id, f_body);
    let ret_ty = composed_body.ty.clone();
    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![
                (g_acc_id, g_acc_ty.clone()),
                (f_param_id, f_param_ty.clone()),
            ],
            body: Box::new(composed_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![g_acc_ty, f_param_ty],
            ret: Box::new(ret_ty),
        },
        span: None, def_id: None,
    })
}

/// Compose two flat_map functions: λx. list.flat_map(f(x), g). `f`
/// is unary (x → List[U]), `g` is unary (U → List[V]). Reuses `f`'s
/// param VarId so the generated binding keeps its original name.
fn compose_flatmaps_fresh(
    f: &IrExpr,
    g: &IrExpr,
    _vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let g_ty = g.ty.clone();
    let g_ret = lambda_ret_ty(g).unwrap_or_else(|| g_ty.clone());

    let inner_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: sym("list"),
                func: sym("flat_map"),
                def_id: None,
            },
            args: vec![f_body.clone(), g.clone()],
            type_args: vec![],
        },
        ty: g_ret.clone(),
        span: None, def_id: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(f_param_id, f_param_ty.clone())],
            body: Box::new(inner_call),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![f_param_ty],
            ret: Box::new(g_ret),
        },
        span: None, def_id: None,
    })
}

/// Compose map f and filter p into a filter_map lambda:
///   λx. if p(f(x)) then some(f(x)) else none
/// Reuses `f`'s param VarId as the outer lambda param.
fn compose_map_filter_fresh(
    f: &IrExpr,
    p: &IrExpr,
    _vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (f_param_id, f_param_ty, f_body) = unary_lambda_parts(f)?;
    let (p_param_id, _p_param_ty, p_body) = unary_lambda_parts(p)?;

    let p_applied = substitute_var_in_expr(p_body, p_param_id, f_body);
    let result_ty = f_body.ty.clone();
    let composed_body = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(p_applied),
            then: Box::new(IrExpr {
                kind: IrExprKind::OptionSome { expr: Box::new(f_body.clone()) },
                ty: Ty::option(result_ty.clone()),
                span: None, def_id: None,
            }),
            else_: Box::new(IrExpr {
                kind: IrExprKind::OptionNone,
                ty: Ty::option(result_ty.clone()),
                span: None, def_id: None,
            }),
        },
        ty: Ty::option(result_ty.clone()),
        span: None, def_id: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![(f_param_id, f_param_ty.clone())],
            body: Box::new(composed_body),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![f_param_ty],
            ret: Box::new(Ty::option(result_ty)),
        },
        span: None, def_id: None,
    })
}

/// Compose filter_map lambda into fold reducer: produce
///   λ(acc, x). match fm(x) { some(y) ⇒ g(acc, y), none ⇒ acc }
/// `fm` is unary (x → Option[U]), `g` is binary (acc, U → acc').
/// Reuses `g.acc`, `fm.param`, and `g.elem` VarIds.
fn compose_filter_map_into_fold_fresh(
    fm: &IrExpr,
    g: &IrExpr,
    _vt: &mut VarTable,
) -> Result<IrExpr, LowerError> {
    let (fm_param_id, fm_param_ty, fm_body) = unary_lambda_parts(fm)?;
    let (g_acc_id, g_acc_ty, g_elem_id, g_elem_ty, g_body) = binary_lambda_parts(g)?;

    let acc_ref = IrExpr {
        kind: IrExprKind::Var { id: g_acc_id },
        ty: g_acc_ty.clone(),
        span: None, def_id: None,
    };

    use almide_ir::{IrMatchArm, IrPattern};
    let some_arm = IrMatchArm {
        pattern: IrPattern::Some {
            inner: Box::new(IrPattern::Bind { var: g_elem_id, ty: g_elem_ty.clone() }),
        },
        guard: None,
        body: g_body.clone(),
    };
    let none_arm = IrMatchArm {
        pattern: IrPattern::None,
        guard: None,
        body: acc_ref,
    };
    let match_expr = IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(fm_body.clone()),
            arms: vec![some_arm, none_arm],
        },
        ty: g_acc_ty.clone(),
        span: None, def_id: None,
    };

    Ok(IrExpr {
        kind: IrExprKind::Lambda {
            params: vec![
                (g_acc_id, g_acc_ty.clone()),
                (fm_param_id, fm_param_ty.clone()),
            ],
            body: Box::new(match_expr),
            lambda_id: None,
        },
        ty: Ty::Fn {
            params: vec![g_acc_ty.clone(), fm_param_ty],
            ret: Box::new(g_acc_ty),
        },
        span: None, def_id: None,
    })
}
