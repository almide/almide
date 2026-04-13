//! Lambda Type Resolution pass (top-down).
//!
//! Resolves lambda parameter types from call-site context before closure
//! conversion. After this pass, every lambda parameter reachable from a
//! typed call site (list.map, list.filter, etc.) has a concrete type in
//! both its IR annotation and the VarTable.
//!
//! This is the "first half" of a two-pass design inspired by OCaml's
//! flambda: types are propagated top-down, then closure conversion runs
//! bottom-up on fully-typed IR.
//!
//! Postcondition: all Lambda param VarTable entries that are transitively
//! reachable from a typed list-callback call are `!is_unresolved_structural()`.

use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct LambdaTypeResolvePass;

impl NanoPass for LambdaTypeResolvePass {
    fn name(&self) -> &str { "LambdaTypeResolve" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(check_lambda_params_resolved)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for func in &mut program.functions {
            resolve_expr(&mut func.body, &mut program.var_table);
        }
        for tl in &mut program.top_lets {
            resolve_expr(&mut tl.value, &mut program.var_table);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                resolve_expr(&mut func.body, &mut module.var_table);
            }
            for tl in &mut module.top_lets {
                resolve_expr(&mut tl.value, &mut module.var_table);
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Postcondition check ─────────────────────────────────────────────

fn check_lambda_params_resolved(program: &IrProgram) -> Vec<String> {
    let mut violations = Vec::new();
    struct Checker<'a> { vt: &'a VarTable, violations: &'a mut Vec<String> }
    impl<'a> IrVisitor for Checker<'a> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Lambda { params, .. } = &expr.kind {
                for (vid, pty) in params {
                    let vt_ty = &self.vt.get(*vid).ty;
                    if pty.is_unresolved_structural() && vt_ty.is_unresolved_structural() {
                        self.violations.push(format!(
                            "Lambda param {:?} still unresolved: ir={:?} vt={:?}",
                            vid, pty, vt_ty
                        ));
                    }
                }
            }
            walk_expr(self, expr);
        }
    }
    let mut c = Checker { vt: &program.var_table, violations: &mut violations };
    for func in &program.functions { c.visit_expr(&func.body); }
    // Note: module-level checks would need module.var_table; skip for now
    // as the pass runs per-module and violations surface at WASM emit time.
    violations
}

// ── Top-down expression walker ──────────────────────────────────────
//
// Key invariant: at each Call node, we resolve lambda param types FIRST,
// then recurse into children. This means outer lambdas' params are
// resolved before inner lambdas are visited.

fn resolve_expr(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Call { target, args, .. } => {
            // 1. Resolve lambda params from call-site list element type
            resolve_call_lambdas(target, args, vt);
            // 2. Recurse into target
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    resolve_expr(object, vt);
                }
                _ => {}
            }
            // 3. Recurse into args (including lambda bodies)
            for a in args.iter_mut() {
                resolve_expr(a, vt);
            }
        }
        IrExprKind::Lambda { params, .. } => {
            // Sync param types: VarTable ↔ IR annotation (concrete wins)
            // Use .has_unresolved_deep() to catch Applied(List, [TypeVar(A)])
            for (vid, pty) in params.iter_mut() {
                if (vid.0 as usize) < vt.len() {
                    let vt_ty = vt.get(*vid).ty.clone();
                    if pty.has_unresolved_deep() && !(vt_ty).has_unresolved_deep() {
                        *pty = vt_ty;
                    } else if !pty.has_unresolved_deep() && (vt_ty).has_unresolved_deep() {
                        vt.entries[vid.0 as usize].ty = pty.clone();
                    }
                }
            }
            // Update Ty::Fn wrapper to match resolved params
            refresh_lambda_fn_ty(expr, vt);
            // Recurse into body (params are now resolved for inner lambdas to see)
            if let IrExprKind::Lambda { body, .. } = &mut expr.kind {
                resolve_expr(body, vt);
            }
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() { resolve_stmt(s, vt); }
            if let Some(e) = tail { resolve_expr(e, vt); }
        }
        IrExprKind::If { cond, then, else_ } => {
            resolve_expr(cond, vt);
            resolve_expr(then, vt);
            resolve_expr(else_, vt);
        }
        IrExprKind::Match { subject, arms } => {
            resolve_expr(subject, vt);
            for arm in arms.iter_mut() {
                if let Some(g) = &mut arm.guard { resolve_expr(g, vt); }
                resolve_expr(&mut arm.body, vt);
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            resolve_expr(iterable, vt);
            for s in body.iter_mut() { resolve_stmt(s, vt); }
        }
        IrExprKind::While { cond, body } => {
            resolve_expr(cond, vt);
            for s in body.iter_mut() { resolve_stmt(s, vt); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            resolve_expr(left, vt); resolve_expr(right, vt);
        }
        IrExprKind::UnOp { operand, .. } => resolve_expr(operand, vt),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements.iter_mut() { resolve_expr(e, vt); }
        }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, e) in fields.iter_mut() { resolve_expr(e, vt); }
        }
        IrExprKind::OptionSome { expr: inner } | IrExprKind::ResultOk { expr: inner }
        | IrExprKind::ResultErr { expr: inner } | IrExprKind::Try { expr: inner }
        | IrExprKind::Await { expr: inner } | IrExprKind::Clone { expr: inner }
        | IrExprKind::Deref { expr: inner } => resolve_expr(inner, vt),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::IndexAccess { object, .. } => resolve_expr(object, vt),
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries.iter_mut() { resolve_expr(k, vt); resolve_expr(v, vt); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts.iter_mut() {
                if let IrStringPart::Expr { expr: e } = p { resolve_expr(e, vt); }
            }
        }
        IrExprKind::Range { start, end, .. } => {
            resolve_expr(start, vt); resolve_expr(end, vt);
        }
        IrExprKind::MapAccess { object, key } => {
            resolve_expr(object, vt); resolve_expr(key, vt);
        }
        _ => {}
    }

    // Post-visit: sync expr.ty from VarTable for Var nodes,
    // and resolve TupleIndex result type from the object's Tuple type.
    match &expr.kind {
        IrExprKind::Var { id } => {
            if expr.ty.is_unresolved_structural() && (id.0 as usize) < vt.len() {
                let vt_ty = &vt.get(*id).ty;
                if !vt_ty.is_unresolved_structural() {
                    expr.ty = vt_ty.clone();
                }
            }
        }
        IrExprKind::TupleIndex { object, index } => {
            // Resolve from object's Tuple type (object.ty may have been updated above)
            let obj_ty = if let Ty::Tuple(_) = &object.ty {
                &object.ty
            } else if let IrExprKind::Var { id } = &object.kind {
                if (id.0 as usize) < vt.len() { &vt.get(*id).ty } else { &object.ty }
            } else {
                &object.ty
            };
            if let Ty::Tuple(elems) = obj_ty {
                if let Some(elem_ty) = elems.get(*index) {
                    if !elem_ty.is_unresolved_structural() && expr.ty.is_unresolved_structural() {
                        expr.ty = elem_ty.clone();
                    }
                }
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            // If BinOp result is unresolved but operands are resolved, propagate
            if expr.ty.is_unresolved_structural() {
                if !left.ty.is_unresolved_structural() {
                    expr.ty = left.ty.clone();
                } else if !right.ty.is_unresolved_structural() {
                    expr.ty = right.ty.clone();
                }
            }
        }
        _ => {}
    }
}

fn resolve_stmt(stmt: &mut IrStmt, vt: &mut VarTable) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => resolve_expr(value, vt),
        IrStmtKind::BindDestructure { value, .. } => resolve_expr(value, vt),
        IrStmtKind::Assign { value, .. } => resolve_expr(value, vt),
        IrStmtKind::IndexAssign { index, value, .. } => {
            resolve_expr(index, vt); resolve_expr(value, vt);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            resolve_expr(key, vt); resolve_expr(value, vt);
        }
        IrStmtKind::FieldAssign { value, .. } => resolve_expr(value, vt),
        IrStmtKind::Expr { expr } => resolve_expr(expr, vt),
        IrStmtKind::Guard { cond, else_ } => {
            resolve_expr(cond, vt); resolve_expr(else_, vt);
        }
        _ => {}
    }
}

// ── Call-site lambda param resolution ───────────────────────────────
//
// For `list.map(xs, (x) => ...)`, resolve `x` from the element type of `xs`.
// Also handles list.zip, list.fold accumulator, etc.

/// List callback methods whose lambda's FIRST param is the element type.
/// Form: `method(xs, f)` where `f: (elem) -> ?`.
const LIST_ELEM_FIRST_METHODS: &[&str] = &[
    "map", "filter", "filter_map", "flat_map",
    "find", "any", "all", "each", "count", "partition",
    "sort_by", "group_by", "unique_by", "take_while", "drop_while",
    "min_by", "max_by", "chunk_by", "dedup_by",
];

/// List callback methods whose lambda's SECOND param is the element type.
/// Form: `method(xs, init, f)` where `f: (acc, elem) -> acc`.
const LIST_ELEM_SECOND_METHODS: &[&str] = &[
    "fold", "scan",
];

/// List callback methods where elem is BOTH params (reduce: (elem, elem) -> elem).
const LIST_ELEM_BOTH_METHODS: &[&str] = &["reduce"];

fn resolve_call_lambdas(target: &CallTarget, args: &mut Vec<IrExpr>, vt: &mut VarTable) {
    let method_name = match target {
        CallTarget::Method { method, .. } => Some(method.as_str()),
        CallTarget::Module { module, func } if module.as_str() == "list" => Some(func.as_str()),
        _ => None,
    };
    let Some(name) = method_name else { return };
    // Determine which param(s) of the lambda receive the element type
    let elem_param_indices: &[usize] = if LIST_ELEM_FIRST_METHODS.iter().any(|m| *m == name) {
        &[0]
    } else if LIST_ELEM_SECOND_METHODS.iter().any(|m| *m == name) {
        &[1]
    } else if LIST_ELEM_BOTH_METHODS.iter().any(|m| *m == name) {
        &[0, 1]
    } else {
        return;
    };

    // Resolve list element type from first arg
    let elem_ty = match args.first() {
        Some(a) => resolve_list_elem_ty(a, vt),
        None => None,
    };
    let Some(elem_ty) = elem_ty else { return };

    // Propagate to inline Lambda params
    for arg in args.iter_mut() {
        let is_lambda = matches!(&arg.kind, IrExprKind::Lambda { .. });
        if !is_lambda { continue }

        if let IrExprKind::Lambda { params, body, .. } = &mut arg.kind {
            // Update designated param(s) — use has_deep_unresolved to catch
            // Applied(List, [TypeVar(A)]) which is_unresolved_structural() misses.
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
            // Infer return type from body + resolved params
            let body_ret = infer_body_result_ty(body, params);
            // Update Ty::Fn wrapper
            if let Ty::Fn { params: fparams, ret } = &mut arg.ty {
                for &pidx in elem_param_indices {
                    if let Some(fp) = fparams.get_mut(pidx) {
                        if fp.has_unresolved_deep() { *fp = elem_ty.clone(); }
                    }
                }
                if ret.has_unresolved_deep() {
                    if let Some(r) = body_ret { **ret = r; }
                }
            }
        }
    }
}

/// Update a Lambda expression's Ty::Fn wrapper to reflect resolved params.
fn refresh_lambda_fn_ty(expr: &mut IrExpr, _vt: &VarTable) {
    if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
        if let Ty::Fn { params: fparams, ret } = &expr.ty {
            let mut new_fparams = fparams.clone();
            let mut changed = false;
            for (i, (_, pty)) in params.iter().enumerate() {
                if let Some(fp) = new_fparams.get_mut(i) {
                    if fp.has_unresolved_deep() && !pty.has_unresolved_deep() {
                        *fp = pty.clone();
                        changed = true;
                    }
                }
            }
            let new_ret = if ret.has_unresolved_deep() {
                if let Some(r) = infer_body_result_ty(body, params) {
                    changed = true;
                    Box::new(r)
                } else {
                    ret.clone()
                }
            } else {
                ret.clone()
            };
            if changed {
                expr.ty = Ty::Fn { params: new_fparams, ret: new_ret };
            }
        }
    }
}

// ── List element type extraction ────────────────────────────────────

/// Resolve the element type of a list expression.
/// Checks: direct expr.ty → VarTable → list.zip inference.
/// Rejects types with deep unresolved components.
fn resolve_list_elem_ty(expr: &IrExpr, vt: &VarTable) -> Option<Ty> {
    // Direct type
    if let Some(elem) = extract_list_elem(&expr.ty) {
        if !(elem).has_unresolved_deep() { return Some(elem); }
    }
    // VarTable lookup for Var/EnvLoad
    let vid = match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::EnvLoad { env_var, .. } => Some(*env_var),
        _ => None,
    };
    if let Some(id) = vid {
        if (id.0 as usize) < vt.len() {
            if let Some(elem) = extract_list_elem(&vt.get(id).ty) {
                if !(elem).has_unresolved_deep() { return Some(elem); }
            }
        }
    }
    // TupleIndex: `pair.0` where pair: Tuple([List[A], List[B]]) → List[A]'s elem = A
    if let IrExprKind::TupleIndex { object, index } = &expr.kind {
        if let Some(tuple_elem) = resolve_tuple_elem_ty(object, *index, vt) {
            if let Some(elem) = extract_list_elem(&tuple_elem) {
                if !(elem).has_unresolved_deep() { return Some(elem); }
            }
        }
    }
    // list.zip(xs, ys) → Tuple(xs_elem, ys_elem)
    if let IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } = &expr.kind {
        if module.as_str() == "list" && func.as_str() == "zip" && args.len() >= 2 {
            let a = resolve_list_elem_ty(&args[0], vt);
            let b = resolve_list_elem_ty(&args[1], vt);
            if let (Some(a), Some(b)) = (a, b) {
                return Some(Ty::Tuple(vec![a, b]));
            }
        }
    }
    None
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

/// Infer a lambda parameter's type by scanning the body for operations
/// that constrain it (e.g., `p + 1.0` → p is Float).
pub(crate) fn infer_param_ty_from_body(body: &IrExpr, target: VarId) -> Option<Ty> {
    fn walk(expr: &IrExpr, target: VarId) -> Option<Ty> {
        match &expr.kind {
            IrExprKind::BinOp { left, right, .. } => {
                if let IrExprKind::Var { id } = &left.kind {
                    if *id == target && !right.ty.is_unresolved_structural() {
                        return Some(right.ty.clone());
                    }
                }
                if let IrExprKind::Var { id } = &right.kind {
                    if *id == target && !left.ty.is_unresolved_structural() {
                        return Some(left.ty.clone());
                    }
                }
                walk(left, target).or_else(|| walk(right, target))
            }
            IrExprKind::If { cond, then, else_ } => {
                walk(cond, target).or_else(|| walk(then, target)).or_else(|| walk(else_, target))
            }
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts { if let Some(t) = walk_stmt(s, target) { return Some(t); } }
                tail.as_ref().and_then(|e| walk(e, target))
            }
            IrExprKind::Match { subject, arms } => {
                walk(subject, target).or_else(|| {
                    arms.iter().find_map(|arm| walk(&arm.body, target))
                })
            }
            IrExprKind::Call { args, .. } => {
                args.iter().find_map(|a| walk(a, target))
            }
            _ => None,
        }
    }
    fn walk_stmt(stmt: &IrStmt, target: VarId) -> Option<Ty> {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } => walk(value, target),
            IrStmtKind::BindDestructure { value, .. } => walk(value, target),
            IrStmtKind::Assign { value, .. } => walk(value, target),
            IrStmtKind::Expr { expr } => walk(expr, target),
            _ => None,
        }
    }
    walk(body, target)
}
