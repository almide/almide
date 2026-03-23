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

use crate::ir::*;
use crate::types::Ty;
use crate::types::constructor::TypeConstructorId;
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct TailCallOptPass;

impl NanoPass for TailCallOptPass {
    fn name(&self) -> &str { "TailCallOpt" }

    fn targets(&self) -> Option<Vec<Target>> {
        None // All targets benefit from TCO
    }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        run_tco(&mut program.functions, &mut program.var_table);
        for module in &mut program.modules {
            run_tco(&mut module.functions, &mut module.var_table);
        }
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
fn is_tco_candidate(func: &IrFunction) -> bool {
    if func.name.starts_with("__test_") {
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
            let mut has = subj_has;
            let mut all = !subj_has || subj_all;
            for arm in arms {
                let (arm_has, arm_all) = all_self_calls_in_tail_pos(&arm.body, fn_name);
                has = has || arm_has;
                all = all && (!arm_has || arm_all);
                // Guards are NOT tail
                if let Some(guard) = &arm.guard {
                    let (g_has, g_all) = scan_non_tail(guard, fn_name);
                    has = has || g_has;
                    all = all && (!g_has || g_all);
                }
            }
            (has, all)
        }

        // Block: stmts are NOT tail, only the trailing expr is tail
        IrExprKind::Block { stmts, expr } => {
            let mut has = false;
            let mut all = true;
            for stmt in stmts {
                let (s_has, s_all) = scan_non_tail_stmt(stmt, fn_name);
                has = has || s_has;
                all = all && (!s_has || s_all);
            }
            if let Some(tail) = expr {
                let (t_has, t_all) = all_self_calls_in_tail_pos(tail, fn_name);
                has = has || t_has;
                all = all && (!t_has || t_all);
            }
            (has, all)
        }

        // DoBlock: same as Block
        IrExprKind::DoBlock { stmts, expr } => {
            let mut has = false;
            let mut all = true;
            for stmt in stmts {
                let (s_has, s_all) = scan_non_tail_stmt(stmt, fn_name);
                has = has || s_has;
                all = all && (!s_has || s_all);
            }
            if let Some(tail) = expr {
                let (t_has, t_all) = all_self_calls_in_tail_pos(tail, fn_name);
                has = has || t_has;
                all = all && (!t_has || t_all);
            }
            (has, all)
        }

        // Anything else: scan for non-tail self-calls
        _ => scan_non_tail(expr, fn_name),
    }
}

/// Scan an expression that is NOT in tail position. Any self-call found here
/// means the function has a non-tail self-call.
fn scan_non_tail(expr: &IrExpr, fn_name: &str) -> (bool, bool) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            // Self-call in non-tail position: disqualify
            // But also scan args for additional self-calls
            let mut has = true;
            for arg in args {
                let (a_has, _) = scan_non_tail(arg, fn_name);
                has = has || a_has;
            }
            (has, false)
        }
        IrExprKind::Call { target, args, .. } => {
            let mut has = false;
            // Scan target if Computed or Method
            match target {
                CallTarget::Computed { callee } => {
                    let (c_has, _) = scan_non_tail(callee, fn_name);
                    has = has || c_has;
                }
                CallTarget::Method { object, .. } => {
                    let (o_has, _) = scan_non_tail(object, fn_name);
                    has = has || o_has;
                }
                _ => {}
            }
            for arg in args {
                let (a_has, _) = scan_non_tail(arg, fn_name);
                has = has || a_has;
            }
            (has, !has) // all=true only if no self-calls found
        }
        IrExprKind::BinOp { left, right, .. } => {
            let (l_has, _) = scan_non_tail(left, fn_name);
            let (r_has, _) = scan_non_tail(right, fn_name);
            let has = l_has || r_has;
            (has, !has)
        }
        IrExprKind::UnOp { operand, .. } => {
            scan_non_tail(operand, fn_name)
        }
        IrExprKind::If { cond, then, else_ } => {
            let (c_has, _) = scan_non_tail(cond, fn_name);
            let (t_has, _) = scan_non_tail(then, fn_name);
            let (e_has, _) = scan_non_tail(else_, fn_name);
            let has = c_has || t_has || e_has;
            (has, !has)
        }
        IrExprKind::Match { subject, arms } => {
            let (s_has, _) = scan_non_tail(subject, fn_name);
            let mut has = s_has;
            for arm in arms {
                let (a_has, _) = scan_non_tail(&arm.body, fn_name);
                has = has || a_has;
                if let Some(guard) = &arm.guard {
                    let (g_has, _) = scan_non_tail(guard, fn_name);
                    has = has || g_has;
                }
            }
            (has, !has)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            let mut has = false;
            for stmt in stmts {
                let (s_has, _) = scan_non_tail_stmt(stmt, fn_name);
                has = has || s_has;
            }
            if let Some(e) = expr {
                let (e_has, _) = scan_non_tail(e, fn_name);
                has = has || e_has;
            }
            (has, !has)
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            let mut has = false;
            for e in elements {
                let (e_has, _) = scan_non_tail(e, fn_name);
                has = has || e_has;
            }
            (has, !has)
        }
        IrExprKind::Record { fields, .. } => {
            let mut has = false;
            for (_, v) in fields {
                let (v_has, _) = scan_non_tail(v, fn_name);
                has = has || v_has;
            }
            (has, !has)
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambdas are independent scopes; a self-call in a lambda
            // is not a direct self-recursive tail call
            let (b_has, _) = scan_non_tail(body, fn_name);
            (b_has, !b_has)
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            scan_non_tail(expr, fn_name)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            scan_non_tail(object, fn_name)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            let (o_has, _) = scan_non_tail(object, fn_name);
            let (i_has, _) = scan_non_tail(index, fn_name);
            let has = o_has || i_has;
            (has, !has)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            let (b_has, _) = scan_non_tail(base, fn_name);
            let mut has = b_has;
            for (_, v) in fields {
                let (v_has, _) = scan_non_tail(v, fn_name);
                has = has || v_has;
            }
            (has, !has)
        }
        IrExprKind::StringInterp { parts } => {
            let mut has = false;
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    let (e_has, _) = scan_non_tail(expr, fn_name);
                    has = has || e_has;
                }
            }
            (has, !has)
        }
        IrExprKind::MapLiteral { entries } => {
            let mut has = false;
            for (k, v) in entries {
                let (k_has, _) = scan_non_tail(k, fn_name);
                let (v_has, _) = scan_non_tail(v, fn_name);
                has = has || k_has || v_has;
            }
            (has, !has)
        }
        IrExprKind::Range { start, end, .. } => {
            let (s_has, _) = scan_non_tail(start, fn_name);
            let (e_has, _) = scan_non_tail(end, fn_name);
            let has = s_has || e_has;
            (has, !has)
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            let (i_has, _) = scan_non_tail(iterable, fn_name);
            let mut has = i_has;
            for stmt in body {
                let (s_has, _) = scan_non_tail_stmt(stmt, fn_name);
                has = has || s_has;
            }
            (has, !has)
        }
        IrExprKind::While { cond, body } => {
            let (c_has, _) = scan_non_tail(cond, fn_name);
            let mut has = c_has;
            for stmt in body {
                let (s_has, _) = scan_non_tail_stmt(stmt, fn_name);
                has = has || s_has;
            }
            (has, !has)
        }
        IrExprKind::Fan { exprs } => {
            let mut has = false;
            for e in exprs {
                let (e_has, _) = scan_non_tail(e, fn_name);
                has = has || e_has;
            }
            (has, !has)
        }
        IrExprKind::RustMacro { args, .. } => {
            let mut has = false;
            for a in args {
                let (a_has, _) = scan_non_tail(a, fn_name);
                has = has || a_has;
            }
            (has, !has)
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
        "__tco_result".to_string(),
        ret_ty.clone(),
        Mutability::Var,
        None,
    );

    // Allocate temporaries for each param (to avoid order-dependent assignment)
    let temps: Vec<(VarId, Ty)> = func.params.iter().map(|p| {
        let tmp = var_table.alloc(
            format!("__tco_tmp_{}", p.name),
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
    let old_body = func.body.clone();
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

        // DoBlock: same as Block
        IrExprKind::DoBlock { stmts, expr: Some(tail) } => {
            let new_tail = rewrite_tail_expr(*tail, fn_name, params, temps, result_var, is_effect);
            IrExpr {
                kind: IrExprKind::DoBlock {
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
fn default_for_type(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        Ty::String => IrExprKind::LitStr { value: String::new() },
        Ty::Unit => IrExprKind::Unit,
        Ty::Applied(TypeConstructorId::Result, _) => {
            IrExprKind::ResultOk { expr: Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None }) }
        }
        Ty::Applied(TypeConstructorId::Option, _) => {
            IrExprKind::OptionNone
        }
        Ty::Applied(TypeConstructorId::List, _) => {
            IrExprKind::List { elements: vec![] }
        }
        // For other complex types, use Unit as placeholder -- the result var is always
        // assigned before it is read (every control path ends in assign+break or
        // assign params+continue).
        _ => IrExprKind::Unit,
    };
    IrExpr {
        kind,
        ty: ty.clone(),
        span: None,
    }
}
