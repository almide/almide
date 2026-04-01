//! Tail Call Optimization pass: converts self-recursive tail calls into loops.
//!
//! Transforms:
//! ```text
//! fn sum_to(n: Int, acc: Int) -> Int =
//!   if n <= 0 then acc else sum_to(n - 1, acc + n)
//! ```
//! Into (conceptually):
//! ```text
//! fn sum_to(n: Int, acc: Int) -> Int {
//!   var __tco_result = <default>
//!   while true {
//!     if n <= 0 { __tco_result = acc; break }
//!     else { let __t0 = n - 1; let __t1 = acc + n; n = __t0; acc = __t1; continue }
//!   }
//!   __tco_result
//! }
//! ```
//!
//! This eliminates stack growth for self-recursive tail calls, critical for
//! WASM where the stack is limited and there is no native tail call support.

use almide_ir::*;
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct TailCallOptPass;

impl NanoPass for TailCallOptPass {
    fn name(&self) -> &str { "TailCallOpt" }

    fn targets(&self) -> Option<Vec<Target>> {
        None // All targets benefit from TCO
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        run_tco(&mut program.functions, &mut program.var_table);
        for module in &mut program.modules {
            run_tco(&mut module.functions, &mut module.var_table);
        }
        PassResult { program, changed: true }
    }
}

fn run_tco(functions: &mut [IrFunction], var_table: &mut VarTable) {
    for func in functions.iter_mut() {
        if is_tco_candidate(func) {
            rewrite_to_loop(func, var_table);
        }
    }
}

/// Returns true if the function is eligible for TCO:
/// - Has at least one self-recursive call
/// - ALL self-recursive calls are in tail position
/// - Not a test helper (name starts with `__test_`)
/// - Return type can be default-initialized (primitives, tuples of primitives, etc.)
fn is_tco_candidate(func: &IrFunction) -> bool {
    if func.name.starts_with("__test_") {
        return false;
    }
    if !can_default_init(&func.ret_ty) {
        return false;
    }
    let (has_any, all_in_tail) = all_self_calls_in_tail_pos(&func.body, &func.name);
    has_any && all_in_tail
}

/// Scan an expression tree, returning (has_any_self_call, all_self_calls_in_tail_position).
///
/// "Tail position" means:
/// - The expression itself (top-level body)
/// - Last expression in a Block
/// - Both branches of an If
/// - All arm bodies in a Match
///
/// NOT tail position:
/// - Condition of If
/// - Subject of Match
/// - Inside BinOp, UnOp, or any compound expression
/// - Block.stmts (only Block.expr can be tail)
fn all_self_calls_in_tail_pos(expr: &IrExpr, fn_name: &str) -> (bool, bool) {
    match &expr.kind {
        // Direct self-call in tail position
        IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => {
            (true, true)
        }

        // If: condition is NOT tail, both branches ARE tail
        IrExprKind::If { cond, then, else_ } => {
            let (cond_has, cond_all) = scan_non_tail(cond, fn_name);
            if cond_has && !cond_all {
                return (true, false);
            }
            let (then_has, then_all) = all_self_calls_in_tail_pos(then, fn_name);
            let (else_has, else_all) = all_self_calls_in_tail_pos(else_, fn_name);
            let has = cond_has || then_has || else_has;
            let all = (!cond_has || cond_all) && (!then_has || then_all) && (!else_has || else_all);
            (has, all)
        }

        // Match: subject is NOT tail, arm bodies ARE tail
        IrExprKind::Match { subject, arms } => {
            let (subj_has, subj_all) = scan_non_tail(subject, fn_name);
            if subj_has && !subj_all {
                return (true, false);
            }
            let (has, all) = arms.iter().fold((subj_has, !subj_has || subj_all), |(has, all), arm| {
                let (arm_has, arm_all) = all_self_calls_in_tail_pos(&arm.body, fn_name);
                let (g_has, g_all) = arm.guard.as_ref().map_or((false, true), |g| scan_non_tail(g, fn_name));
                (has || arm_has || g_has, all && (!arm_has || arm_all) && (!g_has || g_all))
            });
            (has, all)
        }

        // Block: stmts are NOT tail, only the trailing expr is tail

        IrExprKind::Block { stmts, expr } => {
            let (has, all) = stmts.iter().fold((false, true), |(has, all), stmt| {
                let (s_has, s_all) = scan_non_tail_stmt(stmt, fn_name);
                (has || s_has, all && (!s_has || s_all))
            });
            let (has, all) = expr.as_ref().map_or((has, all), |tail| {
                let (t_has, t_all) = all_self_calls_in_tail_pos(tail, fn_name);
                (has || t_has, all && (!t_has || t_all))
            });
            (has, all)
        }

        // Anything else: scan for non-tail self-calls
        _ => scan_non_tail(expr, fn_name),
    }
}

/// Check whether any expression in an iterator contains a self-call (non-tail).
/// Returns `(has_any, !has_any)` — the `all` component is simply the negation of `has`.
fn any_has_self_call<'a>(exprs: impl Iterator<Item = &'a IrExpr>, fn_name: &str) -> (bool, bool) {
    let has = exprs.fold(false, |has, e| has || scan_non_tail(e, fn_name).0);
    (has, !has)
}

/// Scan an expression that is NOT in tail position. Any self-call found here
/// means the function has a non-tail self-call.
fn scan_non_tail(expr: &IrExpr, fn_name: &str) -> (bool, bool) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            // Self-call in non-tail position: disqualify
            // But also scan args for additional self-calls
            let has = args.iter().fold(true, |has, arg| has || scan_non_tail(arg, fn_name).0);
            (has, false)
        }
        IrExprKind::Call { target, args, .. } => {
            let target_has = match target {
                CallTarget::Computed { callee } => scan_non_tail(callee, fn_name).0,
                CallTarget::Method { object, .. } => scan_non_tail(object, fn_name).0,
                _ => false,
            };
            let has = args.iter().fold(target_has, |has, arg| has || scan_non_tail(arg, fn_name).0);
            (has, !has)
        }
        IrExprKind::BinOp { left, right, .. } => {
            let has = scan_non_tail(left, fn_name).0 || scan_non_tail(right, fn_name).0;
            (has, !has)
        }
        IrExprKind::UnOp { operand, .. } => {
            scan_non_tail(operand, fn_name)
        }
        IrExprKind::If { cond, then, else_ } => {
            let has = scan_non_tail(cond, fn_name).0
                || scan_non_tail(then, fn_name).0
                || scan_non_tail(else_, fn_name).0;
            (has, !has)
        }
        IrExprKind::Match { subject, arms } => {
            let has = arms.iter().fold(scan_non_tail(subject, fn_name).0, |has, arm| {
                let g_has = arm.guard.as_ref().map_or(false, |g| scan_non_tail(g, fn_name).0);
                has || scan_non_tail(&arm.body, fn_name).0 || g_has
            });
            (has, !has)
        }
        IrExprKind::Block { stmts, expr } => {
            let has = stmts.iter().fold(false, |has, stmt| has || scan_non_tail_stmt(stmt, fn_name).0);
            let has = expr.as_ref().map_or(has, |e| has || scan_non_tail(e, fn_name).0);
            (has, !has)
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            any_has_self_call(elements.iter(), fn_name)
        }
        IrExprKind::Record { fields, .. } => {
            any_has_self_call(fields.iter().map(|(_, v)| v), fn_name)
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambdas are independent scopes; a self-call in a lambda
            // is not a direct self-recursive tail call
            let (b_has, _) = scan_non_tail(body, fn_name);
            (b_has, !b_has)
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            scan_non_tail(expr, fn_name)
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            let has = scan_non_tail(expr, fn_name).0 || scan_non_tail(fallback, fn_name).0;
            (has, !has)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            scan_non_tail(object, fn_name)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            let has = scan_non_tail(object, fn_name).0 || scan_non_tail(index, fn_name).0;
            (has, !has)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            let has = fields.iter().fold(scan_non_tail(base, fn_name).0, |has, (_, v)| {
                has || scan_non_tail(v, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::StringInterp { parts } => {
            let has = parts.iter().fold(false, |has, p| {
                if let IrStringPart::Expr { expr } = p { has || scan_non_tail(expr, fn_name).0 } else { has }
            });
            (has, !has)
        }
        IrExprKind::MapLiteral { entries } => {
            let has = entries.iter().fold(false, |has, (k, v)| {
                has || scan_non_tail(k, fn_name).0 || scan_non_tail(v, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::Range { start, end, .. } => {
            let has = scan_non_tail(start, fn_name).0 || scan_non_tail(end, fn_name).0;
            (has, !has)
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            let has = body.iter().fold(scan_non_tail(iterable, fn_name).0, |has, stmt| {
                has || scan_non_tail_stmt(stmt, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::While { cond, body } => {
            let has = body.iter().fold(scan_non_tail(cond, fn_name).0, |has, stmt| {
                has || scan_non_tail_stmt(stmt, fn_name).0
            });
            (has, !has)
        }
        IrExprKind::Fan { exprs } => {
            any_has_self_call(exprs.iter(), fn_name)
        }
        IrExprKind::RustMacro { args, .. } => {
            any_has_self_call(args.iter(), fn_name)
        }
        // Leaf nodes: no self-calls
        _ => (false, true),
    }
}

fn scan_non_tail_stmt(stmt: &IrStmt, fn_name: &str) -> (bool, bool) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::BindDestructure { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::Expr { expr } => {
            scan_non_tail(expr, fn_name)
        }
        IrStmtKind::Guard { cond, else_ } => {
            let (c_has, _) = scan_non_tail(cond, fn_name);
            let (e_has, _) = scan_non_tail(else_, fn_name);
            let has = c_has || e_has;
            (has, !has)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            let (i_has, _) = scan_non_tail(index, fn_name);
            let (v_has, _) = scan_non_tail(value, fn_name);
            let has = i_has || v_has;
            (has, !has)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            let (k_has, _) = scan_non_tail(key, fn_name);
            let (v_has, _) = scan_non_tail(value, fn_name);
            let has = k_has || v_has;
            (has, !has)
        }
        IrStmtKind::FieldAssign { value, .. } => {
            scan_non_tail(value, fn_name)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            let (a_has, _) = scan_non_tail(a, fn_name);
            let (b_has, _) = scan_non_tail(b, fn_name);
            let has = a_has || b_has;
            (has, !has)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            scan_non_tail(end, fn_name)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            scan_non_tail(len, fn_name)
        }
        IrStmtKind::Comment { .. } => (false, true),
    }
}

/// Rewrite a TCO-eligible function body from recursive form to a loop.
fn rewrite_to_loop(func: &mut IrFunction, var_table: &mut VarTable) {
    let fn_name = func.name.clone();
    // For effect fns returning Result[T, E], the TCO result variable should hold T
    // because the Rust codegen auto-unwraps Result via `?` operator.
    let ret_ty = if func.is_effect {
        match &func.ret_ty {
            Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => args[0].clone(),
            _ => func.ret_ty.clone(),
        }
    } else {
        func.ret_ty.clone()
    };

    // Mark all param VarIds as mutable (they'll be reassigned in the loop)
    for param in &func.params {
        var_table.entries[param.var.0 as usize].mutability = Mutability::Var;
    }

    // Allocate a result variable
    let result_var = var_table.alloc(
        "__tco_result".into(),
        ret_ty.clone(),
        Mutability::Var,
        None,
    );

    // Allocate temporaries for each param (to avoid order-dependent assignment)
    let temps: Vec<(VarId, Ty)> = func.params.iter().map(|p| {
        let tmp = var_table.alloc(
            format!("__tco_tmp_{}", p.name).into(),
            p.ty.clone(),
            Mutability::Let,
            None,
        );
        (tmp, p.ty.clone())
    }).collect();

    // Collect param info for rewrite
    let params: Vec<(VarId, Ty)> = func.params.iter()
        .map(|p| (p.var, p.ty.clone()))
        .collect();

    // Rewrite the body expression
    let old_body = std::mem::take(&mut func.body);
    let is_effect = func.is_effect;
    let rewritten = rewrite_tail_expr(old_body, &fn_name, &params, &temps, result_var, is_effect);

    // Build the default value for the result variable
    let default_val = default_for_type(&ret_ty);

    // Construct: { var __tco_result = default; while true { rewritten_body }; __tco_result }
    let bind_result = IrStmt {
        kind: IrStmtKind::Bind {
            var: result_var,
            mutability: Mutability::Var,
            ty: ret_ty.clone(),
            value: default_val,
        },
        span: None,
    };

    // The while body is a single Expr statement wrapping the rewritten body
    let while_body_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: rewritten },
        span: None,
    };

    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: true },
                ty: Ty::Bool,
                span: None,
            }),
            body: vec![while_body_stmt],
        },
        ty: Ty::Unit,
        span: None,
    };

    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: while_expr },
        span: None,
    };

    let tail_var = IrExpr {
        kind: IrExprKind::Var { id: result_var },
        ty: ret_ty.clone(),
        span: None,
    };

    // For effect fns, wrap the result in Ok() since the function returns Result
    let tail_expr = if func.is_effect {
        IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(tail_var) },
            ty: func.ret_ty.clone(),
            span: None,
        }
    } else {
        tail_var
    };

    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![bind_result, while_stmt],
            expr: Some(Box::new(tail_expr)),
        },
        ty: func.ret_ty.clone(),
        span: func.body.span,
    };
}

/// Rewrite an expression in tail position:
/// - Self-calls become: bind temps, assign params, continue
/// - If/Match: recurse into branches
/// - Block: recurse into trailing expr
/// - Anything else (base case): assign to result var, break
fn rewrite_tail_expr(
    expr: IrExpr,
    fn_name: &str,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    result_var: VarId,
    is_effect: bool,
) -> IrExpr {
    match expr.kind {
        // Self-recursive call in tail position -> reassign params and continue
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            emit_tail_call_replacement(args, params, temps, result_var)
        }

        // Effect fn: unwrap ok(expr) in tail position — assign the inner value
        IrExprKind::ResultOk { expr: inner } if is_effect => {
            emit_base_case(*inner, result_var)
        }

        // If: recurse into both branches
        IrExprKind::If { cond, then, else_ } => {
            let new_then = rewrite_tail_expr(*then, fn_name, params, temps, result_var, is_effect);
            let new_else = rewrite_tail_expr(*else_, fn_name, params, temps, result_var, is_effect);
            IrExpr {
                kind: IrExprKind::If {
                    cond,
                    then: Box::new(new_then),
                    else_: Box::new(new_else),
                },
                ty: Ty::Unit,
                span: expr.span,
            }
        }

        // Match: recurse into arm bodies
        IrExprKind::Match { subject, arms } => {
            let new_arms = arms.into_iter().map(|arm| {
                IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard,
                    body: rewrite_tail_expr(arm.body, fn_name, params, temps, result_var, is_effect),
                }
            }).collect();
            IrExpr {
                kind: IrExprKind::Match { subject, arms: new_arms },
                ty: Ty::Unit,
                span: expr.span,
            }
        }

        // Block: recurse into trailing expr
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let new_tail = rewrite_tail_expr(*tail, fn_name, params, temps, result_var, is_effect);
            IrExpr {
                kind: IrExprKind::Block {
                    stmts,
                    expr: Some(Box::new(new_tail)),
                },
                ty: Ty::Unit,
                span: expr.span,
            }
        }

        // Base case: assign result and break
        _ => {
            emit_base_case(expr, result_var)
        }
    }
}

/// Emit the replacement for a tail self-call:
/// ```text
/// let __tco_tmp_0 = arg0_expr
/// let __tco_tmp_1 = arg1_expr
/// param0 = __tco_tmp_0
/// param1 = __tco_tmp_1
/// continue
/// ```
fn emit_tail_call_replacement(
    args: Vec<IrExpr>,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    _result_var: VarId,
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();

    // Bind temporaries to argument expressions
    for (i, arg) in args.into_iter().enumerate() {
        let (tmp_var, tmp_ty) = &temps[i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tmp_var,
                mutability: Mutability::Let,
                ty: tmp_ty.clone(),
                value: arg,
            },
            span: None,
        });
    }

    // Assign params from temporaries
    for (i, (param_var, _)) in params.iter().enumerate() {
        let (tmp_var, tmp_ty) = &temps[i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: *param_var,
                value: IrExpr {
                    kind: IrExprKind::Var { id: *tmp_var },
                    ty: tmp_ty.clone(),
                    span: None,
                },
            },
            span: None,
        });
    }

    // Continue the loop
    let continue_expr = IrExpr {
        kind: IrExprKind::Continue,
        ty: Ty::Unit,
        span: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(continue_expr)),
        },
        ty: Ty::Unit,
        span: None,
    }
}

/// Emit the base case: assign to result variable and break.
/// ```text
/// __tco_result = expr
/// break
/// ```
fn emit_base_case(expr: IrExpr, result_var: VarId) -> IrExpr {
    let assign = IrStmt {
        kind: IrStmtKind::Assign {
            var: result_var,
            value: expr,
        },
        span: None,
    };

    let break_expr = IrExpr {
        kind: IrExprKind::Break,
        ty: Ty::Unit,
        span: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![assign],
            expr: Some(Box::new(break_expr)),
        },
        ty: Ty::Unit,
        span: None,
    }
}

/// Produce a default value for a given type (used to initialize the result variable).
/// The value is never observed — every control path assigns before reading — but
/// Rust's type checker requires a valid expression of the correct type.
fn default_for_type(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        Ty::String => IrExprKind::LitStr { value: String::new() },
        Ty::Unit => IrExprKind::Unit,
        Ty::Applied(TypeConstructorId::Result, args) => {
            let inner_ty = args.first().cloned().unwrap_or(Ty::Unit);
            let inner = default_for_type(&inner_ty);
            IrExprKind::ResultOk { expr: Box::new(inner) }
        }
        Ty::Applied(TypeConstructorId::Option, _) => {
            IrExprKind::OptionNone
        }
        Ty::Applied(TypeConstructorId::List, _) => {
            IrExprKind::List { elements: vec![] }
        }
        Ty::Applied(TypeConstructorId::Map, _) => {
            IrExprKind::MapLiteral { entries: vec![] }
        }
        Ty::Tuple(elems) => {
            IrExprKind::Tuple {
                elements: elems.iter().map(|t| default_for_type(t)).collect(),
            }
        }
        // Named types and other complex types: cannot synthesize a default value.
        // TCO should not be applied to functions returning these types.
        // Return Unit as unreachable placeholder (guarded by can_default_init check).
        _ => IrExprKind::Unit,
    };
    IrExpr {
        kind,
        ty: ty.clone(),
        span: None,
    }
}

/// Returns true if we can produce a valid default value for this type.
/// Types that fail this check should not be TCO'd (the result variable
/// cannot be initialized without unsafe code).
fn can_default_init(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit => true,
        Ty::Applied(TypeConstructorId::Result, args) => {
            args.first().map_or(true, |inner| can_default_init(inner))
        }
        Ty::Applied(TypeConstructorId::Option, _) => true,
        Ty::Applied(TypeConstructorId::List, _) => true,
        Ty::Applied(TypeConstructorId::Map, _) => true,
        Ty::Tuple(elems) => elems.iter().all(|t| can_default_init(t)),
        _ => false,
    }
}
