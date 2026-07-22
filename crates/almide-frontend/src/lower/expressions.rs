// ── Expression lowering ─────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::LowerCtx;
use super::calls::{lower_call, lower_call_target};
use super::statements::lower_stmt;
use super::statements::lower_pattern;
use super::types::resolve_type_expr;

pub(super) fn lower_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let ty = ctx.expr_ty(expr);
    let span = expr.span;

    match &expr.kind {
        // ── Literals ──
        ast::ExprKind::Int { raw, .. } => {
            let value = if raw.starts_with("0x") || raw.starts_with("0X") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 16).unwrap_or(0)
            } else if raw.starts_with("0b") || raw.starts_with("0B") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 2).unwrap_or(0)
            } else if raw.starts_with("0o") || raw.starts_with("0O") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 8).unwrap_or(0)
            } else {
                raw.replace('_', "").parse::<i64>().unwrap_or(0)
            };
            ctx.mk(IrExprKind::LitInt { value }, ty, span)
        }
        ast::ExprKind::Float { value, .. } => ctx.mk(IrExprKind::LitFloat { value: *value }, ty, span),
        ast::ExprKind::String { value, .. } => ctx.mk(IrExprKind::LitStr { value: value.clone() }, ty, span),
        ast::ExprKind::Bool { value, .. } => ctx.mk(IrExprKind::LitBool { value: *value }, ty, span),
        ast::ExprKind::Unit => ctx.mk(IrExprKind::Unit, Ty::Unit, span),

        // ── Variables ──
        ast::ExprKind::Ident { name, .. } => lower_expr_ident(ctx, expr, ty, span),
        ast::ExprKind::TypeName { name, .. } => lower_expr_type_name(ctx, expr, ty, span),

        // ── Collections ──
        ast::ExprKind::List { elements, .. } => {
            let elems = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::List { elements: elems }, ty, span)
        }
        ast::ExprKind::MapLiteral { entries, .. } => {
            let pairs = entries.iter().map(|(k, v)| (lower_expr(ctx, k), lower_expr(ctx, v))).collect();
            ctx.mk(IrExprKind::MapLiteral { entries: pairs }, ty, span)
        }
        ast::ExprKind::EmptyMap => ctx.mk(IrExprKind::EmptyMap, ty, span),
        ast::ExprKind::Tuple { elements, .. } => {
            let elems: Vec<IrExpr> = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            // Type-checker fills `ty` from `expr_types`; for a tuple whose
            // element exprs depend on a pattern-bound name, that ty can be
            // `Tuple([Unknown, ..])` even when the lowered elements now
            // carry concrete types (see the same fix on `Ident`). Rebuild
            // the tuple ty from the lowered elements when the checker's ty
            // is unresolved so downstream `Some(tuple)` / `List[tuple]`
            // chains get a clean propagation path.
            let resolved_ty = if ty.has_unresolved_deep()
                && elems.iter().all(|e| !e.ty.has_unresolved_deep())
            {
                Ty::Tuple(elems.iter().map(|e| e.ty.clone()).collect())
            } else { ty };
            ctx.mk(IrExprKind::Tuple { elements: elems }, resolved_ty, span)
        }

        // ── Records ──
        ast::ExprKind::Record { name, fields, .. } => lower_expr_record(ctx, expr, ty, span),
        ast::ExprKind::SpreadRecord { base, fields, .. } => {
            let ir_base = lower_expr(ctx, base);
            let fs = fields.iter().map(|f| (f.name, lower_expr(ctx, &f.value))).collect();
            ctx.mk(IrExprKind::SpreadRecord { base: Box::new(ir_base), fields: fs }, ty, span)
        }

        // ── Operators ──
        ast::ExprKind::Binary { op, left, right, .. } => lower_expr_binary(ctx, expr, ty, span),
        ast::ExprKind::Unary { op, operand, .. } => lower_expr_unary(ctx, expr, ty, span),

        // ── Control flow ──
        ast::ExprKind::If { cond, then, else_, .. } => {
            let c = lower_expr(ctx, cond);
            let t = lower_expr(ctx, then);
            let e = lower_expr(ctx, else_);
            ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
        }
        ast::ExprKind::Match { subject, arms, .. } => lower_expr_match_arm(ctx, expr, ty, span),
        ast::ExprKind::IfLet { name, scrutinee, then, else_ } => lower_expr_if_let(ctx, expr, ty, span),
        ast::ExprKind::Block { stmts, expr, .. } => {
            ctx.push_scope();
            let body = lower_block_body(ctx, stmts, expr.as_deref(), &ty, span);
            ctx.pop_scope();
            body
        }

        ast::ExprKind::Fan { exprs, .. } => {
            let ir_exprs: Vec<IrExpr> = exprs.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::Fan { exprs: ir_exprs }, ty, span)
        }

        // ── Loops ──
        ast::ExprKind::ForIn { var, var_tuple, iterable, body, .. } => lower_expr_for_in(ctx, expr, ty, span),
        ast::ExprKind::While { cond, body, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            ctx.push_scope();
            let ir_body: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::While { cond: Box::new(ir_cond), body: ir_body }, ty, span)
        }
        ast::ExprKind::Break => ctx.mk(IrExprKind::Break, Ty::Unit, span),
        ast::ExprKind::Continue => ctx.mk(IrExprKind::Continue, Ty::Unit, span),
        ast::ExprKind::Range { start, end, inclusive, .. } => {
            let s = lower_expr(ctx, start);
            let e = lower_expr(ctx, end);
            ctx.mk(IrExprKind::Range { start: Box::new(s), end: Box::new(e), inclusive: *inclusive }, ty, span)
        }

        // ── Calls ──
        ast::ExprKind::Call { callee, args, named_args, type_args, .. } => {
            lower_call(ctx, callee, args, named_args, type_args.as_ref(), ty, span)
        }

        // ── Pipe: desugar `a |> f(b)` → `f(a, b)` ──
        ast::ExprKind::Pipe { left, right, .. } => {
            lower_pipe(ctx, left, right, ty, span)
        }

        // ── Compose: desugar `f >> g` → `(x) => g(f(x))` ──
        ast::ExprKind::Compose { .. } => lower_expr_compose(ctx, expr, ty, span),

        // ── Lambda ──
        ast::ExprKind::Lambda { params, body, .. } => {
            ctx.push_scope();
            // Get lambda type from checker to resolve inferred param types
            let lambda_param_tys: Vec<Ty> = match &ty {
                Ty::Fn { params: ptys, .. } => ptys.clone(),
                _ => vec![],
            };
            let ir_params: Vec<(VarId, Ty)> = params.iter().enumerate().map(|(i, p)| {
                let param_ty = p.ty.as_ref().map(|te| resolve_type_expr(te))
                    .or_else(|| lambda_param_tys.get(i).cloned())
                    .unwrap_or(Ty::Unknown);
                let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, None);
                (var, param_ty)
            }).collect();
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            let lambda_id = Some(ctx.next_lambda_id());
            ctx.mk(IrExprKind::Lambda { params: ir_params, body: Box::new(ir_body), lambda_id }, ty, span)
        }

        // ── Access ──
        ast::ExprKind::Member { .. } => lower_expr_member(ctx, expr, ty, span),
        ast::ExprKind::TupleIndex { object, index, .. } => {
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::TupleIndex { object: Box::new(obj), index: *index }, ty, span)
        }
        ast::ExprKind::IndexAccess { .. } => lower_expr_index_access(ctx, expr, ty, span),

        // ── String interpolation ──
        ast::ExprKind::InterpolatedString { .. } => lower_expr_interp_string(ctx, expr, ty, span),

        // ── Result / Option ──
        ast::ExprKind::Some { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::OptionSome { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Ok { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultOk { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Err { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultErr { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::None => ctx.mk(IrExprKind::OptionNone, ty, span),
        ast::ExprKind::Try { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Try { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Await { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Await { expr: Box::new(inner) }, ty, span)
        }

        // expr! — keep as Unwrap (distinct from auto-? Try)
        ast::ExprKind::Unwrap { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(inner) }, ty, span)
        }
        // expr ?? fallback — lower to match: ok(v)/some(v) → v, else → fallback
        ast::ExprKind::UnwrapOr { expr, fallback, .. } => {
            let inner = lower_expr(ctx, expr);
            let fb = lower_expr(ctx, fallback);
            // For now, use a dedicated UnwrapOr node if it exists, otherwise fallback to Call
            ctx.mk(IrExprKind::UnwrapOr { expr: Box::new(inner), fallback: Box::new(fb) }, ty, span)
        }
        // expr? — lower to ToOption
        ast::ExprKind::ToOption { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ToOption { expr: Box::new(inner) }, ty, span)
        }
        // expr?.field — keep as IR node for target-specific rendering
        ast::ExprKind::OptionalChain { expr: inner_expr, field, .. } => {
            let inner = lower_expr(ctx, inner_expr);
            ctx.mk(IrExprKind::OptionalChain { expr: Box::new(inner), field: *field }, ty, span)
        }

        // ── Misc ──
        ast::ExprKind::Paren { expr, .. } => lower_expr(ctx, expr),
        ast::ExprKind::TypeAscription { expr, ty: ascribed_te } => {
            // The ascription pins the inner expression's type (`[]: List[Int]`).
            // Lower the inner expr, then adopt the ascribed type when the inner
            // came back less resolved — an empty collection literal otherwise
            // carries an unresolved element type, which codegen renders as an
            // uninferable `Vec::<_>::new()` (native E0282) under `almide_repr`.
            // The annotation's own `TypeExpr` is the authoritative source: the
            // checker's resolved type-map entry for the ascription can still be
            // an unresolved `List[?]` when nothing outside the annotation
            // constrained the element.
            let mut inner = lower_expr(ctx, expr);
            if inner.ty.has_unresolved_deep() {
                let ascribed = resolve_type_expr(ascribed_te);
                if !ascribed.has_unresolved_deep() {
                    inner.ty = ascribed;
                } else if !ty.has_unresolved_deep() {
                    inner.ty = ty;
                }
            }
            inner
        }
        ast::ExprKind::Hole => ctx.mk(IrExprKind::Hole, ty, span),
        ast::ExprKind::Todo { message, .. } => ctx.mk(IrExprKind::Todo { message: message.clone() }, ty, span),
        ast::ExprKind::Error => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
        ast::ExprKind::Placeholder => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
    }
}

/// Lower a block body (stmts + optional tail), desugaring `guard let`. A `guard let
/// name = scrutinee else { alt }` binds `name` for the REST of the block, so everything
/// after it (the remaining stmts + the tail) becomes the Some/Ok arm of a match on the
/// scrutinee, and `alt` the wildcard arm. Statements before the guard stay as block
/// stmts. Recurses so multiple guard-lets nest. Without a guard-let it lowers normally.
/// The caller owns the block scope (push/pop around this).
fn lower_block_body(
    ctx: &mut LowerCtx,
    stmts: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    if let Some(i) = stmts.iter().position(|s| matches!(s, ast::Stmt::GuardLet { .. })) {
        let pre: Vec<IrStmt> = stmts[..i].iter().map(|s| lower_stmt(ctx, s)).collect();
        let (name, scrutinee, else_) = match &stmts[i] {
            ast::Stmt::GuardLet { name, scrutinee, else_, .. } => (*name, scrutinee, else_),
            _ => unreachable!(),
        };
        let s = lower_expr(ctx, scrutinee);
        let subject_ty = if let IrExprKind::Var { id } = &s.kind {
            let vt_ty = &ctx.var_table.get(*id).ty;
            if matches!(vt_ty, Ty::Applied(_, _)) && !matches!(&s.ty, Ty::Applied(_, _)) {
                vt_ty.clone()
            } else {
                s.ty.clone()
            }
        } else {
            s.ty.clone()
        };
        let s = if subject_ty != s.ty { IrExpr { ty: subject_ty.clone(), ..s } } else { s };
        let inner = ast::Pattern::Ident { name };
        let bind_pat = match &subject_ty {
            Ty::Applied(TypeConstructorId::Result, _) => {
                ast::Pattern::Ok { inner: Box::new(inner) }
            }
            _ => ast::Pattern::Some { inner: Box::new(inner) },
        };
        // Some/Ok arm: bind name, then the rest of the block (recurse for nested guards).
        ctx.push_scope();
        let pat1 = lower_pattern(ctx, &bind_pat, &subject_ty);
        let rest = lower_block_body(ctx, &stmts[i + 1..], tail, ty, span);
        ctx.pop_scope();
        let arm1 = IrMatchArm { pattern: pat1, guard: None, body: rest };
        // Wildcard arm: the else branch (must diverge).
        ctx.push_scope();
        let pat2 = lower_pattern(ctx, &ast::Pattern::Wildcard, &subject_ty);
        let alt = lower_expr(ctx, else_);
        ctx.pop_scope();
        let arm2 = IrMatchArm { pattern: pat2, guard: None, body: alt };
        let match_expr =
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: vec![arm1, arm2] }, ty.clone(), span);
        ctx.mk(IrExprKind::Block { stmts: pre, expr: Some(Box::new(match_expr)) }, ty.clone(), span)
    } else {
        let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
        let ir_expr = tail.map(|e| Box::new(lower_expr(ctx, e)));
        ctx.mk(IrExprKind::Block { stmts: ir_stmts, expr: ir_expr }, ty.clone(), span)
    }
}

/// Lower pipe expression, unwrapping postfix operators (??, !, ?) on the RHS
/// so the pipe targets the inner Call. e.g. `xs |> list.find(p) ?? fallback`
/// becomes `list.find(xs, p) ?? fallback` rather than treating `??` as part of the pipe target.
fn lower_pipe(ctx: &mut LowerCtx, left: &ast::Expr, right: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    match &right.kind {
        // Transparent postfix: pipe into inner, then wrap with the operator
        ast::ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
            // The inner pipe result is Option[ty] or Result[ty, _]; codegen needs the wrapper
            // type on the piped expression to generate correct match (Some/None vs Ok/Err).
            // Use the checker's resolved type for the inner expression.
            let inner_checked_ty = ctx.expr_ty(inner);
            let is_wrapper = inner_checked_ty.is_option()
                || matches!(inner_checked_ty, Ty::Applied(TypeConstructorId::Result, _));
            let inner_ty = if is_wrapper {
                inner_checked_ty
            } else {
                Ty::Applied(TypeConstructorId::Option, vec![ty.clone()])
            };
            let piped = lower_pipe(ctx, left, inner, inner_ty, span.clone());
            let ir_fallback = lower_expr(ctx, fallback);
            ctx.mk(IrExprKind::UnwrapOr { expr: Box::new(piped), fallback: Box::new(ir_fallback) }, ty, span)
        }
        ast::ExprKind::Unwrap { expr: inner, .. } => {
            // Use the checker's resolved type for the inner expression.
            // This preserves the actual error type (e.g., List[String] from result.collect)
            // instead of hardcoding String.
            let inner_checked_ty = ctx.expr_ty(inner);
            let inner_ty = if inner_checked_ty.is_result() || inner_checked_ty.is_option() {
                inner_checked_ty
            } else {
                Ty::result(ty.clone(), Ty::String)
            };
            let piped = lower_pipe(ctx, left, inner, inner_ty, span.clone());
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(piped) }, ty, span)
        }
        ast::ExprKind::Try { expr: inner, .. } => {
            let piped = lower_pipe(ctx, left, inner, ty.clone(), span.clone());
            ctx.mk(IrExprKind::ToOption { expr: Box::new(piped) }, ty, span)
        }

        // Direct pipe targets
        ast::ExprKind::Call { callee, args, type_args, .. } => {
            let ir_left = lower_expr(ctx, left);
            let mut all_args = vec![ir_left];
            all_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
            let target = lower_call_target(ctx, callee);
            let ta = type_args.as_ref().map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
            let resolved_ty = if matches!(ty, Ty::Unknown) {
                if let CallTarget::Named { name } = &target {
                    ctx.env.functions.get(name).map(|f| f.ret.clone()).unwrap_or(ty)
                } else { ty }
            } else { ty };
            ctx.mk(IrExprKind::Call { target, args: all_args, type_args: ta }, resolved_ty, span)
        }
        ast::ExprKind::Ident { .. } | ast::ExprKind::Member { .. } => {
            let ir_left = lower_expr(ctx, left);
            let target = lower_call_target(ctx, right);
            ctx.mk(IrExprKind::Call { target, args: vec![ir_left], type_args: vec![] }, ty, span)
        }
        // `a |> (n) => body` — INLINE the immediately-applied lambda to `{ let n = a; body }`.
        // A pipe RHS lambda is applied exactly once, so binding its single param to the piped value
        // and evaluating the body is identical on BOTH targets — and it avoids a Computed-callee
        // call, which v1 MIR cannot lower as a first-class closure (it silently mis-lowered
        // `5 |> (n) => n * n` to 0). Multi-param / zero-param lambdas keep the Computed-call form.
        ast::ExprKind::Lambda { params, body, .. } if params.len() == 1 => {
            let ir_left = lower_expr(ctx, left);
            let p = &params[0];
            let param_ty = p
                .ty
                .as_ref()
                .map(|te| resolve_type_expr(te))
                .unwrap_or_else(|| ctx.expr_ty(left));
            ctx.push_scope();
            let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, span.clone());
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            let bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var,
                    mutability: Mutability::Let,
                    ty: param_ty,
                    value: ir_left,
                },
                span: span.clone(),
            };
            ctx.mk(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(ir_body)) }, ty, span)
        }
        _ => {
            let ir_left = lower_expr(ctx, left);
            let ir_right = lower_expr(ctx, right);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(ir_right) },
                args: vec![ir_left], type_args: vec![],
            }, ty, span)
        }
    }
}

/// Eta-expand a module function reference (`string.len`, `list.map`, ...)
/// into a lambda that calls it. Used when the reference appears in value
/// position rather than as a callee, e.g. `xs |> list.map(string.len)`.
fn eta_expand_module_fn(
    ctx: &mut LowerCtx,
    module: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    params: Vec<Ty>,
    ret_ty: Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    ctx.push_scope();
    let mut param_vars: Vec<(VarId, Ty)> = Vec::with_capacity(params.len());
    for (i, pt) in params.iter().enumerate() {
        let name = format!("__eta_{}", i);
        let var = ctx.define_var(&name, pt.clone(), Mutability::Let, span.clone());
        param_vars.push((var, pt.clone()));
    }
    let args: Vec<IrExpr> = param_vars.iter()
        .map(|(var, pt)| ctx.mk(IrExprKind::Var { id: *var }, pt.clone(), span.clone()))
        .collect();
    // For stdlib modules (e.g. `string`) use CallTarget::Module so codegen
    // picks the stdlib runtime function. For user convention methods
    // (`Type.method`) use CallTarget::Named with the dotted key.
    let mod_name = module.as_str();
    let target = if crate::stdlib::is_stdlib_module(mod_name)
        || crate::stdlib::is_any_stdlib(mod_name)
        || ctx.env.user_modules.contains(&module)
        || ctx.env.import_table.aliases.contains_key(&module)
    {
        let resolved = ctx.env.import_table.aliases.get(&module).copied().unwrap_or(module);
        CallTarget::Module { module: resolved, func: field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", resolved, field))).copied() }
    } else {
        CallTarget::Named { name: sym(&format!("{}.{}", module, field)) }
    };
    let call = ctx.mk(IrExprKind::Call {
        target, args, type_args: vec![],
    }, ret_ty.clone(), span.clone());
    ctx.pop_scope();
    let lambda_id = Some(ctx.next_lambda_id());
    let lambda_ty = Ty::Fn {
        params: params.clone(),
        ret: Box::new(ret_ty),
    };
    ctx.mk(IrExprKind::Lambda {
        params: param_vars,
        body: Box::new(call),
        lambda_id,
    }, lambda_ty, span)
}

/// Resolve `mod.NAME` against the cross-module top-let table and build the
/// synthetic use-site Var: CLEAN uppercase name in the IR, `module_origin`
/// carrying the (versioned) module for emit-time prefixing. ONE rule shared
/// by every syntactic position that references a module top-let — reads
/// (`Member`) and assignment lvalues (`m.x = v`, #505); a position that
/// re-derives this resolution is a #500-class hole waiting to happen.
pub(super) fn module_top_let_var(
    ctx: &mut LowerCtx,
    mod_name: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    ty: &Ty,
) -> Option<(VarId, Option<almide_ir::DefId>)> {
    let resolved_mod = ctx.env.import_table.resolve(&mod_name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| mod_name.to_string());
    let qual_let_key = format!("{}.{}", resolved_mod, field);
    if !ctx.env.top_lets.contains_key(&sym(&qual_let_key)) {
        return None;
    }
    // Use the versioned module name if available (e.g. "snaidhm_v0.web.gpu")
    // to match the constant definition generated by lower_module. Exact
    // match first, then walk up parent segments to the package root (only
    // root modules have pkg_id → versioned name).
    let mod_ident = ctx.env.module_versioned_names.get(&sym(&resolved_mod))
        .map(|s| s.as_str().to_string())
        .or_else(|| {
            let parts: Vec<&str> = resolved_mod.split('.').collect();
            for i in (1..parts.len()).rev() {
                let prefix = parts[..i].join(".");
                if let Some(versioned) = ctx.env.module_versioned_names.get(&sym(&prefix)) {
                    let suffix = &resolved_mod[prefix.len()..];
                    return Some(format!("{}{}", versioned.as_str(), suffix));
                }
            }
            None
        })
        .unwrap_or_else(|| resolved_mod.clone());
    let clean_name = field.as_str().to_uppercase();
    let origin = mod_ident.replace('.', "_");
    let var_id = ctx.var_table.alloc(sym(&clean_name), ty.clone(), Mutability::Let, None);
    ctx.var_table.entries[var_id.0 as usize].module_origin = Some(origin);
    let def_id = ctx.def_map.get(&sym(&qual_let_key)).copied();
    Some((var_id, def_id))
}

include!("expressions_p2.rs");
