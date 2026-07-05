
/// Check if a Call expression's callee matches a path (e.g. ["http","get"] or ["double"]).
fn call_matches_path(expr: &ast::Expr, path: &[Sym]) -> bool {
    let ast::ExprKind::Call { callee, .. } = &expr.kind else { return false };
    match path.len() {
        1 => matches!(&callee.kind, ast::ExprKind::Ident { name } if *name == path[0]),
        2 => matches!(&callee.kind, ast::ExprKind::Member { object, field, .. }
            if matches!(&object.kind, ast::ExprKind::Ident { name } if *name == path[0]) && *field == path[1]),
        _ => false,
    }
}

/// Replace the callee of a Call expression with an override variable.
fn rewrite_call_callee(expr: &mut ast::Expr, override_name: &str) {
    if let ast::ExprKind::Call { args, named_args, .. } = &mut expr.kind {
        let new_callee = ast::Expr::new(ast::ExprId(0), None, ast::ExprKind::Ident { name: sym(override_name) });
        expr.kind = ast::ExprKind::Call {
            callee: Box::new(new_callee),
            args: std::mem::take(args),
            named_args: std::mem::take(named_args),
            type_args: None,
        };
    }
}

/// Rewrite calls in an AST expression: replace matching calls with override var.
fn rewrite_calls_in_expr(expr: &mut ast::Expr, path: &[Sym], override_name: &str) {
    if call_matches_path(expr, path) {
        rewrite_call_callee(expr, override_name);
        return;
    }
    ast::visit_expr_mut(expr, &mut |e| {
        if call_matches_path(e, path) { rewrite_call_callee(e, override_name); }
    });
}

fn lower_where_bind(ctx: &mut LowerCtx, bind_name: &Sym, value: &ast::Expr) -> IrStmt {
    // The checker pins this binding's lambda param types during inference
    // (`unify_where_override_with_fn_sig` in check/mod.rs), so `lower_expr`
    // reads correct types straight from the TypeMap — no lowering-side patch.
    let ir_val = lower_expr(ctx, value);
    let ty = ir_val.ty.clone();
    let var = ctx.define_var(bind_name.as_str(), ty.clone(), Mutability::Let, None);
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: ir_val }, span: None }
}

fn lower_where_override(ctx: &mut LowerCtx, path: &[Sym], value: &ast::Expr, stmts: &mut Vec<IrStmt>, overrides: &mut Vec<(Vec<Sym>, String)>) {
    // Param types already resolved by the checker (see lower_where_bind).
    let override_name = where_override_name(path);
    let ir_val = lower_expr(ctx, value);
    let ty = ir_val.ty.clone();
    let var = ctx.define_var(&override_name, ty.clone(), Mutability::Let, None);
    stmts.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: ir_val }, span: None });
    overrides.push((path.to_vec(), override_name));
}

fn lower_where_call_response(ctx: &mut LowerCtx, target: &[Sym], params: &[ast::Pattern], response: &ast::Expr, stmts: &mut Vec<IrStmt>, overrides: &mut Vec<(Vec<Sym>, String)>) {
    let override_name = where_override_name(target);
    let lambda_params: Vec<ast::LambdaParam> = params.iter().enumerate().map(|(i, pat)| {
        let pname = match pat {
            ast::Pattern::Ident { name } => name.as_str().to_string(),
            _ => format!("_arg{}", i),
        };
        ast::LambdaParam { name: sym(&pname), tuple_names: None, ty: None }
    }).collect();
    let lambda = ast::Expr::new(ast::ExprId(0), None, ast::ExprKind::Lambda {
        params: lambda_params, body: Box::new(response.clone()),
    });
    let mut ir_val = lower_expr(ctx, &lambda);
    // Resolve param types from the original function's signature and patch
    // the lambda's IR param VarIds so WASM codegen gets correct types.
    let original_fn_ty = resolve_target_fn_type(ctx, target);
    let sig_param_tys: Vec<Ty> = match &original_fn_ty {
        Some(Ty::Fn { params: ptys, .. }) => ptys.iter().map(erase_typevars).collect(),
        _ => params.iter().map(|_| Ty::Unknown).collect(),
    };
    let sig_ret_ty = match &original_fn_ty {
        Some(Ty::Fn { ret, .. }) => erase_typevars(ret),
        _ => Ty::Unknown,
    };
    // Patch lambda IR: update param VarIds in var_table with concrete types
    if let IrExprKind::Lambda { params: ir_params, body, .. } = &mut ir_val.kind {
        for (i, (var_id, var_ty)) in ir_params.iter_mut().enumerate() {
            if let Some(concrete) = sig_param_tys.get(i) {
                if !matches!(concrete, Ty::Unknown) {
                    *var_ty = concrete.clone();
                    ctx.var_table.entries[var_id.0 as usize].ty = concrete.clone();
                }
            }
        }
        // Patch body return type if it's Unknown
        if matches!(body.ty, Ty::Unknown) && !matches!(sig_ret_ty, Ty::Unknown) {
            body.ty = sig_ret_ty.clone();
        }
    }
    let ty = Ty::Fn { params: sig_param_tys, ret: Box::new(sig_ret_ty) };
    ir_val.ty = ty.clone();
    let var = ctx.define_var(&override_name, ty.clone(), Mutability::Let, None);
    ctx.var_table.entries[var.0 as usize].ty = ty.clone();
    stmts.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: ir_val }, span: None });
    overrides.push((target.to_vec(), override_name));
}

/// Look up the Fn type of a target path (e.g. ["double"] or ["http","get"]) from the environment.
fn resolve_target_fn_type(ctx: &LowerCtx, target: &[Sym]) -> Option<Ty> {
    let name = if target.len() == 1 {
        target[0]
    } else {
        sym(&target.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
    };
    // Check local scope first (function defined in same file)
    if let Some(var_id) = ctx.lookup_var(name.as_str()) {
        let ty = &ctx.var_table.get(var_id).ty;
        if matches!(ty, Ty::Fn { .. }) { return Some(ty.clone()); }
    }
    // Check environment functions
    if let Some(sig) = ctx.env.functions.get(&name) {
        return Some(Ty::Fn { params: sig.params.iter().map(|(_, t)| t.clone()).collect(), ret: Box::new(sig.ret.clone()) });
    }
    // For module.func, check module functions
    if target.len() == 2 {
        let qual = sym(&format!("{}.{}", target[0], target[1]));
        if let Some(sig) = ctx.env.functions.get(&qual) {
            return Some(Ty::Fn { params: sig.params.iter().map(|(_, t)| t.clone()).collect(), ret: Box::new(sig.ret.clone()) });
        }
    }
    None
}

/// Erase TypeVars from a type — replace with Unknown so Rust emits `_` instead of `A`.
fn erase_typevars(ty: &Ty) -> Ty {
    match ty {
        Ty::TypeVar(_) => Ty::Unknown,
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(erase_typevars).collect(),
            ret: Box::new(erase_typevars(ret)),
        },
        Ty::Applied(tc, args) => Ty::Applied(tc.clone(), args.iter().map(erase_typevars).collect()),
        other => other.clone(),
    }
}

fn where_override_name(path: &[Sym]) -> String {
    format!("__where_{}", path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("_"))
}

fn lower_test(ctx: &mut LowerCtx, name: &str, body: &ast::Expr) -> IrFunction {
    lower_test_with_where(ctx, name, body, &[])
}

fn lower_test_with_where(ctx: &mut LowerCtx, name: &str, body: &ast::Expr, where_clauses: &[ast::TestWhere]) -> IrFunction {
    ctx.push_scope();
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut overrides: Vec<(Vec<Sym>, String)> = Vec::new();
    for wc in where_clauses {
        match wc {
            ast::TestWhere::Bind { name: bind_name, value } =>
                stmts.push(lower_where_bind(ctx, bind_name, value)),
            ast::TestWhere::Override { path, value } =>
                lower_where_override(ctx, path, value, &mut stmts, &mut overrides),
            ast::TestWhere::CallResponse { target, params, response } =>
                lower_where_call_response(ctx, target, params, response, &mut stmts, &mut overrides),
            ast::TestWhere::Case { .. } => {}
        }
    }
    // Rewrite test body AST: replace overridden calls
    let mut body_rewritten = body.clone();
    for (path, override_name) in &overrides {
        rewrite_calls_in_expr(&mut body_rewritten, path, override_name);
    }
    let ir_body = lower_expr(ctx, &body_rewritten);
    let final_body = if stmts.is_empty() {
        ir_body
    } else {
        // Wrap: { let bindings...; body }
        let ty = ir_body.ty.clone();
        let span = ir_body.span;
        IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(ir_body)) }, ty, span, def_id: None }
    };
    ctx.pop_scope();
    IrFunction {
        name: sym(&format!("{}{}", almide_ir::TEST_NAME_PREFIX, name)),
        params: vec![], ret_ty: Ty::Unit, body: final_body,
        is_effect: true, is_async: false, is_test: true,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}
