// ── Call-target resolution ──────────────────────────────────────
//
// Deciding WHICH function a call names: UFCS receivers, cross-module
// references, convention methods, protocol type vars, and the record-field
// closure case. Split out of `calls.rs`, which lowers the call itself; both
// halves share that file's imports via `include!`.

pub(super) fn lower_call_target(ctx: &mut LowerCtx, callee: &ast::Expr) -> CallTarget {
    match &callee.kind {
        ast::ExprKind::Ident { name, .. } | ast::ExprKind::TypeName { name, .. } => {
            // A name that resolves to a local binding is called *through that
            // variable* (Computed), never as a top-level function — a local
            // shadows any same-named fn, and Computed makes use-count / Perceus
            // liveness count the call as a use of the variable.
            //
            // Callability is decided from the callee's use-site type, which the
            // checker has already auto-`?`-unwrapped to the function type. The
            // var's *stored* type can still lag at `Result[Fn, _]` here: in an
            // effect fn the auto-`?` rewrite that unwraps the binding (auto_try)
            // runs AFTER lowering, so `var_table[add5].ty` is `Result[Fn, _]`
            // at this point. Reading only the stored type would mis-resolve
            // `add5(10)` to `Named { add5 }`, which has no such function — the
            // WASM emit then traps on an unresolved call and Perceus, seeing no
            // use of the binding, frees the closure before the call (use-after-free).
            // The Result-stripped stored type is a final fallback.
            if let Some(var_id) = ctx.lookup_var(name) {
                let use_ty = ctx.expr_ty(callee);
                let stored = ctx.var_table.get(var_id).ty.clone();
                if matches!(use_ty, Ty::Fn { .. })
                    || matches!(stored, Ty::Fn { .. })
                    || matches!(strip_result_ok(&stored), Ty::Fn { .. })
                {
                    let callee_ty = if matches!(use_ty, Ty::Fn { .. }) {
                        use_ty
                    } else {
                        strip_result_ok(&stored)
                    };
                    return CallTarget::Computed {
                        callee: Box::new(ctx.mk(IrExprKind::Var { id: var_id }, callee_ty, callee.span)),
                    };
                }
            }
            // Selective import: bare `from_string` → Module { json, from_string }.
            // (used-mark happens in checker pass; lowering only rewrites.)
            if let Some(module) = ctx.env.import_table.direct.get(name).copied() {
                return CallTarget::Module { module, func: *name, def_id: ctx.def_map.get(&sym(&format!("{}.{}", module, name))).copied() };
            }
            CallTarget::Named { name: *name }
        }
        ast::ExprKind::Member { object, field, .. } => lower_call_target_member(ctx, callee, object, field),
        _ => {
            let ir_callee = lower_expr(ctx, callee);
            CallTarget::Computed { callee: Box::new(ir_callee) }
        }
    }
}

/// The `Member { object, field }` arm of [`lower_call_target`] — resolves
/// `object.field(...)` to a module call, UFCS method, convention method,
/// protocol dispatch, or cross-module UFCS. Each check below is an
/// independent guard that either resolves a `CallTarget` or falls through to
/// the next, with no state shared across checks (mirrors
/// `resolve_static_member`'s guard-chain shape) — split into one helper per
/// guard so each stays independently readable.
fn lower_call_target_member(ctx: &mut LowerCtx, callee: &ast::Expr, object: &ast::Expr, field: &Sym) -> CallTarget {
    if let Some(t) = lower_call_target_cross_module_type(ctx, object, field) {
        return t;
    }
    if let Some(t) = lower_call_target_module_call(ctx, object, field) {
        return t;
    }
    // Dot-chain submodule fallback: still resolve so codegen doesn't break
    // (checker emits error for these, but lowering must still produce valid IR)
    if let Some(dotted) = ctx.env.import_table.resolve_dotted_path(&object.kind) {
        return CallTarget::Module { module: sym(&dotted), func: *field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", dotted, field))).copied() };
    }
    if let Some(t) = lower_call_target_typename(ctx, object, field) {
        return t;
    }
    // Record field call: h.run("hello") where run is a Fn-typed field
    // Must check before UFCS so field-access + call takes priority
    let obj_ty = ctx.expr_ty(object);
    if let Some(t) = lower_call_target_record_field(ctx, callee, object, field, &obj_ty) {
        return t;
    }
    if let Some(t) = lower_call_target_builtin_module(ctx, object, field, &obj_ty) {
        return t;
    }
    if let Some(t) = lower_call_target_convention_method(ctx, object, field, &obj_ty) {
        return t;
    }
    if let Some(t) = lower_call_target_protocol_typevar(ctx, object, field, &obj_ty) {
        return t;
    }
    if let Some(t) = lower_call_target_cross_module_ufcs(ctx, object, field, &obj_ty) {
        return t;
    }
    // Generic method call: obj.method(args) → UFCS
    let ir_obj = lower_expr(ctx, object);
    CallTarget::Method { object: Box::new(ir_obj), method: *field }
}

/// `module.Type.method(...)` — a cross-module type's convention/Codec
/// method (`shapes.Dot.encode`). Resolve to the bare `Type.method` Named
/// call; the module prefix is reattached at codegen (#411-B). Mirrors the
/// checker's `resolve_static_member` (新①).
fn lower_call_target_cross_module_type(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym) -> Option<CallTarget> {
    if let ast::ExprKind::Member { object: inner, field: type_name } = &object.kind {
        if let ast::ExprKind::Ident { name: module, .. } = &inner.kind {
            if ctx.lookup_var(module).is_none()
                && ctx.env.import_table.resolve(module).is_some()
            {
                let key = sym(&format!("{}.{}", type_name, field));
                if ctx.env.functions.contains_key(&key) {
                    return Some(CallTarget::Named { name: key });
                }
            }
        }
    }
    None
}

/// Module call (`string.trim`, `list.map`) and `Type.method` on a bare
/// ident (protocol impl, e.g. `Val.double`).
fn lower_call_target_module_call(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym) -> Option<CallTarget> {
    if let ast::ExprKind::Ident { name: module, .. } = &object.kind {
        // Local variables take precedence over module names
        if ctx.lookup_var(module).is_none() && (module == "fan"
            || crate::stdlib::is_stdlib_module(module) || crate::stdlib::is_any_stdlib(module)
            || ctx.env.user_modules.contains(module)
            || ctx.env.import_table.aliases.contains_key(module))
        {
            // Cross-module variant constructor call: binary.ImportFunc(0).
            // Owner-filtered (#1426), mirroring the checker exactly.
            let resolved = ctx.env.import_table.aliases.get(module).copied()
                .unwrap_or(*module);
            if let Some((type_name, _)) = ctx.env.lookup_ctor_owned(field, resolved.as_str()) {
                let qualified = format!("{}.{}", resolved.as_str(), type_name.as_str());
                if ctx.env.types.contains_key(&sym(&qualified)) {
                    return Some(CallTarget::Named { name: *field });
                }
            }
            let resolved = ctx.env.import_table.aliases.get(module).copied()
                .unwrap_or(*module);
            return Some(CallTarget::Module { module: resolved, func: *field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", resolved, field))).copied() });
        }
        // Ident that's not a module: check if Type.method (protocol impl, e.g. Val.double)
        if ctx.lookup_var(module).is_none() {
            let key = format!("{}.{}", module, field);
            if ctx.env.functions.contains_key(&sym(&key))
                || ctx.find_convention_fn(&Ty::Named(sym(module), vec![]), field).is_some()
            {
                return Some(CallTarget::Named { name: sym(&key) });
            }
        }
    }
    None
}

/// `TypeName.method(args)` → direct named call (not UFCS, no object prepend).
fn lower_call_target_typename(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym) -> Option<CallTarget> {
    if let ast::ExprKind::TypeName { name: type_name, .. } = &object.kind {
        let key = format!("{}.{}", type_name, field);
        if ctx.env.functions.contains_key(&sym(&key))
            || ctx.find_convention_fn(&Ty::Named(sym(type_name), vec![]), field).is_some()
        {
            return Some(CallTarget::Named { name: sym(&key) });
        }
    }
    None
}

/// Record field call: `h.run("hello")` where `run` is a Fn-typed field.
fn lower_call_target_record_field(ctx: &mut LowerCtx, callee: &ast::Expr, object: &ast::Expr, field: &Sym, obj_ty: &Ty) -> Option<CallTarget> {
    let resolved = ctx.env.resolve_named(obj_ty);
    let fn_field = match &resolved {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().find(|(n, _)| *n == *field)
                .and_then(|(_, t)| if matches!(t, Ty::Fn { .. }) { Some(()) } else { None })
        }
        _ => None,
    };
    if fn_field.is_some() {
        let ir_obj = lower_expr(ctx, object);
        let field_ty = ctx.expr_ty(callee);
        let member = ctx.mk(IrExprKind::Member { object: Box::new(ir_obj), field: *field }, field_ty, callee.span);
        return Some(CallTarget::Computed { callee: Box::new(member) });
    }
    None
}

/// Built-in generic types: `xs.len()` → `list.len(xs)` for List, Map, etc.
fn lower_call_target_builtin_module(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym, obj_ty: &Ty) -> Option<CallTarget> {
    let builtin_module = builtin_module_for(obj_ty);
    if let Some(module) = builtin_module {
        let key = format!("{}.{}", module, field);
        if ctx.env.functions.contains_key(&sym(&key))
            || crate::stdlib::resolve_ufcs_candidates(field).contains(&module)
        {
            let ir_obj = lower_expr(ctx, object);
            return Some(CallTarget::Method { object: Box::new(ir_obj), method: sym(&key) });
        }
    }
    None
}

/// The stdlib module that owns UFCS methods for a receiver type.
///
/// `x.len()` resolves to `list.len` or `string.len` by this table alone — the
/// receiver's type is the whole decision, so it is written as data rather than
/// threaded through the call-target resolver.
fn builtin_module_for(obj_ty: &Ty) -> Option<&'static str> {
    builtin_module_core(obj_ty).or_else(|| builtin_module_sized(obj_ty))
}

/// The core container and scalar receivers.
///
/// One group of the receiver-type table, arms verbatim and in source order.
/// `None` means "not my group"; `builtin_module_for` tries the groups in that
/// order, so which module a receiver resolves to is unchanged.
fn builtin_module_core(obj_ty: &Ty) -> Option<&'static str> {
    match obj_ty {
    Ty::Applied(TypeConstructorId::List, _) => Some("list"),
    Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
    Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
    Ty::String => Some("string"),
    Ty::Int => Some("int"),
    Ty::Float => Some("float"),
    // Sized numeric types (Stage 3 of the sized-numeric-types arc).
    Ty::Int8 => Some("int8"),
    Ty::Int16 => Some("int16"),
        _ => None,
    }
}

/// The sized numeric receivers and the `Result` / `Option` wrappers.
///
/// One group of the receiver-type table, arms verbatim and in source order.
/// `None` means "not my group"; `builtin_module_for` tries the groups in that
/// order, so which module a receiver resolves to is unchanged.
fn builtin_module_sized(obj_ty: &Ty) -> Option<&'static str> {
    match obj_ty {
    Ty::Int32 => Some("int32"),
    Ty::Int64 => Some("int64"),
    Ty::UInt8 => Some("uint8"),
    Ty::UInt16 => Some("uint16"),
    Ty::UInt32 => Some("uint32"),
    Ty::UInt64 => Some("uint64"),
    Ty::Float32 => Some("float32"),
    Ty::Float64 => Some("float64"),
    Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
    Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        _ => None,
    }
}

/// Convention method: `dog.repr()` → `Dog.repr(dog)`.
fn lower_call_target_convention_method(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym, obj_ty: &Ty) -> Option<CallTarget> {
    let type_name_opt = match obj_ty {
        Ty::Named(name, _) => Some(name.to_string()),
        Ty::Record { .. } | Ty::Variant { .. } => {
            ctx.env.types.iter().find_map(|(name, ty)| {
                if ty == obj_ty && name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    Some(name.to_string())
                } else { None }
            })
        }
        _ => None,
    };
    if let Some(type_name) = type_name_opt {
        // Emit the key that EXISTS, not a key built from the receiver's type
        // name. A derived method is registered (and defined) under the bare
        // `P.encode`; constructing `lib.P.encode` here produced a call the IR
        // verifier could not resolve (#1087).
        let resolved = crate::canonicalize::registration::convention_emit_key(ctx.env, &type_name, field)
            .or_else(|| ctx.find_convention_fn(&Ty::Named(sym(&type_name), vec![]), field));
        if let Some(key) = resolved {
            let ir_obj = lower_expr(ctx, object);
            return Some(CallTarget::Method { object: Box::new(ir_obj), method: key });
        }
    }
    None
}

/// Identify which TypeVar `field` might be a protocol method of: either
/// `obj_ty` IS a bare TypeVar, or (for a TypeVar hidden behind an Fn
/// wrapper — see [`lower_call_target_protocol_typevar`]) scan all protocol
/// bounds in scope for one whose protocol declares `field` as a method.
/// Verbatim text move out of [`lower_call_target_protocol_typevar`].
fn lower_call_target_typevar_for_field(ctx: &mut LowerCtx, field: &Sym, obj_ty: &Ty) -> Option<Sym> {
    if let Ty::TypeVar(tv) = obj_ty {
        return Some(*tv);
    }
    // Check all protocol bounds to see if this method belongs to one,
    // and identify which TypeVar it corresponds to.
    for (tv, protos) in ctx.protocol_bounds.iter() {
        for proto_name in protos {
            if let Some(proto_def) = ctx.env.protocols.get(&sym(proto_name)) {
                if proto_def.methods.iter().any(|m| m.name == *field) {
                    return Some(*tv);
                }
            }
        }
    }
    None
}

/// Protocol method on TypeVar: `item.show()` where `item: T, T: Showable`.
/// Lowered as a `T.show` convention key — the monomorphizer substitutes
/// `T` → concrete type. Also checks for a TypeVar behind an Fn wrapper:
/// inside lambdas, the type checker may assign Fn type to the parameter
/// (partial application of a protocol method), but the generic function's
/// param list may still carry the real TypeVar.
fn lower_call_target_protocol_typevar(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym, obj_ty: &Ty) -> Option<CallTarget> {
    let tv_from_obj = lower_call_target_typevar_for_field(ctx, field, obj_ty)?;
    if let Some(proto_names) = ctx.protocol_bounds.get(&tv_from_obj).cloned() {
        for proto_name in &proto_names {
            if let Some(proto_def) = ctx.env.protocols.get(&sym(proto_name)) {
                if proto_def.methods.iter().any(|m| m.name == *field) {
                    let ir_obj = lower_expr(ctx, object);
                    let convention_key = sym(&format!("{}.{}", tv_from_obj, field));
                    return Some(CallTarget::Method { object: Box::new(ir_obj), method: convention_key });
                }
            }
        }
    }
    None
}

/// Cross-module UFCS: object type is `Named` → find the defining module.
fn lower_call_target_cross_module_ufcs(ctx: &mut LowerCtx, object: &ast::Expr, field: &Sym, obj_ty: &Ty) -> Option<CallTarget> {
    if let Ty::Named(type_name, _) = obj_ty {
        // A pinned QUALIFIED name (`box.Box`) carries its defining
        // module directly (same repair as the checker's UFCS arm —
        // the suffix scan only matched historical bare names).
        let defining_module = match type_name.as_str().rsplit_once('.') {
            Some((m, _)) => Some(m.to_string()),
            None => {
                // Hoisted out of the `find` closure — the old code rebuilt
                // the `.<type>` needle for every key in `env.types`.
                let needle = format!(".{}", type_name.as_str());
                ctx.env.types.keys()
                    .find(|k| {
                        let s = k.as_str();
                        s.ends_with(&needle) && s.len() > type_name.as_str().len() + 1
                    })
                    .map(|k| k.as_str()[..k.as_str().len() - type_name.as_str().len() - 1].to_string())
            }
        };
        if let Some(module) = defining_module {
            let key = format!("{}.{}", module, field);
            if ctx.env.functions.contains_key(&sym(&key)) {
                let ir_obj = lower_expr(ctx, object);
                // Return Method with "module.func" key — lower_call converts to Module target
                return Some(CallTarget::Method { object: Box::new(ir_obj), method: sym(&key) });
            }
        }
    }
    None
}

/// Replace each `Ident { name }` that names a call parameter with the AST of the
/// value bound to that parameter, used to fill a default value that references an
/// earlier parameter (`fn rect(w, h: Int = w)`) with the actual argument instead
/// of the callee-local name, which is out of scope at the call site (E0425) (#664).
/// A self-referential argument (`rect(w)` passing a caller-local `w` for param
/// `w`) is left untouched: that name already resolves correctly at the call site,
/// and replacing it would re-enter this pre-order visitor forever.
/// Qualify a default expression's bare identifiers against the module that
/// DECLARED it, before lowering it at a call site in another module.
///
/// A default is written in the callee's scope (`greeting: String = GREETING`)
/// but lowered in the caller's, where that name does not exist. Lowering then
/// bound it to whatever global happened to be first in the CALLER — a silently
/// wrong value that only rustc's type check caught, and only when the types
/// differed. Rewriting `GREETING` to `lib.GREETING` sends it through the
/// ordinary cross-module top-let path instead (#1088).
///
/// Only names the callee module actually declares are touched: a default
/// referencing an earlier PARAMETER is already substituted by
/// `substitute_call_params`, and must keep resolving to the caller's argument.
pub(super) fn qualify_callee_module_idents(expr: &mut ast::Expr, module: Sym, env: &crate::types::TypeEnv) {
    ast::visit_expr_mut(expr, &mut |e| {
        // A SCREAMING_CASE constant lexes as a `TypeName`, not an `Ident`, so
        // both spellings have to be considered or the common shape — a
        // module-level constant as the default — is the one that slips through.
        let name = match &e.kind {
            ast::ExprKind::Ident { name } | ast::ExprKind::TypeName { name } => *name,
            _ => return,
        };
        if !env.top_lets.contains_key(&sym(&format!("{}.{}", module, name))) {
            return;
        }
        let obj = ast::Expr::new(e.id, e.span, ast::ExprKind::Ident { name: module });
        e.kind = ast::ExprKind::Member { object: Box::new(obj), field: name };
    });
}

fn substitute_call_params(expr: &mut ast::Expr, param_values: &std::collections::HashMap<Sym, ast::Expr>) {
    ast::visit_expr_mut(expr, &mut |e| {
        if let ast::ExprKind::Ident { name } = &e.kind {
            if let Some(repl) = param_values.get(name) {
                if !matches!(&repl.kind, ast::ExprKind::Ident { name: rn } if rn == name) {
                    *e = repl.clone();
                }
            }
        }
    });
}
