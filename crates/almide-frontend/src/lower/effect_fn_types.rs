// The effect-fn carrier normalization (#1055). `include!`d by lower/mod.rs (the
// 800-line file budget); it shares that module's scope and imports.

/// #1055: rewrite every `effect (A) -> B` fn TYPE in the IR to its runtime
/// carrier `(A) -> Result[B, String]`. The checker keeps the effect form for
/// its diagnostics; downstream (v0 codegen, the v1 renders, almide-interp)
/// then see EXACTLY the shape the landed D3 fallible-slot machinery already
/// handles — no backend learns a new type. Recursive, so a nested effect fn
/// type inside a container or another fn type normalizes too.
fn normalize_effect_fn_types(program: &mut IrProgram) {
    use almide_ir::visit_mut::IrMutVisitor;
    let mut v = EffectFnNorm;
    for f in program.functions.iter_mut().chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut())) {
        for p in f.params.iter_mut() {
            norm_effect_fn_in_place(&mut p.ty);
        }
        norm_effect_fn_in_place(&mut f.ret_ty);
        v.visit_expr_mut(&mut f.body);
    }
    // #2588: a top-level `let app = http.router([...])` and a record field
    // holding a handler (`Route.handler`) carry the same effect fn type —
    // left in effect form, the native static and the struct field rendered
    // `dyn Fn(A) -> B` while every value flowing in was the carrier.
    for tl in program.top_lets.iter_mut() {
        norm_effect_fn_in_place(&mut tl.ty);
        v.visit_expr_mut(&mut tl.value);
    }
    for td in program.type_decls.iter_mut() {
        norm_type_decl_effect_fns(&mut td.kind);
    }
    for entry in program.var_table.entries.iter_mut() {
        norm_effect_fn_in_place(&mut entry.ty);
    }
}

/// `ty` with every effect fn type (at any depth) in its carrier form.
fn norm_effect_fn(ty: &Ty) -> Ty {
    use almide_lang::types::constructor::TypeConstructorId;
    let mapped = ty.map_children(&mut |c: &Ty| norm_effect_fn(c));
    match mapped {
        Ty::Fn { params, ret, is_effect: true } => Ty::Fn {
            params,
            ret: Box::new(Ty::Applied(TypeConstructorId::Result, vec![*ret, Ty::String])),
            is_effect: false,
        },
        other => other,
    }
}

fn has_effect_fn(ty: &Ty) -> bool {
    if let Ty::Fn { is_effect: true, .. } = ty {
        return true;
    }
    ty.children().into_iter().any(has_effect_fn)
}

/// Normalize `ty` in place, leaving it untouched when it holds no effect fn.
fn norm_effect_fn_in_place(ty: &mut Ty) {
    if has_effect_fn(ty) {
        *ty = norm_effect_fn(ty);
    }
}

/// The field, payload and alias types of a type declaration.
fn norm_type_decl_effect_fns(kind: &mut almide_ir::IrTypeDeclKind) {
    let norm_fields = |fields: &mut [almide_ir::IrFieldDecl]| fields.iter_mut().for_each(|fd| norm_effect_fn_in_place(&mut fd.ty));
    match kind {
        almide_ir::IrTypeDeclKind::Record { fields } => norm_fields(fields),
        almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
            for c in cases.iter_mut() {
                match &mut c.kind {
                    almide_ir::IrVariantKind::Tuple { fields } => fields.iter_mut().for_each(norm_effect_fn_in_place),
                    almide_ir::IrVariantKind::Record { fields } => norm_fields(fields),
                    almide_ir::IrVariantKind::Unit => {}
                }
            }
        }
        almide_ir::IrTypeDeclKind::Alias { target } => norm_effect_fn_in_place(target),
    }
}

/// The expression-side walk: expression types, lambda param types, call
/// type arguments, `let` statement types and pattern binder types.
struct EffectFnNorm;

impl almide_ir::visit_mut::IrMutVisitor for EffectFnNorm {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        norm_effect_fn_in_place(&mut expr.ty);
        // A lambda carries its param types inline: an eta-expanded
        // middleware (`[server_header]`, a fn taking a handler) kept
        // `_fn_arg0: effect (A) -> B` and rendered the non-carrier shape.
        if let IrExprKind::Lambda { params, .. } = &mut expr.kind {
            params.iter_mut().for_each(|(_, pt)| norm_effect_fn_in_place(pt));
        }
        if let IrExprKind::Call { type_args, .. } = &mut expr.kind {
            type_args.iter_mut().for_each(norm_effect_fn_in_place);
        }
        almide_ir::visit_mut::walk_expr_mut(self, expr);
    }
    // #2664: a `let` carries its declared type on the STATEMENT, not on
    // an expression. `let h = mk("t:")` with `mk -> Handler` (an alias of
    // `effect (A) -> B`) kept the effect form there while the value was
    // the carrier, and the structural wasm leg interned two fn signatures
    // for one value (ty-mismatch:Fn).
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &mut stmt.kind {
            norm_effect_fn_in_place(ty);
        }
        almide_ir::visit_mut::walk_stmt_mut(self, stmt);
    }
    // A pattern binder names its type too (`some(h) => h(x)!` over a
    // `List[Handler]` element, a destructured handler field).
    fn visit_pattern_mut(&mut self, pat: &mut IrPattern) {
        if let IrPattern::Bind { ty, .. } | IrPattern::As { ty, .. } = pat {
            norm_effect_fn_in_place(ty);
        }
        almide_ir::visit_mut::walk_pattern_mut(self, pat);
    }
}
