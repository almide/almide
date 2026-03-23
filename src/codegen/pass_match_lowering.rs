//! MatchLowering Nanopass: convert `match` to `if/else` chains.
//!
//! Target: TypeScript (and future GC languages like Python, Go)
//!
//! Rust has native `match`, so this pass is skipped for Rust.
//! TS/JS have no `match` — we lower to if/else chains.
//!
//! Supported patterns:
//! - some(x) / none  → if (subject !== null) { let x = subject; body } else { body }
//! - ok(x) / err(e)  → if (subject.ok) { let x = subject.value; body } else { let e = subject.error; body }
//! - literal          → if (subject === literal) { body }
//! - wildcard / bind  → else { body }

use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct MatchLoweringPass;

impl NanoPass for MatchLoweringPass {
    fn name(&self) -> &str { "MatchLowering" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::TypeScript, Target::Python, Target::Go])
    }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        // Rewrite all functions
        for func in &mut program.functions {
            func.body = rewrite_expr(func.body.clone(), &mut program.var_table);
        }
        // Rewrite top-level lets
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(tl.value.clone(), &mut program.var_table);
        }
        // Rewrite module functions and top_lets (each module has its own var_table)
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = rewrite_expr(func.body.clone(), &mut module.var_table);
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(tl.value.clone(), &mut module.var_table);
            }
        }
    }
}

/// Recursively rewrite match expressions to if/else chains.
fn rewrite_expr(expr: IrExpr, vt: &mut VarTable) -> IrExpr {
    let kind = match expr.kind {
        IrExprKind::Match { subject, arms } => {
            let subject = Box::new(rewrite_expr(*subject, vt));
            let arms: Vec<IrMatchArm> = arms.into_iter()
                .map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(|g| rewrite_expr(g, vt)),
                    body: rewrite_expr(arm.body, vt),
                })
                .collect();
            lower_match(*subject, arms, &expr.ty, vt)
        }

        // Recurse into all sub-expressions
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond, vt)),
            then: Box::new(rewrite_expr(*then, vt)),
            else_: Box::new(rewrite_expr(*else_, vt)),
        },
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts, vt),
            expr: e.map(|e| Box::new(rewrite_expr(*e, vt))),
        },
        IrExprKind::DoBlock { stmts, expr: e } => IrExprKind::DoBlock {
            stmts: rewrite_stmts(stmts, vt),
            expr: e.map(|e| Box::new(rewrite_expr(*e, vt))),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(rewrite_expr(*left, vt)),
            right: Box::new(rewrite_expr(*right, vt)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(rewrite_expr(*operand, vt)),
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target,
            args: args.into_iter().map(|a| rewrite_expr(a, vt)).collect(),
            type_args,
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params,
            body: Box::new(rewrite_expr(*body, vt)),
            lambda_id,
        },
        IrExprKind::OptionSome { expr: inner } => IrExprKind::OptionSome {
            expr: Box::new(rewrite_expr(*inner, vt)),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(rewrite_expr(*inner, vt)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(rewrite_expr(*inner, vt)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e, vt)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v, vt))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object, vt)),
            field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable, vt)),
            body: rewrite_stmts(body, vt),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond, vt)),
            body: rewrite_stmts(body, vt),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr, vt) },
                other => other,
            }).collect(),
        },

        // Leaf nodes — return as-is
        other => other,
    };

    IrExpr { kind, ty: expr.ty, span: expr.span }
}

fn rewrite_stmts(stmts: Vec<IrStmt>, vt: &mut VarTable) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty,
                value: rewrite_expr(value, vt),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
                var,
                value: rewrite_expr(value, vt),
            },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr {
                expr: rewrite_expr(expr, vt),
            },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond, vt),
                else_: rewrite_expr(else_, vt),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern,
                value: rewrite_expr(value, vt),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Lower a match expression to an if/else chain.
fn lower_match(subject: IrExpr, arms: Vec<IrMatchArm>, result_ty: &Ty, vt: &mut VarTable) -> IrExprKind {
    if arms.is_empty() {
        return IrExprKind::Unit;
    }

    // Store subject in a temp variable to avoid re-evaluation
    let subj_var = vt.alloc("__match_subj".into(), subject.ty.clone(), Mutability::Let, None);
    let subj_ref = IrExpr {
        kind: IrExprKind::Var { id: subj_var },
        ty: subject.ty.clone(),
        span: None,
    };

    // Build the if/else chain from the arms (bottom-up)
    let if_chain = build_if_chain(&subj_ref, &arms, result_ty, vt);

    // Wrap in a block: { let __match_subj = subject; if_chain }
    IrExprKind::Block {
        stmts: vec![IrStmt {
            kind: IrStmtKind::Bind {
                var: subj_var,
                mutability: Mutability::Let,
                ty: subject.ty.clone(),
                value: subject,
            },
            span: None,
        }],
        expr: Some(Box::new(if_chain)),
    }
}

fn build_if_chain(subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty, vt: &mut VarTable) -> IrExpr {
    if arms.is_empty() {
        // Fallback: should never happen in well-typed code
        return IrExpr { kind: IrExprKind::Unit, ty: result_ty.clone(), span: None };
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    match &arm.pattern {
        // Wildcard — always matches (unless guarded)
        IrPattern::Wildcard => {
            if let Some(ref guard) = arm.guard {
                let else_body = build_if_chain(subject, rest, result_ty, vt);
                IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(guard.clone()),
                        then: Box::new(arm.body.clone()),
                        else_: Box::new(else_body),
                    },
                    ty: result_ty.clone(),
                    span: None,
                }
            } else {
                arm.body.clone()
            }
        }
        IrPattern::Bind { var, .. } => {
            // let var = subject; body (with optional guard)
            let bind_stmt = IrStmt {
                kind: IrStmtKind::Bind {
                    var: *var,
                    mutability: Mutability::Let,
                    ty: subject.ty.clone(),
                    value: subject.clone(),
                },
                span: None,
            };
            let body_with_bind = if let Some(ref guard) = arm.guard {
                let else_body = build_if_chain(subject, rest, result_ty, vt);
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![bind_stmt],
                        expr: Some(Box::new(IrExpr {
                            kind: IrExprKind::If {
                                cond: Box::new(guard.clone()),
                                then: Box::new(arm.body.clone()),
                                else_: Box::new(else_body),
                            },
                            ty: result_ty.clone(),
                            span: None,
                        })),
                    },
                    ty: result_ty.clone(),
                    span: None,
                }
            } else {
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![bind_stmt],
                        expr: Some(Box::new(arm.body.clone())),
                    },
                    ty: result_ty.clone(),
                    span: None,
                }
            };
            body_with_bind
        }

        // some(inner) → if (subject !== null) { let inner = subject; body }
        IrPattern::Some { inner } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Neq,
                    left: Box::new(subject.clone()),
                    right: Box::new(IrExpr { kind: IrExprKind::OptionNone, ty: subject.ty.clone(), span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            let then_body = build_pattern_bind(subject, inner, &arm.body, result_ty);
            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // none → if (subject === null) { body }
        IrPattern::None => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(subject.clone()),
                    right: Box::new(IrExpr { kind: IrExprKind::OptionNone, ty: subject.ty.clone(), span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(arm.body.clone()),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // Literal — if (subject === literal) { body }
        IrPattern::Literal { expr: lit } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(subject.clone()),
                    right: Box::new(lit.clone()),
                },
                ty: Ty::Bool,
                span: None,
            };

            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(arm.body.clone()),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // Ok(inner) → if (subject.ok === true) { let inner = subject.value; body }
        IrPattern::Ok { inner } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: "ok".into(),
                        },
                        ty: Ty::Bool,
                        span: None,
                    }),
                    right: Box::new(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            let value_expr = IrExpr {
                kind: IrExprKind::Member {
                    object: Box::new(subject.clone()),
                    field: "value".into(),
                },
                ty: unwrap_result_ok(&subject.ty),
                span: None,
            };

            let then_body = build_pattern_bind(&value_expr, inner, &arm.body, result_ty);
            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // Err(inner) → if (subject.ok === false) { let inner = subject.error; body }
        IrPattern::Err { inner } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: "ok".into(),
                        },
                        ty: Ty::Bool,
                        span: None,
                    }),
                    right: Box::new(IrExpr { kind: IrExprKind::LitBool { value: false }, ty: Ty::Bool, span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            let error_expr = IrExpr {
                kind: IrExprKind::Member {
                    object: Box::new(subject.clone()),
                    field: "error".into(),
                },
                ty: unwrap_result_err(&subject.ty),
                span: None,
            };

            let then_body = build_pattern_bind(&error_expr, inner, &arm.body, result_ty);
            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // Constructor(args) → if (subject.tag === "Name") { let args = subject.value; body }
        IrPattern::Constructor { name, args } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: "tag".into(),
                        },
                        ty: Ty::String,
                        span: None,
                    }),
                    right: Box::new(IrExpr { kind: IrExprKind::LitStr { value: name.clone() }, ty: Ty::String, span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            // Bind tuple args from subject.value array
            let mut bind_stmts = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                if let IrPattern::Bind { var, .. } = arg {
                    let val_expr = IrExpr {
                        kind: IrExprKind::IndexAccess {
                            object: Box::new(IrExpr {
                                kind: IrExprKind::Member {
                                    object: Box::new(subject.clone()),
                                    field: "value".into(),
                                },
                                ty: Ty::Unknown,
                                span: None,
                            }),
                            index: Box::new(IrExpr { kind: IrExprKind::LitInt { value: i as i64 }, ty: Ty::Int, span: None }),
                        },
                        ty: Ty::Unknown,
                        span: None,
                    };
                    bind_stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            mutability: Mutability::Let,
                            ty: Ty::Unknown,
                            value: val_expr,
                        },
                        span: None,
                    });
                }
            }

            let then_body = IrExpr {
                kind: IrExprKind::Block {
                    stmts: bind_stmts,
                    expr: Some(Box::new(arm.body.clone())),
                },
                ty: result_ty.clone(),
                span: None,
            };
            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // RecordPattern { fields } → if (subject.tag === "Name") { let field = subject.field; body }
        IrPattern::RecordPattern { name, fields, .. } => {
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: "tag".into(),
                        },
                        ty: Ty::String,
                        span: None,
                    }),
                    right: Box::new(IrExpr { kind: IrExprKind::LitStr { value: name.clone() }, ty: Ty::String, span: None }),
                },
                ty: Ty::Bool,
                span: None,
            };

            // Bind each field from subject
            let mut bind_stmts = Vec::new();
            for fp in fields {
                if fp.pattern.is_none() {
                    // Shorthand: field name = var name
                    let field_var = vt.alloc(fp.name.clone(), Ty::Unknown, Mutability::Let, None);
                    let val_expr = IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: fp.name.clone(),
                        },
                        ty: Ty::Unknown,
                        span: None,
                    };
                    bind_stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: field_var,
                            mutability: Mutability::Let,
                            ty: Ty::Unknown,
                            value: val_expr,
                        },
                        span: None,
                    });
                } else if let Some(IrPattern::Bind { var, .. }) = &fp.pattern {
                    let val_expr = IrExpr {
                        kind: IrExprKind::Member {
                            object: Box::new(subject.clone()),
                            field: fp.name.clone(),
                        },
                        ty: Ty::Unknown,
                        span: None,
                    };
                    bind_stmts.push(IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            mutability: Mutability::Let,
                            ty: Ty::Unknown,
                            value: val_expr,
                        },
                        span: None,
                    });
                }
            }

            let then_body = IrExpr {
                kind: IrExprKind::Block {
                    stmts: bind_stmts,
                    expr: Some(Box::new(arm.body.clone())),
                },
                ty: result_ty.clone(),
                span: None,
            };
            let else_body = build_if_chain(subject, rest, result_ty, vt);

            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }

        // Tuple pattern — pass through (shouldn't appear in variant match)
        IrPattern::Tuple { .. } => {
            arm.body.clone()
        }
    }
}

/// Bind pattern variable to a value, then evaluate body.
fn build_pattern_bind(value: &IrExpr, pattern: &IrPattern, body: &IrExpr, result_ty: &Ty) -> IrExpr {
    match pattern {
        IrPattern::Bind { var, .. } => {
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![IrStmt {
                        kind: IrStmtKind::Bind {
                            var: *var,
                            mutability: Mutability::Let,
                            ty: value.ty.clone(),
                            value: value.clone(),
                        },
                        span: None,
                    }],
                    expr: Some(Box::new(body.clone())),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }
        IrPattern::Wildcard => body.clone(),
        _ => body.clone(), // Complex nested patterns not yet supported
    }
}

fn unwrap_result_ok(ty: &Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
        _ => Ty::Unknown,
    }
}

fn unwrap_result_err(ty: &Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(),
        _ => Ty::Unknown,
    }
}
