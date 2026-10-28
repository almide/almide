use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::Ty;
use super::utils::ty_to_name;
use super::varid_remap::{collect_var_id, collect_varids_in_expr, remap_expr_varids};

/// Specialize a function for concrete types.
///
/// Each specialization gets **fresh VarIds** (alpha-renaming) so that multiple
/// specializations of the same generic function never share VarTable entries.
/// The fresh VarIds are allocated in `vt` with already-substituted types,
/// eliminating the need for a separate `update_var_table_types` pass.
pub(super) fn specialize_function(
    orig: &IrFunction,
    suffix: &str,
    bindings: &HashMap<String, Ty>,
    vt: &mut VarTable,
    globals: &std::collections::HashSet<VarId>,
    operator_requirements: &HashMap<(almide_base::intern::Sym, almide_base::intern::Sym), CallTarget>,
) -> IrFunction {
    // Phase 1: Collect all VarIds referenced in the original function
    let mut old_ids = Vec::new();
    for p in &orig.params { collect_var_id(p.var, &mut old_ids); }
    collect_varids_in_expr(&orig.body, &mut old_ids);

    // Phase 2: Allocate fresh VarIds with substituted types. A module-level
    // global (`var _dirty`) referenced by the body is a FREE variable, not a
    // binding of this function — alpha-renaming it would detach the reference
    // from the storage annotation (keyed by the top-let's VarId), so the
    // walker rendered a bare local name and rustc E0425'd (#788). Globals
    // keep their original VarId across every specialization.
    let mut remap: HashMap<VarId, VarId> = HashMap::with_capacity(old_ids.len());
    for old in &old_ids {
        if remap.contains_key(old) || globals.contains(old) { continue; }
        let info = vt.get(*old);
        let (new_ty, mutability) = (substitute_ty(&info.ty, bindings), info.mutability);
        let new_id = vt.alloc_like(*old, new_ty, mutability);
        remap.insert(*old, new_id);
    }

    // Phase 3: Build specialized params with fresh VarIds
    let params: Vec<IrParam> = orig.params.iter().enumerate().map(|(i, param)| {
        let open_key = format!("__open_{}", i);
        let new_ty = if let Some(concrete) = bindings.get(&open_key) {
            concrete.clone()
        } else {
            substitute_ty(&param.ty, bindings)
        };
        IrParam {
            var: remap.get(&param.var).copied().unwrap_or(param.var),
            ty: new_ty,
            ..param.clone()
        }
    }).collect();

    // Phase 4: Clone body, substitute types, and remap VarIds
    let mut body = orig.body.clone();
    substitute_expr_types(&mut body, bindings);
    if !operator_requirements.is_empty() {
        lower_requirement_operators(&mut body, operator_requirements);
    }
    remap_expr_varids(&mut body, &remap);

    // Phase 5: rename self-recursive `Named { orig.name }` calls in the
    // specialized body so they refer to the specialized fn. Top-level mono's
    // `rewrite_calls` will later normalize the same thing for top-level fns
    // (idempotent no-op here); module-scoped mono relies on this step
    // because its rewriter walks module bodies but does not re-discover
    // intra-fn recursive edges.
    // The suffix arrives dot-free: `mangle_ty` writes a module-qualified type
    // name's `.` as `_d` at the mint point (#1496), so every consumer — this
    // definition, the module-call rewriter, top-level mono's rewrite_calls —
    // agrees on the identifier spelling. The instance is named in the
    // compiler's `__almd_mono` space, which no user fn can spell (#3492).
    let spec_name = almide_ir::mono_instance_name(orig.name.as_str(), suffix);
    rename_named_calls(&mut body, orig.name.as_str(), &spec_name);

    IrFunction {
        name: spec_name.into(),
        params,
        ret_ty: substitute_ty(&orig.ret_ty, bindings),
        body,
        generics: None, // specialized function is concrete
        is_effect: orig.is_effect,
        is_test: orig.is_test,
        extern_attrs: orig.extern_attrs.clone(),
        export_attrs: orig.export_attrs.clone(),
        attrs: orig.attrs.clone(),
        visibility: orig.visibility.clone(),
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        // The `mut` param set travels with the specialization — params are
        // positional and the instance keeps the generic's order, so the
        // indices stay valid. Dropping it detached the instance from the
        // C-132 move-mode rewrite: `bump_twice__Tally(t)`'s call site was
        // never given the write-back, so the caller's binding silently kept
        // its pre-call value on the v1 leg (native's walker has its own
        // mut-param handling and masked it — the #1551 wasm follow-through).
        mutated_params: orig.mutated_params.clone(), module_origin: None,
    }
}

// ── Operators on a bound conforming type (#4099) ───────────────

/// The requirement method an arithmetic `BinOp` stands for.
fn requirement_method(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::AddInt | BinOp::AddFloat => "add",
        BinOp::SubInt | BinOp::SubFloat => "sub",
        BinOp::MulInt | BinOp::MulFloat => "mul",
        BinOp::DivInt | BinOp::DivFloat => "div",
        BinOp::ModInt | BinOp::ModFloat => "rem",
        _ => return None,
    })
}

/// The requirement function of `method` on the (now concrete) type `ty`.
fn operator_requirement_target(
    ty: &Ty, method: &str,
    table: &HashMap<(almide_base::intern::Sym, almide_base::intern::Sym), CallTarget>,
) -> Option<CallTarget> {
    let Ty::Named(name, _) = ty else { return None };
    let m = almide_base::intern::sym(method);
    table.get(&(*name, m)).cloned().or_else(|| {
        let bare = name.as_str().rsplit_once('.').map(|(_, b)| b)?;
        table.get(&(almide_base::intern::sym(bare), m)).cloned()
    })
}

/// A generic body's operator on a type parameter lowered to a `BinOp` /
/// `UnOp`, since nothing about `T` named a function. Once `T` is bound to a
/// declared type conforming to the operator's protocol (`BigInt: Numeric`),
/// the operator is that type's requirement — the call lowering gives the
/// same operator on a concrete `BigInt` (R-bigint-04): `a + b` is
/// `bigint.add(a, b)`, `a < b` is `bigint.compare(a, b) < 0`, `-a` is
/// `bigint.neg(a)`. `==` stays: equality is structural on every type.
fn lower_requirement_operators(
    expr: &mut IrExpr,
    table: &HashMap<(almide_base::intern::Sym, almide_base::intern::Sym), CallTarget>,
) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    struct Lower<'a> { table: &'a HashMap<(almide_base::intern::Sym, almide_base::intern::Sym), CallTarget> }
    impl IrMutVisitor for Lower<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let span = expr.span;
            match &mut expr.kind {
                IrExprKind::BinOp { op, left, right } => {
                    if let Some(method) = requirement_method(*op) {
                        let Some(target) = operator_requirement_target(&left.ty, method, self.table) else { return };
                        let (l, r) = (std::mem::take(&mut **left), std::mem::take(&mut **right));
                        expr.kind = IrExprKind::Call { target, args: vec![l, r], type_args: vec![] };
                    } else if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte) {
                        let Some(target) = operator_requirement_target(&left.ty, "compare", self.table) else { return };
                        let (l, r) = (std::mem::take(&mut **left), std::mem::take(&mut **right));
                        **left = IrExpr {
                            kind: IrExprKind::Call { target, args: vec![l, r], type_args: vec![] },
                            ty: Ty::Int, span, def_id: None,
                        };
                        **right = IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span, def_id: None };
                    }
                }
                IrExprKind::UnOp { op: UnOp::NegInt | UnOp::NegFloat, operand } => {
                    let Some(target) = operator_requirement_target(&operand.ty, "neg", self.table) else { return };
                    let a = std::mem::take(&mut **operand);
                    expr.kind = IrExprKind::Call { target, args: vec![a], type_args: vec![] };
                }
                _ => {}
            }
        }
    }
    Lower { table }.visit_expr_mut(expr);
}

// ── Self-recursive Named rename ────────────────────────────────

fn rename_named_calls(expr: &mut IrExpr, from: &str, to: &str) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    use almide_base::intern::sym;

    struct Renamer<'a> { from: &'a str, to: &'a str }
    impl<'a> IrMutVisitor for Renamer<'a> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &mut expr.kind {
                if name.as_str() == self.from {
                    *name = sym(self.to);
                }
            }
        }
    }

    Renamer { from, to }.visit_expr_mut(expr);
}

/// Re-dispatch a type-dispatched `BinOp` when a generic binding
/// resolves to a concrete numeric width. Returns the input `op`
/// unchanged when the pairing already matches (or when the operator
/// is already kind-neutral like `Eq` / `Lt` / `ConcatStr`).
fn repair_binop_for_types(op: BinOp, left_ty: &Ty, right_ty: &Ty) -> BinOp {
    if let Some(repaired) = repair_binop_for_float(op, left_ty, right_ty) {
        return repaired;
    }
    // `+` on a TypeVar lowered to the default `AddInt` (lower/expressions.rs):
    // when the generic instantiates to String/List, re-dispatch to the
    // overloaded concat — mirroring lowering's own type dispatch. Without this
    // a `fn dup(x: T) -> T = x + x` specialized at String/List kept `AddInt`
    // and the IR-verify gate (#532) panicked on the non-Int operands (#558).
    // A genuinely non-concatenable instantiation (e.g. a record) stays `AddInt`
    // and is correctly still rejected by that gate.
    repair_binop_for_str_or_list(op, left_ty, right_ty)
}

/// Re-dispatch an `Int`-flavored numeric `BinOp` to its `Float` twin when either
/// operand resolved to a float type. Returns `None` (no repair) otherwise.
fn repair_binop_for_float(op: BinOp, left_ty: &Ty, right_ty: &Ty) -> Option<BinOp> {
    let is_float = |t: &Ty| matches!(t, Ty::Float | Ty::Float32);
    if !is_float(left_ty) && !is_float(right_ty) { return None; }
    match op {
        BinOp::AddInt => Some(BinOp::AddFloat),
        BinOp::SubInt => Some(BinOp::SubFloat),
        BinOp::MulInt => Some(BinOp::MulFloat),
        BinOp::DivInt => Some(BinOp::DivFloat),
        BinOp::ModInt => Some(BinOp::ModFloat),
        BinOp::PowInt => Some(BinOp::PowFloat),
        _ => None,
    }
}

/// Re-dispatch `AddInt` to the overloaded concat operator when either operand
/// resolved to `String` or `List`. Leaves every other operator unchanged.
fn repair_binop_for_str_or_list(op: BinOp, left_ty: &Ty, right_ty: &Ty) -> BinOp {
    if !matches!(op, BinOp::AddInt) { return op; }
    let is_str = |t: &Ty| matches!(t, Ty::String);
    let is_list = |t: &Ty| matches!(t, Ty::Applied(almide_lang::types::TypeConstructorId::List, _));
    if is_str(left_ty) || is_str(right_ty) { return BinOp::ConcatStr; }
    if is_list(left_ty) || is_list(right_ty) { return BinOp::ConcatList; }
    op
}

/// Substitute TypeVars with concrete types.
/// Uses Ty::map_children for uniform recursive traversal.
pub(super) fn substitute_ty(ty: &Ty, bindings: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => bindings.get(name.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        // In IR, TypeVar("T") may appear as Named("T", [])
        Ty::Named(name, args) if args.is_empty() && bindings.contains_key(name.as_str()) => {
            bindings[name.as_str()].clone()
        }
        Ty::OpenRecord { .. } => {
            // OpenRecord パラメータを具体型に置換（__open_N → 具体型）
            for (_, concrete) in bindings.iter() {
                if let Ty::Named(_, _) | Ty::Record { .. } = concrete {
                    return concrete.clone();
                }
            }
            ty.map_children(&|child| substitute_ty(child, bindings))
        }
        // All other types: recursively substitute children
        _ => ty.map_children(&|child| substitute_ty(child, bindings)),
    }
}

fn substitute_expr_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    expr.ty = substitute_ty(&expr.ty, bindings);
    match &mut expr.kind {
        IrExprKind::BinOp { op, left, right } => {
            substitute_expr_types(left, bindings);
            substitute_expr_types(right, bindings);
            // Re-dispatch the binop kind when the operand types become
            // concrete. Numeric protocol bounds (`T: Numeric`) admit
            // `Int` or `Float` at mono time — a `T + T` that lowered
            // as `AddInt` under TypeVar must flip to `AddFloat` when
            // T resolves to Float, otherwise the IR verifier flags the
            // mismatch.
            *op = repair_binop_for_types(*op, &left.ty, &right.ty);
        }
        IrExprKind::UnOp { .. } | IrExprKind::If { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. } => substitute_control_types(expr, bindings),
        IrExprKind::Match { .. } => substitute_match_types(expr, bindings),
        IrExprKind::Block { .. } => substitute_block_types(expr, bindings),
        IrExprKind::Call { .. } => substitute_call_types(expr, bindings),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::Fan { .. } | IrExprKind::RustMacro { .. } => substitute_container_literal_types(expr, bindings),
        IrExprKind::Range { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } => substitute_container_access_types(expr, bindings),
        IrExprKind::Lambda { body, params, .. } => {
            for (_, ty) in params { *ty = substitute_ty(ty, bindings); }
            substitute_expr_types(body, bindings);
        }
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::Try { .. }
       
        | IrExprKind::Unwrap { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::UnwrapOr { .. } => substitute_wrap_types(expr, bindings),
        _ => {}
    }
}

/// UnOp/If/ForIn/While: substitute in operand, condition, and bodies.
fn substitute_control_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    match &mut expr.kind {
        IrExprKind::UnOp { operand, .. } => substitute_expr_types(operand, bindings),
        IrExprKind::If { cond, then, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(then, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            substitute_expr_types(iterable, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        IrExprKind::While { cond, body } => {
            substitute_expr_types(cond, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        _ => unreachable!(),
    }
}

/// Block: substitute in statements and tail.
fn substitute_block_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    let IrExprKind::Block { stmts, expr } = &mut expr.kind else { unreachable!() };
    for s in stmts { substitute_stmt_types(s, bindings); }
    if let Some(e) = expr { substitute_expr_types(e, bindings); }
}

/// List/Tuple/Record/SpreadRecord/MapLiteral/Fan/RustMacro: substitute in each child expression.
fn substitute_container_literal_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    match &mut expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { substitute_expr_types(e, bindings); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { substitute_expr_types(e, bindings); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            substitute_expr_types(base, bindings);
            for (_, e) in fields { substitute_expr_types(e, bindings); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                substitute_expr_types(k, bindings);
                substitute_expr_types(v, bindings);
            }
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { substitute_expr_types(a, bindings); }
        }
        _ => unreachable!(),
    }
}

/// Range/Member/TupleIndex/IndexAccess/MapAccess/StringInterp: substitute in each accessed sub-expression.
fn substitute_container_access_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    match &mut expr.kind {
        IrExprKind::Range { start, end, .. } => {
            substitute_expr_types(start, bindings);
            substitute_expr_types(end, bindings);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            substitute_expr_types(object, bindings);
        }
        IrExprKind::IndexAccess { object, index } => {
            substitute_expr_types(object, bindings);
            substitute_expr_types(index, bindings);
        }
        IrExprKind::MapAccess { object, key } => {
            substitute_expr_types(object, bindings);
            substitute_expr_types(key, bindings);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    substitute_expr_types(expr, bindings);
                }
            }
        }
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await/Unwrap/ToOption/Clone/Deref/Borrow/BoxNew/RcWrap/
/// ToVec/OptionalChain/UnwrapOr: substitute in the wrapped expression(s).
fn substitute_wrap_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    match &mut expr.kind {
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
       
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::RcWrap { expr, .. } | IrExprKind::ToVec { expr }
        | IrExprKind::OptionalChain { expr, .. } => substitute_expr_types(expr, bindings),
        IrExprKind::UnwrapOr { expr, fallback } => {
            substitute_expr_types(expr, bindings);
            substitute_expr_types(fallback, bindings);
        }
        _ => unreachable!(),
    }
}

fn substitute_match_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    substitute_expr_types(subject, bindings);
    for arm in arms {
        substitute_pattern_types(&mut arm.pattern, bindings);
        if let Some(g) = &mut arm.guard { substitute_expr_types(g, bindings); }
        substitute_expr_types(&mut arm.body, bindings);
    }
}

fn substitute_call_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    // A protocol requirement reached through a type variable, rewritten once
    // the variable is concrete: `T.show` → `Dog.show` (a convention method,
    // `Named`), `S.__trunc` → `float32.__trunc` (a built-in type's module
    // function, `Module`). `Some(receiver)` for the method form, whose
    // receiver becomes argument 0; `None` for the receiver-less `A.zero()`.
    let mut specialized: Option<(CallTarget, Option<IrExpr>)> = None;
    match target {
        CallTarget::Method { object, method } => {
            let was_generic = matches!(object.ty, Ty::TypeVar(_));
            substitute_expr_types(object, bindings);
            if let Some(t) = requirement_target(method.as_str(), bindings) {
                specialized = Some((t, Some((**object).clone())));
            } else if was_generic && !method.contains('.') {
                // A bare method on a receiver that was a type variable and is
                // now a built-in type: the module call every leg understands
                // (codegen's Rust-only UFCS fallback is not on the wasm leg).
                if let Some(module) = object.ty.stdlib_module()
                    .filter(|m| almide_lang::stdlib_info::resolve_ufcs_candidates(method.as_str()).contains(m))
                {
                    let t = CallTarget::Module { module: almide_base::intern::sym(module), func: *method, def_id: None };
                    specialized = Some((t, Some((**object).clone())));
                }
            }
        }
        CallTarget::Named { name } => {
            if let Some(t) = requirement_target(name.as_str(), bindings) {
                specialized = Some((t, None));
            }
        }
        CallTarget::Computed { callee: object } => {
            substitute_expr_types(object, bindings);
        }
        _ => {}
    }
    if let Some((t, receiver)) = specialized {
        *target = t;
        if let Some(r) = receiver { args.insert(0, r); }
    }
    for a in args { substitute_expr_types(a, bindings); }
}

/// The concrete call a `TV.method` requirement key becomes when `TV` is bound.
/// A built-in or stdlib-declared type's requirement is its stdlib module's
/// function of that name (`Ty::owning_module`: `int8.__trunc`, `bigint.add`);
/// a user-declared
/// type's is its convention method `Type.method`. The DEFINITION of a
/// convention method is named with the BARE type and carries its module in
/// `module_origin`, and only the `Named` path re-attaches that module (#1087).
fn requirement_target(key: &str, bindings: &HashMap<String, Ty>) -> Option<CallTarget> {
    let (tv_name, method_name) = key.split_once('.')?;
    let concrete_ty = bindings.get(tv_name)?;
    if let Some(module) = concrete_ty.owning_module() {
        // A per-type requirement (`__self_close`, which several types of one
        // module satisfy) names the type: `net.__tcp_listener_close`.
        let func = match ty_to_name(concrete_ty) {
            Some(n) => almide_lang::stdlib_info::requirement_fn_name(&n, method_name),
            None => method_name.to_string(),
        };
        return Some(CallTarget::Module {
            module: almide_base::intern::sym(module),
            func: almide_base::intern::sym(&func),
            def_id: None,
        });
    }
    let concrete_name = ty_to_name(concrete_ty)?;
    let bare = concrete_name.rsplit_once('.').map_or(concrete_name.as_str(), |(_, t)| t);
    Some(CallTarget::Named { name: almide_base::intern::sym(&format!("{}.{}", bare, method_name)) })
}

fn substitute_pattern_types(pattern: &mut IrPattern, bindings: &HashMap<String, Ty>) {
    match pattern {
        IrPattern::Bind { ty, .. } => { *ty = substitute_ty(ty, bindings); }
        IrPattern::Constructor { args, .. } => { for a in args { substitute_pattern_types(a, bindings); } }
        IrPattern::Tuple { elements } => { for e in elements { substitute_pattern_types(e, bindings); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => { substitute_pattern_types(inner, bindings); }
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &mut f.pattern { substitute_pattern_types(p, bindings); } } }
        IrPattern::As { ty, inner, .. } => {
            *ty = substitute_ty(ty, bindings);
            substitute_pattern_types(inner, bindings);
        }
        // #2748: a list pattern's element and rest binds are specialized like
        // every other bind — a generic `[x, ..] => x` kept `x: A` in the
        // `__Int` instance and the structural wasm leg sized the bind as Unit.
        IrPattern::List { elements, rest } => {
            for e in elements { substitute_pattern_types(e, bindings); }
            if let Some(r) = rest { substitute_pattern_types(r, bindings); }
        }
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {}
    }
}

fn substitute_stmt_types(stmt: &mut IrStmt, bindings: &HashMap<String, Ty>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            *ty = substitute_ty(ty, bindings);
            substitute_expr_types(value, bindings);
        }
        // #2748: the destructure's binds are specialized with its value — a
        // generic `let (a, b) = p` kept `a: A` in the instance.
        IrStmtKind::BindDestructure { pattern, value } => {
            substitute_pattern_types(pattern, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::Assign { value, .. } => {
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            substitute_expr_types(index, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            substitute_expr_types(key, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::FieldAssign { value, .. } => substitute_expr_types(value, bindings),
        IrStmtKind::ListSwap { a, b, .. } => {
            substitute_expr_types(a, bindings);
            substitute_expr_types(b, bindings);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            substitute_expr_types(end, bindings);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            substitute_expr_types(len, bindings);
        }
        IrStmtKind::Expr { expr } => substitute_expr_types(expr, bindings),
        IrStmtKind::Guard { cond, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
    }
}
