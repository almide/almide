/// Register top-level and module function return-type signatures (keyed by
/// `(module_name_or_empty, fn_name)`) into `sigs`. Extracted from
/// `build_symbol_table` (cog>25 decomposition): a write-only accumulator
/// loop, never read back within this function.
fn register_named_fn_sigs(program: &IrProgram, sigs: &mut std::collections::HashMap<(String, String), Ty>) {
    // Top-level functions (Named call targets)
    for func in &program.functions {
        if !func.ret_ty.has_unresolved_deep() {
            sigs.insert((String::new(), func.name.to_string()), func.ret_ty.clone());
        }
    }
    // Module functions (Module call targets)
    for module in &program.modules {
        let mname = module.name.to_string();
        for func in &module.functions {
            if func.is_test { continue; }
            if !func.ret_ty.has_unresolved_deep() {
                sigs.insert((mname.clone(), func.name.to_string()), func.ret_ty.clone());
            }
        }
    }
}

/// Register a single type declaration's record field list(s) into
/// `record_fields` — a plain record's own fields, or every Record-shaped
/// variant case's fields. Extracted from `build_symbol_table` (cog>25
/// decomposition): was a local closure, promoted to a named function so
/// its own complexity is independently measured (never wrap in a closure
/// to dodge measurement).
fn register_type_decl_fields(decl: &almide_ir::IrTypeDecl, record_fields: &mut std::collections::HashMap<String, Vec<(almide_base::intern::Sym, Ty)>>) {
    match &decl.kind {
        almide_ir::IrTypeDeclKind::Record { fields } => {
            let fs: Vec<_> = fields.iter()
                .map(|f| (f.name, f.ty.clone()))
                .collect();
            record_fields.insert(decl.name.to_string(), fs);
        }
        almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
            for case in cases {
                if let almide_ir::IrVariantKind::Record { fields } = &case.kind {
                    let v: Vec<_> = fields.iter()
                        .map(|f| (f.name, f.ty.clone()))
                        .collect();
                    record_fields.insert(case.name.to_string(), v);
                }
            }
        }
        _ => {}
    }
}

fn build_symbol_table(program: &IrProgram) -> SymbolTable {
    let mut sigs = std::collections::HashMap::new();
    register_named_fn_sigs(program, &mut sigs);

    let mut record_fields = std::collections::HashMap::new();
    for decl in &program.type_decls {
        register_type_decl_fields(decl, &mut record_fields);
    }
    for module in &program.modules {
        for decl in &module.type_decls {
            register_type_decl_fields(decl, &mut record_fields);
        }
    }
    SymbolTable { sigs, record_fields }
}

/// For `ResultErr(payload)` with `ty = Result[Unknown, E]` inside an
/// effect fn whose ret_ty was lifted to `Result[T, String]`, fill the
/// Unknown Ok slot with `T`. The err-channel type stays whatever the
/// inner expression produced so `err("msg")` / `err(custom_err)` both
/// work. Returns `None` when the enclosing fn isn't a lifted Result
/// or the inner doesn't have an Err ty yet.
fn infer_err_ty_from_enclosing(enclosing_ret: &Ty, inner_ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    // Case 1: enclosing fn already returns Result[T, E] (post-ResultPropagation lift)
    if let Ty::Applied(TypeConstructorId::Result, args) = enclosing_ret {
        if args.len() == 2 && !args[0].has_unresolved_deep() {
            let ok_ty = args[0].clone();
            let err_ty = if !inner_ty.has_unresolved_deep() {
                inner_ty.clone()
            } else {
                args[1].clone()
            };
            return Some(Ty::Applied(TypeConstructorId::Result, vec![ok_ty, err_ty]));
        }
    }
    // Case 2: enclosing fn returns T (pre-lift, e.g. effect fn safe_div -> Int).
    // The Ok slot of err() should be T, and Err slot is String (effect fn convention).
    if !enclosing_ret.has_unresolved_deep() && *enclosing_ret != Ty::Unit {
        let ok_ty = enclosing_ret.clone();
        let err_ty = if !inner_ty.has_unresolved_deep() {
            inner_ty.clone()
        } else {
            Ty::String
        };
        return Some(Ty::Applied(TypeConstructorId::Result, vec![ok_ty, err_ty]));
    }
    None
}

/// Push an expected type into an expression whose own inference left it
/// `Unknown`. Narrow by design: the target is `Applied(List, [Unknown])`
/// (the empty-list literal case) and the expected type fills the element
/// slot. Other shapes could be added as specific gaps surface, but kept
/// out for now so the audit keeps teeth around shapes we don't fully
/// understand.
fn propagate_expected_ty(expr: &mut IrExpr, expected: &Ty) {
    use almide_lang::types::constructor::TypeConstructorId;
    match (&expr.ty, expected) {
        (Ty::Applied(TypeConstructorId::List, args),
         Ty::Applied(TypeConstructorId::List, exp_args))
            if args.len() == 1 && exp_args.len() == 1
                && args[0].has_unresolved_deep()
                && !exp_args[0].has_unresolved_deep() =>
        {
            expr.ty = expected.clone();
            // Tighten the List literal's declared element type too so
            // downstream consumers (e.g. `emit_wasm::values::byte_size`)
            // see the resolved shape.
            if let IrExprKind::List { elements } = &mut expr.kind {
                if elements.is_empty() {
                    // nothing to rewrite inside — ty was the only carrier
                }
            }
        }
        _ => {}
    }
}

// ── Generic back-propagation helpers ────────────────────────────────

/// Shape-aware merge: returns the most concrete type when `a` and `b`
/// share a shape but differ in `Unknown` slots. Returns `None` when the
/// shapes disagree (can't safely merge). Leaves TypeVar alone since the
/// pre-pass is already expected to have substituted generics.
fn merge_more_concrete(a: &Ty, b: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    let _ = TypeConstructorId::List; // silence unused warning in some builds
    match (a, b) {
        (Ty::Unknown, other) | (other, Ty::Unknown) => Some(other.clone()),
        (Ty::Applied(c1, a1), Ty::Applied(c2, a2)) if c1 == c2 && a1.len() == a2.len() => {
            let merged: Option<Vec<Ty>> = a1.iter().zip(a2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            merged.map(|m| Ty::Applied(c1.clone(), m))
        }
        (Ty::Tuple(e1), Ty::Tuple(e2)) if e1.len() == e2.len() => {
            let merged: Option<Vec<Ty>> = e1.iter().zip(e2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            merged.map(Ty::Tuple)
        }
        (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 })
            if p1.len() == p2.len() =>
        {
            let merged_params: Option<Vec<Ty>> = p1.iter().zip(p2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            let merged_ret = merge_more_concrete(r1, r2);
            match (merged_params, merged_ret) {
                (Some(ps), Some(r)) => Some(Ty::Fn { params: ps, ret: Box::new(r) }),
                _ => None,
            }
        }
        (x, y) if x == y => Some(x.clone()),
        _ => None,
    }
}

/// `(List { elements }, Applied(List, [elem_ty]))` arm of [`propagate_ty_down`].
fn propagate_ty_down_list(elements: &mut [IrExpr], elem_ty: &Ty) {
    for e in elements.iter_mut() { propagate_ty_down(e, elem_ty); }
}

/// `(Tuple { elements }, Tuple(ts))` arm of [`propagate_ty_down`].
fn propagate_ty_down_tuple(elements: &mut [IrExpr], ts: &[Ty]) {
    for (e, t) in elements.iter_mut().zip(ts.iter()) {
        propagate_ty_down(e, t);
    }
}

/// `(Match { arms, .. }, _)` arm of [`propagate_ty_down`].
fn propagate_ty_down_match_arms(arms: &mut [IrMatchArm], expected: &Ty) {
    for arm in arms.iter_mut() {
        propagate_ty_down(&mut arm.body, expected);
    }
}

/// Push `expected` down into `expr`, recursing into wrappers (OptionSome,
/// Result*, List, Tuple). Updates expr.ty and any matching sub-expressions
/// whose own types have unresolved slots compatible with `expected`.
fn propagate_ty_down(expr: &mut IrExpr, expected: &Ty) {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    if let Some(merged) = merge_more_concrete(&expr.ty, expected) {
        expr.ty = merged;
    }
    match (&mut expr.kind, expected) {
        (IrExprKind::OptionSome { expr: inner }, Ty::Applied(TCI::Option, args))
            if args.len() == 1 =>
        {
            propagate_ty_down(inner, &args[0]);
        }
        (IrExprKind::ResultOk { expr: inner }, Ty::Applied(TCI::Result, args))
            if !args.is_empty() =>
        {
            propagate_ty_down(inner, &args[0]);
        }
        (IrExprKind::ResultErr { expr: inner }, Ty::Applied(TCI::Result, args))
            if args.len() >= 2 =>
        {
            propagate_ty_down(inner, &args[1]);
        }
        (IrExprKind::List { elements }, Ty::Applied(TCI::List, args))
            if args.len() == 1 =>
        {
            propagate_ty_down_list(elements, &args[0]);
        }
        (IrExprKind::Tuple { elements }, Ty::Tuple(ts)) if elements.len() == ts.len() => {
            propagate_ty_down_tuple(elements, ts);
        }
        (IrExprKind::If { then, else_, .. }, _) => {
            propagate_ty_down(then, expected);
            propagate_ty_down(else_, expected);
        }
        (IrExprKind::Block { expr: Some(tail), .. }, _) => {
            propagate_ty_down(tail, expected);
        }
        (IrExprKind::Match { arms, .. }, _) => propagate_ty_down_match_arms(arms, expected),
        // Explicit-preserve (total-by-construction). The guarded arms above
        // handle the wrapper/branch shapes whose `expr.kind` and `expected`
        // line up; every other (kind, expected) pairing — including a
        // wrapper kind whose `expected` shape did NOT match its guard —
        // falls here and is a no-op, exactly as the old `_ => {}`. The merge
        // of `expr.ty` with `expected` already happened above, so there is
        // nothing further to push down. Wildcarding only the `expected`
        // slot keeps the fall-through identical while making the first
        // tuple element exhaustive: a new IrExprKind variant is a compile
        // error here. `If`/`Match` are NOT re-listed: their arms above are
        // unguarded, so they already cover every `expected` and re-listing
        // them would be an unreachable pattern.
        (IrExprKind::OptionSome { .. }, _)
        | (IrExprKind::ResultOk { .. }, _)
        | (IrExprKind::ResultErr { .. }, _)
        | (IrExprKind::List { .. }, _)
        | (IrExprKind::Tuple { .. }, _)
        | (IrExprKind::Block { .. }, _)
        | (IrExprKind::LitInt { .. }, _) | (IrExprKind::LitFloat { .. }, _)
        | (IrExprKind::LitStr { .. }, _) | (IrExprKind::LitBool { .. }, _)
        | (IrExprKind::Unit, _) | (IrExprKind::Var { .. }, _)
        | (IrExprKind::FnRef { .. }, _) | (IrExprKind::BinOp { .. }, _)
        | (IrExprKind::UnOp { .. }, _) | (IrExprKind::Fan { .. }, _)
        | (IrExprKind::ForIn { .. }, _) | (IrExprKind::While { .. }, _)
        | (IrExprKind::Break, _) | (IrExprKind::Continue, _)
        | (IrExprKind::Call { .. }, _) | (IrExprKind::TailCall { .. }, _)
        | (IrExprKind::RuntimeCall { .. }, _)
        | (IrExprKind::MapLiteral { .. }, _) | (IrExprKind::EmptyMap, _)
        | (IrExprKind::Record { .. }, _) | (IrExprKind::SpreadRecord { .. }, _)
        | (IrExprKind::Range { .. }, _) | (IrExprKind::Member { .. }, _)
        | (IrExprKind::TupleIndex { .. }, _) | (IrExprKind::IndexAccess { .. }, _)
        | (IrExprKind::MapAccess { .. }, _) | (IrExprKind::Lambda { .. }, _)
        | (IrExprKind::StringInterp { .. }, _) | (IrExprKind::OptionNone, _)
        | (IrExprKind::Try { .. }, _) | (IrExprKind::Unwrap { .. }, _)
        | (IrExprKind::UnwrapOr { .. }, _) | (IrExprKind::ToOption { .. }, _)
        | (IrExprKind::OptionalChain { .. }, _) | (IrExprKind::Await { .. }, _)
        | (IrExprKind::Clone { .. }, _) | (IrExprKind::Deref { .. }, _)
        | (IrExprKind::Borrow { .. }, _) | (IrExprKind::BoxNew { .. }, _)
        | (IrExprKind::RcWrap { .. }, _) | (IrExprKind::RustMacro { .. }, _)
        | (IrExprKind::ToVec { .. }, _) | (IrExprKind::RenderedCall { .. }, _)
        | (IrExprKind::InlineRust { .. }, _) | (IrExprKind::ClosureCreate { .. }, _)
        | (IrExprKind::EnvLoad { .. }, _) | (IrExprKind::IterChain { .. }, _)
        | (IrExprKind::Hole, _) | (IrExprKind::Todo { .. }, _) => {}
    }
}

/// Propagate a subject type into a match pattern, updating `Bind` pattern
/// ty + the matching VarTable entry. Supports Some/Ok/Err/Tuple destructuring
/// (the shapes that actually surface in spec/).
fn propagate_pattern_ty(pat: &mut IrPattern, subj_ty: &Ty, vt: &mut VarTable) {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    match (pat, subj_ty) {
        (IrPattern::Bind { var, ty }, t) => {
            if ty.has_unresolved_deep() && !t.has_unresolved_deep() {
                *ty = t.clone();
                if (var.0 as usize) < vt.len() {
                    vt.entries[var.0 as usize].ty = t.clone();
                }
            }
        }
        (IrPattern::Some { inner }, Ty::Applied(TCI::Option, args)) if args.len() == 1 => {
            propagate_pattern_ty(inner, &args[0], vt);
        }
        (IrPattern::Ok { inner }, Ty::Applied(TCI::Result, args)) if !args.is_empty() => {
            propagate_pattern_ty(inner, &args[0], vt);
        }
        (IrPattern::Err { inner }, Ty::Applied(TCI::Result, args)) if args.len() >= 2 => {
            propagate_pattern_ty(inner, &args[1], vt);
        }
        (IrPattern::Tuple { elements }, Ty::Tuple(ts)) if elements.len() == ts.len() => {
            for (e, t) in elements.iter_mut().zip(ts.iter()) {
                propagate_pattern_ty(e, t, vt);
            }
        }
        _ => {}
    }
}

fn is_fold_like_call(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
            module.as_str() == "list" && matches!(func.as_str(), "fold" | "scan")
        }
        _ => false,
    }
}

/// Accumulator type for `list.fold(xs, init, f)`: merge `init_ty` and
/// `body_ty`, picking the most concrete shape when both are known and
/// compatible, falling back to whichever side is concrete when only one
/// is. Extracted from `back_propagate_fold_acc` (cog>25 decomposition).
fn compute_fold_acc_ty(init_ty: &Ty, body_ty: &Ty) -> Option<Ty> {
    if !init_ty.has_unresolved_deep() && !body_ty.has_unresolved_deep() {
        merge_more_concrete(init_ty, body_ty)
    } else if !init_ty.has_unresolved_deep() {
        Some(init_ty.clone())
    } else if !body_ty.has_unresolved_deep() {
        Some(body_ty.clone())
    } else {
        None
    }
}

/// Update the fold lambda's acc param (IR annotation + VarTable) and its
/// `Ty::Fn` wrapper to `acc_ty`, wherever still unresolved. `args[2]` is
/// the fold lambda arg. Extracted from `back_propagate_fold_acc` (cog>25
/// decomposition). Returns true when a change was made.
fn update_fold_lambda_acc(args: &mut [IrExpr], acc_ty: &Ty, vt: &mut VarTable) -> bool {
    let mut changed = false;
    if let IrExprKind::Lambda { params, .. } = &mut args[2].kind {
        if let Some((vid, pty)) = params.get_mut(0) {
            if pty.has_unresolved_deep() {
                *pty = acc_ty.clone();
                if (vid.0 as usize) < vt.len() {
                    vt.entries[vid.0 as usize].ty = acc_ty.clone();
                }
                changed = true;
            }
        }
    }
    if let Ty::Fn { params: ps, ret } = &mut args[2].ty {
        if let Some(p0) = ps.get_mut(0) {
            if p0.has_unresolved_deep() {
                *p0 = acc_ty.clone();
                changed = true;
            }
        }
        if ret.has_unresolved_deep() {
            **ret = acc_ty.clone();
            changed = true;
        }
    }
    changed
}

/// For `list.fold(xs, init, f)` where `f: (acc, t) -> acc`: the accumulator
/// type `A` has two sources — `init.ty` and `f.body.ty` — which must agree.
/// Pick the most concrete form available, then push it back into the init
/// sub-expression, the lambda's acc parameter (IR annotation + VarTable),
/// the Ty::Fn wrapper, and the Call's own ty. Returns true when changes
/// were made.
fn back_propagate_fold_acc(expr: &mut IrExpr, vt: &mut VarTable) -> bool {
    let args = match &mut expr.kind {
        IrExprKind::Call { args, .. } => args,
        _ => return false,
    };
    if args.len() < 3 { return false; }

    let body_ty = match &args[2].kind {
        IrExprKind::Lambda { body, .. } => body.ty.clone(),
        _ => return false,
    };
    let init_ty = args[1].ty.clone();

    let Some(acc_ty) = compute_fold_acc_ty(&init_ty, &body_ty) else { return false; };

    let mut changed = false;

    // Push acc_ty into the init sub-expression when init has weaker shape
    if init_ty != acc_ty {
        propagate_ty_down(&mut args[1], &acc_ty);
        changed = true;
    }

    if update_fold_lambda_acc(args, &acc_ty, vt) { changed = true; }

    // Update Call's own ty if it's still unresolved
    if expr.ty.has_unresolved_deep() {
        expr.ty = acc_ty;
        changed = true;
    }
    changed
}
