//! StdlibLoweringPass: transform Module calls into Named calls with IR-level arg decoration.
//!
//! Uses build.rs-generated `arg_transforms::lookup()` table to know exactly
//! how each argument should be decorated (BorrowStr, BorrowRef, ToVec, LambdaClone, Direct).
//!
//! NO string rendering. All decisions are structural IR transformations.

use crate::ir::*;
use crate::types::Ty;
use crate::generated::arg_transforms::{self, ArgTransform};
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct StdlibLoweringPass;

impl NanoPass for StdlibLoweringPass {
    fn name(&self) -> &str { "StdlibLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn run(&self, program: &mut IrProgram, _target: Target) {
        for func in &mut program.functions {
            func.body = rewrite_expr(func.body.clone(), func.is_effect);
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(tl.value.clone(), false);
        }
    }
}

fn rewrite_expr(expr: IrExpr, in_effect: bool) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, type_args } => {
            // Recurse into args first (fan auto-try is handled by FanLoweringPass)
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a, in_effect)).collect();

            // Look up per-function transform table
            let info = arg_transforms::lookup(&module, &func);
            let rt_name = info.as_ref().map(|i| i.name.to_string())
                .unwrap_or_else(|| format!("almide_rt_{}_{}", module, func));
            let is_effect_call = info.as_ref().map(|i| i.effect).unwrap_or(false);

            // Fill missing optional args with OptionNone
            let total_params = info.as_ref().map(|i| i.args.len()).unwrap_or(args.len());
            let mut args = args;
            while args.len() < total_params {
                args.push(IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: Ty::Option(Box::new(Ty::Unknown)),
                    span: None,
                });
            }

            // Decorate each arg based on the transform table
            let decorated_args: Vec<IrExpr> = args.into_iter().enumerate().map(|(i, arg)| {
                let transform = info.as_ref()
                    .and_then(|info| info.args.get(i).copied())
                    .unwrap_or(ArgTransform::Direct);

                decorate_arg(arg, transform)
            }).collect();

            // Build the Named call
            let mut call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: rt_name },
                    args: decorated_args,
                    type_args,
                },
                ty: ty.clone(),
                span,
            };

            // Effect fn: auto-? for Result-returning effect calls
            if in_effect && is_effect_call && matches!(&ty, Ty::Result(_, _)) {
                let inner_ty = match &ty {
                    Ty::Result(ok, _) => ok.as_ref().clone(),
                    _ => ty.clone(),
                };
                call = IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(call) },
                    ty: inner_ty,
                    span,
                };
            }

            return call;
        }

        // Recurse into all sub-expressions (same as before)
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| rewrite_expr(a, in_effect)).collect();
            let target = match target {
                CallTarget::Method { object, method } => {
                    let object = Box::new(rewrite_expr(*object, in_effect));
                    // UFCS: "module.func" method → convert to Module call and process
                    // Only if the module.func exists in stdlib (arg_transforms table)
                    if method.contains('.') && !method.ends_with(".encode") && !method.ends_with(".decode") {
                        if let Some(dot_pos) = method.find('.') {
                            let mod_name = &method[..dot_pos];
                            let func_name = &method[dot_pos+1..];
                            // Check if this is a real stdlib function
                            if arg_transforms::lookup(mod_name, func_name).is_none() {
                                // Not a stdlib function — leave as Method call for BuiltinLoweringPass
                                return IrExpr { kind: IrExprKind::Call {
                                    target: CallTarget::Method { object, method },
                                    args, type_args,
                                }, ty, span };
                            }
                            let mut call_args = vec![*object];
                            call_args.extend(args);
                            // Recursively process as Module call
                            let module_call = IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Module { module: mod_name.to_string(), func: func_name.to_string() },
                                    args: call_args, type_args,
                                },
                                ty: ty.clone(), span,
                            };
                            return rewrite_expr(module_call, in_effect);
                        }
                    }
                    CallTarget::Method { object, method }
                }
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_expr(*callee, in_effect)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond, in_effect)),
            then: Box::new(rewrite_expr(*then, in_effect)),
            else_: Box::new(rewrite_expr(*else_, in_effect)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts, in_effect),
            expr: expr.map(|e| Box::new(rewrite_expr(*e, in_effect))),
        },
        IrExprKind::DoBlock { stmts, expr } => IrExprKind::DoBlock {
            stmts: rewrite_stmts(stmts, in_effect),
            expr: expr.map(|e| Box::new(rewrite_expr(*e, in_effect))),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject, in_effect)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rewrite_expr(g, in_effect)),
                body: rewrite_expr(arm.body, in_effect),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_expr(*left, in_effect)), right: Box::new(rewrite_expr(*right, in_effect)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_expr(*operand, in_effect)),
        },
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_expr(*body, in_effect)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e, in_effect)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rewrite_expr(e, in_effect)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v, in_effect))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base, in_effect)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v, in_effect))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr, in_effect)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr, in_effect)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr, in_effect)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object, in_effect)), field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable, in_effect)),
            body: rewrite_stmts(body, in_effect),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond, in_effect)),
            body: rewrite_stmts(body, in_effect),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr, in_effect) },
                other => other,
            }).collect(),
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr, in_effect)) },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k, in_effect), rewrite_expr(v, in_effect))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start, in_effect)),
            end: Box::new(rewrite_expr(*end, in_effect)),
            inclusive,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object, in_effect)),
            index: Box::new(rewrite_expr(*index, in_effect)),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            // FanLoweringPass will strip auto-try from these later
            exprs: exprs.into_iter().map(|e| rewrite_expr(e, in_effect)).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn rewrite_stmts(stmts: Vec<IrStmt>, in_effect: bool) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: rewrite_expr(value, in_effect),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_expr(value, in_effect) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_expr(expr, in_effect) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond, in_effect), else_: rewrite_expr(else_, in_effect),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: rewrite_expr(value, in_effect),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Decorate a single argument based on the per-function transform.
fn decorate_arg(arg: IrExpr, transform: ArgTransform) -> IrExpr {
    let ty = arg.ty.clone();
    let span = arg.span;

    match transform {
        ArgTransform::Direct => arg,

        ArgTransform::BorrowStr => {
            // &*expr
            IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true },
                ty, span,
            }
        }

        ArgTransform::BorrowRef => {
            // &expr
            IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false },
                ty, span,
            }
        }

        ArgTransform::ToVec => {
            // (expr).to_vec()
            IrExpr {
                kind: IrExprKind::ToVec { expr: Box::new(arg) },
                ty, span,
            }
        }

        ArgTransform::LambdaClone => {
            // Lambda: add clone bindings for each param
            match arg.kind {
                IrExprKind::Lambda { params, body } => {
                    let clone_stmts: Vec<IrStmt> = params.iter()
                        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
                        .map(|(id, param_ty)| {
                            IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *id,
                                    mutability: Mutability::Let,
                                    ty: param_ty.clone(),
                                    value: IrExpr {
                                        kind: IrExprKind::Clone {
                                            expr: Box::new(IrExpr {
                                                kind: IrExprKind::Var { id: *id },
                                                ty: param_ty.clone(),
                                                span: None,
                                            }),
                                        },
                                        ty: param_ty.clone(),
                                        span: None,
                                    },
                                },
                                span: None,
                            }
                        }).collect();

                    let wrapped_body = if clone_stmts.is_empty() {
                        *body
                    } else {
                        let body_ty = body.ty.clone();
                        let body_span = body.span;
                        IrExpr {
                            kind: IrExprKind::Block {
                                stmts: clone_stmts,
                                expr: Some(body),
                            },
                            ty: body_ty,
                            span: body_span,
                        }
                    };

                    IrExpr {
                        kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body) },
                        ty, span,
                    }
                }
                // FnRef: pass as-is (function reference, not a lambda)
                _ => arg,
            }
        }

        ArgTransform::WrapSome => {
            // Some(expr) — but if arg is already OptionNone, pass as-is (optional param omitted)
            if matches!(&arg.kind, IrExprKind::OptionNone) {
                arg
            } else {
                IrExpr {
                    kind: IrExprKind::OptionSome { expr: Box::new(arg) },
                    ty: Ty::Option(Box::new(ty)),
                    span,
                }
            }
        }

        ArgTransform::LambdaResultWrap => {
            // Lambda with Ok(body) wrapping: callback body gets wrapped in ResultOk
            match arg.kind {
                IrExprKind::Lambda { params, body } => {
                    // Clone bindings (same as LambdaClone)
                    let clone_stmts: Vec<IrStmt> = params.iter()
                        .filter(|(_, t)| !matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit))
                        .map(|(id, param_ty)| {
                            IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *id,
                                    mutability: Mutability::Let,
                                    ty: param_ty.clone(),
                                    value: IrExpr {
                                        kind: IrExprKind::Clone {
                                            expr: Box::new(IrExpr {
                                                kind: IrExprKind::Var { id: *id },
                                                ty: param_ty.clone(),
                                                span: None,
                                            }),
                                        },
                                        ty: param_ty.clone(),
                                        span: None,
                                    },
                                },
                                span: None,
                            }
                        }).collect();

                    // Wrap body in ResultOk
                    let body_ty = body.ty.clone();
                    let ok_body = IrExpr {
                        kind: IrExprKind::ResultOk { expr: body },
                        ty: Ty::Result(Box::new(body_ty.clone()), Box::new(Ty::String)),
                        span: None,
                    };

                    let wrapped_body = if clone_stmts.is_empty() {
                        ok_body
                    } else {
                        IrExpr {
                            kind: IrExprKind::Block {
                                stmts: clone_stmts,
                                expr: Some(Box::new(ok_body)),
                            },
                            ty: Ty::Result(Box::new(body_ty), Box::new(Ty::String)),
                            span: None,
                        }
                    };

                    IrExpr {
                        kind: IrExprKind::Lambda { params, body: Box::new(wrapped_body) },
                        ty, span,
                    }
                }
                _ => arg,
            }
        }
    }
}
