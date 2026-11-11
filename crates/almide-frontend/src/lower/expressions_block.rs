// Block bodies, loop bodies with guards, pipes, eta expansion and module
// top-let references. `include!`d by expressions.rs (the 800-line file
// budget); it shares that module's scope and imports.

/// Lower a block body (stmts + optional tail), desugaring `guard let`. A `guard let
/// name = scrutinee else { alt }` binds `name` for the REST of the block, so everything
/// after it (the remaining stmts + the tail) becomes the Some/Ok arm of a match on the
/// scrutinee, and `alt` the wildcard arm. Statements before the guard stay as block
/// stmts. Recurses so multiple guard-lets nest. Without a guard-let it lowers normally.
/// The caller owns the block scope (push/pop around this).
/// Lower a LOOP BODY's statement list (#1204).
///
/// A loop body is a statement list like a block's, but it has no tail — so it
/// cannot go through `lower_block_body`, which is where `guard let` is
/// rewritten into its `match { ok/some => rest, _ => else }` form. Mapping
/// `lower_stmt` straight over the list therefore handed a `GuardLet` to
/// `lower_stmt`, whose arm is an `unreachable!("guard let is desugared by the
/// enclosing block")` — a compiler PANIC on `for … { guard let x = … else {
/// continue } … }`, which is `guard let`'s most natural use (it exists for
/// early exit, and `continue` is a loop's early exit). Present since the
/// construct landed; `spec/lang/guard_let_test.almd` never put one in a loop.
///
/// The rewrite is the block one, minus the tail: everything after the guard
/// becomes the Some/Ok arm's body, the `else` becomes the wildcard arm, and the
/// result is ONE statement in the loop body. Nested guards fall out of the
/// recursion, exactly as in a block.
fn lower_loop_body_stmts(ctx: &mut LowerCtx, body: &[ast::Stmt]) -> Vec<IrStmt> {
    let Some(i) = body.iter().position(|s| matches!(s, ast::Stmt::GuardLet { .. })) else {
        return body.iter().map(|s| lower_stmt(ctx, s)).collect();
    };
    let mut out: Vec<IrStmt> = body[..i].iter().map(|s| lower_stmt(ctx, s)).collect();
    // `lower_block_body` owns the guard rewrite; a Unit-typed, tail-less block
    // over the REST is exactly the shape it expects, and its result is a single
    // expression statement here.
    let rest = lower_block_body_in(ctx, &body[i..], None, &Ty::Unit, None, BlockPos::LoopBody);
    out.push(IrStmt { kind: IrStmtKind::Expr { expr: rest }, span: None });
    out
}

/// Lower `e`, a branch of a control-flow form that sits at `stmt_pos`: the
/// branch of a statement is itself a statement ([`LowerCtx::stmt_pos`]).
fn lower_branch(ctx: &mut LowerCtx, e: &ast::Expr, stmt_pos: bool) -> IrExpr {
    ctx.stmt_pos = stmt_pos;
    lower_expr(ctx, e)
}

/// Where a block stands, which decides how a `guard let` else in it leaves.
#[derive(Clone, Copy, PartialEq)]
enum BlockPos {
    /// The block's value is used: the fn's tail, a lambda body, a `let` value.
    Value,
    /// The block is a statement, or a branch of one: its value is dropped and
    /// the fn carries on after it ([`LowerCtx::stmt_pos`]).
    Stmt,
    /// The block is the rest of a loop body.
    LoopBody,
}

/// The block lowering, with the POSITION fact (#1543, #4655). The else of a
/// `guard let` leaves the FN with the else's value. The guard-let match is the
/// block's tail, so where the block's value is the fn's value the wildcard
/// arm's else IS the fn's return and stays the arm. It cannot stay the arm:
///
/// - in a STATEMENT (a loop body, a block in a branch arm of an `if` / `match`
///   statement): the arm's value is dropped there and the fn carries on;
/// - in a Unit block whose else is a value of another type (`else { log();
///   err(e) }` at the tail of a Unit effect fn): the arms do not join, and the
///   value would be dropped at the fn's end. An else that can only yield a
///   spelled `ok(..)` is the exception: at a Unit tail that is what falling
///   off the end returns, so it stays the arm every leg already runs.
///
/// The IR's one fn-exit spelling from statement position is the `Guard`
/// statement, so such an else is wrapped as `{ guard false else <else>; () }`
/// whatever its shape — every backend already renders a Guard's return
/// channel (ok-wrapping included). Two elses stay bare arms: `break` /
/// `continue` (loop control is valid in arm position), and outside a loop a
/// bare `err(e)`, which in a match arm already IS the early return on every
/// leg (the checker joins it as `Never`, `match_arm_join_ty`).
fn lower_block_body_in(
    ctx: &mut LowerCtx,
    stmts: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
    pos: BlockPos,
) -> IrExpr {
    if let Some(i) = stmts.iter().position(|s| matches!(s, ast::Stmt::GuardLet { .. })) {
        let pre: Vec<IrStmt> = stmts[..i].iter().map(|s| lower_stmt(ctx, s)).collect();
        let (name, scrutinee, else_, mutable, fail) = match &stmts[i] {
            ast::Stmt::GuardLet { name, scrutinee, else_, mutable, fail, .. } => (*name, scrutinee, else_, *mutable, fail),
            _ => unreachable!(),
        };
        let s = lower_expr(ctx, scrutinee);
        let subject_ty = if let IrExprKind::Var { id } = &s.kind {
            let vt_ty = &ctx.var_table.get(*id).ty;
            if matches!(vt_ty, Ty::Applied(_, _)) && !matches!(&s.ty, Ty::Applied(_, _)) {
                vt_ty.clone()
            } else {
                s.ty.clone()
            }
        } else {
            s.ty.clone()
        };
        let s = if subject_ty != s.ty { IrExpr { ty: subject_ty.clone(), ..s } } else { s };
        // #4543 `guard var`: the arm binds a fresh name and the rest of the
        // block opens with `var name = <it>` — the same `var` statement the
        // writer could have spelled, so no pattern-level mutability exists.
        let arm_name = if mutable { sym(&format!("__guard_var_{name}")) } else { name };
        let inner = ast::Pattern::Ident { name: arm_name };
        let bind_pat = match &subject_ty {
            Ty::Applied(TypeConstructorId::Result, _) => {
                ast::Pattern::Ok { inner: Box::new(inner) }
            }
            _ => ast::Pattern::Some { inner: Box::new(inner) },
        };
        // Some/Ok arm: bind name, then the rest of the block (recurse for nested guards).
        ctx.push_scope();
        let pat1 = lower_pattern(ctx, &bind_pat, &subject_ty);
        let rest = if mutable {
            lower_guard_var_rest(ctx, name, arm_name, &pat1, &stmts[i + 1..], tail, ty, span, pos)
        } else {
            lower_block_body_in(ctx, &stmts[i + 1..], tail, ty, span, pos)
        };
        ctx.pop_scope();
        let arm1 = IrMatchArm { pattern: pat1, guard: None, body: rest };
        // Wildcard arm: the else branch (must diverge).
        ctx.push_scope();
        // #4543: `else err(e) => …` matches `err(e)` so the else sees the
        // error; `none =>` / `err(_) =>` / the plain else keep the wildcard
        // arm every guard let had before.
        let fail_pat = match fail {
            Some(ast::Pattern::Err { inner }) if !matches!(**inner, ast::Pattern::Wildcard) => {
                ast::Pattern::Err { inner: inner.clone() }
            }
            _ => ast::Pattern::Wildcard,
        };
        let pat2 = lower_pattern(ctx, &fail_pat, &subject_ty);
        let mut alt = lower_expr(ctx, else_);
        ctx.pop_scope();
        let alt_is_loop_control = |e: &IrExpr| -> bool {
            matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue)
                || matches!(&e.kind, IrExprKind::Block { stmts, expr: None }
                    if stmts.len() == 1
                        && matches!(&stmts[0].kind, IrStmtKind::Expr { expr }
                            if matches!(&expr.kind, IrExprKind::Break | IrExprKind::Continue)))
        };
        if pos == BlockPos::LoopBody && alt_is_loop_control(&alt) {
            // Normalize a block-wrapped `{ continue }` / `{ break }` else to the
            // BARE loop-control expression: a statement-only Block in match-arm
            // position renders as `continue;` (invalid Rust in an arm without
            // braces). The bare node renders as the arm value `continue`.
            if let IrExprKind::Block { stmts, expr: None } = &alt.kind {
                if let Some(IrStmtKind::Expr { expr }) = stmts.first().map(|s| &s.kind) {
                    alt = expr.clone();
                }
            }
        }
        let raise_leaf = matches!(alt.kind, IrExprKind::ResultErr { .. });
        let by_guard = match pos {
            BlockPos::LoopBody => true,
            BlockPos::Stmt => !raise_leaf,
            BlockPos::Value => !raise_leaf && alt.ty != Ty::Unit && !yields_only_ok(&alt),
        };
        if by_guard && *ty == Ty::Unit && !alt_is_loop_control(&alt) {
            // See the doc comment: the else rides the Guard statement's return
            // channel; the arm's own value becomes Unit so the match types.
            let guard_stmt = IrStmt {
                kind: IrStmtKind::Guard {
                    cond: ctx.mk(IrExprKind::LitBool { value: false }, Ty::Bool, None),
                    else_: alt,
                },
                span: None,
            };
            alt = ctx.mk(
                IrExprKind::Block {
                    stmts: vec![guard_stmt],
                    expr: Some(Box::new(ctx.mk(IrExprKind::Unit, Ty::Unit, None))),
                },
                Ty::Unit,
                None,
            );
        }
        let arm2 = IrMatchArm { pattern: pat2, guard: None, body: alt };
        let match_expr =
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: vec![arm1, arm2] }, ty.clone(), span);
        ctx.mk(IrExprKind::Block { stmts: pre, expr: Some(Box::new(match_expr)) }, ty.clone(), span)
    } else {
        let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
        let ir_expr = tail.map(|e| Box::new(lower_branch(ctx, e, pos != BlockPos::Value)));
        ctx.mk(IrExprKind::Block { stmts: ir_stmts, expr: ir_expr }, ty.clone(), span)
    }
}

/// Does `e`'s value come only from spelled `ok(..)` constructors — through a
/// block's tail and the arms of an `if` / `match`?
fn yields_only_ok(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::ResultOk { .. } => true,
        IrExprKind::Block { expr: Some(tail), .. } => yields_only_ok(tail),
        IrExprKind::If { then, else_, .. } => yields_only_ok(then) && yields_only_ok(else_),
        IrExprKind::Match { arms, .. } => arms.iter().all(|a| yields_only_ok(&a.body)),
        _ => false,
    }
}

/// The success arm of a `guard var` (#4543): `var name = <arm binder>`, then
/// the rest of the block. The binder's type is the payload the arm bound.
#[allow(clippy::too_many_arguments)]
fn lower_guard_var_rest(
    ctx: &mut LowerCtx,
    name: almide_base::intern::Sym,
    arm_name: almide_base::intern::Sym,
    arm_pat: &IrPattern,
    rest: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
    pos: BlockPos,
) -> IrExpr {
    let payload_ty = match arm_pat {
        IrPattern::Ok { inner } | IrPattern::Some { inner } => match &**inner {
            IrPattern::Bind { ty, .. } => ty.clone(),
            _ => Ty::Unknown,
        },
        _ => Ty::Unknown,
    };
    let from = ctx.lookup_var(arm_name.as_str()).unwrap_or(VarId(0));
    let var = ctx.define_var(name.as_str(), payload_ty.clone(), Mutability::Var, span);
    let bind = IrStmt {
        kind: IrStmtKind::Bind {
            var,
            mutability: Mutability::Var,
            ty: payload_ty.clone(),
            value: ctx.mk(IrExprKind::Var { id: from }, payload_ty, span),
        },
        span,
    };
    let body = lower_block_body_in(ctx, rest, tail, ty, span, pos);
    ctx.mk(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(body)) }, ty.clone(), span)
}

/// Lower pipe expression, unwrapping postfix operators (`!`, `?`) on the RHS
/// so the pipe targets the inner Call. e.g. `xs |> list.find(p)!` becomes
/// `list.find(xs, p)!` rather than treating `!` as part of the pipe target.
fn lower_pipe(ctx: &mut LowerCtx, left: &ast::Expr, right: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    match &right.kind {
        // Transparent postfix: pipe into inner, then wrap with the operator.
        // `??` is an infix level below `|>` since dialect 16 (#4531), so it
        // never arrives here as a stage; only `!` and `?` do.
        ast::ExprKind::Unwrap { expr: inner, .. } => {
            // Use the checker's resolved type for the inner expression.
            // This preserves the actual error type (e.g., List[String] from result.collect)
            // instead of hardcoding String.
            let inner_checked_ty = ctx.expr_ty(inner);
            // #4215: a piped CALL the checker typed as a plain value cannot
            // fail, so its `!` is a no-op — the rule the direct `expr!`
            // spelling follows (#1049, `lower_expr`). In a test block
            // `xs |> list.map((x) => f(x)!)!` stays the plain map (the
            // callback's `!` unwraps), and a fabricated `Result` here
            // rendered `.unwrap()` on the collected `Vec`. A bare callee
            // (`p |> fs.read_text!`) is typed as the fn, not its result, so
            // it keeps the Result built below.
            let plain_call = matches!(inner.kind, ast::ExprKind::Call { .. })
                && !matches!(inner_checked_ty, Ty::Unknown | Ty::TypeVar(_) | Ty::Fn { .. });
            if plain_call && !inner_checked_ty.is_result() && !inner_checked_ty.is_option() {
                return lower_pipe(ctx, left, inner, ty, span);
            }
            let inner_ty = if inner_checked_ty.is_result() || inner_checked_ty.is_option() {
                inner_checked_ty
            } else {
                Ty::result(ty.clone(), Ty::String)
            };
            let piped = lower_pipe(ctx, left, inner, inner_ty, span.clone());
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(piped) }, ty, span)
        }
        ast::ExprKind::Try { expr: inner, .. } => {
            let piped = lower_pipe(ctx, left, inner, ty.clone(), span.clone());
            ctx.mk(IrExprKind::ToOption { expr: Box::new(piped) }, ty, span)
        }

        // Direct pipe targets
        ast::ExprKind::Call { callee, args, type_args, .. } => {
            let ir_left = lower_expr(ctx, left);
            let mut all_args = vec![ir_left];
            all_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
            let target = lower_call_target(ctx, callee);
            let ta = type_args.as_ref().map(|tas| tas.iter().map(|t| super::types::resolve_type_expr_env(ctx, t)).collect()).unwrap_or_default();
            let resolved_ty = if matches!(ty, Ty::Unknown) {
                if let CallTarget::Named { name } = &target {
                    ctx.env.functions.get(name).map(|f| f.ret.clone()).unwrap_or(ty)
                } else { ty }
            } else { ty };
            let target = ctx.ir_call_target(target);
            ctx.mk(IrExprKind::Call { target, args: all_args, type_args: ta }, resolved_ty, span)
        }
        ast::ExprKind::Ident { .. } | ast::ExprKind::Member { .. } => {
            let ir_left = lower_expr(ctx, left);
            let target = lower_call_target(ctx, right);
            let target = ctx.ir_call_target(target);
            ctx.mk(IrExprKind::Call { target, args: vec![ir_left], type_args: vec![] }, ty, span)
        }
        // `a |> (n) => body` — INLINE the immediately-applied lambda to `{ let n = a; body }`.
        // A pipe RHS lambda is applied exactly once, so binding its single param to the piped value
        // and evaluating the body is identical on BOTH targets — and it avoids a Computed-callee
        // call, which v1 MIR cannot lower as a first-class closure (it silently mis-lowered
        // `5 |> (n) => n * n` to 0). Multi-param / zero-param lambdas keep the Computed-call form.
        ast::ExprKind::Lambda { params, body, .. } if params.len() == 1 => {
            let ir_left = lower_expr(ctx, left);
            let p = &params[0];
            let param_ty = p
                .ty
                .as_ref()
                .map(|te| super::types::resolve_type_expr_env(ctx, te))
                .unwrap_or_else(|| ctx.expr_ty(left));
            ctx.push_scope();
            let bind = lower_pipe_lambda_bind(ctx, p, param_ty, ir_left, span);
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            ctx.mk(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(ir_body)) }, ty, span)
        }
        _ => {
            let ir_left = lower_expr(ctx, left);
            let ir_right = lower_expr(ctx, right);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(ir_right) },
                args: vec![ir_left], type_args: vec![],
            }, ty, span)
        }
    }
}

/// The bind a pipe-inlined single-param lambda opens its block with: a
/// tuple-pattern parameter (`x |> ((a, b)) => body`) destructures the
/// piped value through the pattern — `let (a, b) = x` — exactly as the
/// direct-call lowering (`lower_expr_lambda`) does; a plain name binds it
/// whole. Binding `p.name` alone aliased the WHOLE tuple under the first
/// name and left the rest unbound (#1873: check green, invalid Rust and
/// invalid wasm). Runs inside the lambda's scope (the caller pushes it).
fn lower_pipe_lambda_bind(ctx: &mut LowerCtx, p: &ast::LambdaParam, param_ty: Ty, ir_left: IrExpr, span: Option<ast::Span>) -> IrStmt {
    match &p.tuple_names {
        // A tuple-pattern parameter (`x |> ((a, b)) => body`) binds
        // EVERY name through the pattern — `{ let (a, b) = x; body }`,
        // the same destructure the direct-call lowering emits
        // (`lower_expr_lambda`). Binding `p.name` alone aliased the
        // whole tuple under the first name and left the rest unbound
        // (#1873: check green, invalid Rust and invalid wasm).
        Some(names) if names.len() > 1 => {
            let elem_tys: Vec<Ty> = match &param_ty {
                Ty::Tuple(es) if es.len() == names.len() => es.to_vec(),
                _ => vec![Ty::Unknown; names.len()],
            };
            let elements: Vec<IrPattern> = names.iter().zip(elem_tys.iter())
                .map(|(n, et)| {
                    if n.as_str() == "_" {
                        IrPattern::Wildcard
                    } else {
                        let v = ctx.define_var(n, et.clone(), Mutability::Let, span);
                        IrPattern::Bind { var: v, ty: et.clone() }
                    }
                })
                .collect();
            IrStmt {
                kind: IrStmtKind::BindDestructure {
                    pattern: IrPattern::Tuple { elements },
                    value: ir_left,
                },
                span,
            }
        }
        _ => {
            let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, span);
            IrStmt {
                kind: IrStmtKind::Bind {
                    var,
                    mutability: Mutability::Let,
                    ty: param_ty,
                    value: ir_left,
                },
                span,
            }
        }
    }
}

/// Eta-expand a module function reference (`string.len`, `list.map`, ...)
/// into a lambda that calls it. Used when the reference appears in value
/// position rather than as a callee, e.g. `xs |> list.map(string.len)`.
fn eta_expand_module_fn(
    ctx: &mut LowerCtx,
    module: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    params: Vec<Ty>,
    ret_ty: Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    ctx.push_scope();
    let mut param_vars: Vec<(VarId, Ty)> = Vec::with_capacity(params.len());
    for (i, pt) in params.iter().enumerate() {
        let name = format!("__eta_{}", i);
        let var = ctx.define_var(&name, pt.clone(), Mutability::Let, span.clone());
        param_vars.push((var, pt.clone()));
    }
    let args: Vec<IrExpr> = param_vars.iter()
        .map(|(var, pt)| ctx.mk(IrExprKind::Var { id: *var }, pt.clone(), span.clone()))
        .collect();
    // For stdlib modules (e.g. `string`) use CallTarget::Module so codegen
    // picks the stdlib runtime function. For user convention methods
    // (`Type.method`) use CallTarget::Named with the dotted key.
    let mod_name = module.as_str();
    let target = if crate::stdlib::is_stdlib_module(mod_name)
        || crate::stdlib::is_any_stdlib(mod_name)
        || ctx.env.user_modules.contains(&module)
        || ctx.env.import_table.aliases.contains_key(&module)
    {
        let resolved = ctx.env.import_table.aliases.get(&module).copied().unwrap_or(module);
        CallTarget::Module { module: resolved, func: field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", resolved, field))).copied() }
    } else {
        CallTarget::Named { name: sym(&format!("{}.{}", module, field)) }
    };
    // #3504: a user module's fn spelled `__x` is named as its module names it.
    let target = ctx.ir_call_target(target);
    let call = ctx.mk(IrExprKind::Call {
        target, args, type_args: vec![],
    }, ret_ty.clone(), span.clone());
    ctx.pop_scope();
    let lambda_id = Some(ctx.next_lambda_id());
    let lambda_ty = Ty::Fn { is_effect: false, 
        params: params.clone(),
        ret: Box::new(ret_ty),
    };
    ctx.mk(IrExprKind::Lambda {
        params: param_vars,
        body: Box::new(call),
        lambda_id,
    }, lambda_ty, span)
}

/// The use-site expression of a module top-let — the `Var` that
/// [`module_top_let_var`] mints, carrying the let's `DefId` when it has one.
/// Shared by `mod.NAME` and a selectively imported bare `NAME` (#3388).
pub(super) fn module_top_let_ref(
    ctx: &mut LowerCtx,
    mod_name: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    ty: &Ty,
    span: Option<crate::ast::Span>,
) -> Option<IrExpr> {
    let (var_id, def_id) = module_top_let_var(ctx, mod_name, field, ty)?;
    Some(match def_id {
        Some(def_id) => ctx.mk_def(IrExprKind::Var { id: var_id }, ty.clone(), span, def_id),
        None => ctx.mk(IrExprKind::Var { id: var_id }, ty.clone(), span),
    })
}

/// Resolve `mod.NAME` against the cross-module top-let table and build the
/// synthetic use-site Var: CLEAN uppercase name in the IR, `module_origin`
/// carrying the (versioned) module for emit-time prefixing. ONE rule shared
/// by every syntactic position that references a module top-let — reads
/// (`Member`) and assignment lvalues (`m.x = v`, #505); a position that
/// re-derives this resolution is a #500-class hole waiting to happen.
pub(super) fn module_top_let_var(
    ctx: &mut LowerCtx,
    mod_name: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    ty: &Ty,
) -> Option<(VarId, Option<almide_ir::DefId>)> {
    let resolved_mod = ctx.env.import_table.resolve(&mod_name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| mod_name.to_string());
    let qual_let_key = format!("{}.{}", resolved_mod, field);
    if !ctx.env.top_lets.contains_key(&sym(&qual_let_key)) {
        return None;
    }
    // Use the versioned module name if available (e.g. "snaidhm_v0.web.gpu")
    // to match the constant definition generated by lower_module. Exact
    // match first, then walk up parent segments to the package root (only
    // root modules have pkg_id → versioned name).
    let mod_ident = ctx.env.module_versioned_names.get(&sym(&resolved_mod))
        .map(|s| s.as_str().to_string())
        .or_else(|| {
            let parts: Vec<&str> = resolved_mod.split('.').collect();
            for i in (1..parts.len()).rev() {
                let prefix = parts[..i].join(".");
                if let Some(versioned) = ctx.env.module_versioned_names.get(&sym(&prefix)) {
                    let suffix = &resolved_mod[prefix.len()..];
                    return Some(format!("{}{}", versioned.as_str(), suffix));
                }
            }
            None
        })
        .unwrap_or_else(|| resolved_mod.clone());
    // The use-site Var keeps the SOURCE spelling (#3316): `buf` and `BUF`
    // are two top-lets, and every resolver that keys on this name must be
    // able to tell them apart.
    let origin = almide_base::names::module_ident(&mod_ident);
    let var_id = ctx.var_table.alloc_source(field, ty.clone(), Mutability::Let, None);
    ctx.var_table.entries[var_id.0 as usize].module_origin = Some(origin);
    let def_id = ctx.def_map.get(&sym(&qual_let_key)).copied();
    Some((var_id, def_id))
}
