//! ResultPropagation: lift effect fn return types and insert error propagation.
//!
//! Three phases:
//!   Phase 1 — Signature lift: effect fn ret_ty `T → Result[T, String]`
//!   Phase 2 — Body transform: resolve err() types, wrap tails in Ok()
//!   Phase 3 — Call site rewrite: update call types, insert Try (`?`)

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use almide_lang::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ResultPropagationPass;

impl NanoPass for ResultPropagationPass {
    fn name(&self) -> &str { "ResultPropagation" }

    fn targets(&self) -> Option<Vec<Target>> {
        None // Run for all targets
    }

    fn run(&self, mut program: IrProgram, target: Target) -> PassResult {
        // Result wrapping is Rust/WASM-only. TS uses ResultErasurePass.
        let wrap_non_result = matches!(target, Target::Rust | Target::Wasm);

        // `@inline_rust` / `@wasm_intrinsic` / `@intrinsic` fns dispatch
        // through per-target templates whose runtime fn returns the unwrapped
        // type. Lifting their IR signature would desync with the actual emit.
        let is_template_dispatch = |attrs: &[almide_lang::ast::Attribute]| -> bool {
            attrs.iter().any(|a| matches!(a.name.as_str(),
                "inline_rust" | "wasm_intrinsic" | "intrinsic"))
        };

        // ── Phase 1: Lift effect fn signatures ──────────────────────
        //
        // For each non-test, non-extern, non-template effect fn: T → Result[T, String].
        // Also register mangled names (almide_rt_<mod>_<fn>) so lookups succeed
        // after StdlibLowering renames call targets.

        let mut lifted_fns: HashMap<String, Ty> = HashMap::new();

        for func in &mut program.functions {
            if func.is_effect && !func.is_test && wrap_non_result && !func.ret_ty.is_result()
                && func.extern_attrs.is_empty()
                && !is_template_dispatch(&func.attrs)
            {
                let orig = std::mem::replace(&mut func.ret_ty, Ty::Unit);
                func.ret_ty = Ty::result(orig, Ty::String);
                lifted_fns.insert(func.name.to_string(), func.ret_ty.clone());
            }
        }

        for module in &mut program.modules {
            let mod_name = module.versioned_name
                .map(|v| v.to_string())
                .unwrap_or_else(|| module.name.to_string());
            let mod_ident = mod_name.replace('.', "_");
            for func in &mut module.functions {
                if func.is_effect && !func.is_test && wrap_non_result && !func.ret_ty.is_result()
                    && func.extern_attrs.is_empty()
                    && !is_template_dispatch(&func.attrs)
                {
                    let orig = std::mem::replace(&mut func.ret_ty, Ty::Unit);
                    func.ret_ty = Ty::result(orig, Ty::String);
                    let bare = func.name.to_string();
                    lifted_fns.insert(bare.clone(), func.ret_ty.clone());
                    let sanitized = bare
                        .replace(' ', "_")
                        .replace('-', "_")
                        .replace('.', "_");
                    let mangled = format!("almide_rt_{}_{}", mod_ident, sanitized);
                    lifted_fns.insert(mangled, func.ret_ty.clone());
                }
            }
        }

        // ── Phase 2: Transform lifted function bodies ───────────────
        //
        // 1. resolve_err_types: fill Unknown in err() expressions using
        //    the function's Ok type. Must run BEFORE wrap_tail_in_ok.
        // 2. wrap_tail_in_ok: wrap all exit paths in Ok(...).

        for func in &mut program.functions {
            if lifted_fns.contains_key(func.name.as_str()) {
                let ok_ty = extract_ok_type(&func.ret_ty);
                resolve_err_types(&mut func.body, &ok_ty);
                func.body = wrap_tail_in_ok(std::mem::take(&mut func.body), &lifted_fns);
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if lifted_fns.contains_key(func.name.as_str()) {
                    let ok_ty = extract_ok_type(&func.ret_ty);
                    resolve_err_types(&mut func.body, &ok_ty);
                    func.body = wrap_tail_in_ok(std::mem::take(&mut func.body), &lifted_fns);
                }
            }
        }

        // ── Phase 3: Insert auto-? at call sites ──────────────────
        //
        // The checker now types user effect fn calls as Result[T, String].
        // auto_unwrap strips Result in let/var bindings and match arms.
        // But the IR still has a gap: Call.ty = Result[T, String] while
        // Bind.ty = T. insert_try_body bridges this by wrapping Result-
        // typed calls in Try nodes.
        //
        // NOTE: update_call_types is gone — the checker provides correct
        // types. insert_try_body should eventually move to lowering.

        for func in &mut program.functions {
            if !func.is_test {
                let returns_result = func.ret_ty.is_result();
                func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
            }
            if func.is_test {
                func.body = insert_try_in_fan(std::mem::take(&mut func.body));
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let returns_result = func.ret_ty.is_result();
                func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
            }
        }

        PassResult { program, changed: true }
    }
}

// ── Phase 2: Body transformation ─────────────────────────────────────

/// Extract the Ok type T from Result[T, String].
fn extract_ok_type(ty: &Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => args[0].clone(),
        _ => Ty::Unknown,
    }
}

/// Resolve `Result[Unknown, String]` on `ResultErr` nodes and their wrappers.
///
/// When `err("msg")` appears inside an effect fn, the checker assigns
/// `Result[Unknown, String]` because `err()` alone doesn't constrain the
/// Ok type. This visitor fills the Unknown slot from the enclosing
/// function's Ok type so ConcretizeTypes' postcondition passes.
fn resolve_err_types(body: &mut IrExpr, ok_ty: &Ty) {
    struct ErrResolver<'a> { ok_ty: &'a Ty }

    impl IrMutVisitor for ErrResolver<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            // Bottom-up: resolve children first
            walk_expr_mut(self, expr);

            // ResultErr with unresolved Ok slot → fill from function ret_ty
            if matches!(&expr.kind, IrExprKind::ResultErr { .. }) {
                if expr.ty.has_unresolved_deep() {
                    expr.ty = Ty::result(self.ok_ty.clone(), Ty::String);
                }
            }

            // Try/Unwrap wrapping ResultErr: Ok type is unresolved → fill
            match &expr.kind {
                IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner } => {
                    if matches!(&inner.kind, IrExprKind::ResultErr { .. }) && expr.ty.has_unresolved_deep() {
                        expr.ty = self.ok_ty.clone();
                    }
                }
                _ => {}
            }

            // Block wrapping a single Try/Unwrap { ResultErr } or bare ResultErr
            if let IrExprKind::Block { stmts, expr: Some(tail) } = &expr.kind {
                if stmts.is_empty() && expr.ty.has_unresolved_deep() {
                    let is_err_wrapper = match &tail.kind {
                        IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }
                            => matches!(&inner.kind, IrExprKind::ResultErr { .. }),
                        IrExprKind::ResultErr { .. } => true,
                        _ => false,
                    };
                    if is_err_wrapper {
                        expr.ty = tail.ty.clone();
                    }
                }
            }
        }
    }

    ErrResolver { ok_ty }.visit_expr_mut(body);
}

/// Wrap the tail expression of an effect fn body in Ok(...).
///
/// Recurses into branching structures (Block, If, Match) to find all
/// exit paths. Guard-else bodies are divergent and never wrapped.
fn wrap_tail_in_ok(expr: IrExpr, lifted: &HashMap<String, Ty>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    match expr.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            // Wrap non-divergent guard-else bodies in Ok().
            // Divergent bodies (err(...)!, break, continue) are left as-is.
            let stmts = stmts.into_iter().map(|stmt| {
                let span = stmt.span;
                match stmt.kind {
                    IrStmtKind::Guard { cond, else_ } if !is_divergent(&else_) => IrStmt {
                        kind: IrStmtKind::Guard {
                            cond,
                            else_: wrap_tail_in_ok(else_, lifted),
                        },
                        span,
                    },
                    other => IrStmt { kind: other, span },
                }
            }).collect();
            let wrapped = wrap_tail_in_ok(*tail, lifted);
            IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(wrapped)) },
                ty: Ty::result(ty, Ty::String), span, def_id: None,
            }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(wrap_tail_in_ok(*then, lifted)),
                else_: Box::new(wrap_tail_in_ok(*else_, lifted)),
            },
            ty: Ty::result(ty, Ty::String), span, def_id: None,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject,
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern, guard: arm.guard,
                    body: wrap_tail_in_ok(arm.body, lifted),
                }).collect(),
            },
            ty: Ty::result(ty, Ty::String), span, def_id: None,
        },
        // Already Result — don't double-wrap
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => expr,
        // Call to another lifted effect fn — already returns Result
        IrExprKind::Call { ref target, .. } => {
            let callee_name = match target {
                CallTarget::Named { name } => Some(name.to_string()),
                CallTarget::Module { func, .. } => Some(func.to_string()),
                _ => None,
            };
            if callee_name.as_ref().is_some_and(|n| lifted.contains_key(n)) {
                expr
            } else {
                let result_ty = Ty::result(ty.clone(), Ty::String);
                IrExpr {
                    kind: IrExprKind::ResultOk {
                        expr: Box::new(IrExpr { kind: expr.kind, ty, span, def_id: None }),
                    },
                    ty: result_ty, span, def_id: None,
                }
            }
        }
        // ForIn/While: execute as statement, return Ok(Unit)
        kind @ (IrExprKind::ForIn { .. } | IrExprKind::While { .. }) => {
            let result_ty = Ty::result(Ty::Unit, Ty::String);
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![IrStmt {
                        kind: IrStmtKind::Expr {
                            expr: IrExpr { kind, ty, span, def_id: None },
                        },
                        span,
                    }],
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::ResultOk {
                            expr: Box::new(IrExpr {
                                kind: IrExprKind::Unit,
                                ty: Ty::Unit,
                                span, def_id: None,
                            }),
                        },
                        ty: result_ty.clone(),
                        span, def_id: None,
                    })),
                },
                ty: result_ty, span, def_id: None,
            }
        }
        // Everything else: wrap in Ok(expr)
        other => {
            let result_ty = Ty::result(ty.clone(), Ty::String);
            IrExpr {
                kind: IrExprKind::ResultOk {
                    expr: Box::new(IrExpr { kind: other, ty, span, def_id: None }),
                },
                ty: result_ty, span, def_id: None,
            }
        }
    }
}

/// Check if an expression is divergent (never produces a value).
/// Used to decide whether guard-else bodies need Ok() wrapping.
fn is_divergent(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Break | IrExprKind::Continue => true,
        // err(...)! — error propagation, always diverges
        IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner } =>
            matches!(&inner.kind, IrExprKind::ResultErr { .. }),
        // Block wrapping a divergent tail
        IrExprKind::Block { expr: Some(tail), .. } => is_divergent(tail),
        // ResultErr alone is a value, not divergent. But ResultErr
        // followed by ! (Try/Unwrap) IS divergent (handled above).
        _ => false,
    }
}

// ── Try insertion ─────────────────────────────────────────────────────
//
// The checker types user effect fn calls as Result[T, String] and
// auto_unwrap strips Result in let/var bindings. But the IR value
// expression still carries Result — insert_try bridges the gap by
// wrapping Result-typed calls in Try { expr }, producing the T that
// the binding expects.
//
// ── Try insertion ─────────────────────────────────────────────────────
//
// The checker types user effect fn calls as Result[T, String] and
// auto_unwrap strips Result in let/var/match. The IR value expression
// still carries Result — insert_try bridges the gap by wrapping
// Result-typed calls in Try, producing the T that bindings expect.
//
// This should eventually move to lowering (desugaring layer).
// update_call_types was deleted — the checker provides correct types.

fn match_has_result_arms(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }))
}

/// Collect VarIds used as match subjects with Ok/Err arms.
/// These bindings must keep Result type (no auto-unwrap).
fn collect_result_match_vars(stmts: &[IrStmt], tail: Option<&IrExpr>) -> HashSet<u32> {
    let mut vars = HashSet::new();
    for s in stmts { collect_result_match_vars_stmt(s, &mut vars); }
    if let Some(e) = tail { collect_result_match_vars_expr(e, &mut vars); }
    vars
}

fn collect_result_match_vars_expr(expr: &IrExpr, vars: &mut HashSet<u32>) {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            if match_has_result_arms(arms) {
                if let IrExprKind::Var { id } = &subject.kind {
                    vars.insert(id.0);
                }
            }
            for arm in arms { collect_result_match_vars_expr(&arm.body, vars); }
            collect_result_match_vars_expr(subject, vars);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { collect_result_match_vars_stmt(s, vars); }
            if let Some(e) = tail { collect_result_match_vars_expr(e, vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_result_match_vars_expr(cond, vars);
            collect_result_match_vars_expr(then, vars);
            collect_result_match_vars_expr(else_, vars);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_result_match_vars_expr(iterable, vars);
            for s in body { collect_result_match_vars_stmt(s, vars); }
        }
        IrExprKind::While { cond, body } => {
            collect_result_match_vars_expr(cond, vars);
            for s in body { collect_result_match_vars_stmt(s, vars); }
        }
        _ => {}
    }
}

fn collect_result_match_vars_stmt(stmt: &IrStmt, vars: &mut HashSet<u32>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_result_match_vars_expr(value, vars),
        IrStmtKind::Expr { expr } => collect_result_match_vars_expr(expr, vars),
        IrStmtKind::Guard { cond, else_ } => {
            collect_result_match_vars_expr(cond, vars);
            collect_result_match_vars_expr(else_, vars);
        }
        _ => {}
    }
}

/// Insert Try in function body. Skip tail Try if fn returns Result.
fn insert_try_body(expr: IrExpr, fn_returns_result: bool) -> IrExpr {
    if fn_returns_result {
        match expr.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let skip_unwrap = collect_result_match_vars(&stmts, Some(&tail));
                let stmts = stmts.into_iter()
                    .map(|s| insert_try_stmt_with_skip(s, &skip_unwrap))
                    .collect();
                let tail = insert_try(*tail, false);
                let tail = strip_tail_try(tail);
                return IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
                    ty: expr.ty, span: expr.span, def_id: None,
                };
            }
            _ => {
                let result = insert_try(expr, false);
                return strip_tail_try(result);
            }
        }
    }
    // Non-Result-returning fn: still skip unwrap on Result-match vars.
    if let IrExprKind::Block { stmts, expr: tail } = expr.kind {
        let skip_unwrap = collect_result_match_vars(&stmts, tail.as_deref());
        let stmts = stmts.into_iter()
            .map(|s| insert_try_stmt_with_skip(s, &skip_unwrap))
            .collect();
        let tail = tail.map(|e| Box::new(insert_try(*e, false)));
        return IrExpr {
            kind: IrExprKind::Block { stmts, expr: tail },
            ty: expr.ty, span: expr.span, def_id: None,
        };
    }
    insert_try(expr, false)
}

/// Skip Try insertion for bindings whose vars are used in Result-match subjects.
fn insert_try_stmt_with_skip(stmt: IrStmt, skip: &HashSet<u32>) -> IrStmt {
    if let IrStmtKind::Bind { var, .. } = &stmt.kind {
        if skip.contains(&var.0) {
            if let IrStmtKind::Bind { var, mutability, ty, value } = stmt.kind {
                let new_value = insert_try(value, false);
                let unwrapped = match new_value.kind {
                    IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
                    _ => new_value,
                };
                return IrStmt {
                    kind: IrStmtKind::Bind { var, mutability, ty, value: unwrapped },
                    span: stmt.span,
                };
            }
        }
    }
    insert_try_stmt(stmt)
}

/// Strip Try from tail positions of a Result-returning expression.
fn strip_tail_try(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
        IrExprKind::Match { subject, arms } => {
            let arms = arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern, guard: arm.guard,
                body: strip_tail_try(arm.body),
            }).collect();
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: expr.ty, span: expr.span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(strip_tail_try(*then)),
                else_: Box::new(strip_tail_try(*else_)),
            },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(strip_tail_try(*tail))) },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        _ => expr,
    }
}

/// Recursively insert Try around Result-returning calls.
/// `in_match_subject` suppresses wrapping (match on Result directly).
fn insert_try(expr: IrExpr, in_match_subject: bool) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let should_wrap = !in_match_subject && is_result_call(&expr);

    let kind = match expr.kind {
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(insert_try_stmt).collect(),
            expr: e.map(|e| Box::new(insert_try(*e, false))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try(*cond, false)),
            then: Box::new(insert_try(*then, false)),
            else_: Box::new(insert_try(*else_, false)),
        },
        IrExprKind::Match { subject, arms } => {
            let arms_match_result = arms.iter().any(|a|
                matches!(&a.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }));
            IrExprKind::Match {
                subject: Box::new(insert_try(*subject, arms_match_result)),
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(|g| insert_try(g, false)),
                    body: insert_try(arm.body, false),
                }).collect(),
            }
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_try(*left, false)),
            right: Box::new(insert_try(*right, false)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(insert_try(*operand, false)),
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target,
            args: args.into_iter().map(|a| insert_try(a, false)).collect(),
            type_args,
        },
        // Don't recurse into lambdas — independent scopes
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body, lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, insert_try(v, false))).collect(),
        },
        IrExprKind::OptionSome { expr: inner } => IrExprKind::OptionSome {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_try(*object, false)),
            field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_try(*expr, false)),
            field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try(*iterable, false)),
            body: body.into_iter().map(insert_try_stmt).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try(*cond, false)),
            body: body.into_iter().map(insert_try_stmt).collect(),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_try(expr, false) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(insert_try(*base, false)),
            fields: fields.into_iter().map(|(k, v)| (k, insert_try(v, false))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(insert_try(*object, false)),
            index: Box::new(insert_try(*index, false)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_try(*object, false)),
            index,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone {
            expr: Box::new(insert_try(*expr, false)),
        },
        IrExprKind::Deref { expr } => IrExprKind::Deref {
            expr: Box::new(insert_try(*expr, false)),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_try(k, false), insert_try(v, false))).collect(),
        },
        // Unwrap/Try/ToOption/UnwrapOr: the inner expression is already
        // handled, suppress outer wrap but recurse into args.
        IrExprKind::Unwrap { expr: inner } => IrExprKind::Unwrap {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::ToOption { expr: inner } => IrExprKind::ToOption {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::UnwrapOr { expr: inner, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_try(*inner, true)),
            fallback: Box::new(insert_try(*fallback, false)),
        },
        other => other,
    };

    let mut result = IrExpr { kind, ty: ty.clone(), span, def_id: None };

    if should_wrap {
        let inner_ty = match &ty {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            _ => ty,
        };
        result = IrExpr {
            kind: IrExprKind::Try { expr: Box::new(result) },
            ty: inner_ty,
            span, def_id: None,
        };
    }

    result
}

fn insert_try_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let mut new_value = insert_try(value, false);
            // Wrap Result-producing values (ok()/err()) in Try at binding site,
            // unless the binding type is itself Result (explicit matching).
            if !matches!(&new_value.kind, IrExprKind::Try { .. })
                && is_result_value(&new_value)
                && !ty.is_result()
            {
                let inner_ty = match &new_value.ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => new_value.ty.clone(),
                };
                let span = new_value.span;
                new_value = IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(new_value) },
                    ty: inner_ty,
                    span, def_id: None,
                };
            }
            let new_ty = if matches!(&new_value.kind, IrExprKind::Try { .. }) {
                new_value.ty.clone()
            } else {
                ty
            };
            IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
        }
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: insert_try(value, false),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: insert_try(expr, false),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try(cond, false),
            else_: insert_try(else_, false),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

fn is_result_call(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind, IrExprKind::Call { .. })
}

fn is_result_value(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind,
        IrExprKind::Call { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
    )
}

// ── Fan block Try insertion (test functions) ─────────────────────────

/// Insert Try only inside Fan blocks in test functions.
fn insert_try_in_fan(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let kind = match expr.kind {
        IrExprKind::Fan { exprs } => {
            IrExprKind::Fan {
                exprs: exprs.into_iter().map(|e| insert_try(e, false)).collect(),
            }
        }
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(insert_try_in_fan_stmt).collect(),
            expr: e.map(|e| Box::new(insert_try_in_fan(*e))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try_in_fan(*cond)),
            then: Box::new(insert_try_in_fan(*then)),
            else_: Box::new(insert_try_in_fan(*else_)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(insert_try_in_fan(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| insert_try_in_fan(g)),
                body: insert_try_in_fan(arm.body),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try_in_fan(*iterable)),
            body: body.into_iter().map(insert_try_in_fan_stmt).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try_in_fan(*cond)),
            body: body.into_iter().map(insert_try_in_fan_stmt).collect(),
        },
        other => other,
    };
    IrExpr { kind, ty, span, def_id: None }
}

fn insert_try_in_fan_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let new_value = insert_try_in_fan(value);
            let new_ty = if matches!(&new_value.kind, IrExprKind::Fan { .. }) {
                new_value.ty.clone()
            } else {
                ty
            };
            IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
        }
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_try_in_fan(expr) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try_in_fan(cond),
            else_: insert_try_in_fan(else_),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: insert_try_in_fan(value),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}
