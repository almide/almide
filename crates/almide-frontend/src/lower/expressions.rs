// ── Expression lowering ─────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::LowerCtx;
use super::calls::{lower_call, lower_call_target};
use super::statements::lower_stmt;
use super::statements::lower_pattern;
use super::types::resolve_type_expr_env;

pub(super) fn lower_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let mut e = lower_expr_dispatch(ctx, expr);
    // #880: the PEER-JOIN shapes — a list literal and the arms of an `if` /
    // `match` — get their width from the checker's join, not from an
    // annotation, and their bare literal members stay at the default `Ty::Int`.
    // `[1, u8v]` therefore emitted `vec![1i64, 3u8]` into a `Vec<u8>` slot,
    // which rustc rejects. The declared-type path already runs this coercion
    // from a `let xs: List[UInt8]` / `let v: UInt8` annotation; the node's OWN
    // inferred type is the same authority when there is no annotation, so run
    // it here from that — one rule, both spellings. Restricted to those three
    // kinds: every other slot with a width already coerces where it is
    // declared, and this runs on every lowered node.
    if matches!(e.kind, IrExprKind::List { .. } | IrExprKind::If { .. } | IrExprKind::Match { .. }) {
        let own = e.ty.clone();
        super::statements::coerce_literal_to_sized(&mut e, &own, ctx.env);
    }
    e
}

fn lower_expr_dispatch(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let ty = ctx.expr_ty(expr);
    let span = expr.span;

    if let Some(e) = lower_expr_literal(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_collection(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_operator(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_control(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_call(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_lambda(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_access(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_variant(ctx, expr, ty.clone(), span) { return e; }
    if let Some(e) = lower_expr_misc(ctx, expr, ty.clone(), span) { return e; }
    unreachable!("lower_expr: no lowering for {:?}", std::mem::discriminant(&expr.kind))
}

/// Literals and variable references.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_literal(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Literals ──
        ast::ExprKind::Int { raw, .. } => {
            let value = crate::literals::int_value(raw);
            ctx.mk(IrExprKind::LitInt { value }, ty, span)
        }
        ast::ExprKind::Float { value, .. } => ctx.mk(IrExprKind::LitFloat { value: *value }, ty, span),
        ast::ExprKind::String { value, .. } => ctx.mk(IrExprKind::LitStr { value: value.clone() }, ty, span),
        ast::ExprKind::Bool { value, .. } => ctx.mk(IrExprKind::LitBool { value: *value }, ty, span),
        ast::ExprKind::Unit => ctx.mk(IrExprKind::Unit, Ty::Unit, span),
        // ── Variables ──
        ast::ExprKind::Ident { name: _, .. } => lower_expr_ident(ctx, expr, ty, span),
        ast::ExprKind::TypeName { name: _, .. } => lower_expr_type_name(ctx, expr, ty, span),
        _ => return None,
    })
}

/// Collection and record construction.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_collection(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
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
        ast::ExprKind::Record { name: _, fields: _, .. } => lower_expr_record(ctx, expr, ty, span),
        ast::ExprKind::SpreadRecord { base, fields, .. } => {
            let ir_base = lower_expr(ctx, base);
            let fs = fields.iter().map(|f| (f.name, lower_expr(ctx, &f.value))).collect();
            ctx.mk(IrExprKind::SpreadRecord { base: Box::new(ir_base), fields: fs }, ty, span)
        }
        _ => return None,
    })
}

/// Operators.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_operator(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Operators ──
        ast::ExprKind::Binary { op: _, left: _, right: _, .. } => lower_expr_binary(ctx, expr, ty, span),
        ast::ExprKind::Unary { op: _, operand: _, .. } => lower_expr_unary(ctx, expr, ty, span),
        _ => return None,
    })
}

/// Control flow and loops.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_control(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Control flow ──
        ast::ExprKind::If { cond, then, else_, .. } => {
            let c = lower_expr(ctx, cond);
            let t = lower_expr(ctx, then);
            let e = lower_expr(ctx, else_);
            ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
        }
        ast::ExprKind::Match { subject: _, arms: _, .. } => lower_expr_match_arm(ctx, expr, ty, span),
        ast::ExprKind::IfLet { name: _, scrutinee: _, then: _, else_: _ } => lower_expr_if_let(ctx, expr, ty, span),
        ast::ExprKind::Block { stmts, expr, .. } => {
            ctx.push_scope();
            let body = lower_block_body(ctx, stmts, expr.as_deref(), &ty, span);
            ctx.pop_scope();
            body
        }
        // `scoped { … }` (#1997): the body is OUTLINED into a synthesized fn
        // `__almd_scoped_N(captures…)` marked as a scoped ENTRY, and the site
        // becomes one call to it. The checker admitted the block, so every
        // capture is a scalar (a by-value snapshot is exact), nothing inside
        // assigns an outer binding, and no `!` / `guard` / `break` crosses the
        // boundary — outlining changes no observable behaviour. The call is
        // the region boundary each leg honours (docs/specs/scoped.md).
        ast::ExprKind::Scoped { body, .. } => {
            let body_ir = lower_expr(ctx, body);
            let call = outline_ir_as_fn(ctx, body_ir, "__almd_scoped", span);
            if let Some(f) = ctx.synthesized_fns.last_mut() {
                f.attrs.push(almide_ir::IrFunction::scoped_marker(almide_ir::SCOPED_BLOCK_ATTR));
            }
            call
        }

        ast::ExprKind::Fan { exprs, .. } => {
            let ir_exprs: Vec<IrExpr> = exprs.iter()
                .map(|e| { let arm = lower_expr(ctx, e); fan_arm_scope(ctx, arm) })
                .collect();
            ctx.mk(IrExprKind::Fan { exprs: ir_exprs }, ty, span)
        }
        // fan.bounded(budget) { body } — Stage 2 v1 desugar by OUTLINING.
        // The region becomes a synthesized PLAIN fn `__almd_bounded_N(budget,
        // args…) -> T` whose body is: budget_enter → the body call over the
        // params → budget_exit (which PERSISTS the exhaustion verdict). The
        // call site reads the verdict as a SCALAR and builds the result —
        // `bounded ?? fb` fuses into a fully scalar If (the native rung has no
        // heap-Result ABI yet); the bare form yields ok/err Result nodes.
        ast::ExprKind::FanBounded { .. } => {
            let (call, verdict) = lower_fan_bounded_call(ctx, expr, span);
            let body_ty = call.ty.clone();
            let result_ty = ty.clone(); // Result[T, String] from the checker
            let val_var = ctx.var_table.alloc(sym("__b_v"), body_ty.clone(), almide_ir::Mutability::Let, span);
            let ex_var = ctx.var_table.alloc(sym("__b_ex2"), Ty::Int, almide_ir::Mutability::Let, span);
            let stmts = vec![
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: val_var, mutability: almide_ir::Mutability::Let, ty: body_ty.clone(), value: call,
                }, span },
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: ex_var, mutability: almide_ir::Mutability::Let, ty: Ty::Int, value: verdict,
                }, span },
            ];
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ex_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let err_arm = ctx.mk(IrExprKind::ResultErr {
                expr: Box::new(ctx.mk(IrExprKind::LitStr {
                    value: "fan.bounded: budget exhausted".to_string(),
                }, Ty::String, span)),
            }, result_ty.clone(), span);
            let ok_arm = ctx.mk(IrExprKind::ResultOk {
                expr: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty, span)),
            }, result_ty.clone(), span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(err_arm),
                else_: Box::new(ok_arm),
            }, result_ty.clone(), span);
            let full = ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, result_ty.clone(), span);
            // The BARE form outlines the whole region+wrap into a synthesized
            // Result-returning plain fn (T1-3): the call site becomes a DIRECT
            // `CallFn` bind both renderers track — wasm materializes the fn's
            // result block as for any user fn, native rides the Res carrier.
            outline_ir_as_fn(ctx, full, "__almd_res", span)
        }
        // fan.race(budget?) { arms } — the bare form yields ok/err Result nodes
        // over the lex-min fold (wasm renders it; the native rung's heap-Result
        // wall applies as with bare bounded — the fused `?? fb` form below is
        // the fully scalar path).
        // fan.settle { arms } — SEQUENTIAL settle (T2-4): each arm evaluates
        // in arm order into its own Result slot (a plain arm wraps in Ok, a
        // Result arm passes through — its Err is CAPTURED, never propagated),
        // and the value is the tuple of the slots. The pinned contract is the
        // RESULT order; sequential evaluation realizes it deterministically
        // on every leg.
        ast::ExprKind::FanSettle { arms } => {
            use almide_lang::types::constructor::TypeConstructorId;
            // A tuple literal evaluates its elements in exactly arm order
            // (the same guarantee the fan{} desugar rides), so the settle IS
            // the literal — and a destructuring bind then splits it into
            // DIRECT per-arm binds the downstream match tracking understands.
            let elems: Vec<IrExpr> = arms
                .iter()
                .map(|arm| {
                    let a = lower_expr(ctx, arm);
                    match &a.ty {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => a,
                        _ => {
                            let rt = Ty::result(a.ty.clone(), Ty::String);
                            ctx.mk(IrExprKind::ResultOk { expr: Box::new(a) }, rt, span)
                        }
                    }
                })
                .collect();
            ctx.mk(IrExprKind::Tuple { elements: elems }, ty, span)
        }
        ast::ExprKind::FanTimeout { .. } => {
            let (call, verdict) = lower_fan_timeout_call(ctx, expr, span);
            let body_ty = call.ty.clone();
            let result_ty = ty.clone();
            let val_var = ctx.var_table.alloc(sym("__t_v"), body_ty.clone(), almide_ir::Mutability::Let, span);
            let ex_var = ctx.var_table.alloc(sym("__t_hit2"), Ty::Int, almide_ir::Mutability::Let, span);
            let stmts = vec![
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: val_var, mutability: almide_ir::Mutability::Let, ty: body_ty.clone(), value: call,
                }, span },
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: ex_var, mutability: almide_ir::Mutability::Let, ty: Ty::Int, value: verdict,
                }, span },
            ];
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ex_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let err_arm = ctx.mk(IrExprKind::ResultErr {
                expr: Box::new(ctx.mk(IrExprKind::LitStr {
                    value: "fan.timeout: deadline exceeded".to_string(),
                }, Ty::String, span)),
            }, result_ty.clone(), span);
            let ok_arm = ctx.mk(IrExprKind::ResultOk {
                expr: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty, span)),
            }, result_ty.clone(), span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(err_arm),
                else_: Box::new(ok_arm),
            }, result_ty.clone(), span);
            let full = ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, result_ty.clone(), span);
            // Same BARE-form outlining as bounded (a tracked direct CallFn).
            outline_ir_as_fn(ctx, full, "__almd_res", span)
        }
        ast::ExprKind::FanRaceMap { .. } => {
            // Same tail construction as the block form below — only the fold
            // differs (dynamic while-scan vs static unrolled arms).
            let result_ty = ty.clone();
            let (stmts, ok_var, val_var, arm_ty) = lower_fan_race_map_fold(ctx, expr, span);
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ok_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let ok_arm = ctx.mk(IrExprKind::ResultOk {
                expr: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, arm_ty, span)),
            }, result_ty.clone(), span);
            let err_arm = ctx.mk(IrExprKind::ResultErr {
                expr: Box::new(ctx.mk(IrExprKind::LitStr {
                    value: "fan.race: no branch completed within budget".to_string(),
                }, Ty::String, span)),
            }, result_ty.clone(), span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(ok_arm),
                else_: Box::new(err_arm),
            }, result_ty.clone(), span);
            let full = ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, result_ty.clone(), span);
            outline_ir_as_fn(ctx, full, "__almd_res", span)
        }
        ast::ExprKind::FanRace { .. } => {
            let result_ty = ty.clone();
            let (stmts, ok_var, val_var, arm_ty) = lower_fan_race_fold(ctx, expr, span);
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ok_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let ok_arm = ctx.mk(IrExprKind::ResultOk {
                expr: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, arm_ty, span)),
            }, result_ty.clone(), span);
            let err_arm = ctx.mk(IrExprKind::ResultErr {
                expr: Box::new(ctx.mk(IrExprKind::LitStr {
                    value: "fan.race: no branch completed within budget".to_string(),
                }, Ty::String, span)),
            }, result_ty.clone(), span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(ok_arm),
                else_: Box::new(err_arm),
            }, result_ty.clone(), span);
            let full = ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, result_ty.clone(), span);
            // Same BARE-form outlining as bounded (see there).
            outline_ir_as_fn(ctx, full, "__almd_res", span)
        }
        // ── Loops ──
        ast::ExprKind::ForIn { var: _, var_tuple: _, iterable: _, body: _, .. } => lower_expr_for_in(ctx, expr, ty, span),
        ast::ExprKind::While { cond, body, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            ctx.push_scope();
            let ir_body: Vec<IrStmt> = lower_loop_body_stmts(ctx, body);
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
        _ => return None,
    })
}

/// Calls, and the pipe/compose desugars.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_call(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Calls ──
        ast::ExprKind::Call { callee, args, named_args, type_args, .. } => {
            lower_call(ctx, callee, super::calls::CallArgs {
                args, named_args, type_args: type_args.as_ref(),
            }, ty, span)
        }
        // ── Pipe: desugar `a |> f(b)` → `f(a, b)` ──
        ast::ExprKind::Pipe { left, right, .. } => {
            lower_pipe(ctx, left, right, ty, span)
        }
        // ── Compose: desugar `f >> g` → `(x) => g(f(x))` ──
        ast::ExprKind::Compose { .. } => lower_expr_compose(ctx, expr, ty, span),
        _ => return None,
    })
}

/// Lambdas.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_lambda(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Lambda ──
        ast::ExprKind::Lambda { params, body, .. } => {
            ctx.push_scope();
            // Get lambda type from checker to resolve inferred param types
            let lambda_param_tys: Vec<Ty> = match &ty {
                Ty::Fn { params: ptys, .. } => ptys.clone(),
                _ => vec![],
            };
            // A tuple-pattern parameter stays ONE runtime parameter and is
            // destructured on entry, which is exactly what the documented
            // `let (a, b) = entry` workaround did by hand (#1060).
            let mut destructure: Vec<IrStmt> = Vec::new();
            let ir_params: Vec<(VarId, Ty)> = params.iter().enumerate().map(|(i, p)| {
                let param_ty = p.ty.as_ref().map(|te| resolve_type_expr_env(ctx, te))
                    .or_else(|| lambda_param_tys.get(i).cloned())
                    .unwrap_or(Ty::Unknown);
                match &p.tuple_names {
                    Some(names) if names.len() > 1 => {
                        let var = ctx.define_var(
                            &format!("__tuple_param_{}", i), param_ty.clone(), Mutability::Let, None);
                        let elem_tys: Vec<Ty> = match &param_ty {
                            Ty::Tuple(es) if es.len() == names.len() => es.clone(),
                            _ => vec![Ty::Unknown; names.len()],
                        };
                        let elements: Vec<IrPattern> = names.iter().zip(elem_tys.iter())
                            .map(|(n, et)| {
                                let v = ctx.define_var(n, et.clone(), Mutability::Let, None);
                                IrPattern::Bind { var: v, ty: et.clone() }
                            })
                            .collect();
                        let value = ctx.mk(IrExprKind::Var { id: var }, param_ty.clone(), None);
                        destructure.push(IrStmt {
                            kind: IrStmtKind::BindDestructure {
                                pattern: IrPattern::Tuple { elements }, value,
                            },
                            span: None,
                        });
                        (var, param_ty)
                    }
                    _ => {
                        let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, None);
                        (var, param_ty)
                    }
                }
            }).collect();
            let mut ir_body = lower_expr(ctx, body);
            if !destructure.is_empty() {
                let body_ty = ir_body.ty.clone();
                ir_body = ctx.mk(IrExprKind::Block {
                    stmts: destructure, expr: Some(Box::new(ir_body)),
                }, body_ty, span);
            }
            ctx.pop_scope();
            // ADR-0006 D1 (#1108 Phase 2b): a FALLIBLE lambda — the checker
            // typed it `(A) -> Result[T, String]` while its body's value
            // exits are still T — gets the same value-tail ok(...) lift a
            // `-> T!` fn body gets. Type-driven: Result-typed exits pass
            // through, so a pass-through / explicit-ok body is untouched.
            if let Ty::Fn { ret, .. } = &ty {
                if ret.is_result() {
                    // An OPTION operand's `!` maps none → err("none") (L4).
                    // Inside a fn body the codegen's ok_or template does this;
                    // a CLOSURE body lacks that context on every backend, so
                    // desugar it here: `e!` (e: Option[T]) becomes
                    // `option.to_result(e, "none")!` — a plain Result unwrap
                    // all three consumers already handle.
                    convert_option_unwraps_to_result(&mut ir_body);
                    if !ir_body.ty.is_result() {
                        // The lambda's E comes off its checked Result return
                        // (ADR-0012 D2: typed-E fallible callbacks lift with
                        // their own E, not the String default).
                        let err_ty = match &**ret {
                            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                                a[1].clone()
                            }
                            _ => Ty::String,
                        };
                        ir_body = crate::lower::wrap_fallible_value_tail(ir_body, &err_ty);
                    }
                }
            }
            let lambda_id = Some(ctx.next_lambda_id());
            ctx.mk(IrExprKind::Lambda { params: ir_params, body: Box::new(ir_body), lambda_id }, ty, span)
        }
        _ => return None,
    })
}

/// Member/index access and string interpolation.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_access(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
        // ── Access ──
        ast::ExprKind::Member { .. } => lower_expr_member(ctx, expr, ty, span),
        ast::ExprKind::TupleIndex { object, index, .. } => {
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::TupleIndex { object: Box::new(obj), index: *index }, ty, span)
        }
        ast::ExprKind::IndexAccess { .. } => lower_expr_index_access(ctx, expr, ty, span),
        // ── String interpolation ──
        ast::ExprKind::InterpolatedString { .. } => lower_expr_interp_string(ctx, expr, ty, span),
        _ => return None,
    })
}

/// `Result`/`Option` construction and the unwrap family.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_variant(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
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

        // expr! — keep as Unwrap (distinct from auto-? Try)
        ast::ExprKind::Unwrap { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            // #1049: the checker admits `!` on a never-err effect call as a
            // no-op — the value is already the raw T. Erase the node here so
            // no downstream pass ever sees an Unwrap over a non-container
            // type. Unknown/TypeVar keep the node (error recovery / generic
            // slots resolve later).
            let is_container = inner.ty.result_ok_ty().is_some()
                || inner.ty.option_inner().is_some()
                || matches!(inner.ty, Ty::Unknown | Ty::TypeVar(_));
            if !is_container {
                return Some(inner);
            }
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(inner) }, ty, span)
        }
        // expr ?? fallback — lower to match: ok(v)/some(v) → v, else → fallback
        ast::ExprKind::UnwrapOr { expr, fallback, .. }
            if matches!(expr.kind, ast::ExprKind::FanRaceMap { .. }) =>
        {
            // FUSED `race-mapper ?? fb`: winner-or-fallback as a scalar If —
            // the exact shape of the block form's fused arm below.
            let (stmts, ok_var, val_var, arm_ty) = lower_fan_race_map_fold(ctx, expr, span);
            let fb_ir = lower_expr(ctx, fallback);
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ok_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, arm_ty.clone(), span)),
                else_: Box::new(fb_ir),
            }, arm_ty.clone(), span);
            ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, arm_ty, span)
        }
        ast::ExprKind::UnwrapOr { expr, fallback, .. }
            if matches!(expr.kind, ast::ExprKind::FanRace { .. }) =>
        {
            // FUSED `race ?? fb`: winner-or-fallback as a scalar If — no Result
            // value exists, so the shape renders on the native rung today.
            let (stmts, ok_var, val_var, arm_ty) = lower_fan_race_fold(ctx, expr, span);
            let fb_ir = lower_expr(ctx, fallback);
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ok_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, arm_ty.clone(), span)),
                else_: Box::new(fb_ir),
            }, arm_ty.clone(), span);
            ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, arm_ty, span)
        }
        ast::ExprKind::UnwrapOr { expr, fallback, .. }
            if matches!(expr.kind, ast::ExprKind::FanTimeout { .. }) =>
        {
            // FUSED `timeout ?? fb` — the same fully-scalar shape as
            // bounded's below, with the wall-clock bracket instead.
            let (call, verdict) = lower_fan_timeout_call(ctx, expr, span);
            let body_ty = call.ty.clone();
            let fb_ir = lower_expr(ctx, fallback);
            let val_var = ctx.var_table.alloc(sym("__t_v"), body_ty.clone(), almide_ir::Mutability::Let, span);
            let ex_var = ctx.var_table.alloc(sym("__t_hit2"), Ty::Int, almide_ir::Mutability::Let, span);
            let stmts = vec![
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: val_var, mutability: almide_ir::Mutability::Let, ty: body_ty.clone(), value: call,
                }, span },
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: ex_var, mutability: almide_ir::Mutability::Let, ty: Ty::Int, value: verdict,
                }, span },
            ];
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ex_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(fb_ir),
                else_: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty.clone(), span)),
            }, body_ty.clone(), span);
            ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, body_ty, span)
        }
        ast::ExprKind::UnwrapOr { expr, fallback, .. }
            if matches!(expr.kind, ast::ExprKind::FanBounded { .. }) =>
        {
            // FUSED `bounded ?? fb`: verdict and fallback stay SCALAR — no
            // Result value ever exists, so the shape renders on the native
            // rung today. Semantically identical to unwrap-or over the
            // general form (the verdict decides which branch is observed).
            let (call, verdict) = lower_fan_bounded_call(ctx, expr, span);
            let body_ty = call.ty.clone();
            let fb_ir = lower_expr(ctx, fallback);
            let val_var = ctx.var_table.alloc(sym("__b_v"), body_ty.clone(), almide_ir::Mutability::Let, span);
            let ex_var = ctx.var_table.alloc(sym("__b_ex2"), Ty::Int, almide_ir::Mutability::Let, span);
            let stmts = vec![
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: val_var, mutability: almide_ir::Mutability::Let, ty: body_ty.clone(), value: call,
                }, span },
                almide_ir::IrStmt { kind: almide_ir::IrStmtKind::Bind {
                    var: ex_var, mutability: almide_ir::Mutability::Let, ty: Ty::Int, value: verdict,
                }, span },
            ];
            let cond = ctx.mk(IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(ctx.mk(IrExprKind::Var { id: ex_var }, Ty::Int, span)),
                right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
            }, Ty::Bool, span);
            let tail = ctx.mk(IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(fb_ir),
                else_: Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty.clone(), span)),
            }, body_ty.clone(), span);
            ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, body_ty, span)
        }
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
        _ => return None,
    })
}

/// Everything else, including the forms that lower to a diagnostic.
///
/// Extracted from `lower_expr` (name-router split): `None` means "not my group".
/// The groups are the comment sections the function already carried. Lowering
/// reads types from the checker's `TypeMap` and never re-infers, so a split here
/// cannot change inference — but a DROPPED arm would silently lower an expression
/// to nothing, which is why the router aborts loudly instead of falling through.
fn lower_expr_misc(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> Option<IrExpr> {
    Some(match &expr.kind {
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
                let ascribed = resolve_type_expr_env(ctx, ascribed_te);
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
        _ => return None,
    })
}

include!("expressions_block.rs");

include!("expressions_access.rs");

include!("expressions_fan.rs");
