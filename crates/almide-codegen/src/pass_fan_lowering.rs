//! Fan Lowering Pass — strip auto-try (?) from fan spawn closures.
//!
//! Fan expressions run in spawn closures which return raw Result.
//! The auto-try (?) is applied at the join point by the walker, not inside the closure.
//! This pass strips any Try nodes that StdlibLoweringPass inserted
//! inside Fan expressions and fan.map/fan.race/fan.any lambda arguments.

use almide_ir::*;

/// Strip Try nodes from inside Fan expressions and fan.* call arguments.
pub fn strip_fan_auto_try(program: &mut IrProgram) {
    for func in &mut program.functions {
        adapt_var_thunk_lists(&mut func.body);
        func.body = rewrite_expr(std::mem::take(&mut func.body), false);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            adapt_var_thunk_lists(&mut func.body);
            func.body = rewrite_expr(std::mem::take(&mut func.body), false);
        }
    }
}

/// #599: the Ok-adapter (`ok_adapt_thunk_list`) only descends into an INLINE
/// `[...]` literal at the fan call site. When a race/any/settle thunk list is
/// bound to a `let` first — `let ts = [...]; fan.race(ts)` — the call's arg is
/// a `Var`, the adapter's `_ => arg` passthrough leaves the thunks as the
/// uniform `Rc<dyn Fn>` repr, and the runtime bound (Send+Sync `Fn()->Result`
/// native / Result-shaped indirect-call sig wasm) is never satisfied → native
/// invalid-Rust ICE, wasm trap. A runtime re-wrap (list.map) cannot satisfy the
/// native Send+Sync bound, so we adapt STRUCTURALLY at the BINDING: collect the
/// VarIds used as a race/any/settle arg-0, then Ok-adapt the `List` value of
/// every matching `let` bind (so it reaches the call already adapted).
fn adapt_var_thunk_lists(body: &mut IrExpr) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    use almide_ir::visit_mut::{IrMutVisitor, walk_stmt_mut};
    use std::collections::HashSet;

    struct Collect { vars: HashSet<u32> }
    impl IrVisitor for Collect {
        fn visit_expr(&mut self, e: &IrExpr) {
            let (is_fan_list, arg0) = match &e.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "fan" && thunk_list_method(func.as_str()) => (true, args.first()),
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if name.as_str().starts_with("almide_rt_fan_")
                        && thunk_list_method(name.as_str().trim_start_matches("almide_rt_fan_")) => (true, args.first()),
                _ => (false, None),
            };
            if is_fan_list {
                if let Some(IrExpr { kind: IrExprKind::Var { id }, .. }) = arg0 {
                    self.vars.insert(id.0);
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = Collect { vars: HashSet::new() };
    c.visit_expr(body);
    if c.vars.is_empty() { return; }

    struct Adapt { vars: HashSet<u32> }
    impl IrMutVisitor for Adapt {
        fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
            if let IrStmtKind::Bind { var, value, .. } = &mut stmt.kind {
                if self.vars.contains(&var.0) && matches!(&value.kind, IrExprKind::List { .. }) {
                    let taken = std::mem::replace(value, unit_placeholder());
                    *value = ok_adapt_thunk_list(taken);
                }
            }
            walk_stmt_mut(self, stmt);
        }
    }
    let mut a = Adapt { vars: c.vars };
    a.visit_expr_mut(body);
}

fn unit_placeholder() -> IrExpr {
    IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None, def_id: None }
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
        IrExprKind::Call { target: CallTarget::Module { ref module, ref func, .. }, .. }
            if module == "fan" =>
        {
            let adapt = thunk_list_method(func.as_str());
            let IrExprKind::Call { target, args, type_args } = expr.kind else { unreachable!() };
            IrExprKind::Call {
                target,
                args: args.into_iter().enumerate()
                    .map(|(i, a)| {
                        let a = rewrite_fan_arg(a);
                        if adapt && i == 0 { ok_adapt_thunk_list(a) } else { a }
                    })
                    .collect(),
                type_args,
            }
        }
        IrExprKind::Call { target: CallTarget::Named { ref name }, .. }
            if name.starts_with("almide_rt_fan_") =>
        {
            let adapt = thunk_list_method(name.as_str().trim_start_matches("almide_rt_fan_"));
            let IrExprKind::Call { target, args, type_args } = expr.kind else { unreachable!() };
            IrExprKind::Call {
                target,
                args: args.into_iter().enumerate()
                    .map(|(i, a)| {
                        let a = rewrite_fan_arg(a);
                        if adapt && i == 0 { ok_adapt_thunk_list(a) } else { a }
                    })
                    .collect(),
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
        IrExprKind::RuntimeCall { symbol, args } => IrExprKind::RuntimeCall {
            symbol,
            args: args.into_iter().map(|a| rewrite_expr(a, inside_fan)).collect(),
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
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_expr(*expr, inside_fan)), field,
        },
        // Any other kind: recurse into every child (total by construction).
        other => return IrExpr { kind: other, ty, span, def_id: None }
            .map_children(&mut |e| rewrite_expr(e, inside_fan)),
    };

    IrExpr { kind, ty, span, def_id: None }
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
            other => return IrStmt { kind: other, ..stmt }
                .map_exprs(&mut |e| rewrite_expr(e, inside_fan)),
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
        // No IrExpr children — total by construction (new variant = compile error).
        other @ (CallTarget::Named { .. } | CallTarget::Module { .. }) => other,
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
            ty, span, def_id: None,
        },
        IrExprKind::List { elements } => IrExpr {
            kind: IrExprKind::List {
                elements: elements.into_iter().map(rewrite_fan_arg).collect(),
            },
            ty, span, def_id: None,
        },
        _ => rewrite_expr(arg, false),
    }
}


/// The fan APIs whose FIRST argument is a thunk LIST with a
/// `Fn() -> Result[T, String]` runtime bound.
fn thunk_list_method(name: &str) -> bool {
    matches!(name, "race" | "any" | "settle")
}

/// #514: the race/any/settle runtimes (both targets) require thunks that
/// return `Result[T, String]` — native's `Vec<impl Fn() -> Result<T,_>>`
/// bound, wasm's `(env: i32) -> i32` indirect-call signature. A PURE thunk
/// (`fn() -> Int`) satisfied the CHECKER but broke each backend in its own
/// way: native emitted invalid Rust (E0271), wasm trapped with an indirect
/// call type mismatch. Adapt once, here, for both: wrap every non-Result
/// thunk so it returns `Ok(value)`.
fn ok_adapt_thunk_list(arg: IrExpr) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;
    match arg.kind {
        IrExprKind::List { elements } => IrExpr {
            kind: IrExprKind::List {
                elements: elements.into_iter().map(ok_adapt_thunk).collect(),
            },
            ty, span, def_id: None,
        },
        _ => IrExpr { kind: arg.kind, ty, span, def_id: arg.def_id },
    }
}

fn ok_adapt_thunk(el: IrExpr) -> IrExpr {
    use almide_lang::types::Ty;
    let Ty::Fn { params, ret } = &el.ty else { return el };
    if !params.is_empty() || ret.is_result() {
        return el;
    }
    let span = el.span;
    let ok_ty = Ty::result((**ret).clone(), Ty::String);
    let fn_ty = Ty::Fn { params: vec![], ret: Box::new(ok_ty.clone()) };
    match el.kind {
        // A literal pure lambda: wrap its BODY — no new call frame needed.
        IrExprKind::Lambda { params: ps, body, lambda_id } => IrExpr {
            kind: IrExprKind::Lambda {
                params: ps,
                body: Box::new(IrExpr {
                    ty: ok_ty,
                    span: body.span,
                    kind: IrExprKind::ResultOk { expr: body },
                    def_id: None,
                }),
                lambda_id,
            },
            ty: fn_ty, span, def_id: None,
        },
        // A fn ref / stored closure value: synthesize `() => ok(el())`.
        _ => {
            let ret_ty = (**ret).clone();
            let call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Computed { callee: Box::new(el) },
                    args: vec![],
                    type_args: vec![],
                },
                ty: ret_ty,
                span,
                def_id: None,
            };
            IrExpr {
                kind: IrExprKind::Lambda {
                    params: vec![],
                    body: Box::new(IrExpr {
                        ty: ok_ty,
                        span,
                        kind: IrExprKind::ResultOk { expr: Box::new(call) },
                        def_id: None,
                    }),
                    lambda_id: None,
                },
                ty: fn_ty,
                span,
                def_id: None,
            }
        }
    }
}
