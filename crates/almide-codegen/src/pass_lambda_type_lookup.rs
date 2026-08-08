// ─────────── type lookups the lambda-param resolution reads ───────────
//
// Split out of pass_lambda_type_resolve.rs at the call-target boundary
// (max-lines). A pure text move: this file is `include!`d back at the end of
// that one, so it shares its imports exactly as before. Everything here is a
// pure `&VarTable` query — nothing mutates IR.

/// Extract (module, method) from every call-target shape the
/// frontend / ResolveCalls / IntrinsicLowering produce:
///   - `Method { method }`                    — UFCS, unresolved module
///   - `Module { <mod>, func }`               — pre-ResolveCalls
///   - `Named { "almide_rt_<mod>_<func>" }`   — post-ResolveCalls
fn resolve_call_target_module_method(target: &CallTarget) -> Option<(Option<&str>, String)> {
    match target {
        CallTarget::Method { method, .. } => Some((None, method.as_str().to_string())),
        CallTarget::Module { module, func, .. } => {
            let m = module.as_str();
            if m == "list" || m == "option" || m == "result" {
                Some((Some(m), func.as_str().to_string()))
            } else {
                None
            }
        }
        CallTarget::Named { name } => {
            let s = name.as_str();
            if let Some(rest) = s.strip_prefix("almide_rt_list_") {
                Some((Some("list"), rest.to_string()))
            } else if let Some(rest) = s.strip_prefix("almide_rt_option_") {
                Some((Some("option"), rest.to_string()))
            } else if let Some(rest) = s.strip_prefix("almide_rt_result_") {
                Some((Some("result"), rest.to_string()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Decide (param-elem source, lambda-param indices) based on (module, method).
fn resolve_elem_source(module: Option<&str>, name: &str) -> Option<(ElemSource, &'static [usize])> {
    match module {
        Some("option") if OPTION_INNER_METHODS.iter().any(|m| *m == name) => Some((ElemSource::OptionInner, &[0])),
        Some("result") if RESULT_OK_METHODS.iter().any(|m| *m == name) => Some((ElemSource::ResultOk, &[0])),
        Some("result") if RESULT_ERR_METHODS.iter().any(|m| *m == name) => Some((ElemSource::ResultErr, &[0])),
        // list (or unresolved Method — fallback to list semantics, matching the original behavior)
        _ if LIST_ELEM_FIRST_METHODS.iter().any(|m| *m == name) => Some((ElemSource::ListElem, &[0])),
        _ if LIST_ELEM_SECOND_METHODS.iter().any(|m| *m == name) => Some((ElemSource::ListElem, &[1])),
        _ if LIST_ELEM_BOTH_METHODS.iter().any(|m| *m == name) => Some((ElemSource::ListElem, &[0, 1])),
        _ => None,
    }
}

/// Resolve the callback param type from the call's first arg.
fn resolve_call_elem_ty(source: &ElemSource, args: &[IrExpr], vt: &VarTable) -> Option<Ty> {
    let a = args.first()?;
    match source {
        ElemSource::ListElem    => resolve_list_elem_ty(a, vt),
        ElemSource::OptionInner => resolve_option_inner_ty(a, vt),
        ElemSource::ResultOk    => resolve_result_ok_ty(a, vt),
        ElemSource::ResultErr   => resolve_result_err_ty(a, vt),
    }
}

/// For `fold(xs, init, f)` and `scan`, the accumulator's type is whatever
/// `init` resolves to — propagated into lambda param 0 in addition to the
/// elem-type propagation.
fn resolve_fold_acc_ty(module: Option<&str>, name: &str, args: &[IrExpr], vt: &VarTable) -> Option<Ty> {
    if !(module == Some("list") && (name == "fold" || name == "scan")) {
        return None;
    }
    args.get(1).and_then(|a| {
        if !a.ty.has_unresolved_deep() {
            Some(a.ty.clone())
        } else if let IrExprKind::Var { id } = &a.kind {
            if (id.0 as usize) < vt.len() {
                let t = &vt.get(*id).ty;
                if !t.has_unresolved_deep() { Some(t.clone()) } else { None }
            } else { None }
        } else { None }
    })
}

/// Propagate the resolved elem/accumulator types into one Lambda argument's
/// params, Fn-type wrapper, and infer its return type from the body. A
/// no-op for non-Lambda args (the original loop's `continue` for those).
fn apply_lambda_param_types(
    arg: &mut IrExpr,
    elem_param_indices: &[usize],
    elem_ty: &Ty,
    acc_ty: &Option<Ty>,
    vt: &mut VarTable,
) {
    let IrExprKind::Lambda { params, body, .. } = &mut arg.kind else { return };
    apply_lambda_param_types_update_params(params, elem_param_indices, elem_ty, acc_ty, vt);
    // Infer return type from body + resolved params
    let body_ret = infer_body_result_ty(body, params);
    apply_lambda_fn_ty_wrapper(&mut arg.ty, elem_param_indices, elem_ty, acc_ty, body_ret);
}

/// First phase of `apply_lambda_param_types`: update the Lambda's own
/// param bindings (and their `VarTable` entries) — extracted verbatim
/// (cog>30 decomposition, sequential-phase pattern, no match statement so
/// no arm-count floor concern). Uses `has_unresolved_deep` (not
/// `is_unresolved_structural`) to catch `Applied(List, [TypeVar(A)])`.
fn apply_lambda_param_types_update_params(
    params: &mut [(VarId, Ty)],
    elem_param_indices: &[usize],
    elem_ty: &Ty,
    acc_ty: &Option<Ty>,
    vt: &mut VarTable,
) {
    // Update designated param(s).
    for &pidx in elem_param_indices {
        if let Some((vid, pty)) = params.get_mut(pidx) {
            if pty.has_unresolved_deep() {
                *pty = elem_ty.clone();
                if (vid.0 as usize) < vt.len() && vt.get(*vid).ty.has_unresolved_deep() {
                    vt.entries[vid.0 as usize].ty = elem_ty.clone();
                }
            }
        }
    }
    // For fold/scan, the accumulator (param 0) takes init's type.
    if let Some(a_ty) = acc_ty {
        if let Some((vid, pty)) = params.get_mut(0) {
            if pty.has_unresolved_deep() {
                *pty = a_ty.clone();
                if (vid.0 as usize) < vt.len() && vt.get(*vid).ty.has_unresolved_deep() {
                    vt.entries[vid.0 as usize].ty = a_ty.clone();
                }
            }
        }
    }
}

/// Second phase of `apply_lambda_param_types`: update the Lambda arg's own
/// `Ty::Fn` wrapper to match — extracted verbatim (cog>30 decomposition).
/// One-way dependency on phase 1 only through the already-computed
/// `body_ret` value, not through any shared mutable state.
fn apply_lambda_fn_ty_wrapper(
    arg_ty: &mut Ty,
    elem_param_indices: &[usize],
    elem_ty: &Ty,
    acc_ty: &Option<Ty>,
    body_ret: Option<Ty>,
) {
    let Ty::Fn { params: fparams, ret } = arg_ty else { return };
    for &pidx in elem_param_indices {
        if let Some(fp) = fparams.get_mut(pidx) {
            if fp.has_unresolved_deep() { *fp = elem_ty.clone(); }
        }
    }
    if let Some(a_ty) = acc_ty {
        if let Some(fp) = fparams.get_mut(0) {
            if fp.has_unresolved_deep() { *fp = a_ty.clone(); }
        }
        // The lambda's return is also the accumulator type.
        if ret.has_unresolved_deep() { **ret = a_ty.clone(); }
    }
    if ret.has_unresolved_deep() {
        if let Some(r) = body_ret { **ret = r; }
    }
}

fn resolve_call_lambdas(target: &CallTarget, args: &mut Vec<IrExpr>, vt: &mut VarTable) {
    let Some((module, name)) = resolve_call_target_module_method(target) else { return };
    // Monomorphization rewrites e.g. `fold` → `fold__String_CollapseAcc`.
    // Strip the `__suffix` so all the lookups below operate on the bare
    // method name.
    let bare_name = name.split("__").next().unwrap_or(&name).to_string();
    let name = bare_name.as_str();

    let Some((source, elem_param_indices)) = resolve_elem_source(module, name) else { return };
    let Some(elem_ty) = resolve_call_elem_ty(&source, args.as_slice(), vt) else { return };
    let acc_ty = resolve_fold_acc_ty(module, name, args.as_slice(), vt);

    // Propagate to inline Lambda params
    for arg in args.iter_mut() {
        apply_lambda_param_types(arg, elem_param_indices, &elem_ty, &acc_ty, vt);
    }
}

/// Resolve the inner type of an Option expression.
fn resolve_option_inner_ty(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    if let Some(inner) = extract_applied_arg(&expr.ty, 0) {
        if !inner.has_unresolved_deep() { return Some(inner); }
    }
    if let IrExprKind::Var { id } = &expr.kind {
        if (id.0 as usize) < vt.len() {
            if let Some(inner) = extract_applied_arg(&vt.get(*id).ty, 0) {
                if !inner.has_unresolved_deep() { return Some(inner); }
            }
        }
    }
    None
}

/// Resolve the OK type of a Result expression.
fn resolve_result_ok_ty(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    if let Some(inner) = extract_applied_arg(&expr.ty, 0) {
        if !inner.has_unresolved_deep() { return Some(inner); }
    }
    if let IrExprKind::Var { id } = &expr.kind {
        if (id.0 as usize) < vt.len() {
            if let Some(inner) = extract_applied_arg(&vt.get(*id).ty, 0) {
                if !inner.has_unresolved_deep() { return Some(inner); }
            }
        }
    }
    None
}

/// Resolve the ERR type of a Result expression.
fn resolve_result_err_ty(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    if let Some(inner) = extract_applied_arg(&expr.ty, 1) {
        if !inner.has_unresolved_deep() { return Some(inner); }
    }
    if let IrExprKind::Var { id } = &expr.kind {
        if (id.0 as usize) < vt.len() {
            if let Some(inner) = extract_applied_arg(&vt.get(*id).ty, 1) {
                if !inner.has_unresolved_deep() { return Some(inner); }
            }
        }
    }
    None
}

/// Extract the Nth type argument from a Ty::Applied (e.g. inner of Option/Result).
fn extract_applied_arg(ty: &Ty, idx: usize) -> Option<Ty> {
    if let Ty::Applied(_, args) = ty {
        args.get(idx).cloned()
    } else {
        None
    }
}

/// Update a Lambda expression's Ty::Fn wrapper to reflect resolved params.
fn refresh_lambda_fn_ty(expr: &mut IrExpr, _vt: &VarTable) {
    let IrExprKind::Lambda { params, body, .. } = &expr.kind else { return };
    let Ty::Fn { params: fparams, ret } = &expr.ty else { return };
    let (new_fparams, params_changed) = refresh_lambda_fn_ty_params(params, fparams);
    let (new_ret, ret_changed) = refresh_lambda_fn_ty_ret(ret, body, params);
    if params_changed || ret_changed {
        expr.ty = Ty::Fn { params: new_fparams, ret: new_ret };
    }
}

/// Param-types phase of `refresh_lambda_fn_ty`, extracted verbatim (cog>30
/// decomposition): copy each still-unresolved `Ty::Fn` param slot from the
/// Lambda's own (now-resolved) param type.
fn refresh_lambda_fn_ty_params(params: &[(VarId, Ty)], fparams: &[Ty]) -> (Vec<Ty>, bool) {
    let mut new_fparams = fparams.to_vec();
    let mut changed = false;
    for (i, (_, pty)) in params.iter().enumerate() {
        if let Some(fp) = new_fparams.get_mut(i) {
            if fp.has_unresolved_deep() && !pty.has_unresolved_deep() {
                *fp = pty.clone();
                changed = true;
            }
        }
    }
    (new_fparams, changed)
}

/// Return-type phase of `refresh_lambda_fn_ty`, extracted verbatim
/// (cog>30 decomposition): infer the return type from the body when the
/// `Ty::Fn` wrapper's `ret` is still unresolved.
fn refresh_lambda_fn_ty_ret(ret: &Ty, body: &IrExpr, params: &[(VarId, Ty)]) -> (Box<Ty>, bool) {
    if ret.has_unresolved_deep() {
        if let Some(r) = infer_body_result_ty(body, params) {
            return (Box::new(r), true);
        }
    }
    (Box::new(ret.clone()), false)
}

// ── List element type extraction ────────────────────────────────────

/// Resolve the element type of a list expression.
/// Checks: direct expr.ty → VarTable → list.zip inference.
/// Rejects types with deep unresolved components.
fn resolve_list_elem_ty(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    resolve_list_elem_ty_direct(expr)
        .or_else(|| resolve_list_elem_ty_var_table(expr, vt))
        .or_else(|| resolve_list_elem_ty_tuple_index(expr, vt))
        .or_else(|| resolve_list_elem_ty_zip(expr, vt))
}

/// Direct-type phase of `resolve_list_elem_ty`, extracted verbatim (cog>30
/// decomposition, pattern 1 — the four phases share no state and each
/// independently returns `Some`/`None`).
fn resolve_list_elem_ty_direct(expr: &IrExpr) -> Option<Ty> {
    let elem = extract_list_elem(&expr.ty)?;
    if elem.has_unresolved_deep() { return None; }
    Some(elem)
}

/// VarTable-lookup phase (for `Var`/`EnvLoad`) of `resolve_list_elem_ty`,
/// extracted verbatim (cog>30 decomposition).
fn resolve_list_elem_ty_var_table(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    let vid = match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::EnvLoad { env_var, .. } => Some(*env_var),
        _ => None,
    };
    let id = vid?;
    if !((id.0 as usize) < vt.len()) { return None; }
    let elem = extract_list_elem(&vt.get(id).ty)?;
    if elem.has_unresolved_deep() { return None; }
    Some(elem)
}

/// `TupleIndex` phase of `resolve_list_elem_ty`, extracted verbatim
/// (cog>30 decomposition): `pair.0` where `pair: Tuple([List[A], List[B]])`
/// → `List[A]`'s elem = `A`.
fn resolve_list_elem_ty_tuple_index(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    let IrExprKind::TupleIndex { object, index } = &expr.kind else { return None };
    let tuple_elem = resolve_tuple_elem_ty(object, *index, vt)?;
    let elem = extract_list_elem(&tuple_elem)?;
    if elem.has_unresolved_deep() { return None; }
    Some(elem)
}

/// `list.zip` phase of `resolve_list_elem_ty`, extracted verbatim (cog>30
/// decomposition): `list.zip(xs, ys)` → `Tuple(xs_elem, ys_elem)`. Matches
/// every call-target shape the frontend / ResolveCalls / IntrinsicLowering
/// produce for stdlib `list.zip`: pre-lowering `Module { list, zip }`,
/// frontend-mangled or post-ResolveCalls `Named { "almide_rt_list_zip" }`,
/// and post-IntrinsicLowering `RuntimeCall { symbol: "almide_rt_list_zip", .. }`.
fn resolve_list_elem_ty_zip(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    let zip_args: Option<&Vec<IrExpr>> = match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "zip" => Some(args),
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == "almide_rt_list_zip" => Some(args),
        IrExprKind::RuntimeCall { symbol, args }
            if symbol.as_str() == "almide_rt_list_zip" => Some(args),
        _ => None,
    };
    let args = zip_args?;
    if args.len() < 2 { return None; }
    let a = resolve_list_elem_ty(&args[0], vt);
    let b = resolve_list_elem_ty(&args[1], vt);
    if let (Some(a), Some(b)) = (a, b) {
        Some(Ty::Tuple(vec![a, b]))
    } else {
        None
    }
}

/// Extract element type from Applied(List, [elem]).
fn extract_list_elem(ty: &Ty) -> Option<Ty> {
    if let Ty::Applied(_, args) = ty {
        args.first().cloned()
    } else {
        None
    }
}

/// Resolve `object.index` type when object has Tuple type.
/// Used when the Var is a lambda parameter whose type is a Tuple.
fn resolve_tuple_elem_ty(object: &IrExpr, index: usize, vt: &VarTable) -> Option<Ty> {
    // Prefer VarTable for Var/EnvLoad (authoritative after resolution)
    let ty = match &object.kind {
        IrExprKind::Var { id } if (id.0 as usize) < vt.len() => &vt.get(*id).ty,
        IrExprKind::EnvLoad { env_var, .. } if (env_var.0 as usize) < vt.len() => {
            &vt.get(*env_var).ty
        }
        _ => &object.ty,
    };
    if let Ty::Tuple(elems) = ty {
        return elems.get(index).cloned();
    }
    None
}


// ── Body return type inference ──────────────────────────────────────

/// Infer a lambda body's return type using resolved parameter types.
/// For `(pair) => pair.0 + pair.1` where pair: (Float, Float),
/// TupleIndex(.0) resolves to Float via param types, so BinOp returns Float.
fn infer_body_result_ty(expr: &IrExpr, params: &[(VarId, Ty)]) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::BinOp { op, left, right } => {
            // Try resolving via tuple index on params
            let from_params = resolve_via_tuple_index(left, params)
                .or_else(|| resolve_via_tuple_index(right, params));
            if from_params.is_some() { return from_params; }
            // Fall back to op result type or operand types
            op.result_ty().or_else(|| {
                if !left.ty.is_unresolved() { Some(left.ty.clone()) }
                else if !right.ty.is_unresolved() { Some(right.ty.clone()) }
                else { None }
            })
        }
        IrExprKind::Block { expr: Some(tail), .. } => infer_body_result_ty(tail, params),
        IrExprKind::If { then, else_, .. } => {
            infer_body_result_ty(then, params)
                .filter(|t| !t.is_unresolved())
                .or_else(|| infer_body_result_ty(else_, params))
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter().find_map(|arm|
                infer_body_result_ty(&arm.body, params).filter(|t| !t.is_unresolved())
            )
        }
        IrExprKind::Call { .. } => {
            if !expr.ty.is_unresolved() { Some(expr.ty.clone()) } else { None }
        }
        IrExprKind::LitInt { .. } => Some(Ty::Int),
        IrExprKind::LitFloat { .. } => Some(Ty::Float),
        IrExprKind::LitBool { .. } => Some(Ty::Bool),
        IrExprKind::LitStr { .. } => Some(Ty::String),
        _ => {
            if !expr.ty.is_unresolved() { Some(expr.ty.clone()) } else { None }
        }
    }
}

/// Resolve type from `pair.0` / `pair.1` where pair is a lambda parameter.
fn resolve_via_tuple_index(expr: &IrExpr, params: &[(VarId, Ty)]) -> Option<Ty> {
    if let IrExprKind::TupleIndex { object, index } = &expr.kind {
        if let IrExprKind::Var { id } = &object.kind {
            if let Some((_, ty)) = params.iter().find(|(vid, _)| vid == id) {
                if let Ty::Tuple(elems) = ty {
                    return elems.get(*index).cloned();
                }
            }
        }
    }
    None
}

/// Compute the return type of a stdlib list Call node from the
/// (already-resolved) types of its args. Mirrors the subset of
/// `pass_concretize_types::resolve_call_ret_ty` that can answer
/// without a SymbolTable. Used by LTR to propagate concrete types
/// into downstream Var bindings before ConcretizeTypes runs.
fn compute_stdlib_call_ret(target: &CallTarget, args: &[IrExpr], vt: &VarTable) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    let (module, func): (&str, &str) = match target {
        CallTarget::Module { module, func, .. } => (module.as_str(), func.as_str()),
        CallTarget::Named { name } => {
            let s = name.as_str();
            let rest = s.strip_prefix("almide_rt_")?;
            let under = rest.find('_')?;
            let module = &rest[..under];
            let func = &rest[under + 1..];
            // Returning refs into `s` would outlive the match — rebind.
            return compute_stdlib_call_ret_inner(module, func, args, vt);
        }
        _ => return None,
    };
    compute_stdlib_call_ret_inner(module, func, args, vt)
}

fn compute_stdlib_call_ret_inner(module: &str, func: &str, args: &[IrExpr], vt: &VarTable) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    if module != "list" { return None; }
    let list_elem = |idx: usize| -> Option<Ty> {
        let arg = args.get(idx)?;
        resolve_list_elem_ty(arg, vt)
    };
    let list_of = |t: Ty| Ty::Applied(TCI::List, vec![t]);
    match func {
        "zip" => {
            let a = list_elem(0)?;
            let b = list_elem(1)?;
            Some(list_of(Ty::Tuple(vec![a, b])))
        }
        "enumerate" => {
            let elem = list_elem(0)?;
            Some(list_of(Ty::Tuple(vec![Ty::Int, elem])))
        }
        "map" | "filter_map" | "flat_map" => None,  // needs lambda ret
        "filter" | "take_while" | "drop_while"
        | "take" | "drop" | "reverse" | "sort" | "sort_by"
        | "dedup" | "slice" | "chunks" | "intersperse" => list_elem(0).map(list_of),
        "fold" => {
            let init = args.get(1)?;
            if !init.ty.has_unresolved_deep() { Some(init.ty.clone()) } else { None }
        }
        "any" | "all" => Some(Ty::Bool),
        "count" | "len" => Some(Ty::Int),
        "first" | "last" | "find" => {
            let elem = list_elem(0)?;
            Some(Ty::Applied(TCI::Option, vec![elem]))
        }
        _ => None,
    }
}
