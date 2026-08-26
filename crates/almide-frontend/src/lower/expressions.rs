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

        ast::ExprKind::Fan { exprs, .. } => {
            let ir_exprs: Vec<IrExpr> = exprs.iter().map(|e| lower_expr(ctx, e)).collect();
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
                let param_ty = p.ty.as_ref().map(|te| resolve_type_expr(te))
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
        _ => return None,
    })
}

/// Lower a block body (stmts + optional tail), desugaring `guard let`. A `guard let
/// name = scrutinee else { alt }` binds `name` for the REST of the block, so everything
/// after it (the remaining stmts + the tail) becomes the Some/Ok arm of a match on the
/// scrutinee, and `alt` the wildcard arm. Statements before the guard stay as block
/// stmts. Recurses so multiple guard-lets nest. Without a guard-let it lowers normally.
/// The caller owns the block scope (push/pop around this).
/// Lower a LOOP BODY's statement list (#1204).
///
/// A loop body is a statement list like a block's, but it has no tail — so it
/// cannot go through `lower_block_body`, which is where `guard let` is
/// rewritten into its `match { ok/some => rest, _ => else }` form. Mapping
/// `lower_stmt` straight over the list therefore handed a `GuardLet` to
/// `lower_stmt`, whose arm is an `unreachable!("guard let is desugared by the
/// enclosing block")` — a compiler PANIC on `for … { guard let x = … else {
/// continue } … }`, which is `guard let`'s most natural use (it exists for
/// early exit, and `continue` is a loop's early exit). Present since the
/// construct landed; `spec/lang/guard_let_test.almd` never put one in a loop.
///
/// The rewrite is the block one, minus the tail: everything after the guard
/// becomes the Some/Ok arm's body, the `else` becomes the wildcard arm, and the
/// result is ONE statement in the loop body. Nested guards fall out of the
/// recursion, exactly as in a block.
fn lower_loop_body_stmts(ctx: &mut LowerCtx, body: &[ast::Stmt]) -> Vec<IrStmt> {
    let Some(i) = body.iter().position(|s| matches!(s, ast::Stmt::GuardLet { .. })) else {
        return body.iter().map(|s| lower_stmt(ctx, s)).collect();
    };
    let mut out: Vec<IrStmt> = body[..i].iter().map(|s| lower_stmt(ctx, s)).collect();
    // `lower_block_body` owns the guard rewrite; a Unit-typed, tail-less block
    // over the REST is exactly the shape it expects, and its result is a single
    // expression statement here.
    let rest = lower_block_body_in(ctx, &body[i..], None, &Ty::Unit, None, true);
    out.push(IrStmt { kind: IrStmtKind::Expr { expr: rest }, span: None });
    out
}

fn lower_block_body(
    ctx: &mut LowerCtx,
    stmts: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    lower_block_body_in(ctx, stmts, tail, ty, span, false)
}

/// [`lower_block_body`] with the LOOP-position fact (#1543). In a BLOCK, the
/// guard-let match sits in tail position, so the wildcard arm's else value IS
/// the fn's return. In a LOOP body the match is a Unit STATEMENT — a plain
/// value there is a type error (`match` arms `()` vs `Option<_>`, rustc
/// E0308 → "codegen produced invalid Rust"), and semantically the else must
/// EXIT THE FN. The IR's one fn-exit spelling from statement position is the
/// `Guard` statement, so a fn-exiting else in a loop is wrapped as
/// `{ guard false else <else>; () }` — every backend already renders a Guard's
/// return channel correctly (ok-wrapping included). A `break`/`continue` else
/// stays a bare arm (loop control is valid in arm position).
fn lower_block_body_in(
    ctx: &mut LowerCtx,
    stmts: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
    in_loop: bool,
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
        let rest = lower_block_body_in(ctx, &stmts[i + 1..], tail, ty, span, in_loop);
        ctx.pop_scope();
        let arm1 = IrMatchArm { pattern: pat1, guard: None, body: rest };
        // Wildcard arm: the else branch (must diverge).
        ctx.push_scope();
        let pat2 = lower_pattern(ctx, &ast::Pattern::Wildcard, &subject_ty);
        let mut alt = lower_expr(ctx, else_);
        ctx.pop_scope();
        let alt_is_loop_control = |e: &IrExpr| -> bool {
            matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue)
                || matches!(&e.kind, IrExprKind::Block { stmts, expr: None }
                    if stmts.len() == 1
                        && matches!(&stmts[0].kind, IrStmtKind::Expr { expr }
                            if matches!(&expr.kind, IrExprKind::Break | IrExprKind::Continue)))
        };
        if in_loop && alt_is_loop_control(&alt) {
            // Normalize a block-wrapped `{ continue }` / `{ break }` else to the
            // BARE loop-control expression: a statement-only Block in match-arm
            // position renders as `continue;` (invalid Rust in an arm without
            // braces). The bare node renders as the arm value `continue`.
            if let IrExprKind::Block { stmts, expr: None } = &alt.kind {
                if let Some(IrStmtKind::Expr { expr }) = stmts.first().map(|s| &s.kind) {
                    alt = expr.clone();
                }
            }
        }
        if in_loop && !alt_is_loop_control(&alt) {
            // See the doc comment: a fn-exiting else in a loop rides the Guard
            // statement's return channel; the arm's own value becomes Unit so
            // the statement-position match types.
            let guard_stmt = IrStmt {
                kind: IrStmtKind::Guard {
                    cond: ctx.mk(IrExprKind::LitBool { value: false }, Ty::Bool, None),
                    else_: alt,
                },
                span: None,
            };
            alt = ctx.mk(
                IrExprKind::Block {
                    stmts: vec![guard_stmt],
                    expr: Some(Box::new(ctx.mk(IrExprKind::Unit, Ty::Unit, None))),
                },
                Ty::Unit,
                None,
            );
        }
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
    let lambda_ty = Ty::Fn { is_effect: false, 
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

include!("expressions_access.rs");

/// Shared half of the two `fan.bounded` desugars: synthesize the PLAIN outlined
/// fn `__almd_bounded_N(budget, args…) -> T` (enter → body call → exit, exit
/// persists the verdict), and return `(the call expr, the verdict-read expr)`.
/// The caller decides what to build from the scalar verdict (a fused fallback
/// If, or ok/err Result nodes).
fn lower_fan_bounded_call(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (IrExpr, IrExpr) {
    let ast::ExprKind::FanBounded { budget, body } = &expr.kind else { unreachable!() };
    let budget_ir = lower_expr(ctx, budget);
    let verdict = ctx.mk(IrExprKind::RuntimeCall {
        symbol: sym("almide_rt_prim_budget_exhausted"), args: vec![],
    }, Ty::Int, span);
    let call = outline_metered_arm(ctx, budget_ir, body, span);
    (call, verdict)
}

/// The timeout twin of [`lower_fan_bounded_call`]: the region brackets with
/// the WALL-clock prims and the verdict is the persisted deadline-hit flag.
fn lower_fan_timeout_call(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (IrExpr, IrExpr) {
    let ast::ExprKind::FanTimeout { deadline, body } = &expr.kind else { unreachable!() };
    let deadline_ir = lower_expr(ctx, deadline);
    let verdict = ctx.mk(IrExprKind::RuntimeCall {
        symbol: sym("almide_rt_prim_timeout_hit"), args: vec![],
    }, Ty::Int, span);
    let call = outline_metered_arm_with(
        ctx, deadline_ir, body, span,
        "almide_rt_prim_timeout_enter", "almide_rt_prim_timeout_exit",
    );
    (call, verdict)
}

/// Outline an arbitrary lowered expression into a synthesized plain fn whose
/// params are the expression's FREE VARIABLES (each renamed to a fresh param
/// id via substitution), returning the replacement call expr. The T2-1
/// free-var machinery, reusable: the BARE fan forms use it to become a DIRECT
/// `CallFn` bind (a tracked match subject on both renderers).
fn outline_ir_as_fn(
    ctx: &mut LowerCtx,
    body: IrExpr,
    fn_prefix: &str,
    span: Option<ast::Span>,
) -> IrExpr {
    use almide_ir::{CallTarget, IrFunction, IrParam, IrVisibility, Mutability, ParamBorrow};
    let ret_ty = body.ty.clone();
    ctx.bounded_counter += 1;
    let fn_name = format!("{fn_prefix}_{}", ctx.bounded_counter);
    let mut body = body;
    let free = almide_ir::free_vars::free_vars(&body, &std::collections::HashSet::new());
    let mut params = Vec::with_capacity(free.len());
    let mut call_args = Vec::with_capacity(free.len());
    for fv in free {
        let info = ctx.var_table.get(fv);
        let (fv_ty, fv_name) = (info.ty.clone(), info.name);
        let pv = ctx.var_table.alloc(fv_name, fv_ty.clone(), Mutability::Let, span);
        let pv_expr = ctx.mk(IrExprKind::Var { id: pv }, fv_ty.clone(), span);
        body = almide_ir::substitute::substitute_var_in_expr(&body, fv, &pv_expr);
        params.push(IrParam {
            var: pv, ty: fv_ty.clone(), name: fv_name,
            borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
        });
        call_args.push(ctx.mk(IrExprKind::Var { id: fv }, fv_ty, span));
    }
    ctx.synthesized_fns.push(IrFunction {
        name: sym(&fn_name),
        params,
        ret_ty: ret_ty.clone(),
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: Vec::new(), // fresh-fn: lifted lambda, lambda params cannot be mut
    });
    ctx.mk(IrExprKind::Call {
        target: CallTarget::Named { name: sym(&fn_name) },
        args: call_args,
        type_args: vec![],
    }, ret_ty, span)
}

/// Outline ONE metered region: synthesize `__almd_bounded_N(budget, args…) -> T`
/// (enter → body call → exit; exit persists verdict + spend) and return the
/// call expr. Shared by `fan.bounded` (one region) and `fan.race` (one per arm).
fn outline_metered_arm(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body: &ast::Expr,
    span: Option<ast::Span>,
) -> IrExpr {
    outline_metered_arm_with(
        ctx, budget_arg, body, span,
        "almide_rt_prim_budget_enter", "almide_rt_prim_budget_exit",
    )
}

/// [`outline_metered_arm`] with explicit enter/exit runtime symbols — the
/// timeout head (T5-1) shares the outliner but brackets with the WALL-clock
/// prims instead of the fuel ones.
fn outline_metered_arm_with(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body: &ast::Expr,
    span: Option<ast::Span>,
    enter_sym: &str,
    exit_sym: &str,
) -> IrExpr {
    // A `{ single_call() }` body parses as a one-expr Block — unwrap it so the
    // single-call shape keeps the args-as-params path below.
    let body_ir = {
        let b = lower_expr(ctx, body);
        match b.kind {
            IrExprKind::Block { stmts, expr: Some(e) } if stmts.is_empty() => *e,
            kind => IrExpr { kind, ty: b.ty, span: b.span, def_id: b.def_id },
        }
    };
    outline_metered_arm_ir(ctx, budget_arg, body_ir, span, enter_sym, exit_sym)
}

/// The post-lowering half of [`outline_metered_arm_with`]: outline an
/// ALREADY-LOWERED region body. The mapper form (T7-1) enters here directly —
/// its region body is the mapper lambda's body with the param bound to the
/// per-iteration element var, which the free-var parameterization below turns
/// into a region param like any other capture.
fn outline_metered_arm_ir(
    ctx: &mut LowerCtx,
    budget_arg: IrExpr,
    body_ir: IrExpr,
    span: Option<ast::Span>,
    enter_sym: &str,
    exit_sym: &str,
) -> IrExpr {
    use almide_ir::{CallTarget, IrFunction, IrParam, IrStmt, IrStmtKind, IrVisibility, Mutability, ParamBorrow};
    let body_ty = body_ir.ty.clone();
    let budget_ir = budget_arg;

    let mk_rt = |ctx: &LowerCtx, name: &str, args: Vec<IrExpr>| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args }, Ty::Int, span)
    };

    ctx.bounded_counter += 1;
    let fn_name = format!("__almd_bounded_{}", ctx.bounded_counter);

    let budget_param = ctx.var_table.alloc(sym("__b_budget"), Ty::Int, Mutability::Let, span);
    let mut params = vec![IrParam {
        var: budget_param, ty: Ty::Int, name: sym("__b_budget"),
        borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
    }];
    // Two parameterization shapes:
    //  - single Call body (the v1 shape): the CALLEE'S ARGS become the params
    //    (each arg expr evaluates in the caller, before the meter starts);
    //  - anything else (block bodies, inline exprs — T2-1): the body's FREE
    //    VARIABLES become the params. Each free var is renamed to a fresh
    //    param id inside the body (VarIds stay globally unique) and the
    //    caller passes the original var. The body is PURE (checker rule), so
    //    a by-value snapshot at call time is observationally exact.
    let (metered_body, call_tail_args): (IrExpr, Vec<IrExpr>) = match body_ir.kind {
        IrExprKind::Call { target, args: body_args, type_args } => {
            let mut inner_args: Vec<IrExpr> = Vec::with_capacity(body_args.len());
            for (i, a) in body_args.iter().enumerate() {
                let pname = format!("__b_a{i}");
                let pv = ctx.var_table.alloc(sym(&pname), a.ty.clone(), Mutability::Let, span);
                params.push(IrParam {
                    var: pv, ty: a.ty.clone(), name: sym(&pname),
                    borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
                });
                inner_args.push(ctx.mk(IrExprKind::Var { id: pv }, a.ty.clone(), span));
            }
            let call = ctx.mk(
                IrExprKind::Call { target, args: inner_args, type_args },
                body_ty.clone(),
                span,
            );
            (call, body_args)
        }
        kind => {
            let mut body = IrExpr { kind, ty: body_ty.clone(), span, def_id: None };
            let free =
                almide_ir::free_vars::free_vars(&body, &std::collections::HashSet::new());
            let mut tail_args = Vec::with_capacity(free.len());
            for fv in free {
                let info = ctx.var_table.get(fv);
                let (fv_ty, fv_name) = (info.ty.clone(), info.name);
                let pv = ctx.var_table.alloc(fv_name, fv_ty.clone(), Mutability::Let, span);
                let pv_expr = ctx.mk(IrExprKind::Var { id: pv }, fv_ty.clone(), span);
                body = almide_ir::substitute::substitute_var_in_expr(&body, fv, &pv_expr);
                params.push(IrParam {
                    var: pv, ty: fv_ty.clone(), name: fv_name,
                    borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
                });
                tail_args.push(ctx.mk(IrExprKind::Var { id: fv }, fv_ty, span));
            }
            (body, tail_args)
        }
    };

    let saved_var = ctx.var_table.alloc(sym("__b_saved"), Ty::Int, Mutability::Let, span);
    let val_var = ctx.var_table.alloc(sym("__b_val"), body_ty.clone(), Mutability::Let, span);
    let exit_var = ctx.var_table.alloc(sym("__b_exit"), Ty::Int, Mutability::Let, span);
    let stmts = vec![
        IrStmt { kind: IrStmtKind::Bind {
            var: saved_var, mutability: Mutability::Let, ty: Ty::Int,
            value: mk_rt(ctx, enter_sym,
                vec![ctx.mk(IrExprKind::Var { id: budget_param }, Ty::Int, span)]),
        }, span },
        IrStmt { kind: IrStmtKind::Bind {
            var: val_var, mutability: Mutability::Let, ty: body_ty.clone(),
            value: metered_body,
        }, span },
        IrStmt { kind: IrStmtKind::Bind {
            var: exit_var, mutability: Mutability::Let, ty: Ty::Int,
            value: mk_rt(ctx, exit_sym,
                vec![ctx.mk(IrExprKind::Var { id: saved_var }, Ty::Int, span)]),
        }, span },
    ];
    let fn_body = ctx.mk(IrExprKind::Block {
        stmts,
        expr: Some(Box::new(ctx.mk(IrExprKind::Var { id: val_var }, body_ty.clone(), span))),
    }, body_ty.clone(), span);

    ctx.synthesized_fns.push(IrFunction {
        name: sym(&fn_name),
        params,
        ret_ty: body_ty.clone(),
        body: fn_body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: Vec::new(), // fresh-fn: lifted lambda, lambda params cannot be mut
    });

    let mut call_args = vec![budget_ir];
    call_args.extend(call_tail_args);
    ctx.mk(IrExprKind::Call {
        target: CallTarget::Named { name: sym(&fn_name) },
        args: call_args,
        type_args: vec![],
    }, body_ty, span)
}

/// Lower `fan.race(budget?) { arms }` into the sequential lex-min fold: each
/// arm is an outlined metered region; after each call the PERSISTED verdict and
/// spend are read as scalars; the winner is the (spend, index)-lexicographic
/// minimum among non-exhausted arms — folded with scalar if-values, so the
/// whole thing (bar the arm bodies) stays on the native rung. Returns
/// (stmts, ok_var, val_var, arm_ty): tail construction is the caller's
/// (fused fallback vs ok/err Result nodes).
fn lower_fan_race_fold(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (Vec<almide_ir::IrStmt>, almide_ir::VarId, almide_ir::VarId, Ty) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
    let ast::ExprKind::FanRace { budget, arms } = &expr.kind else { unreachable!() };

    let mk_rt = |ctx: &LowerCtx, name: &str| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args: vec![] }, Ty::Int, span)
    };
    let mk_var = |ctx: &LowerCtx, id: VarId, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::Var { id }, ty, span)
    };
    let mk_int = |ctx: &LowerCtx, v: i64| -> IrExpr {
        ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span)
    };
    let mk_if_int = |ctx: &LowerCtx, c: IrExpr, t: IrExpr, e: IrExpr, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
    };
    let mk_cmp = |ctx: &LowerCtx, op: almide_ir::BinOp, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool, span)
    };
    let bind = |ctx: &mut LowerCtx, name: &str, ty: Ty, value: IrExpr| -> (VarId, IrStmt) {
        let v = ctx.var_table.alloc(sym(name), ty.clone(), Mutability::Let, span);
        (v, IrStmt { kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty, value }, span })
    };

    let mut stmts: Vec<IrStmt> = Vec::new();
    // The budget is evaluated ONCE; the no-budget form is the i64::MAX sentinel
    // (effectively infinite — the divergence-guard is simply absent).
    let budget_ir = match budget {
        Some(b) => lower_expr(ctx, b),
        None => mk_int(ctx, i64::MAX),
    };
    let (bv, st) = bind(ctx, "__r_budget", Ty::Int, budget_ir);
    stmts.push(st);

    let mut arm_ty = Ty::Unknown;
    let mut ok_var: Option<VarId> = None;
    let mut bs_var: Option<VarId> = None;
    let mut val_var: Option<VarId> = None;

    for (i, arm) in arms.iter().enumerate() {
        let call = outline_metered_arm(ctx, mk_var(ctx, bv, Ty::Int), arm, span);
        let call_ty = call.ty.clone();
        // T2-2: a Result arm SELF-DISQUALIFIES on Err (symmetric with
        // fan.any). Its Ok payload is the candidate value.
        use almide_lang::types::constructor::TypeConstructorId;
        let res_elem = match &call_ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[0].clone()),
            _ => None,
        };
        arm_ty = res_elem.clone().unwrap_or_else(|| call_ty.clone());

        let (vi, exi, spi, arm_ok) = if let Some(elem) = res_elem {
            let (ri, st) = bind(ctx, &format!("__r_r{i}"), call_ty.clone(), call);
            stmts.push(st);
            let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
            let (exi, st) = bind(ctx, &format!("__r_ex{i}"), Ty::Int, ex);
            stmts.push(st);
            let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
            let (spi, st) = bind(ctx, &format!("__r_sp{i}"), Ty::Int, sp);
            stmts.push(st);
            use almide_ir::{IrMatchArm, IrPattern};
            // arm_ok = match ri { ok(_) => 1, err(_) => 0 }
            let subj = mk_var(ctx, ri, call_ty.clone());
            let one = mk_int(ctx, 1);
            let zero = mk_int(ctx, 0);
            let m_ok = ctx.mk(IrExprKind::Match {
                subject: Box::new(subj),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: one,
                    },
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: zero,
                    },
                ],
            }, Ty::Int, span);
            let (oki, st) = bind(ctx, &format!("__r_isok{i}"), Ty::Int, m_ok);
            stmts.push(st);
            // vi = match ri { ok(v) => v, err(_) => <default of T> } — the
            // default is dead (the ok flag guards it), it only types the arm.
            let payload = ctx.var_table.alloc(sym("__r_okv"), elem.clone(), Mutability::Let, span);
            let payload_body = mk_var(ctx, payload, elem.clone());
            let default = match &elem {
                Ty::String => ctx.mk(IrExprKind::LitStr { value: String::new() }, Ty::String, span),
                _ => mk_int(ctx, 0),
            };
            let subj2 = mk_var(ctx, ri, call_ty.clone());
            let m_val = ctx.mk(IrExprKind::Match {
                subject: Box::new(subj2),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok {
                            inner: Box::new(IrPattern::Bind { var: payload, ty: elem.clone() }),
                        },
                        guard: None,
                        body: payload_body,
                    },
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: default,
                    },
                ],
            }, elem.clone(), span);
            let (vi, st) = bind(ctx, &format!("__r_v{i}"), elem, m_val);
            stmts.push(st);
            (vi, exi, spi, Some(oki))
        } else {
            let (vi, st) = bind(ctx, &format!("__r_v{i}"), arm_ty.clone(), call);
            stmts.push(st);
            let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
            let (exi, st) = bind(ctx, &format!("__r_ex{i}"), Ty::Int, ex);
            stmts.push(st);
            let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
            let (spi, st) = bind(ctx, &format!("__r_sp{i}"), Ty::Int, sp);
            stmts.push(st);
            (vi, exi, spi, None)
        };

        // candidate = (exhausted == 0) AND (the arm's own Ok, when Result)
        let within = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, exi, Ty::Int), mk_int(ctx, 0));
        let cand = match arm_ok {
            None => within,
            Some(oki) => {
                let ok_flag = mk_if_int(ctx, within, mk_var(ctx, oki, Ty::Int), mk_int(ctx, 0), Ty::Int);
                let (ci, st) = bind(ctx, &format!("__r_cand{i}"), Ty::Int, ok_flag);
                stmts.push(st);
                mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, ci, Ty::Int), mk_int(ctx, 1))
            }
        };
        match (ok_var, bs_var, val_var) {
            (None, None, None) => {
                let cand2 = cand.clone();
                let (ok0, st) = bind(ctx, "__r_ok0", Ty::Int,
                    mk_if_int(ctx, cand, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let _ = cand2;
                ok_var = Some(ok0);
                bs_var = Some(spi);
                val_var = Some(vi);
            }
            (Some(ok_p), Some(bs_p), Some(val_p)) => {
                // better = candidate AND (no winner yet OR spend < best) —
                // strict <: ties keep the earlier index, the source-order
                // rule. The OR is 0/1 ARITHMETIC (a + b − ab) over const-arm
                // flag ifs, never a nested if-value flowing out as an arm
                // value: that shape certs the inner merge as `i{m|}` /
                // `i{|m}` — a one-sided released-merge object the
                // kernel-proven ownership checker rejects (the PCC
                // corpus-wall catch, 2026-08-03).
                let no_winner = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, ok_p, Ty::Int), mk_int(ctx, 0));
                let (nw, st) = bind(ctx, &format!("__r_nw{i}"), Ty::Int,
                    mk_if_int(ctx, no_winner, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let cheaper = mk_cmp(ctx, almide_ir::BinOp::Lt, mk_var(ctx, spi, Ty::Int), mk_var(ctx, bs_p, Ty::Int));
                let (ch, st) = bind(ctx, &format!("__r_ch{i}"), Ty::Int,
                    mk_if_int(ctx, cheaper, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let ab = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::MulInt,
                    left: Box::new(mk_var(ctx, nw, Ty::Int)),
                    right: Box::new(mk_var(ctx, ch, Ty::Int)),
                }, Ty::Int, span);
                let a_plus_b = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::AddInt,
                    left: Box::new(mk_var(ctx, nw, Ty::Int)),
                    right: Box::new(mk_var(ctx, ch, Ty::Int)),
                }, Ty::Int, span);
                let inner = ctx.mk(IrExprKind::BinOp {
                    op: almide_ir::BinOp::SubInt,
                    left: Box::new(a_plus_b),
                    right: Box::new(ab),
                }, Ty::Int, span);
                let (bet, st) = bind(ctx, &format!("__r_bet{i}"), Ty::Int,
                    mk_if_int(ctx, cand, inner, mk_int(ctx, 0), Ty::Int));
                stmts.push(st);
                let is_bet = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, bet, Ty::Int), mk_int(ctx, 1));
                let (ok_n, st) = bind(ctx, &format!("__r_ok{i}"), Ty::Int,
                    mk_if_int(ctx, is_bet.clone(), mk_int(ctx, 1), mk_var(ctx, ok_p, Ty::Int), Ty::Int));
                stmts.push(st);
                let (bs_n, st) = bind(ctx, &format!("__r_bs{i}"), Ty::Int,
                    mk_if_int(ctx, is_bet.clone(), mk_var(ctx, spi, Ty::Int), mk_var(ctx, bs_p, Ty::Int), Ty::Int));
                stmts.push(st);
                let (val_n, st) = bind(ctx, &format!("__r_val{i}"), arm_ty.clone(),
                    mk_if_int(ctx, is_bet, mk_var(ctx, vi, arm_ty.clone()), mk_var(ctx, val_p, arm_ty.clone()), arm_ty.clone()));
                stmts.push(st);
                ok_var = Some(ok_n);
                bs_var = Some(bs_n);
                val_var = Some(val_n);
            }
            _ => unreachable!(),
        }
    }
    (stmts, ok_var.unwrap(), val_var.unwrap(), arm_ty)
}

/// Lower `fan.race(budget?, xs, f)` — the MAPPER form (T7-1) — into a dynamic
/// (spend, index) lex-min fold: a while loop over `xs` calls ONE outlined
/// metered region per element (the mapper body with its param bound to the
/// element — per-branch budget, exactly the block form's rule), reads the
/// persisted verdict + spend, and folds the minimum into scalar `var`s.
/// Strict `<` keeps the earlier index on ties (the source-order rule, here
/// list order). Same return contract as [`lower_fan_race_fold`]:
/// (stmts, ok_var, val_var, winner_ty) — the caller builds the tail
/// (fused fallback vs ok/err Result nodes).
fn lower_fan_race_map_fold(
    ctx: &mut LowerCtx,
    expr: &ast::Expr,
    span: Option<ast::Span>,
) -> (Vec<almide_ir::IrStmt>, almide_ir::VarId, almide_ir::VarId, Ty) {
    use almide_ir::{IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability, VarId};
    use almide_lang::types::constructor::TypeConstructorId;
    let ast::ExprKind::FanRaceMap { budget, list, mapper } = &expr.kind else { unreachable!() };

    let mk_rt = |ctx: &LowerCtx, name: &str| -> IrExpr {
        ctx.mk(IrExprKind::RuntimeCall { symbol: sym(name), args: vec![] }, Ty::Int, span)
    };
    let mk_var = |ctx: &LowerCtx, id: VarId, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::Var { id }, ty, span)
    };
    let mk_int = |ctx: &LowerCtx, v: i64| -> IrExpr {
        ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span)
    };
    let mk_if = |ctx: &LowerCtx, c: IrExpr, t: IrExpr, e: IrExpr, ty: Ty| -> IrExpr {
        ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
    };
    let mk_cmp = |ctx: &LowerCtx, op: almide_ir::BinOp, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool, span)
    };
    let bind = |ctx: &mut LowerCtx, name: &str, m: Mutability, ty: Ty, value: IrExpr| -> (VarId, IrStmt) {
        let v = ctx.var_table.alloc(sym(name), ty.clone(), m, span);
        (v, IrStmt { kind: IrStmtKind::Bind { var: v, mutability: m, ty, value }, span })
    };
    let assign = |v: VarId, value: IrExpr| -> IrStmt {
        IrStmt { kind: IrStmtKind::Assign { var: v, value }, span }
    };
    // The winner type's dead placeholder — the ok flag guards every read, it
    // only types the slot (the block form's exact convention).
    let default_of = |ctx: &LowerCtx, t: &Ty| -> IrExpr {
        match t {
            Ty::String => ctx.mk(IrExprKind::LitStr { value: String::new() }, Ty::String, span),
            _ => ctx.mk(IrExprKind::LitInt { value: 0 }, t.clone(), span),
        }
    };

    // Types from the checker: list → List[X], winner T from the node's own
    // Result[T, String] (the g3 arm's return).
    let list_ir = lower_expr(ctx, list);
    let elem_ty = match &list_ir.ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
        _ => Ty::Unknown,
    };
    let winner_ty = match ctx.expr_ty(expr) {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
        _ => Ty::Unknown,
    };

    let mut stmts: Vec<IrStmt> = Vec::new();
    let budget_ir = match budget {
        Some(b) => lower_expr(ctx, b),
        None => mk_int(ctx, i64::MAX),
    };
    let (bv, st) = bind(ctx, "__rm_budget", Mutability::Let, Ty::Int, budget_ir);
    stmts.push(st);
    let list_ty = list_ir.ty.clone();
    let (xs, st) = bind(ctx, "__rm_xs", Mutability::Let, list_ty.clone(), list_ir);
    stmts.push(st);
    let zero = mk_int(ctx, 0);
    let (okv, st) = bind(ctx, "__rm_ok", Mutability::Var, Ty::Int, zero);
    stmts.push(st);
    let maxs = mk_int(ctx, i64::MAX);
    let (bsv, st) = bind(ctx, "__rm_bs", Mutability::Var, Ty::Int, maxs);
    stmts.push(st);
    let dflt = default_of(ctx, &winner_ty);
    let (valv, st) = bind(ctx, "__rm_val", Mutability::Var, winner_ty.clone(), dflt);
    stmts.push(st);

    // The metered region: the mapper body with its param bound to a fresh
    // element var. The outliner's free-var pass turns that var (and any outer
    // capture) into region params.
    let ast::ExprKind::Lambda { params, body } = &mapper.kind else {
        // The parser only builds this node for a 1-param lambda tail.
        unreachable!("FanRaceMap mapper is parser-gated to a 1-param lambda")
    };
    ctx.push_scope();
    let px = ctx.define_var(&params[0].name, elem_ty.clone(), Mutability::Let, span);
    let region_body = lower_expr(ctx, body);
    ctx.pop_scope();
    let mapper_ret_ty = region_body.ty.clone();
    let region_call = outline_metered_arm_ir(
        ctx, mk_var(ctx, bv, Ty::Int), region_body, span,
        "almide_rt_prim_budget_enter", "almide_rt_prim_budget_exit",
    );

    // Loop body: match list.get(xs, i) { some(x) => <arm eval + fold>, none => () }.
    let mut arm_stmts: Vec<IrStmt> = Vec::new();
    let (rv, st) = bind(ctx, "__rm_r", Mutability::Let, mapper_ret_ty.clone(), region_call);
    arm_stmts.push(st);
    let ex = mk_rt(ctx, "almide_rt_prim_budget_exhausted");
    let (exv, st) = bind(ctx, "__rm_ex", Mutability::Let, Ty::Int, ex);
    arm_stmts.push(st);
    let sp = mk_rt(ctx, "almide_rt_prim_budget_spend");
    let (spv, st) = bind(ctx, "__rm_sp", Mutability::Let, Ty::Int, sp);
    arm_stmts.push(st);
    // isok = match r { ok(_) => 1, err(_) => 0 }
    let one = mk_int(ctx, 1);
    let zero = mk_int(ctx, 0);
    let subj = mk_var(ctx, rv, mapper_ret_ty.clone());
    let m_ok = ctx.mk(IrExprKind::Match {
        subject: Box::new(subj),
        arms: vec![
            IrMatchArm { pattern: IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: one },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: zero },
        ],
    }, Ty::Int, span);
    let (isokv, st) = bind(ctx, "__rm_isok", Mutability::Let, Ty::Int, m_ok);
    arm_stmts.push(st);
    // The admission/lex-min combination is PURE 0/1 ARITHMETIC over const-arm
    // ifs — never an if-value whose ARM is another if's merge dst. That shape
    // classifies the inner merge as a RELEASED object and its one-sided flow
    // certs as `i{m|}`, which the kernel-proven ownership checker rejects
    // (the PCC corpus-wall catch, 2026-08-03). AND = multiply, OR = a + b − ab.
    let mk_flag = |ctx: &mut LowerCtx, c: IrExpr| -> IrExpr {
        mk_if(ctx, c, mk_int(ctx, 1), mk_int(ctx, 0), Ty::Int)
    };
    let mk_mul = |ctx: &LowerCtx, l: IrExpr, r: IrExpr| -> IrExpr {
        ctx.mk(IrExprKind::BinOp { op: almide_ir::BinOp::MulInt, left: Box::new(l), right: Box::new(r) }, Ty::Int, span)
    };
    // within01 = (ex == 0); cand = within01 * isok
    let within = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, exv, Ty::Int), mk_int(ctx, 0));
    let within01 = mk_flag(ctx, within);
    let cand_e = mk_mul(ctx, within01, mk_var(ctx, isokv, Ty::Int));
    let (candv, st) = bind(ctx, "__rm_cand", Mutability::Let, Ty::Int, cand_e);
    arm_stmts.push(st);
    // bet = cand * (no_winner OR cheaper) — strict <, ties keep the earlier
    // element. OR over 0/1 flags = a + b − a·b.
    let no_winner = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, okv, Ty::Int), mk_int(ctx, 0));
    let nw01 = mk_flag(ctx, no_winner);
    let (nwv, st) = bind(ctx, "__rm_nw", Mutability::Let, Ty::Int, nw01);
    arm_stmts.push(st);
    let cheaper = mk_cmp(ctx, almide_ir::BinOp::Lt, mk_var(ctx, spv, Ty::Int), mk_var(ctx, bsv, Ty::Int));
    let ch01 = mk_flag(ctx, cheaper);
    let (chv, st) = bind(ctx, "__rm_ch", Mutability::Let, Ty::Int, ch01);
    arm_stmts.push(st);
    let ab = mk_mul(ctx, mk_var(ctx, nwv, Ty::Int), mk_var(ctx, chv, Ty::Int));
    let a_plus_b = ctx.mk(IrExprKind::BinOp {
        op: almide_ir::BinOp::AddInt,
        left: Box::new(mk_var(ctx, nwv, Ty::Int)),
        right: Box::new(mk_var(ctx, chv, Ty::Int)),
    }, Ty::Int, span);
    let or_e = ctx.mk(IrExprKind::BinOp {
        op: almide_ir::BinOp::SubInt,
        left: Box::new(a_plus_b),
        right: Box::new(ab),
    }, Ty::Int, span);
    let bet_e = mk_mul(ctx, mk_var(ctx, candv, Ty::Int), or_e);
    let (betv, st) = bind(ctx, "__rm_bet", Mutability::Let, Ty::Int, bet_e);
    arm_stmts.push(st);
    // if bet == 1 then { ok = 1; bs = sp; val = payload } else ()
    let payload_var = ctx.var_table.alloc(sym("__rm_okv"), winner_ty.clone(), Mutability::Let, span);
    let payload_body = mk_var(ctx, payload_var, winner_ty.clone());
    let pdflt = default_of(ctx, &winner_ty);
    let subj2 = mk_var(ctx, rv, mapper_ret_ty.clone());
    let m_val = ctx.mk(IrExprKind::Match {
        subject: Box::new(subj2),
        arms: vec![
            IrMatchArm {
                pattern: IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: payload_var, ty: winner_ty.clone() }) },
                guard: None, body: payload_body,
            },
            IrMatchArm { pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) }, guard: None, body: pdflt },
        ],
    }, winner_ty.clone(), span);
    let win_block = ctx.mk(IrExprKind::Block {
        stmts: vec![
            assign(okv, mk_int(ctx, 1)),
            assign(bsv, mk_var(ctx, spv, Ty::Int)),
            assign(valv, m_val),
        ],
        expr: None,
    }, Ty::Unit, span);
    let unit = ctx.mk(IrExprKind::Unit, Ty::Unit, span);
    let is_bet = mk_cmp(ctx, almide_ir::BinOp::Eq, mk_var(ctx, betv, Ty::Int), mk_int(ctx, 1));
    let take = mk_if(ctx, is_bet, win_block, unit, Ty::Unit);
    arm_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: take }, span });

    // The scan is a ForIn over the list — the PROVEN loop shape (balanced
    // ownership certs everywhere in the corpus). Iteration order = list order
    // and the fold's strict `<` keeps the FIRST minimum, so the lex-min
    // tie-break needs no index variable at all. (Earlier shapes — a while +
    // `list.get` + Option match — emitted an `i{m|}` subject cert the
    // kernel-proven checker rejects: the Some arm consumes, the None arm
    // leaks. The PCC corpus-wall catch, 2026-08-03.)
    let for_e = ctx.mk(IrExprKind::ForIn {
        var: px,
        var_tuple: None,
        iterable: Box::new(mk_var(ctx, xs, list_ty.clone())),
        body: arm_stmts,
    }, Ty::Unit, span);
    stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: for_e }, span });

    (stmts, okv, valv, winner_ty)
}

/// L4 (#1108 Phase 2b): inside a FALLIBLE lambda body, rewrite every
/// Option-operand `Unwrap` into `option.to_result(e, "none")!` so the
/// closure's propagation stays a plain Result unwrap on every backend
/// (the fn-body ok_or template has no closure equivalent).
fn convert_option_unwraps_to_result(body: &mut IrExpr) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct Rw;
    impl IrMutVisitor for Rw {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Unwrap { expr: inner } = &mut e.kind else { return };
            if !inner.ty.is_option() {
                return;
            }
            let inner_ty = inner.ty.clone();
            let payload_ty = inner_ty.option_inner().unwrap_or(Ty::Unknown);
            let span = inner.span.clone();
            let opt = std::mem::replace(
                &mut **inner,
                IrExpr { kind: IrExprKind::OptionNone, ty: inner_ty, span: span.clone(), def_id: None },
            );
            **inner = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("option"),
                        func: sym("to_result"),
                        def_id: None,
                    },
                    args: vec![
                        opt,
                        IrExpr { kind: IrExprKind::LitStr { value: "none".into() }, ty: Ty::String, span: span.clone(), def_id: None },
                    ],
                    type_args: Vec::new(),
                },
                ty: Ty::result(payload_ty, Ty::String),
                span,
                def_id: None,
            };
        }
    }
    Rw.visit_expr_mut(body);
}
