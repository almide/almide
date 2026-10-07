// Lowering of the `fan` forms: `fan.bounded`/`fan.timeout` outlining, the
// race folds, and their helpers. `include!`d by expressions.rs (the 800-line
// file budget); it shares that module's scope and imports.

/// Shared half of the two `fan.bounded` desugars: synthesize the PLAIN outlined
/// fn `__almd_bounded_N(budget, args…) -> T` (enter → body call → exit, exit
/// persists the verdict), and return `(the call expr, the verdict-read expr)`.
/// The caller decides what to build from the scalar verdict (a fused fallback
/// If, or ok/err Result nodes).
fn lower_fan_bounded_call(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (IrExpr, IrExpr) {
    let ast::ExprKind::FanBounded { budget, body } = &expr.kind else { unreachable!() };
    let budget_ir = lower_expr(ctx, budget);
    let verdict = ctx.mk(IrExprKind::RuntimeCall {
        symbol: sym("almide_rt_prim_budget_exhausted"), args: vec![],
    }, Ty::Int, span);
    let call = outline_metered_arm(ctx, budget_ir, body, span);
    (call, verdict)
}

/// The timeout twin of [`lower_fan_bounded_call`]: the region brackets with
/// the WALL-clock prims and the verdict is the persisted deadline-hit flag.
fn lower_fan_timeout_call(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (IrExpr, IrExpr) {
    let ast::ExprKind::FanTimeout { deadline, body } = &expr.kind else { unreachable!() };
    let deadline_ir = lower_expr(ctx, deadline);
    let verdict = ctx.mk(IrExprKind::RuntimeCall {
        symbol: sym("almide_rt_prim_timeout_hit"), args: vec![],
    }, Ty::Int, span);
    let call = outline_metered_arm_with(
        ctx, deadline_ir, body, span,
        "almide_rt_prim_timeout_enter", "almide_rt_prim_timeout_exit",
    );
    (call, verdict)
}

/// Outline an arbitrary lowered expression into a synthesized plain fn whose
/// params are the expression's FREE VARIABLES (each renamed to a fresh param
/// id via substitution), returning the replacement call expr. The T2-1
/// free-var machinery, reusable: the BARE fan forms use it to become a DIRECT
/// `CallFn` bind (a tracked match subject on both renderers).
fn outline_ir_as_fn(
    ctx: &mut LowerCtx,
    body: IrExpr,
    fn_prefix: &str,
    span: Option<ast::Span>,
) -> IrExpr {
    use almide_ir::{CallTarget, IrFunction, IrParam, IrVisibility, Mutability, ParamBorrow};
    let ret_ty = body.ty.clone();
    ctx.bounded_counter += 1;
    let fn_name = format!("{fn_prefix}_{}", ctx.bounded_counter);
    let mut body = body;
    let free = almide_ir::free_vars::free_vars(&body, &std::collections::HashSet::new());
    let mut params = Vec::with_capacity(free.len());
    let mut call_args = Vec::with_capacity(free.len());
    for fv in free {
        let info = ctx.var_table.get(fv);
        let (fv_ty, fv_name) = (info.ty.clone(), info.name);
        let pv = ctx.var_table.alloc(fv_name, fv_ty.clone(), Mutability::Let, span);
        let pv_expr = ctx.mk(IrExprKind::Var { id: pv }, fv_ty.clone(), span);
        body = almide_ir::substitute::substitute_var_in_expr(&body, fv, &pv_expr);
        params.push(IrParam {
            var: pv, ty: fv_ty.clone(), name: fv_name,
            borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
        });
        call_args.push(ctx.mk(IrExprKind::Var { id: fv }, fv_ty, span));
    }
    ctx.synthesized_fns.push(IrFunction {
        name: sym(&fn_name),
        params,
        ret_ty: ret_ty.clone(),
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: Vec::new(), // fresh-fn: lifted lambda, lambda params cannot be mut
    });
    ctx.mk(IrExprKind::Call {
        target: CallTarget::Named { name: sym(&fn_name) },
        args: call_args,
        type_args: vec![],
    }, ret_ty, span)
}

/// Outline ONE metered region: synthesize `__almd_bounded_N(budget, args…) -> T`
/// (enter → body call → exit; exit persists verdict + spend) and return the
/// call expr. Shared by `fan.bounded` (one region) and `fan.race` (one per arm).
fn outline_metered_arm(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body: &ast::Expr,
    span: Option<ast::Span>,
) -> IrExpr {
    outline_metered_arm_with(
        ctx, budget_arg, body, span,
        "almide_rt_prim_budget_enter", "almide_rt_prim_budget_exit",
    )
}

/// [`outline_metered_arm`] with explicit enter/exit runtime symbols — the
/// timeout head (T5-1) shares the outliner but brackets with the WALL-clock
/// prims instead of the fuel ones.
fn outline_metered_arm_with(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body: &ast::Expr,
    span: Option<ast::Span>,
    enter_sym: &str,
    exit_sym: &str,
) -> IrExpr {
    // A `{ single_call() }` body parses as a one-expr Block — unwrap it so the
    // single-call shape keeps the args-as-params path below.
    let body_ir = {
        let b = lower_expr(ctx, body);
        match b.kind {
            IrExprKind::Block { stmts, expr: Some(e) } if stmts.is_empty() => *e,
            kind => IrExpr { kind, ty: b.ty, span: b.span, def_id: b.def_id },
        }
    };
    outline_metered_arm_ir(ctx, budget_arg, body_ir, span, enter_sym, exit_sym)
}

/// The post-lowering half of [`outline_metered_arm_with`]: outline an
/// ALREADY-LOWERED region body. The mapper form (T7-1) enters here directly —
/// its region body is the mapper lambda's body with the param bound to the
/// per-iteration element var, which the free-var parameterization below turns
/// into a region param like any other capture.
fn outline_metered_arm_ir(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body_ir: IrExpr,
    span: Option<ast::Span>,
    enter_sym: &str,
    exit_sym: &str,
) -> IrExpr {
    use almide_ir::{CallTarget, IrFunction, IrParam, IrStmt, IrStmtKind, IrVisibility, Mutability, ParamBorrow};
    let body_ty = body_ir.ty.clone();
    let budget_ir = budget_arg;

    let mk_rt = |ctx: &LowerCtx, name: &str, args: Vec<IrExpr>| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args }, Ty::Int, span)
    };

    ctx.bounded_counter += 1;
    let fn_name = format!("__almd_bounded_{}", ctx.bounded_counter);

    let budget_param = ctx.var_table.alloc(sym("__b_budget"), Ty::Int, Mutability::Let, span);
    let mut params = vec![IrParam {
        var: budget_param, ty: Ty::Int, name: sym("__b_budget"),
        borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
    }];
    // Two parameterization shapes:
    //  - single Call body (the v1 shape): the CALLEE'S ARGS become the params
    //    (each arg expr evaluates in the caller, before the meter starts);
    //  - anything else (block bodies, inline exprs — T2-1): the body's FREE
    //    VARIABLES become the params. Each free var is renamed to a fresh
    //    param id inside the body (VarIds stay globally unique) and the
    //    caller passes the original var. The body is PURE (checker rule), so
    //    a by-value snapshot at call time is observationally exact.
    let (metered_body, call_tail_args): (IrExpr, Vec<IrExpr>) = match body_ir.kind {
        IrExprKind::Call { target, args: body_args, type_args } => {
            let mut inner_args: Vec<IrExpr> = Vec::with_capacity(body_args.len());
            for (i, a) in body_args.iter().enumerate() {
                let pname = format!("__b_a{i}");
                let pv = ctx.var_table.alloc(sym(&pname), a.ty.clone(), Mutability::Let, span);
                params.push(IrParam {
                    var: pv, ty: a.ty.clone(), name: sym(&pname),
                    borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
                });
                inner_args.push(ctx.mk(IrExprKind::Var { id: pv }, a.ty.clone(), span));
            }
            let call = ctx.mk(
                IrExprKind::Call { target, args: inner_args, type_args },
                body_ty.clone(),
                span,
            );
            (call, body_args)
        }
        kind => {
            let mut body = IrExpr { kind, ty: body_ty.clone(), span, def_id: None };
            let free =
                almide_ir::free_vars::free_vars(&body, &std::collections::HashSet::new());
            let mut tail_args = Vec::with_capacity(free.len());
            for fv in free {
                let info = ctx.var_table.get(fv);
                let (fv_ty, fv_name) = (info.ty.clone(), info.name);
                let pv = ctx.var_table.alloc(fv_name, fv_ty.clone(), Mutability::Let, span);
                let pv_expr = ctx.mk(IrExprKind::Var { id: pv }, fv_ty.clone(), span);
                body = almide_ir::substitute::substitute_var_in_expr(&body, fv, &pv_expr);
                params.push(IrParam {
                    var: pv, ty: fv_ty.clone(), name: fv_name,
                    borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
                });
                tail_args.push(ctx.mk(IrExprKind::Var { id: fv }, fv_ty, span));
            }
            (body, tail_args)
        }
    };

    let saved_var = ctx.var_table.alloc(sym("__b_saved"), Ty::Int, Mutability::Let, span);
    let val_var = ctx.var_table.alloc(sym("__b_val"), body_ty.clone(), Mutability::Let, span);
    let exit_var = ctx.var_table.alloc(sym("__b_exit"), Ty::Int, Mutability::Let, span);
    let stmts = vec![
        IrStmt { kind: IrStmtKind::Bind {
            var: saved_var, mutability: Mutability::Let, ty: Ty::Int,
            value: mk_rt(ctx, enter_sym,
                vec![ctx.mk(IrExprKind::Var { id: budget_param }, Ty::Int, span)]),
        }, span },
        IrStmt { kind: IrStmtKind::Bind {
            var: val_var, mutability: Mutability::Let, ty: body_ty.clone(),
            value: metered_body,
        }, span },
        IrStmt { kind: IrStmtKind::Bind {
            var: exit_var, mutability: Mutability::Let, ty: Ty::Int,
            value: mk_rt(ctx, exit_sym,
                vec![ctx.mk(IrExprKind::Var { id: saved_var }, Ty::Int, span)]),
        }, span },
    ];
    let fn_body = ctx.mk(IrExprKind::Block {
        stmts,
        expr: Some(Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty.clone(), span))),
    }, body_ty.clone(), span);

    ctx.synthesized_fns.push(IrFunction {
        name: sym(&fn_name),
        params,
        ret_ty: body_ty.clone(),
        body: fn_body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: Vec::new(), // fresh-fn: lifted lambda, lambda params cannot be mut
    });

    let mut call_args = vec![budget_ir];
    call_args.extend(call_tail_args);
    ctx.mk(IrExprKind::Call {
        target: CallTarget::Named { name: sym(&fn_name) },
        args: call_args,
        type_args: vec![],
    }, body_ty, span)
}

/// Lower `fan.race(budget?) { arms }` into the sequential lex-min fold: each
/// arm is an outlined metered region; after each call the PERSISTED verdict and
/// spend are read as scalars; the winner is the (spend, index)-lexicographic
/// minimum among non-exhausted arms — folded with scalar if-values, so the
/// whole thing (bar the arm bodies) stays on the native rung. Returns
/// (stmts, ok_var, val_var, arm_ty): tail construction is the caller's
/// (fused fallback vs ok/err Result nodes).
fn lower_fan_race_fold(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (Vec<almide_ir::IrStmt>, almide_ir::VarId, almide_ir::VarId, Ty) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
    let ast::ExprKind::FanRace { budget, arms } = &expr.kind else { unreachable!() };

    let mk_rt = |ctx: &LowerCtx, name: &str| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args: vec![] }, Ty::Int, span)
    };
    let mk_var = |ctx: &LowerCtx, id: VarId, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::Var { id }, ty, span)
    };
    let mk_int = |ctx: &LowerCtx, v: i64| -> IrExpr {
        ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span)
    };
    let mk_if_int = |ctx: &LowerCtx, c: IrExpr, t: IrExpr, e: IrExpr, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
    };
    let mk_cmp = |ctx: &LowerCtx, op: almide_ir::BinOp, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool, span)
    };
    let bind = |ctx: &mut LowerCtx, name: &str, ty: Ty, value: IrExpr| -> (VarId, IrStmt) {
        let v = ctx.var_table.alloc(sym(name), ty.clone(), Mutability::Let, span);
        (v, IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty, value }, span })
    };

    let mut stmts: Vec<IrStmt> = Vec::new();
    // The budget is evaluated ONCE; the no-budget form is the i64::MAX sentinel
    // (effectively infinite — the divergence-guard is simply absent).
    let budget_ir = match budget {
        Some(b) => lower_expr(ctx, b),
        None => mk_int(ctx, i64::MAX),
    };
    let (bv, st) = bind(ctx, "__r_budget", Ty::Int, budget_ir);
    stmts.push(st);

    let mut arm_ty = Ty::Unknown;
    let mut ok_var: Option<VarId> = None;
    let mut bs_var: Option<VarId> = None;
    let mut val_var: Option<VarId> = None;

    for (i, arm) in arms.iter().enumerate() {
        let call = outline_metered_arm(ctx, mk_var(ctx, bv, Ty::Int), arm, span);
        let call_ty = call.ty.clone();
        // T2-2: a Result arm SELF-DISQUALIFIES on Err (symmetric with
        // fan.any). Its Ok payload is the candidate value.
        use almide_lang::types::constructor::TypeConstructorId;
        let res_elem = match &call_ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[0].clone()),
            _ => None,
        };
        arm_ty = res_elem.clone().unwrap_or_else(|| call_ty.clone());

        let (vi, exi, spi, arm_ok) = if let Some(elem) = res_elem {
            let (ri, st) = bind(ctx, &format!("__r_r{i}"), call_ty.clone(), call);
            stmts.push(st);
            let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
            let (exi, st) = bind(ctx, &format!("__r_ex{i}"), Ty::Int, ex);
            stmts.push(st);
            let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
            let (spi, st) = bind(ctx, &format!("__r_sp{i}"), Ty::Int, sp);
            stmts.push(st);
            use almide_ir::{IrMatchArm, IrPattern};
            // arm_ok = match ri { ok(_) => 1, err(_) => 0 }
            let subj = mk_var(ctx, ri, call_ty.clone());
            let one = mk_int(ctx, 1);
            let zero = mk_int(ctx, 0);
            let m_ok = ctx.mk(IrExprKind::Match {
                subject: Box::new(subj),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: one,
                    },
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: zero,
                    },
                ],
            }, Ty::Int, span);
            let (oki, st) = bind(ctx, &format!("__r_isok{i}"), Ty::Int, m_ok);
            stmts.push(st);
            // vi = match ri { ok(v) => v, err(_) => <default of T> } — the
            // default is dead (the ok flag guards it), it only types the arm.
            let payload = ctx.var_table.alloc(sym("__r_okv"), elem.clone(), Mutability::Let, span);
            let payload_body = mk_var(ctx, payload, elem.clone());
            let default = match &elem {
                Ty::String => ctx.mk(IrExprKind::LitStr { value: String::new() }, Ty::String, span),
                _ => mk_int(ctx, 0),
            };
            let subj2 = mk_var(ctx, ri, call_ty.clone());
            let m_val = ctx.mk(IrExprKind::Match {
                subject: Box::new(subj2),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok {
                            inner: Box::new(IrPattern::Bind { var: payload, ty: elem.clone() }),
                        },
                        guard: None,
                        body: payload_body,
                    },
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: default,
                    },
                ],
            }, elem.clone(), span);
            let (vi, st) = bind(ctx, &format!("__r_v{i}"), elem, m_val);
            stmts.push(st);
            (vi, exi, spi, Some(oki))
        } else {
            let (vi, st) = bind(ctx, &format!("__r_v{i}"), arm_ty.clone(), call);
            stmts.push(st);
            let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
            let (exi, st) = bind(ctx, &format!("__r_ex{i}"), Ty::Int, ex);
            stmts.push(st);
            let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
            let (spi, st) = bind(ctx, &format!("__r_sp{i}"), Ty::Int, sp);
            stmts.push(st);
            (vi, exi, spi, None)
        };

        // candidate = (exhausted == 0) AND (the arm's own Ok, when Result)
        let within = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, exi, Ty::Int), mk_int(ctx, 0));
        let cand = match arm_ok {
            None => within,
            Some(oki) => {
                let ok_flag = mk_if_int(ctx, within, mk_var(ctx, oki, Ty::Int), mk_int(ctx, 0), Ty::Int);
                let (ci, st) = bind(ctx, &format!("__r_cand{i}"), Ty::Int, ok_flag);
                stmts.push(st);
                mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, ci, Ty::Int), mk_int(ctx, 1))
            }
        };
        match (ok_var, bs_var, val_var) {
            (None, None, None) => {
                let cand2 = cand.clone();
                let (ok0, st) = bind(ctx, "__r_ok0", Ty::Int,
                    mk_if_int(ctx, cand, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let _ = cand2;
                ok_var = Some(ok0);
                bs_var = Some(spi);
                val_var = Some(vi);
            }
            (Some(ok_p), Some(bs_p), Some(val_p)) => {
                // better = candidate AND (no winner yet OR spend < best) —
                // strict <: ties keep the earlier index, the source-order
                // rule. The OR is 0/1 ARITHMETIC (a + b − ab) over const-arm
                // flag ifs, never a nested if-value flowing out as an arm
                // value: that shape certs the inner merge as `i{m|}` /
                // `i{|m}` — a one-sided released-merge object the
                // kernel-proven ownership checker rejects (the PCC
                // corpus-wall catch, 2026-08-03).
                let no_winner = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, ok_p, Ty::Int), mk_int(ctx, 0));
                let (nw, st) = bind(ctx, &format!("__r_nw{i}"), Ty::Int,
                    mk_if_int(ctx, no_winner, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let cheaper = mk_cmp(ctx, almide_ir::BinOp::Lt, mk_var(ctx, spi, Ty::Int), mk_var(ctx, bs_p, Ty::Int));
                let (ch, st) = bind(ctx, &format!("__r_ch{i}"), Ty::Int,
                    mk_if_int(ctx, cheaper, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let ab = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::MulInt,
                    left: Box::new(mk_var(ctx, nw, Ty::Int)),
                    right: Box::new(mk_var(ctx, ch, Ty::Int)),
                }, Ty::Int, span);
                let a_plus_b = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::AddInt,
                    left: Box::new(mk_var(ctx, nw, Ty::Int)),
                    right: Box::new(mk_var(ctx, ch, Ty::Int)),
                }, Ty::Int, span);
                let inner = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::SubInt,
                    left: Box::new(a_plus_b),
                    right: Box::new(ab),
                }, Ty::Int, span);
                let (bet, st) = bind(ctx, &format!("__r_bet{i}"), Ty::Int,
                    mk_if_int(ctx, cand, inner, mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let is_bet = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, bet, Ty::Int), mk_int(ctx, 1));
                let (ok_n, st) = bind(ctx, &format!("__r_ok{i}"), Ty::Int,
                    mk_if_int(ctx, is_bet.clone(), mk_int(ctx, 1), mk_var(ctx, ok_p, Ty::Int), Ty::Int));
                stmts.push(st);
                let (bs_n, st) = bind(ctx, &format!("__r_bs{i}"), Ty::Int,
                    mk_if_int(ctx, is_bet.clone(), mk_var(ctx, spi, Ty::Int), mk_var(ctx, bs_p, Ty::Int), Ty::Int));
                stmts.push(st);
                let (val_n, st) = bind(ctx, &format!("__r_val{i}"), arm_ty.clone(),
                    mk_if_int(ctx, is_bet, mk_var(ctx, vi, arm_ty.clone()), mk_var(ctx, val_p, arm_ty.clone()), arm_ty.clone()));
                stmts.push(st);
                ok_var = Some(ok_n);
                bs_var = Some(bs_n);
                val_var = Some(val_n);
            }
            _ => unreachable!(),
        }
    }
    // The parser refuses an armless `fan.race`, so the first arm set all three.
    let (Some(ok_var), Some(val_var)) = (ok_var, val_var) else { unreachable!("fan.race lowered with no arms") };
    (stmts, ok_var, val_var, arm_ty)
}

/// Lower `fan.race(budget?, xs, f)` — the MAPPER form (T7-1) — into a dynamic
/// (spend, index) lex-min fold: a while loop over `xs` calls ONE outlined
/// metered region per element (the mapper body with its param bound to the
/// element — per-branch budget, exactly the block form's rule), reads the
/// persisted verdict + spend, and folds the minimum into scalar `var`s.
/// Strict `<` keeps the earlier index on ties (the source-order rule, here
/// list order). Same return contract as [`lower_fan_race_fold`]:
/// (stmts, ok_var, val_var, winner_ty) — the caller builds the tail
/// (fused fallback vs ok/err Result nodes).
fn lower_fan_race_map_fold(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (Vec<almide_ir::IrStmt>, almide_ir::VarId, almide_ir::VarId, Ty) {
    use almide_ir::{IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability, VarId};
    use almide_lang::types::constructor::TypeConstructorId;
    let ast::ExprKind::FanRaceMap { budget, list, mapper } = &expr.kind else { unreachable!() };

    let mk_rt = |ctx: &LowerCtx, name: &str| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args: vec![] }, Ty::Int, span)
    };
    let mk_var = |ctx: &LowerCtx, id: VarId, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::Var { id }, ty, span)
    };
    let mk_int = |ctx: &LowerCtx, v: i64| -> IrExpr {
        ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span)
    };
    let mk_if = |ctx: &LowerCtx, c: IrExpr, t: IrExpr, e: IrExpr, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
    };
    let mk_cmp = |ctx: &LowerCtx, op: almide_ir::BinOp, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool, span)
    };
    let bind = |ctx: &mut LowerCtx, name: &str, m: Mutability, ty: Ty, value: IrExpr| -> (VarId, IrStmt) {
        let v = ctx.var_table.alloc(sym(name), ty.clone(), m, span);
        (v, IrStmt { kind: IrStmtKind::Bind { var: v, mutability: m, ty, value }, span })
    };
    let assign = |v: VarId, value: IrExpr| -> IrStmt {
        IrStmt { kind: IrStmtKind::Assign { var: v, value }, span }
    };
    // The winner type's dead placeholder — the ok flag guards every read, it
    // only types the slot (the block form's exact convention).
    let default_of = |ctx: &LowerCtx, t: &Ty| -> IrExpr {
        match t {
            Ty::String => ctx.mk(IrExprKind::LitStr { value: String::new() }, Ty::String, span),
            _ => ctx.mk(IrExprKind::LitInt { value: 0 }, t.clone(), span),
        }
    };

    // Types from the checker: list → List[X], winner T from the node's own
    // Result[T, String] (the g3 arm's return).
    let list_ir = lower_expr(ctx, list);
    let elem_ty = match &list_ir.ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
        _ => Ty::Unknown,
    };
    let winner_ty = match ctx.expr_ty(expr) {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
        _ => Ty::Unknown,
    };

    let mut stmts: Vec<IrStmt> = Vec::new();
    let budget_ir = match budget {
        Some(b) => lower_expr(ctx, b),
        None => mk_int(ctx, i64::MAX),
    };
    let (bv, st) = bind(ctx, "__rm_budget", Mutability::Let, Ty::Int, budget_ir);
    stmts.push(st);
    let list_ty = list_ir.ty.clone();
    let (xs, st) = bind(ctx, "__rm_xs", Mutability::Let, list_ty.clone(), list_ir);
    stmts.push(st);
    let zero = mk_int(ctx, 0);
    let (okv, st) = bind(ctx, "__rm_ok", Mutability::Var, Ty::Int, zero);
    stmts.push(st);
    let maxs = mk_int(ctx, i64::MAX);
    let (bsv, st) = bind(ctx, "__rm_bs", Mutability::Var, Ty::Int, maxs);
    stmts.push(st);
    let dflt = default_of(ctx, &winner_ty);
    let (valv, st) = bind(ctx, "__rm_val", Mutability::Var, winner_ty.clone(), dflt);
    stmts.push(st);

    // The metered region: the mapper body with its param bound to a fresh
    // element var. The outliner's free-var pass turns that var (and any outer
    // capture) into region params.
    let ast::ExprKind::Lambda { params, body } = &mapper.kind else {
        // The parser only builds this node for a 1-param lambda tail.
        unreachable!("FanRaceMap mapper is parser-gated to a 1-param lambda")
    };
    ctx.push_scope();
    let px = ctx.define_var(&params[0].name, elem_ty.clone(), Mutability::Let, span);
    let region_body = lower_expr(ctx, body);
    ctx.pop_scope();
    let mapper_ret_ty = region_body.ty.clone();
    let region_call = outline_metered_arm_ir(
        ctx, mk_var(ctx, bv, Ty::Int), region_body, span,
        "almide_rt_prim_budget_enter", "almide_rt_prim_budget_exit",
    );

    // Loop body: match list.get(xs, i) { some(x) => <arm eval + fold>, none => () }.
    let mut arm_stmts: Vec<IrStmt> = Vec::new();
    let (rv, st) = bind(ctx, "__rm_r", Mutability::Let, mapper_ret_ty.clone(), region_call);
    arm_stmts.push(st);
    let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
    let (exv, st) = bind(ctx, "__rm_ex", Mutability::Let, Ty::Int, ex);
    arm_stmts.push(st);
    let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
    let (spv, st) = bind(ctx, "__rm_sp", Mutability::Let, Ty::Int, sp);
    arm_stmts.push(st);
    // isok = match r { ok(_) => 1, err(_) => 0 }
    let one = mk_int(ctx, 1);
    let zero = mk_int(ctx, 0);
    let subj = mk_var(ctx, rv, mapper_ret_ty.clone());
    let m_ok = ctx.mk(IrExprKind::Match {
        subject: Box::new(subj),
        arms: vec![
            IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: one },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: zero },
        ],
    }, Ty::Int, span);
    let (isokv, st) = bind(ctx, "__rm_isok", Mutability::Let, Ty::Int, m_ok);
    arm_stmts.push(st);
    // The admission/lex-min combination is PURE 0/1 ARITHMETIC over const-arm
    // ifs — never an if-value whose ARM is another if's merge dst. That shape
    // classifies the inner merge as a RELEASED object and its one-sided flow
    // certs as `i{m|}`, which the kernel-proven ownership checker rejects
    // (the PCC corpus-wall catch, 2026-08-03). AND = multiply, OR = a + b − ab.
    let mk_flag = |ctx: &mut LowerCtx, c: IrExpr| -> IrExpr {
        mk_if(ctx, c, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int)
    };
    let mk_mul = |ctx: &LowerCtx, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op: almide_ir::BinOp::MulInt, left: Box::new(l), right: Box::new(r) }, Ty::Int, span)
    };
    // within01 = (ex == 0); cand = within01 * isok
    let within = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, exv, Ty::Int), mk_int(ctx, 0));
    let within01 = mk_flag(ctx, within);
    let cand_e = mk_mul(ctx, within01, mk_var(ctx, isokv, Ty::Int));
    let (candv, st) = bind(ctx, "__rm_cand", Mutability::Let, Ty::Int, cand_e);
    arm_stmts.push(st);
    // bet = cand * (no_winner OR cheaper) — strict <, ties keep the earlier
    // element. OR over 0/1 flags = a + b − a·b.
    let no_winner = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, okv, Ty::Int), mk_int(ctx, 0));
    let nw01 = mk_flag(ctx, no_winner);
    let (nwv, st) = bind(ctx, "__rm_nw", Mutability::Let, Ty::Int, nw01);
    arm_stmts.push(st);
    let cheaper = mk_cmp(ctx, almide_ir::BinOp::Lt, mk_var(ctx, spv, Ty::Int), mk_var(ctx, bsv, Ty::Int));
    let ch01 = mk_flag(ctx, cheaper);
    let (chv, st) = bind(ctx, "__rm_ch", Mutability::Let, Ty::Int, ch01);
    arm_stmts.push(st);
    let ab = mk_mul(ctx, mk_var(ctx, nwv, Ty::Int), mk_var(ctx, chv, Ty::Int));
    let a_plus_b = ctx.mk(IrExprKind::BinOp {
        op: almide_ir::BinOp::AddInt,
        left: Box::new(mk_var(ctx, nwv, Ty::Int)),
        right: Box::new(mk_var(ctx, chv, Ty::Int)),
    }, Ty::Int, span);
    let or_e = ctx.mk(IrExprKind::BinOp {
        op: almide_ir::BinOp::SubInt,
        left: Box::new(a_plus_b),
        right: Box::new(ab),
    }, Ty::Int, span);
    let bet_e = mk_mul(ctx, mk_var(ctx, candv, Ty::Int), or_e);
    let (betv, st) = bind(ctx, "__rm_bet", Mutability::Let, Ty::Int, bet_e);
    arm_stmts.push(st);
    // if bet == 1 then { ok = 1; bs = sp; val = payload } else ()
    let payload_var = ctx.var_table.alloc(sym("__rm_okv"), winner_ty.clone(), Mutability::Let, span);
    let payload_body = mk_var(ctx, payload_var, winner_ty.clone());
    let pdflt = default_of(ctx, &winner_ty);
    let subj2 = mk_var(ctx, rv, mapper_ret_ty.clone());
    let m_val = ctx.mk(IrExprKind::Match {
        subject: Box::new(subj2),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: payload_var, ty: winner_ty.clone() }) },
                guard: None, body: payload_body,
            },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: pdflt },
        ],
    }, winner_ty.clone(), span);
    let win_block = ctx.mk(IrExprKind::Block {
        stmts: vec![
            assign(okv, mk_int(ctx, 1)),
            assign(bsv, mk_var(ctx, spv, Ty::Int)),
            assign(valv, m_val),
        ],
        expr: None,
    }, Ty::Unit, span);
    let unit = ctx.mk(IrExprKind::Unit, Ty::Unit, span);
    let is_bet = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, betv, Ty::Int), mk_int(ctx, 1));
    let take = mk_if(ctx, is_bet, win_block, unit, Ty::Unit);
    arm_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: take }, span });

    // The scan is a ForIn over the list — the PROVEN loop shape (balanced
    // ownership certs everywhere in the corpus). Iteration order = list order
    // and the fold's strict `<` keeps the FIRST minimum, so the lex-min
    // tie-break needs no index variable at all. (Earlier shapes — a while +
    // `list.get` + Option match — emitted an `i{m|}` subject cert the
    // kernel-proven checker rejects: the Some arm consumes, the None arm
    // leaks. The PCC corpus-wall catch, 2026-08-03.)
    let for_e = ctx.mk(IrExprKind::ForIn {
        var: px,
        var_tuple: None,
        iterable: Box::new(mk_var(ctx, xs, list_ty.clone())),
        body: arm_stmts,
    }, Ty::Unit, span);
    stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: for_e }, span });

    (stmts, okv, valv, winner_ty)
}

/// L4 (#1108 Phase 2b): inside a FALLIBLE lambda body, rewrite every
/// Option-operand `Unwrap` into `option.to_result(e, "none")!` so the
/// closure's propagation stays a plain Result unwrap on every backend
/// (the fn-body ok_or template has no closure equivalent).
fn convert_option_unwraps_to_result(body: &mut IrExpr) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct Rw;
    impl IrMutVisitor for Rw {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Unwrap { expr: inner } = &mut e.kind else { return };
            if !inner.ty.is_option() {
                return;
            }
            let inner_ty = inner.ty.clone();
            let payload_ty = inner_ty.option_inner().unwrap_or(Ty::Unknown);
            let span = inner.span.clone();
            let opt = std::mem::replace(
                &mut **inner,
                IrExpr { kind: IrExprKind::OptionNone, ty: inner_ty, span: span.clone(), def_id: None },
            );
            **inner = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("option"),
                        func: sym("to_result"),
                        def_id: None,
                    },
                    args: vec![
                        opt,
                        IrExpr { kind: IrExprKind::LitStr { value: "none".into() }, ty: Ty::String, span: span.clone(), def_id: None },
                    ],
                    type_args: Vec::new(),
                },
                ty: Ty::result(payload_ty, Ty::String),
                span,
                def_id: None,
            };
        }
    }
    Rw.visit_expr_mut(body);
}

/// #3462: a `fan { … }` arm whose body propagates with `!` is a propagation
/// SCOPE of its own (C-199, ADR-0024 D1): the `!` ends the ARM with its Err,
/// every sibling still runs, and the join reports the lowest-index Err. The
/// arm is lowered as the zero-arg thunk `() => ok(body)` called in place —
/// the shape a `fan.map` callback's `!` already rides on every consumer — so
/// no backend reads that `!` as an exit from the enclosing fn. An arm whose
/// only marker is its own top-level `!` already is a Result arm and is left
/// alone.
fn fan_arm_scope(ctx: &mut LowerCtx, arm: IrExpr) -> IrExpr {
    let inner = match &arm.kind { IrExprKind::Unwrap { expr } => &**expr, _ => &arm };
    if !propagates_in_scope(inner) {
        return arm;
    }
    let span = arm.span.clone();
    let mut body = arm;
    convert_option_unwraps_to_result(&mut body);
    if !body.ty.is_result() {
        body = crate::lower::wrap_fallible_value_tail(body, &Ty::String);
    }
    let ret = body.ty.clone();
    let lambda_ty = Ty::Fn { params: Vec::new(), ret: Box::new(ret.clone()), is_effect: false };
    let lambda_id = Some(ctx.next_lambda_id());
    let thunk = ctx.mk(IrExprKind::Lambda { params: Vec::new(), body: Box::new(body), lambda_id }, lambda_ty, span.clone());
    let target = CallTarget::Computed { callee: Box::new(thunk) };
    ctx.mk(IrExprKind::Call { target, args: Vec::new(), type_args: Vec::new() }, ret, span)
}

/// Does `e` hold a `!` whose propagation leaves `e` itself — one not inside a
/// lambda (its own channel) or a nested `fan` (its arms are scoped in turn)?
fn propagates_in_scope(e: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match e.kind {
                IrExprKind::Unwrap { .. } => self.0 = true,
                IrExprKind::Lambda { .. } | IrExprKind::Fan { .. } => {}
                _ if !self.0 => walk_expr(self, e),
                _ => {}
            }
        }
    }
    let mut s = Scan(false);
    s.visit_expr(e);
    s.0
}
