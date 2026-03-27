//! Fan Lowering Pass — strip auto-try (?) from fan spawn closures.
//!
//! Fan expressions run in spawn closures which return raw Result.
//! The auto-try (?) is applied at the join point by the walker, not inside the closure.
//! This pass strips any Try nodes that StdlibLoweringPass inserted
//! inside Fan expressions and fan.map/fan.race/fan.any lambda arguments.

use crate::ir::*;

/// Strip Try nodes from inside Fan expressions and fan.* call arguments.
pub fn strip_fan_auto_try(program: &mut IrProgram) {
    for func in &mut program.functions {
        func.body = rewrite_expr(std::mem::take(&mut func.body), false);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body), false);
        }
    }
}

// Note: fan.map/race/any come through as CallTarget::Module { module: "fan" }.
// The walker renders these, but the lambda args may still have Try nodes.
// This pass strips Try from lambdas that are arguments to fan.* calls.

fn rewrite_expr(expr: IrExpr, inside_fan: bool) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        // Fan block: mark children as inside_fan, strip top-level Try from each expr
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| {
                let rewritten = rewrite_expr(e, true);
                strip_try_top(rewritten)
            }).collect(),
        },

        // Fan module calls (fan.map, fan.race, fan.any, etc.): strip Try from lambda args
        // Matches both Module { "fan" } (before StdlibLowering) and Named { "almide_rt_fan_*" } (after)
        IrExprKind::Call { target: CallTarget::Module { ref module, .. }, .. }
            if module == "fan" =>
        {
            let IrExprKind::Call { target, args, type_args } = expr.kind else { unreachable!() };
            IrExprKind::Call {
                target,
                args: args.into_iter().map(rewrite_fan_arg).collect(),
                type_args,
            }
        }
        IrExprKind::Call { target: CallTarget::Named { ref name }, .. }
            if name.starts_with("almide_rt_fan_") =>
        {
            let IrExprKind::Call { target, args, type_args } = expr.kind else { unreachable!() };
            IrExprKind::Call {
                target,
                args: args.into_iter().map(rewrite_fan_arg).collect(),
                type_args,
            }
        }

        // Inside fan: strip Try/Unwrap/ToOption nodes (spawn closures return raw Result)
        IrExprKind::Try { expr: inner } if inside_fan => {
            return rewrite_expr(*inner, true);
        }
        IrExprKind::Unwrap { expr: inner } if inside_fan => {
            return rewrite_expr(*inner, true);
        }
        IrExprKind::ToOption { expr: inner } if inside_fan => {
            return rewrite_expr(*inner, true);
        }
        IrExprKind::UnwrapOr { expr: inner, .. } if inside_fan => {
            return rewrite_expr(*inner, true);
        }

        // Recurse into all other nodes
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts, inside_fan),
            expr: expr.map(|e| Box::new(rewrite_expr(*e, inside_fan))),
        },

        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond, inside_fan)),
            then: Box::new(rewrite_expr(*then, inside_fan)),
            else_: Box::new(rewrite_expr(*else_, inside_fan)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject, inside_fan)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rewrite_expr(g, inside_fan)),
                body: rewrite_expr(arm.body, inside_fan),
            }).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body, inside_fan)), lambda_id,
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: rewrite_target(target, inside_fan),
            args: args.into_iter().map(|a| rewrite_expr(a, inside_fan)).collect(),
            type_args,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable, inside_fan)),
            body: rewrite_stmts(body, inside_fan),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond, inside_fan)),
            body: rewrite_stmts(body, inside_fan),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left, inside_fan)),
            right: Box::new(rewrite_expr(*right, inside_fan)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand, inside_fan)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(rewrite_expr(*inner, inside_fan)),
        },
        IrExprKind::Unwrap { expr: inner } => IrExprKind::Unwrap {
            expr: Box::new(rewrite_expr(*inner, inside_fan)),
        },
        IrExprKind::ToOption { expr: inner } => IrExprKind::ToOption {
            expr: Box::new(rewrite_expr(*inner, inside_fan)),
        },
        IrExprKind::UnwrapOr { expr: inner, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_expr(*inner, inside_fan)),
            fallback: Box::new(rewrite_expr(*fallback, inside_fan)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e, inside_fan)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rewrite_expr(e, inside_fan)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(n, v)| (n, rewrite_expr(v, inside_fan))).collect(),
        },
        // Leaf nodes: pass through
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn rewrite_stmts(stmts: Vec<IrStmt>, inside_fan: bool) -> Vec<IrStmt> {
    stmts.into_iter().map(|stmt| {
        let kind = match stmt.kind {
            IrStmtKind::Bind { var, mutability, value, ty } => IrStmtKind::Bind {
                var, mutability, value: rewrite_expr(value, inside_fan), ty,
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: rewrite_expr(value, inside_fan),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
                var, value: rewrite_expr(value, inside_fan),
            },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr {
                expr: rewrite_expr(expr, inside_fan),
            },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond, inside_fan),
                else_: rewrite_expr(else_, inside_fan),
            },
            other => other,
        };
        IrStmt { kind, ..stmt }
    }).collect()
}

fn rewrite_target(target: CallTarget, inside_fan: bool) -> CallTarget {
    match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(rewrite_expr(*object, inside_fan)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(rewrite_expr(*callee, inside_fan)),
        },
        other => other,
    }
}

/// Strip top-level Try wrapper (fan spawn closures return raw Result).
fn strip_try_top(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Try { expr: inner }
        | IrExprKind::Unwrap { expr: inner }
        | IrExprKind::ToOption { expr: inner } => *inner,
        IrExprKind::UnwrapOr { expr: inner, .. } => *inner,
        _ => expr,
    }
}

/// Rewrite a fan.map/race/any argument — strip Try inside lambdas and thunk lists.
fn rewrite_fan_arg(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::Lambda { params, body, lambda_id } => IrExpr {
            kind: IrExprKind::Lambda {
                params,
                body: Box::new(rewrite_expr(*body, true)),
                lambda_id,
            },
            ty, span,
        },
        IrExprKind::List { elements } => IrExpr {
            kind: IrExprKind::List {
                elements: elements.into_iter().map(rewrite_fan_arg).collect(),
            },
            ty, span,
        },
        _ => rewrite_expr(arg, false),
    }
}
