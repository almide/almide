// #806 step 2 — the MIR-level SMALL-FUNCTION INLINER, taken at the IR layer.
//
// The hot-loop cost this removes: `sum = sum + eval_a(i, j) * v[j]` emits a
// `call $eval_a` per iteration (~80M calls on spectralnorm) that wasmtime never
// inlines across, while native's LLVM reduces the same callee to a few
// instructions. Instead of splicing MIR ops and COMPOSING certificates (the
// engineering mountain the issue names), a PURE-SCALAR callee is reduced to a
// SINGLE EXPRESSION — its `let`s substituted away, its params replaced by the
// call's argument expressions — and substituted at the call site BEFORE
// lowering. Scalars carry NO ownership events, so the inlined tree lowers to
// exactly the certificate it would need: the composition problem dissolves.
// Applied desugar-before-both (pipeline + classify), so the caps counter and
// the lowering see one tree and `mir == ir` accounting is untouched.
//
// ADMISSION (all conservative, decline = keep the call):
//   - the callee is a MAIN-PROGRAM fn (module fns keep their flatten-name
//     calls), non-effect, non-test, non-generic, not `main`;
//   - every param and the return are SCALAR (Int/Float/Bool);
//   - the body has NO free vars beyond its params (no globals/cells);
//   - the body REDUCES to a pure scalar expression: Block-of-`let`s + tail,
//     If, BinOp/UnOp, scalar literals, Vars, and PURE `module.func` calls —
//     no user Named calls (no cascade, no recursion), no Lambda/Match/loops,
//     no heap-typed node anywhere;
//   - the reduced expression stays under a node cap (duplication from a
//     multiply-used `let`/param is bounded);
//   - every ARGUMENT at the call site is CALL-FREE (a duplicated arg is
//     re-evaluated — safe only when it cannot carry an effect or a count).

fn is_scalar_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool)
}

/// Reduce `e` (a candidate callee body node) to a pure expression with `env`
/// substituted for Vars. Returns `None` when the node is outside the pure-scalar
/// subset; `budget` decrements per produced node (duplication-bounded).
fn reduce_expr(
    e: &IrExpr,
    env: &HashMap<VarId, IrExpr>,
    budget: &mut i64,
) -> Option<IrExpr> {
    use almide_ir::IrStmtKind;
    *budget -= 1;
    if *budget < 0 {
        return None;
    }
    if !is_scalar_ty(&e.ty) && !matches!(e.kind, IrExprKind::Block { .. }) {
        return None;
    }
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => {
            Some(e.clone())
        }
        IrExprKind::Var { id } => env.get(id).cloned(),
        IrExprKind::BinOp { op, left, right } => {
            let l = reduce_expr(left, env, budget)?;
            let r = reduce_expr(right, env, budget)?;
            Some(IrExpr {
                kind: IrExprKind::BinOp { op: *op, left: Box::new(l), right: Box::new(r) },
                ..e.clone()
            })
        }
        IrExprKind::UnOp { op, operand } => {
            let o = reduce_expr(operand, env, budget)?;
            Some(IrExpr { kind: IrExprKind::UnOp { op: *op, operand: Box::new(o) }, ..e.clone() })
        }
        IrExprKind::If { cond, then, else_ } => {
            let c = reduce_expr(cond, env, budget)?;
            let t = reduce_expr(then, env, budget)?;
            let el = reduce_expr(else_, env, budget)?;
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(c),
                    then: Box::new(t),
                    else_: Box::new(el),
                },
                ..e.clone()
            })
        }
        // A PURE stdlib module call (`float.from_int(x)`) stays a call — its
        // arguments substitute. The counting sees the same node on both sides.
        IrExprKind::Call { target: CallTarget::Module { module, func, def_id }, args, type_args } => {
            if !crate::purity::is_pure(module.as_str(), func.as_str()) {
                return None;
            }
            let mut new_args = Vec::with_capacity(args.len());
            for a in args {
                new_args.push(reduce_expr(a, env, budget)?);
            }
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: *module,
                        func: *func,
                        def_id: *def_id,
                    },
                    args: new_args,
                    type_args: type_args.clone(),
                },
                ..e.clone()
            })
        }
        // A Block of scalar `let`s: substitute each init into the env, reduce
        // the tail — the `let`s dissolve into the expression.
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let mut env = env.clone();
            for s in stmts {
                let IrStmtKind::Bind { var, ty, value, .. } = &s.kind else { return None };
                if !is_scalar_ty(ty) {
                    return None;
                }
                let init = reduce_expr(value, &env, budget)?;
                env.insert(*var, init);
            }
            reduce_expr(tail, &env, budget)
        }
        _ => None,
    }
}

/// Is `f` an inlinable pure-scalar callee (see the module header)? On success,
/// returns nothing — the reduction happens per call site (each site's args form
/// the env), but the STRUCTURAL admission is site-independent and cached.
fn is_inlinable_shape(f: &almide_ir::IrFunction) -> bool {
    if f.is_effect
        || f.is_test
        || f.generics.as_ref().is_some_and(|g| !g.is_empty())
        || f.name.as_str() == "main"
    {
        return false;
    }
    if !is_scalar_ty(&f.ret_ty) || !f.params.iter().all(|p| is_scalar_ty(&p.ty)) {
        return false;
    }
    let bound: std::collections::HashSet<VarId> = f.params.iter().map(|p| p.var).collect();
    if !almide_ir::free_vars::free_vars(&f.body, &bound).is_empty() {
        return false;
    }
    // A dry-run reduction with the params bound to themselves proves the body
    // is in the reducible subset (and inside the node cap) BEFORE any call
    // site commits.
    let env: HashMap<VarId, IrExpr> = f
        .params
        .iter()
        .map(|p| {
            (
                p.var,
                IrExpr {
                    kind: IrExprKind::Var { id: p.var },
                    ty: p.ty.clone(),
                    span: None,
                    def_id: None,
                },
            )
        })
        .collect();
    let mut budget: i64 = 64;
    reduce_expr(&f.body, &env, &mut budget).is_some()
}

/// Inline every call to a small pure-scalar MAIN-PROGRAM function as the
/// reduced expression (#806 step 2). Shared with classify (desugar-before-both).
pub fn inline_small_scalar_fns(program: &mut almide_ir::IrProgram) {
    let inlinable: HashMap<String, (Vec<VarId>, Vec<Ty>, IrExpr)> = program
        .functions
        .iter()
        .filter(|f| is_inlinable_shape(f))
        .map(|f| {
            (
                f.name.as_str().to_string(),
                (
                    f.params.iter().map(|p| p.var).collect(),
                    f.params.iter().map(|p| p.ty.clone()).collect(),
                    f.body.clone(),
                ),
            )
        })
        .collect();
    if inlinable.is_empty() {
        return;
    }
    struct Inliner<'a> {
        inlinable: &'a HashMap<String, (Vec<VarId>, Vec<Ty>, IrExpr)>,
        exclude: &'a str,
    }
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    impl IrMutVisitor for Inliner<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
            else {
                return;
            };
            let Some((params, _ptys, body)) = self.inlinable.get(name.as_str()) else {
                return;
            };
            if name.as_str() == self.exclude || args.len() != params.len() {
                return;
            }
            // Every argument must be CALL-FREE: a multiply-used param duplicates
            // its argument, which must not re-run an effect or shift the caps
            // count (hot-loop args are plain vars).
            if args.iter().any(crate::lower::expr_contains_call) {
                return;
            }
            let env: HashMap<VarId, IrExpr> =
                params.iter().copied().zip(args.iter().cloned()).collect();
            let mut budget: i64 = 64;
            if let Some(reduced) = reduce_expr(body, &env, &mut budget) {
                *e = IrExpr { span: e.span, ..reduced };
            }
        }
    }
    // Rewrite every function body (main program + modules) EXCEPT the callee's
    // own body (no self-inlining; the reducible subset already forbids user
    // calls inside a callee, so cascades cannot occur).
    for i in 0..program.functions.len() {
        let fname = program.functions[i].name.as_str().to_string();
        let mut body = std::mem::replace(
            &mut program.functions[i].body,
            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
        );
        let mut inl = Inliner { inlinable: &inlinable, exclude: &fname };
        inl.visit_expr_mut(&mut body);
        program.functions[i].body = body;
    }
    for mi in 0..program.modules.len() {
        for fi in 0..program.modules[mi].functions.len() {
            let mut body = std::mem::replace(
                &mut program.modules[mi].functions[fi].body,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            let mut inl = Inliner { inlinable: &inlinable, exclude: "" };
            inl.visit_expr_mut(&mut body);
            program.modules[mi].functions[fi].body = body;
        }
    }
}
