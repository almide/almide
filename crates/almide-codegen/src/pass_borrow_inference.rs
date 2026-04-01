//! BorrowInferencePass: Roc-style "borrowed by default, own when needed" analysis.
//!
//! For each user function parameter of heap type (String, Vec, Record, etc.):
//! 1. Start as Borrowed
//! 2. Walk the function body to find ownership-requiring uses
//! 3. If none found → mark param as Ref/RefStr/RefSlice
//! 4. Insert Borrow nodes at call sites for borrowed params
//!
//! This eliminates unnecessary .clone() at call sites when the callee only reads the value.

use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// Phase 1: Infer borrow signatures for all functions.
pub fn infer_borrow_signatures(program: &mut IrProgram) -> HashMap<String, Vec<ParamBorrow>> {
    let mut sigs: HashMap<String, Vec<ParamBorrow>> = HashMap::new();

    for func in &mut program.functions {
        if func.is_test || is_derive_fn(&func.name) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
        let borrows = infer_function_borrows(func);
        if borrows.iter().any(|b| !matches!(b, ParamBorrow::Own)) {
            sigs.insert(func.name.to_string(), borrows.clone());
        }
        for (param, borrow) in func.params.iter_mut().zip(borrows) {
            param.borrow = borrow;
        }
    }

    for module in &mut program.modules {
        for func in &mut module.functions {
            if func.is_test || is_derive_fn(&func.name) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
            let borrows = infer_function_borrows(func);
            if borrows.iter().any(|b| !matches!(b, ParamBorrow::Own)) {
                sigs.insert(func.name.to_string(), borrows.clone());
            }
            for (param, borrow) in func.params.iter_mut().zip(borrows) {
                param.borrow = borrow;
            }
        }
    }

    sigs
}

fn infer_function_borrows(func: &IrFunction) -> Vec<ParamBorrow> {
    func.params.iter().map(|param| {
        if !is_heap_type(&param.ty) {
            return ParamBorrow::Own;
        }

        // If the function body directly returns this param, it needs ownership
        if is_var(&func.body, param.var) {
            return ParamBorrow::Own;
        }

        let mut needs_own = false;
        check_needs_ownership(&func.body, param.var, &mut needs_own);

        if needs_own {
            ParamBorrow::Own
        } else if matches!(&param.ty, Ty::String) {
            ParamBorrow::RefStr
        } else if matches!(&param.ty, Ty::Applied(TypeConstructorId::List, _)) {
            ParamBorrow::RefSlice
        } else {
            ParamBorrow::Ref
        }
    }).collect()
}

fn is_derive_fn(name: &str) -> bool {
    name.contains("_encode") || name.contains("_decode") || name.contains("_eq")
        || name.contains("_display") || name.contains("_to_string") || name.contains("_from_")
}

fn is_monomorphized(name: &str) -> bool {
    name.contains("__")
}

/// Only String and List are eligible for borrow inference.
/// Records/Variants have field access issues when borrowed (&Record.field → &String, not String).
fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(TypeConstructorId::List, _))
}

/// Check if a parameter variable needs ownership.
/// Conservative: marks as Owned if used in ANY ownership-requiring position.
fn check_needs_ownership(expr: &IrExpr, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &expr.kind {
        // ── Tail position: returned value needs ownership ──
        IrExprKind::Var { id } if *id == var => {
            // Bare var reference — context determines if ownership needed.
            // When used as a standalone expression (tail), it's returned → own.
            // But we handle tail detection at the Block level below.
        }

        IrExprKind::Block { stmts, expr: Some(tail) } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
            if is_var(tail, var) { *needs = true; return; }
            check_needs_ownership(tail, var, needs);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Concatenation consumes operands ──
        IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
            if is_var(left, var) || is_var(right, var) { *needs = true; return; }
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Function call: conservatively, passing to any function needs own ──
        // EXCEPT: if the callee is known to borrow that param (future: use sigs map)
        IrExprKind::Call { target, args, .. } => {
            // Passing as argument to a function → needs own (conservative)
            for arg in args {
                if is_var(arg, var) { *needs = true; return; }
            }
            // Recurse into non-arg sub-expressions
            match target {
                CallTarget::Method { object, .. } => check_needs_ownership(object, var, needs),
                CallTarget::Computed { callee } => check_needs_ownership(callee, var, needs),
                _ => {}
            }
            for arg in args { check_needs_ownership(arg, var, needs); }
        }

        // ── Collection construction consumes ──
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { if is_var(e, var) { *needs = true; return; } }
            for e in elements { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            if is_var(base, var) { *needs = true; return; }
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            check_needs_ownership(base, var, needs);
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { if is_var(k, var) || is_var(v, var) { *needs = true; return; } }
        }

        // ── Wrapping in Result/Option/Some ──
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => {
            if is_var(expr, var) { *needs = true; return; }
            check_needs_ownership(expr, var, needs);
        }

        // ── Lambda capture: captured vars need ownership ──
        IrExprKind::Lambda { body, .. } => {
            if uses_var(body, var) { *needs = true; }
        }

        // ── String interpolation consumes ──
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    if is_var(expr, var) { *needs = true; return; }
                    check_needs_ownership(expr, var, needs);
                }
            }
        }

        // ── ForIn: iterable is consumed ──
        IrExprKind::ForIn { iterable, body, .. } => {
            if is_var(iterable, var) { *needs = true; return; }
            check_needs_ownership(iterable, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── IterChain: source consumed if consume=true ──
        IrExprKind::IterChain { source, consume, steps, collector } => {
            if *consume && is_var(source, var) { *needs = true; return; }
            check_needs_ownership(source, var, needs);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        if uses_var(lambda, var) { *needs = true; return; }
                    }
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => {
                    if is_var(init, var) { *needs = true; return; }
                    if uses_var(lambda, var) { *needs = true; return; }
                }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                    if uses_var(lambda, var) { *needs = true; return; }
                }
            }
        }

        // ── Safe reads (no ownership needed) ──
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            // Indexing borrows — safe
            check_needs_ownership(object, var, needs);
            check_needs_ownership(index, var, needs);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_needs_ownership(object, var, needs);
        }
        IrExprKind::BinOp { left, right, .. } => {
            // Non-concat binop: comparison, arithmetic — safe reads
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Control flow: recurse ──
        IrExprKind::If { cond, then, else_ } => {
            check_needs_ownership(cond, var, needs);
            check_needs_ownership(then, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        IrExprKind::Match { subject, arms } => {
            // Match subject: destructuring a borrowed value changes bind types
            // → needs ownership to avoid &-pattern complications
            if is_var(subject, var) { *needs = true; return; }
            check_needs_ownership(subject, var, needs);
            for arm in arms {
                if let Some(g) = &arm.guard { check_needs_ownership(g, var, needs); }
                check_needs_ownership(&arm.body, var, needs);
            }
        }
        IrExprKind::While { cond, body } => {
            check_needs_ownership(cond, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Wrappers: recurse ──
        IrExprKind::UnOp { operand, .. } => check_needs_ownership(operand, var, needs),
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            check_needs_ownership(expr, var, needs);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            check_needs_ownership(expr, var, needs);
            check_needs_ownership(fallback, var, needs);
        }
        IrExprKind::OptionalChain { expr, .. } => check_needs_ownership(expr, var, needs),
        IrExprKind::Range { start, end, .. } => {
            check_needs_ownership(start, var, needs);
            check_needs_ownership(end, var, needs);
        }
        IrExprKind::Fan { exprs } => {
            for e in exprs { if is_var(e, var) { *needs = true; return; } }
            for e in exprs { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { check_needs_ownership(a, var, needs); }
        }
        _ => {}
    }
}

fn check_needs_ownership_stmt(stmt: &IrStmt, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            check_needs_ownership(index, var, needs);
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::Expr { expr } => check_needs_ownership(expr, var, needs),
        IrStmtKind::Guard { cond, else_ } => {
            check_needs_ownership(cond, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        _ => {}
    }
}

fn is_var(expr: &IrExpr, var: VarId) -> bool {
    matches!(&expr.kind, IrExprKind::Var { id } if *id == var)
}

fn uses_var(expr: &IrExpr, var: VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_uses_var(s, var))
            || expr.as_ref().map_or(false, |e| uses_var(e, var))
        }
        IrExprKind::If { cond, then, else_ } => uses_var(cond, var) || uses_var(then, var) || uses_var(else_, var),
        IrExprKind::Call { args, target, .. } => {
            match target {
                CallTarget::Method { object, .. } => { if uses_var(object, var) { return true; } }
                CallTarget::Computed { callee } => { if uses_var(callee, var) { return true; } }
                _ => {}
            }
            args.iter().any(|a| uses_var(a, var))
        }
        IrExprKind::BinOp { left, right, .. } => uses_var(left, var) || uses_var(right, var),
        IrExprKind::UnOp { operand, .. } => uses_var(operand, var),
        IrExprKind::Lambda { body, .. } => uses_var(body, var),
        IrExprKind::Match { subject, arms } => {
            uses_var(subject, var) || arms.iter().any(|a| {
                a.guard.as_ref().map_or(false, |g| uses_var(g, var)) || uses_var(&a.body, var)
            })
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            uses_var(iterable, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::While { cond, body } => {
            uses_var(cond, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => uses_var(object, var),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            uses_var(object, var) || uses_var(index, var)
        }
        IrExprKind::StringInterp { parts } => parts.iter().any(|p| {
            matches!(p, IrStringPart::Expr { expr } if uses_var(expr, var))
        }),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => uses_var(expr, var),
        IrExprKind::UnwrapOr { expr, fallback } => uses_var(expr, var) || uses_var(fallback, var),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => elements.iter().any(|e| uses_var(e, var)),
        IrExprKind::Record { fields, .. } => fields.iter().any(|(_, v)| uses_var(v, var)),
        IrExprKind::SpreadRecord { base, fields } => {
            uses_var(base, var) || fields.iter().any(|(_, v)| uses_var(v, var))
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            uses_var(source, var)
            || steps.iter().any(|s| match s {
                IterStep::Map { lambda } | IterStep::Filter { lambda }
                | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => uses_var(lambda, var),
            })
            || match collector {
                IterCollector::Collect => false,
                IterCollector::Fold { init, lambda } => uses_var(init, var) || uses_var(lambda, var),
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => uses_var(lambda, var),
            }
        }
        IrExprKind::RustMacro { args, .. } => args.iter().any(|a| uses_var(a, var)),
        IrExprKind::Range { start, end, .. } => uses_var(start, var) || uses_var(end, var),
        IrExprKind::MapLiteral { entries } => entries.iter().any(|(k, v)| uses_var(k, var) || uses_var(v, var)),
        _ => false,
    }
}

fn stmt_uses_var(stmt: &IrStmt, var: VarId) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => uses_var(value, var),
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            uses_var(index, var) || uses_var(value, var)
        }
        IrStmtKind::Expr { expr } => uses_var(expr, var),
        IrStmtKind::Guard { cond, else_ } => uses_var(cond, var) || uses_var(else_, var),
        _ => false,
    }
}

// ── Phase 2: Insert Borrow nodes at call sites ─────────────────────

pub fn insert_borrows_at_call_sites(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    for func in &mut program.functions {
        func.body = rewrite_calls(std::mem::take(&mut func.body), sigs);
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            func.body = rewrite_calls(std::mem::take(&mut func.body), sigs);
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs);
        }
    }
}

fn rewrite_calls(expr: IrExpr, sigs: &HashMap<String, Vec<ParamBorrow>>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_calls(a, sigs)).collect();

            let callee_name = match &target {
                CallTarget::Named { name } => Some(name.to_string()),
                // Convention methods: Walker renders as UFCS `TypeName_method(object, args)`
                // The method name in IR is "TypeName.method" — sigs use the same format
                CallTarget::Method { method, .. } if method.contains('.') => Some(method.to_string()),
                _ => None,
            };

            let args = if let Some(ref name) = callee_name {
                if let Some(borrows) = sigs.get(name) {
                    args.into_iter().enumerate().map(|(i, arg)| {
                        match borrows.get(i) {
                            Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s }
                            }
                            Some(ParamBorrow::RefStr) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s }
                            }
                            _ => arg,
                        }
                    }).collect()
                } else { args }
            } else { args };

            let target = match target {
                CallTarget::Method { object, method } => {
                    let mut obj = rewrite_calls(*object, sigs);
                    if method.contains('.') {
                        if let Some(borrows) = sigs.get(method.as_str()) {
                            if let Some(b) = borrows.first() {
                                match b {
                                    ParamBorrow::Ref | ParamBorrow::RefSlice => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: false, mutable: false }, ty: t, span: s };
                                    }
                                    ParamBorrow::RefStr => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: true, mutable: false }, ty: t, span: s };
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    CallTarget::Method { object: Box::new(obj), method }
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_calls(*callee, sigs)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }

        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rewrite_calls_stmt(s, sigs)).collect(),
            expr: expr.map(|e| Box::new(rewrite_calls(*e, sigs))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_calls(*cond, sigs)),
            then: Box::new(rewrite_calls(*then, sigs)),
            else_: Box::new(rewrite_calls(*else_, sigs)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_calls(*subject, sigs)),
            arms: arms.into_iter().map(|a| IrMatchArm {
                pattern: a.pattern,
                guard: a.guard.map(|g| rewrite_calls(g, sigs)),
                body: rewrite_calls(a.body, sigs),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_calls(*iterable, sigs)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_calls(*cond, sigs)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs)).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_calls(*body, sigs)), lambda_id,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_calls(*left, sigs)), right: Box::new(rewrite_calls(*right, sigs)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_calls(*operand, sigs)),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_calls(*expr, sigs)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_calls(*expr, sigs)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_calls(*expr, sigs)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_calls(*expr, sigs)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_calls(*expr, sigs)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_calls(*expr, sigs)),
            fallback: Box::new(rewrite_calls(*fallback, sigs)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_calls(expr, sigs) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rewrite_calls(e, sigs)).collect(),
        },
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_calls(*source, sigs)),
            consume, steps, collector,
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn rewrite_calls_stmt(stmt: IrStmt, sigs: &HashMap<String, Vec<ParamBorrow>>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rewrite_calls(value, sigs),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_calls(value, sigs) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_calls(expr, sigs) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rewrite_calls(cond, sigs), else_: rewrite_calls(else_, sigs),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rewrite_calls(value, sigs),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}
