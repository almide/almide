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
        let new_ty = substitute_ty(&info.ty, bindings);
        let new_id = vt.alloc(info.name.clone(), new_ty, info.mutability, info.span);
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
    remap_expr_varids(&mut body, &remap);

    // Phase 5: rename self-recursive `Named { orig.name }` calls in the
    // specialized body so they refer to the specialized fn. Top-level mono's
    // `rewrite_calls` will later normalize the same thing for top-level fns
    // (idempotent no-op here); module-scoped mono relies on this step
    // because its rewriter walks module bodies but does not re-discover
    // intra-fn recursive edges.
    let spec_name = format!("{}__{}", orig.name, suffix);
    rename_named_calls(&mut body, orig.name.as_str(), &spec_name);

    IrFunction {
        name: spec_name.into(),
        params,
        ret_ty: substitute_ty(&orig.ret_ty, bindings),
        body,
        generics: None, // specialized function is concrete
        is_effect: orig.is_effect,
        is_async: orig.is_async,
        is_test: orig.is_test,
        extern_attrs: orig.extern_attrs.clone(),
        export_attrs: orig.export_attrs.clone(),
        attrs: orig.attrs.clone(),
        visibility: orig.visibility.clone(),
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
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
    let is_float = |t: &Ty| matches!(t, Ty::Float | Ty::Float32);
    let float_pair = is_float(left_ty) || is_float(right_ty);
    // `+` on a TypeVar lowered to the default `AddInt` (lower/expressions.rs):
    // when the generic instantiates to String/List, re-dispatch to the
    // overloaded concat — mirroring lowering's own type dispatch. Without this
    // a `fn dup(x: T) -> T = x + x` specialized at String/List kept `AddInt`
    // and the IR-verify gate (#532) panicked on the non-Int operands (#558).
    // A genuinely non-concatenable instantiation (e.g. a record) stays `AddInt`
    // and is correctly still rejected by that gate.
    let is_str = |t: &Ty| matches!(t, Ty::String);
    let is_list = |t: &Ty| matches!(t, Ty::Applied(almide_lang::types::TypeConstructorId::List, _));
    let str_pair = is_str(left_ty) || is_str(right_ty);
    let list_pair = is_list(left_ty) || is_list(right_ty);
    match op {
        BinOp::AddInt if float_pair => BinOp::AddFloat,
        BinOp::SubInt if float_pair => BinOp::SubFloat,
        BinOp::MulInt if float_pair => BinOp::MulFloat,
        BinOp::DivInt if float_pair => BinOp::DivFloat,
        BinOp::ModInt if float_pair => BinOp::ModFloat,
        BinOp::PowInt if float_pair => BinOp::PowFloat,
        BinOp::AddInt if str_pair => BinOp::ConcatStr,
        BinOp::AddInt if list_pair => BinOp::ConcatList,
        other => other,
    }
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
        | IrExprKind::Await { .. }
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
        | IrExprKind::Await { expr }
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
    match target {
        CallTarget::Method { object, method } => {
            substitute_expr_types(object, bindings);
            // Rewrite protocol method calls: T.show → Dog.show when T → Dog
            if let Some(dot_pos) = method.find('.') {
                let tv_name = &method[..dot_pos];
                if let Some(concrete_ty) = bindings.get(tv_name) {
                    if let Some(concrete_name) = ty_to_name(concrete_ty) {
                        let method_name = &method[dot_pos+1..];
                        *method = format!("{}.{}", concrete_name, method_name).into();
                    }
                }
            }
        }
        CallTarget::Computed { callee: object } => {
            substitute_expr_types(object, bindings);
        }
        _ => {}
    }
    for a in args { substitute_expr_types(a, bindings); }
}

fn substitute_pattern_types(pattern: &mut IrPattern, bindings: &HashMap<String, Ty>) {
    match pattern {
        IrPattern::Bind { ty, .. } => { *ty = substitute_ty(ty, bindings); }
        IrPattern::Constructor { args, .. } => { for a in args { substitute_pattern_types(a, bindings); } }
        IrPattern::Tuple { elements } => { for e in elements { substitute_pattern_types(e, bindings); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => { substitute_pattern_types(inner, bindings); }
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &mut f.pattern { substitute_pattern_types(p, bindings); } } }
        _ => {}
    }
}

fn substitute_stmt_types(stmt: &mut IrStmt, bindings: &HashMap<String, Ty>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            *ty = substitute_ty(ty, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Assign { value, .. } => {
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
