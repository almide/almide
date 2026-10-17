// `testing.assert_aborts` (ADR-0029 D3): lift each call's closure to a site
// fn and rewrite the call to the shared verdict. `include!`d by calls.rs (the
// 800-line file budget); it shares that module's scope and imports.

/// `testing.assert_aborts(expected, () => body)` called directly in a test
/// block. `body` is lowered as a PROGRAM body (`ctx.in_test` off), so its
/// asserts take the T18 block form and `panic` the C-219 abort, and becomes
/// the site fn `__almd_abort_site_<k>`; the call becomes
/// `testing.__assert_aborts_check(expected, testing.__abort_site_run(k), <line>)`.
/// The checker refuses every other shape (E098 / E099), so a shape this does
/// not recognise is left to the ordinary call path.
fn lower_assert_aborts(ctx: &mut LowerCtx, callee: &ast::Expr, args: &[ast::Expr], span: Option<ast::Span>) -> Option<IrExpr> {
    if !ctx.in_test || args.len() != 2 {
        return None;
    }
    let ast::ExprKind::Member { field, .. } = &callee.kind else { return None };
    if field.as_str() != "assert_aborts" {
        return None;
    }
    let CallTarget::Module { module, .. } = lower_call_target(ctx, callee) else { return None };
    if module.as_str() != "testing" {
        return None;
    }
    let ast::ExprKind::Lambda { params, body } = &args[1].kind else { return None };
    if !params.is_empty() {
        return None;
    }
    let is_effect = matches!(ctx.expr_ty(&args[1]), Ty::Fn { is_effect: true, .. });
    let expected = lower_expr(ctx, &args[0]);
    ctx.in_test = false;
    ctx.push_scope();
    let site_body = lower_expr(ctx, body);
    ctx.pop_scope();
    ctx.in_test = true;
    let site = ctx.abort_sites.len();
    ctx.abort_sites.push(is_effect);
    // The closure's value is discarded: the site fn is `-> Unit` and its body
    // a block that runs the closure body as a statement.
    let stmt = IrStmt { kind: IrStmtKind::Expr { expr: site_body }, span: None };
    let site_body = ctx.mk(IrExprKind::Block { stmts: vec![stmt], expr: None }, Ty::Unit, span);
    ctx.synthesized_fns.push(abort_site_fn(sym(&almide_ir::abort_site_fn_name(site)), vec![], site_body, is_effect));
    let k = ctx.mk(IrExprKind::LitInt { value: site as i64 }, Ty::Int, span);
    let run = testing_call(ctx, "__abort_site_run", vec![k], Ty::list(Ty::String), span);
    let line = ctx.mk(IrExprKind::LitInt { value: span.map_or(0, |s| s.line as i64) }, Ty::Int, span);
    Some(testing_call(ctx, "__assert_aborts_check", vec![expected, run, line], Ty::Unit, span))
}

/// A call of the bundled `testing` module's fn `func`.
fn testing_call(ctx: &mut LowerCtx, func: &str, args: Vec<IrExpr>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let def_id = ctx.def_map.get(&sym(&format!("testing.{func}"))).copied();
    let target = CallTarget::Module { module: sym("testing"), func: sym(func), def_id };
    ctx.mk(IrExprKind::Call { target, args, type_args: vec![] }, ty, span)
}

/// A synthesized zero-or-one-param Unit fn of the abort-site protocol. Public:
/// nothing in the program calls the dispatcher but a leg's child entry, so it
/// is a root of every reachability prune.
fn abort_site_fn(name: Sym, params: Vec<IrParam>, body: IrExpr, is_effect: bool) -> IrFunction {
    IrFunction {
        name,
        params,
        ret_ty: Ty::Unit,
        body,
        is_effect,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: vec![], // fresh-fn: a lifted closure (no params) or the dispatcher's by-value Int
    }
}

/// `effect fn __almd_abort_dispatch(site: Int) -> Unit` over the program's
/// lifted sites: `if site == 0 then site_0() else if site == 1 then … else ()`.
/// None when the program has no `assert_aborts` call.
pub(super) fn abort_site_dispatcher(ctx: &mut LowerCtx) -> Option<IrFunction> {
    if ctx.abort_sites.is_empty() {
        return None;
    }
    let param = ctx.var_table.alloc(sym("site"), Ty::Int, Mutability::Let, None);
    let unit = |ctx: &mut LowerCtx| ctx.mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit, None);
    let mut chain = unit(ctx);
    for (k, is_effect) in ctx.abort_sites.clone().into_iter().enumerate().rev() {
        let call_ty = if is_effect { Ty::result(Ty::Unit, Ty::String) } else { Ty::Unit };
        let target = CallTarget::Named { name: sym(&almide_ir::abort_site_fn_name(k)) };
        let mut call = ctx.mk(IrExprKind::Call { target, args: vec![], type_args: vec![] }, call_ty, None);
        if is_effect {
            call = ctx.mk(IrExprKind::Try { expr: Box::new(call) }, Ty::Unit, None);
        }
        let stmt = IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None };
        let run = ctx.mk(IrExprKind::Block { stmts: vec![stmt], expr: None }, Ty::Unit, None);
        let site = ctx.mk(IrExprKind::Var { id: param }, Ty::Int, None);
        let lit = ctx.mk(IrExprKind::LitInt { value: k as i64 }, Ty::Int, None);
        let cond = ctx.mk(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(site), right: Box::new(lit) }, Ty::Bool, None);
        chain = ctx.mk(IrExprKind::If { cond: Box::new(cond), then: Box::new(run), else_: Box::new(chain) }, Ty::Unit, None);
    }
    let param = IrParam {
        var: param,
        ty: Ty::Int,
        name: sym("site"),
        borrow: ParamBorrow::Own,
        is_mut: false,
        open_record: None,
        default: None,
        attrs: vec![],
    };
    Some(abort_site_fn(sym(almide_ir::ABORT_DISPATCH_FN), vec![param], chain, true))
}
