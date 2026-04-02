//! ListPatternLowering: desugar list patterns in match arms to if/else chains.
//!
//! Rewrites match arms containing `IrPattern::List` into length checks + indexing.
//! This runs on ALL targets before other match-related passes.
//!
//! Example:
//!   match xs {
//!     []     => "empty"
//!     [x]    => f(x)
//!     [a, b] => g(a, b)
//!     _      => "other"
//!   }
//!
//! Becomes (at IR level):
//!   if list.is_empty(xs) then "empty"
//!   else if list.len(xs) == 1 then { let x = xs[0]; f(x) }
//!   else if list.len(xs) == 2 then { let a = xs[0]; let b = xs[1]; g(a, b) }
//!   else "other"

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ListPatternLoweringPass;

impl NanoPass for ListPatternLoweringPass {
    fn name(&self) -> &str { "ListPatternLowering" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            let (body, c) = rewrite_expr(std::mem::take(&mut func.body), &mut program.var_table);
            func.body = body;
            changed |= c;
        }
        for tl in &mut program.top_lets {
            let (val, c) = rewrite_expr(std::mem::take(&mut tl.value), &mut program.var_table);
            tl.value = val;
            changed |= c;
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let (body, c) = rewrite_expr(std::mem::take(&mut func.body), &mut module.var_table);
                func.body = body;
                changed |= c;
            }
            for tl in &mut module.top_lets {
                let (val, c) = rewrite_expr(std::mem::take(&mut tl.value), &mut module.var_table);
                tl.value = val;
                changed |= c;
            }
        }
        PassResult { program, changed }
    }
}

/// Check if a match has any list pattern arms (recursively checks nested patterns).
fn has_list_patterns(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| pattern_contains_list(&arm.pattern))
}

fn pattern_contains_list(pat: &IrPattern) -> bool {
    match pat {
        IrPattern::List { .. } => true,
        IrPattern::Tuple { elements } => elements.iter().any(pattern_contains_list),
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => pattern_contains_list(inner),
        IrPattern::Constructor { args, .. } => args.iter().any(pattern_contains_list),
        _ => false,
    }
}

/// Recursively rewrite expressions, desugaring match with list patterns.
fn rewrite_expr(expr: IrExpr, vt: &mut VarTable) -> (IrExpr, bool) {
    let mut changed = false;
    let kind = match expr.kind {
        IrExprKind::Match { subject, arms } if has_list_patterns(&arms) => {
            let (subject, c1) = rewrite_expr(*subject, vt);
            changed |= c1;
            let arms: Vec<IrMatchArm> = arms.into_iter().map(|arm| {
                let (body, c) = rewrite_expr(arm.body, vt);
                changed |= c;
                let guard = arm.guard.map(|g| { let (g2, c) = rewrite_expr(g, vt); changed |= c; g2 });
                IrMatchArm { pattern: arm.pattern, guard, body }
            }).collect();
            changed = true;
            lower_list_match(subject, arms, &expr.ty, vt)
        }
        IrExprKind::Match { subject, arms } => {
            let (subject, c1) = rewrite_expr(*subject, vt);
            changed |= c1;
            let arms: Vec<IrMatchArm> = arms.into_iter().map(|arm| {
                let (body, c) = rewrite_expr(arm.body, vt);
                changed |= c;
                let guard = arm.guard.map(|g| { let (g2, c) = rewrite_expr(g, vt); changed |= c; g2 });
                IrMatchArm { pattern: arm.pattern, guard, body }
            }).collect();
            IrExprKind::Match { subject: Box::new(subject), arms }
        }
        IrExprKind::If { cond, then, else_ } => {
            let (c, c1) = rewrite_expr(*cond, vt); changed |= c1;
            let (t, c2) = rewrite_expr(*then, vt); changed |= c2;
            let (e, c3) = rewrite_expr(*else_, vt); changed |= c3;
            IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }
        }
        IrExprKind::Block { stmts, expr: e } => {
            let stmts = rewrite_stmts(stmts, vt, &mut changed);
            let e = e.map(|e| { let (r, c) = rewrite_expr(*e, vt); changed |= c; Box::new(r) });
            IrExprKind::Block { stmts, expr: e }
        }
        IrExprKind::Lambda { params, body, lambda_id } => {
            let (b, c) = rewrite_expr(*body, vt); changed |= c;
            IrExprKind::Lambda { params, body: Box::new(b), lambda_id }
        }
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| { let (r, c) = rewrite_expr(a, vt); changed |= c; r }).collect();
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            let (it, c1) = rewrite_expr(*iterable, vt); changed |= c1;
            let body = rewrite_stmts(body, vt, &mut changed);
            IrExprKind::ForIn { var, var_tuple, iterable: Box::new(it), body }
        }
        IrExprKind::While { cond, body } => {
            let (c, c1) = rewrite_expr(*cond, vt); changed |= c1;
            let body = rewrite_stmts(body, vt, &mut changed);
            IrExprKind::While { cond: Box::new(c), body }
        }
        other => other,
    };
    (IrExpr { kind, ty: expr.ty, span: expr.span }, changed)
}

fn rewrite_stmts(stmts: Vec<IrStmt>, vt: &mut VarTable, changed: &mut bool) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => {
                let (v, c) = rewrite_expr(value, vt); *changed |= c;
                IrStmtKind::Bind { var, mutability, ty, value: v }
            }
            IrStmtKind::Assign { var, value } => {
                let (v, c) = rewrite_expr(value, vt); *changed |= c;
                IrStmtKind::Assign { var, value: v }
            }
            IrStmtKind::Expr { expr } => {
                let (e, c) = rewrite_expr(expr, vt); *changed |= c;
                IrStmtKind::Expr { expr: e }
            }
            IrStmtKind::Guard { cond, else_ } => {
                let (c, c1) = rewrite_expr(cond, vt); *changed |= c1;
                let (e, c2) = rewrite_expr(else_, vt); *changed |= c2;
                IrStmtKind::Guard { cond: c, else_: e }
            }
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}

/// Lower a match with list patterns to an if/else chain.
/// Uses the subject expression directly (no temp variable) to avoid
/// type mismatches when borrow passes later change the parameter type.
fn lower_list_match(subject: IrExpr, arms: Vec<IrMatchArm>, result_ty: &Ty, _vt: &mut VarTable) -> IrExprKind {
    build_list_if_chain(&subject, &arms, result_ty, _vt).kind
}

fn build_list_if_chain(subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty, vt: &mut VarTable) -> IrExpr {
    if arms.is_empty() {
        return IrExpr { kind: IrExprKind::Unit, ty: result_ty.clone(), span: None };
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    match &arm.pattern {
        IrPattern::List { elements } => {
            // Build condition: list.len(subject) == N
            let len_call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module: sym("list"), func: sym("len") },
                    args: vec![subject.clone()],
                    type_args: vec![],
                },
                ty: Ty::Int,
                span: None,
            };
            let expected_len = IrExpr {
                kind: IrExprKind::LitInt { value: elements.len() as i64 },
                ty: Ty::Int,
                span: None,
            };
            let cond = IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(len_call),
                    right: Box::new(expected_len),
                },
                ty: Ty::Bool,
                span: None,
            };

            // Build body: let bindings for each element + original body
            let elem_ty = match &subject.ty {
                Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].clone(),
                _ => Ty::Unknown,
            };

            let mut stmts = Vec::new();
            let mut extra_conds: Vec<IrExpr> = Vec::new();
            for (i, elem_pat) in elements.iter().enumerate() {
                let index_expr = IrExpr {
                    kind: IrExprKind::IndexAccess {
                        object: Box::new(subject.clone()),
                        index: Box::new(IrExpr {
                            kind: IrExprKind::LitInt { value: i as i64 },
                            ty: Ty::Int,
                            span: None,
                        }),
                    },
                    ty: elem_ty.clone(),
                    span: None,
                };
                match elem_pat {
                    IrPattern::Bind { var, .. } => {
                        stmts.push(IrStmt {
                            kind: IrStmtKind::Bind {
                                var: *var,
                                mutability: Mutability::Let,
                                ty: elem_ty.clone(),
                                value: index_expr,
                            },
                            span: None,
                        });
                    }
                    IrPattern::Literal { expr: lit_expr } => {
                        // Add equality check: subject[i] == literal
                        extra_conds.push(IrExpr {
                            kind: IrExprKind::BinOp {
                                op: BinOp::Eq,
                                left: Box::new(index_expr),
                                right: Box::new(lit_expr.clone()),
                            },
                            ty: Ty::Bool,
                            span: None,
                        });
                    }
                    _ => {} // Wildcard: no binding or check needed
                }
            }

            // Combine length check with element literal checks
            let mut combined_cond = cond;
            for ec in extra_conds {
                combined_cond = IrExpr {
                    kind: IrExprKind::BinOp {
                        op: BinOp::And,
                        left: Box::new(combined_cond),
                        right: Box::new(ec),
                    },
                    ty: Ty::Bool,
                    span: None,
                };
            }

            // Apply guard if present — guard must be evaluated AFTER let bindings
            let then_body = if let Some(ref guard) = arm.guard {
                let else_body = build_list_if_chain(subject, rest, result_ty, vt);
                // { let bindings; if guard then body else fallthrough }
                let guarded = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(guard.clone()),
                        then: Box::new(arm.body.clone()),
                        else_: Box::new(else_body),
                    },
                    ty: result_ty.clone(),
                    span: None,
                };
                if stmts.is_empty() {
                    guarded
                } else {
                    IrExpr {
                        kind: IrExprKind::Block {
                            stmts,
                            expr: Some(Box::new(guarded)),
                        },
                        ty: result_ty.clone(),
                        span: None,
                    }
                }
            } else if stmts.is_empty() {
                arm.body.clone()
            } else {
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts,
                        expr: Some(Box::new(arm.body.clone())),
                    },
                    ty: result_ty.clone(),
                    span: None,
                }
            };

            let else_body = build_list_if_chain(subject, rest, result_ty, vt);
            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(combined_cond),
                    then: Box::new(then_body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }
        // Tuple containing list patterns: extract list checks from tuple elements
        IrPattern::Tuple { elements } if elements.iter().any(pattern_contains_list) => {
            // Build conditions for list elements within the tuple
            let tuple_tys = match &subject.ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![Ty::Unknown; elements.len()],
            };
            let mut conds: Vec<IrExpr> = Vec::new();
            let mut stmts: Vec<IrStmt> = Vec::new();

            for (i, elem_pat) in elements.iter().enumerate() {
                let elem_access = IrExpr {
                    kind: IrExprKind::TupleIndex {
                        object: Box::new(subject.clone()),
                        index: i,
                    },
                    ty: tuple_tys.get(i).cloned().unwrap_or(Ty::Unknown),
                    span: None,
                };
                match elem_pat {
                    IrPattern::List { elements: list_elems } => {
                        // Length check
                        let len_call = IrExpr {
                            kind: IrExprKind::Call {
                                target: CallTarget::Module { module: sym("list"), func: sym("len") },
                                args: vec![elem_access.clone()],
                                type_args: vec![],
                            },
                            ty: Ty::Int,
                            span: None,
                        };
                        conds.push(IrExpr {
                            kind: IrExprKind::BinOp {
                                op: BinOp::Eq,
                                left: Box::new(len_call),
                                right: Box::new(IrExpr {
                                    kind: IrExprKind::LitInt { value: list_elems.len() as i64 },
                                    ty: Ty::Int,
                                    span: None,
                                }),
                            },
                            ty: Ty::Bool,
                            span: None,
                        });
                        // Bind list elements
                        let inner_elem_ty = match tuple_tys.get(i) {
                            Some(Ty::Applied(TypeConstructorId::List, args)) if !args.is_empty() => args[0].clone(),
                            _ => Ty::Unknown,
                        };
                        for (j, lp) in list_elems.iter().enumerate() {
                            if let IrPattern::Bind { var, .. } = lp {
                                stmts.push(IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: *var,
                                        mutability: Mutability::Let,
                                        ty: inner_elem_ty.clone(),
                                        value: IrExpr {
                                            kind: IrExprKind::IndexAccess {
                                                object: Box::new(elem_access.clone()),
                                                index: Box::new(IrExpr {
                                                    kind: IrExprKind::LitInt { value: j as i64 },
                                                    ty: Ty::Int,
                                                    span: None,
                                                }),
                                            },
                                            ty: inner_elem_ty.clone(),
                                            span: None,
                                        },
                                    },
                                    span: None,
                                });
                            }
                        }
                    }
                    IrPattern::Bind { var, .. } => {
                        stmts.push(IrStmt {
                            kind: IrStmtKind::Bind {
                                var: *var,
                                mutability: Mutability::Let,
                                ty: tuple_tys.get(i).cloned().unwrap_or(Ty::Unknown),
                                value: elem_access,
                            },
                            span: None,
                        });
                    }
                    _ => {} // Wildcard — no action
                }
            }

            // Combine conditions
            let combined_cond = if conds.is_empty() {
                IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None }
            } else {
                conds.into_iter().reduce(|a, b| IrExpr {
                    kind: IrExprKind::BinOp { op: BinOp::And, left: Box::new(a), right: Box::new(b) },
                    ty: Ty::Bool,
                    span: None,
                }).unwrap()
            };

            let body = if stmts.is_empty() {
                arm.body.clone()
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(arm.body.clone())) },
                    ty: result_ty.clone(),
                    span: None,
                }
            };

            let else_body = build_list_if_chain(subject, rest, result_ty, vt);
            IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(combined_cond),
                    then: Box::new(body),
                    else_: Box::new(else_body),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }
        // Non-list patterns: re-wrap into a match with remaining arms
        IrPattern::Wildcard if arm.guard.is_none() => arm.body.clone(),
        IrPattern::Bind { var, .. } if arm.guard.is_none() => {
            let bind_stmt = IrStmt {
                kind: IrStmtKind::Bind {
                    var: *var,
                    mutability: Mutability::Let,
                    ty: subject.ty.clone(),
                    value: subject.clone(),
                },
                span: None,
            };
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![bind_stmt],
                    expr: Some(Box::new(arm.body.clone())),
                },
                ty: result_ty.clone(),
                span: None,
            }
        }
        _ => {
            // For non-list patterns mixed with list patterns, fall through to a sub-match
            let remaining_arms: Vec<IrMatchArm> = std::iter::once(arm.clone())
                .chain(rest.iter().cloned())
                .filter(|a| !matches!(&a.pattern, IrPattern::List { .. }))
                .collect();
            if remaining_arms.is_empty() {
                IrExpr { kind: IrExprKind::Unit, ty: result_ty.clone(), span: None }
            } else {
                IrExpr {
                    kind: IrExprKind::Match {
                        subject: Box::new(subject.clone()),
                        arms: remaining_arms,
                    },
                    ty: result_ty.clone(),
                    span: None,
                }
            }
        }
    }
}
