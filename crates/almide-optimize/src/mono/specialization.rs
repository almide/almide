use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::Ty;
use super::utils::ty_to_name;

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
) -> IrFunction {
    // Phase 1: Collect all VarIds referenced in the original function
    let mut old_ids = Vec::new();
    for p in &orig.params { collect_var_id(p.var, &mut old_ids); }
    collect_varids_in_expr(&orig.body, &mut old_ids);

    // Phase 2: Allocate fresh VarIds with substituted types
    let mut remap: HashMap<VarId, VarId> = HashMap::with_capacity(old_ids.len());
    for old in &old_ids {
        if remap.contains_key(old) { continue; }
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

// ── VarId collection ────────────────────────────────────────────

fn collect_var_id(id: VarId, out: &mut Vec<VarId>) {
    if !out.contains(&id) { out.push(id); }
}

fn collect_varids_in_expr(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::Var { id } => collect_var_id(*id, out),
        IrExprKind::BinOp { left, right, .. } => { collect_varids_in_expr(left, out); collect_varids_in_expr(right, out); }
        IrExprKind::UnOp { operand, .. } => collect_varids_in_expr(operand, out),
        IrExprKind::If { cond, then, else_ } => {
            collect_varids_in_expr(cond, out);
            collect_varids_in_expr(then, out);
            collect_varids_in_expr(else_, out);
        }
        IrExprKind::Match { subject, arms } => {
            collect_varids_in_expr(subject, out);
            for arm in arms {
                collect_varids_in_pattern(&arm.pattern, out);
                if let Some(g) = &arm.guard { collect_varids_in_expr(g, out); }
                collect_varids_in_expr(&arm.body, out);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { collect_varids_in_stmt(s, out); }
            if let Some(e) = expr { collect_varids_in_expr(e, out); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => collect_varids_in_expr(object, out),
                _ => {}
            }
            for a in args { collect_varids_in_expr(a, out); }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            collect_var_id(*var, out);
            if let Some(tvs) = var_tuple { for tv in tvs { collect_var_id(*tv, out); } }
            collect_varids_in_expr(iterable, out);
            for s in body { collect_varids_in_stmt(s, out); }
        }
        IrExprKind::While { cond, body } => {
            collect_varids_in_expr(cond, out);
            for s in body { collect_varids_in_stmt(s, out); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { collect_varids_in_expr(e, out); }
        }
        IrExprKind::Record { fields, .. } => { for (_, e) in fields { collect_varids_in_expr(e, out); } }
        IrExprKind::SpreadRecord { base, fields } => {
            collect_varids_in_expr(base, out);
            for (_, e) in fields { collect_varids_in_expr(e, out); }
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { collect_varids_in_expr(k, out); collect_varids_in_expr(v, out); } }
        IrExprKind::Range { start, end, .. } => { collect_varids_in_expr(start, out); collect_varids_in_expr(end, out); }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => collect_varids_in_expr(object, out),
        IrExprKind::IndexAccess { object, index } => { collect_varids_in_expr(object, out); collect_varids_in_expr(index, out); }
        IrExprKind::MapAccess { object, key } => { collect_varids_in_expr(object, out); collect_varids_in_expr(key, out); }
        IrExprKind::Lambda { params, body, .. } => {
            for (id, _) in params { collect_var_id(*id, out); }
            collect_varids_in_expr(body, out);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { collect_varids_in_expr(expr, out); } }
        }
        IrExprKind::ClosureCreate { captures, .. } => {
            for (id, _) in captures { collect_var_id(*id, out); }
        }
        IrExprKind::EnvLoad { env_var, .. } => collect_var_id(*env_var, out),
        IrExprKind::IterChain { source, steps, collector, .. } => {
            collect_varids_in_expr(source, out);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => collect_varids_in_expr(lambda, out),
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => { collect_varids_in_expr(init, out); collect_varids_in_expr(lambda, out); }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => collect_varids_in_expr(lambda, out),
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::ToVec { expr } | IrExprKind::Unwrap { expr }
        | IrExprKind::ToOption { expr } => collect_varids_in_expr(expr, out),
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_varids_in_expr(expr, out);
            collect_varids_in_expr(fallback, out);
        }
        IrExprKind::RustMacro { args, .. } => { for a in args { collect_varids_in_expr(a, out); } }
        _ => {} // literals, unit, break, continue, etc.
    }
}

fn collect_varids_in_stmt(stmt: &IrStmt, out: &mut Vec<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => { collect_var_id(*var, out); collect_varids_in_expr(value, out); }
        IrStmtKind::BindDestructure { pattern, value } => { collect_varids_in_pattern(pattern, out); collect_varids_in_expr(value, out); }
        IrStmtKind::Assign { var, value } => { collect_var_id(*var, out); collect_varids_in_expr(value, out); }
        IrStmtKind::IndexAssign { target, index, value } => { collect_var_id(*target, out); collect_varids_in_expr(index, out); collect_varids_in_expr(value, out); }
        IrStmtKind::MapInsert { target, key, value } => { collect_var_id(*target, out); collect_varids_in_expr(key, out); collect_varids_in_expr(value, out); }
        IrStmtKind::FieldAssign { target, value, .. } => { collect_var_id(*target, out); collect_varids_in_expr(value, out); }
        IrStmtKind::ListSwap { target, a, b } => { collect_var_id(*target, out); collect_varids_in_expr(a, out); collect_varids_in_expr(b, out); }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => { collect_var_id(*target, out); collect_varids_in_expr(end, out); }
        IrStmtKind::ListCopySlice { dst, src, len } => { collect_var_id(*dst, out); collect_var_id(*src, out); collect_varids_in_expr(len, out); }
        IrStmtKind::Expr { expr } => collect_varids_in_expr(expr, out),
        IrStmtKind::Guard { cond, else_ } => { collect_varids_in_expr(cond, out); collect_varids_in_expr(else_, out); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn collect_varids_in_pattern(pattern: &IrPattern, out: &mut Vec<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => collect_var_id(*var, out),
        IrPattern::Constructor { args, .. } => { for a in args { collect_varids_in_pattern(a, out); } }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => { for e in elements { collect_varids_in_pattern(e, out); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => collect_varids_in_pattern(inner, out),
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &f.pattern { collect_varids_in_pattern(p, out); } } }
        IrPattern::Literal { expr } => collect_varids_in_expr(expr, out),
        _ => {} // Wildcard, None
    }
}

// ── VarId remapping ─────────────────────────────────────────────

fn remap_id(id: VarId, remap: &HashMap<VarId, VarId>) -> VarId {
    remap.get(&id).copied().unwrap_or(id)
}

fn remap_expr_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Var { id } => *id = remap_id(*id, remap),
        IrExprKind::BinOp { left, right, .. } => { remap_expr_varids(left, remap); remap_expr_varids(right, remap); }
        IrExprKind::UnOp { operand, .. } => remap_expr_varids(operand, remap),
        IrExprKind::If { cond, then, else_ } => {
            remap_expr_varids(cond, remap);
            remap_expr_varids(then, remap);
            remap_expr_varids(else_, remap);
        }
        IrExprKind::Match { subject, arms } => {
            remap_expr_varids(subject, remap);
            for arm in arms {
                remap_pattern_varids(&mut arm.pattern, remap);
                if let Some(g) = &mut arm.guard { remap_expr_varids(g, remap); }
                remap_expr_varids(&mut arm.body, remap);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { remap_stmt_varids(s, remap); }
            if let Some(e) = expr { remap_expr_varids(e, remap); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => remap_expr_varids(object, remap),
                _ => {}
            }
            for a in args { remap_expr_varids(a, remap); }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            *var = remap_id(*var, remap);
            if let Some(tvs) = var_tuple { for tv in tvs { *tv = remap_id(*tv, remap); } }
            remap_expr_varids(iterable, remap);
            for s in body { remap_stmt_varids(s, remap); }
        }
        IrExprKind::While { cond, body } => {
            remap_expr_varids(cond, remap);
            for s in body { remap_stmt_varids(s, remap); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { remap_expr_varids(e, remap); }
        }
        IrExprKind::Record { fields, .. } => { for (_, e) in fields { remap_expr_varids(e, remap); } }
        IrExprKind::SpreadRecord { base, fields } => {
            remap_expr_varids(base, remap);
            for (_, e) in fields { remap_expr_varids(e, remap); }
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { remap_expr_varids(k, remap); remap_expr_varids(v, remap); } }
        IrExprKind::Range { start, end, .. } => { remap_expr_varids(start, remap); remap_expr_varids(end, remap); }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => remap_expr_varids(object, remap),
        IrExprKind::IndexAccess { object, index } => { remap_expr_varids(object, remap); remap_expr_varids(index, remap); }
        IrExprKind::MapAccess { object, key } => { remap_expr_varids(object, remap); remap_expr_varids(key, remap); }
        IrExprKind::Lambda { params, body, .. } => {
            for (id, _) in params { *id = remap_id(*id, remap); }
            remap_expr_varids(body, remap);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { remap_expr_varids(expr, remap); } }
        }
        IrExprKind::ClosureCreate { captures, .. } => {
            for (id, _) in captures { *id = remap_id(*id, remap); }
        }
        IrExprKind::EnvLoad { env_var, .. } => *env_var = remap_id(*env_var, remap),
        IrExprKind::IterChain { source, steps, collector, .. } => {
            remap_expr_varids(source, remap);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => remap_expr_varids(lambda, remap),
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => { remap_expr_varids(init, remap); remap_expr_varids(lambda, remap); }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => remap_expr_varids(lambda, remap),
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::ToVec { expr } | IrExprKind::Unwrap { expr }
        | IrExprKind::ToOption { expr } => remap_expr_varids(expr, remap),
        IrExprKind::UnwrapOr { expr, fallback } => {
            remap_expr_varids(expr, remap);
            remap_expr_varids(fallback, remap);
        }
        IrExprKind::RustMacro { args, .. } => { for a in args { remap_expr_varids(a, remap); } }
        _ => {} // literals, unit, break, continue, etc.
    }
}

fn remap_stmt_varids(stmt: &mut IrStmt, remap: &HashMap<VarId, VarId>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { var, value, .. } => { *var = remap_id(*var, remap); remap_expr_varids(value, remap); }
        IrStmtKind::BindDestructure { pattern, value } => { remap_pattern_varids(pattern, remap); remap_expr_varids(value, remap); }
        IrStmtKind::Assign { var, value } => { *var = remap_id(*var, remap); remap_expr_varids(value, remap); }
        IrStmtKind::IndexAssign { target, index, value } => { *target = remap_id(*target, remap); remap_expr_varids(index, remap); remap_expr_varids(value, remap); }
        IrStmtKind::MapInsert { target, key, value } => { *target = remap_id(*target, remap); remap_expr_varids(key, remap); remap_expr_varids(value, remap); }
        IrStmtKind::FieldAssign { target, value, .. } => { *target = remap_id(*target, remap); remap_expr_varids(value, remap); }
        IrStmtKind::ListSwap { target, a, b } => { *target = remap_id(*target, remap); remap_expr_varids(a, remap); remap_expr_varids(b, remap); }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => { *target = remap_id(*target, remap); remap_expr_varids(end, remap); }
        IrStmtKind::ListCopySlice { dst, src, len } => { *dst = remap_id(*dst, remap); *src = remap_id(*src, remap); remap_expr_varids(len, remap); }
        IrStmtKind::Expr { expr } => remap_expr_varids(expr, remap),
        IrStmtKind::Guard { cond, else_ } => { remap_expr_varids(cond, remap); remap_expr_varids(else_, remap); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn remap_pattern_varids(pattern: &mut IrPattern, remap: &HashMap<VarId, VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => *var = remap_id(*var, remap),
        IrPattern::Constructor { args, .. } => { for a in args { remap_pattern_varids(a, remap); } }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => { for e in elements { remap_pattern_varids(e, remap); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => remap_pattern_varids(inner, remap),
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &mut f.pattern { remap_pattern_varids(p, remap); } } }
        _ => {} // Wildcard, None, Literal (literals don't bind VarIds)
    }
}

/// Re-dispatch a type-dispatched `BinOp` when a generic binding
/// resolves to a concrete numeric width. Returns the input `op`
/// unchanged when the pairing already matches (or when the operator
/// is already kind-neutral like `Eq` / `Lt` / `ConcatStr`).
fn repair_binop_for_types(op: BinOp, left_ty: &Ty, right_ty: &Ty) -> BinOp {
    let is_float = |t: &Ty| matches!(t, Ty::Float | Ty::Float32);
    let float_pair = is_float(left_ty) || is_float(right_ty);
    match op {
        BinOp::AddInt if float_pair => BinOp::AddFloat,
        BinOp::SubInt if float_pair => BinOp::SubFloat,
        BinOp::MulInt if float_pair => BinOp::MulFloat,
        BinOp::DivInt if float_pair => BinOp::DivFloat,
        BinOp::ModInt if float_pair => BinOp::ModFloat,
        BinOp::PowInt if float_pair => BinOp::PowFloat,
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
        IrExprKind::UnOp { operand, .. } => substitute_expr_types(operand, bindings),
        IrExprKind::If { cond, then, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(then, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrExprKind::Match { subject, arms } => {
            substitute_expr_types(subject, bindings);
            for arm in arms {
                substitute_pattern_types(&mut arm.pattern, bindings);
                if let Some(g) = &mut arm.guard { substitute_expr_types(g, bindings); }
                substitute_expr_types(&mut arm.body, bindings);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { substitute_stmt_types(s, bindings); }
            if let Some(e) = expr { substitute_expr_types(e, bindings); }
        }
        IrExprKind::Call { target, args, .. } => {
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
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
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
        IrExprKind::ForIn { iterable, body, .. } => {
            substitute_expr_types(iterable, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        IrExprKind::While { cond, body } => {
            substitute_expr_types(cond, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        IrExprKind::Lambda { body, params, .. } => {
            for (_, ty) in params { *ty = substitute_ty(ty, bindings); }
            substitute_expr_types(body, bindings);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    substitute_expr_types(expr, bindings);
                }
            }
        }
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
        IrExprKind::Fan { exprs } => {
            for e in exprs { substitute_expr_types(e, bindings); }
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { substitute_expr_types(a, bindings); }
        }
        _ => {}
    }
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
        IrStmtKind::Comment { .. } => {}
    }
}
