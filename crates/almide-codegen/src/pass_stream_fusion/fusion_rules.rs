//! Individual fusion transforms (IrExpr -> Option<IrExpr>).

use almide_ir::*;
use almide_lang::types::Ty;
use super::lambda_composition::*;

// ── Call target classification ────────────────────────────────────

pub(super) fn is_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "map",
        CallTarget::Named { name } => name.ends_with("_map") && !name.ends_with("flat_map") && !name.ends_with("filter_map"),
        _ => false,
    }
}

pub(super) fn is_filter_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter",
        CallTarget::Named { name } => name.ends_with("_filter") && !name.ends_with("_filter_map"),
        _ => false,
    }
}

pub(super) fn is_fold_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "fold",
        CallTarget::Named { name } => name.ends_with("_fold"),
        _ => false,
    }
}

pub(super) fn is_flatmap_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "flat_map",
        CallTarget::Named { name } => name.ends_with("_flat_map"),
        _ => false,
    }
}

pub(super) fn is_filter_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter_map",
        CallTarget::Named { name } => name.ends_with("_filter_map"),
        _ => false,
    }
}

pub(super) fn is_range_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { module, func } => module == "list" && func == "range",
        CallTarget::Named { name } => name.ends_with("_range"),
        _ => false,
    }
}

// ── FunctorIdentity: map(x, (x) => x) → x ──

pub(super) fn try_eliminate_identity_map(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, .. } = expr.kind {
        if is_map_call(target) && args.len() >= 2 && is_identity_lambda(&args[1]) {
            // `list.map(xs, x => x)` collapses to `xs` ONLY when `xs` is
            // already materialized as a list. A Range literal
            // (`0..5 |> list.map(x => x)`) would otherwise lose the
            // implicit `.collect::<Vec<_>>()` that the map call normally
            // provides, leaving a raw `Range<i64>` where a `Vec<i64>` is
            // expected.
            if matches!(args[0].kind, IrExprKind::Range { .. }) {
                return None;
            }
            return Some(args[0].clone());
        }
    }
    None
}

fn is_identity_lambda(expr: &IrExpr) -> bool {
    if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
        if params.len() == 1 {
            if let IrExprKind::Var { id } = &body.kind {
                return *id == params[0].0;
            }
        }
    }
    false
}

// ── FunctorComposition: map(map(x, f), g) → map(x, x => g(f(x))) ──

pub(super) fn try_fuse_map_map(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_map_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[1];
                    if let Some(composed) = compose_lambdas(f, g) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── FilterComposition: filter(filter(x, p), q) → filter(x, x => p(x) && q(x)) ──

pub(super) fn try_fuse_filter_filter(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_filter_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_filter_call(inner_target) && inner_args.len() >= 2 {
                    let p = &inner_args[1];
                    let q = &args[1];
                    if let Some(composed) = compose_predicates(p, q) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MapFoldFusion: fold(map(x, f), init, g) → fold(x, init, (acc,x) => g(acc, f(x))) ──

pub(super) fn try_fuse_map_fold(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[2];
                    if let Some(fused_reducer) = compose_map_into_fold(f, g) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), args[1].clone(), fused_reducer],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g)) ──

pub(super) fn try_fuse_flatmap_flatmap(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_flatmap_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_flatmap_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[1];
                    if let Some(composed) = compose_flatmaps(f, g, target, type_args) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MapFilterFusion: filter(map(x, f), p) → filter_map(x, ...) ──

pub(super) fn try_fuse_map_filter(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_filter_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let p = &args[1];
                    if let Some(filter_map_lambda) = compose_map_filter(f, p) {
                        let fm_target = match inner_target {
                            CallTarget::Module { module, .. } => CallTarget::Module {
                                module: module.clone(), func: "filter_map".into(),
                            },
                            CallTarget::Named { name } => CallTarget::Named {
                                name: name.replace("_map", "_filter_map").into(),
                            },
                            other => other.clone(),
                        };
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: fm_target,
                                args: vec![inner_args[0].clone(), filter_map_lambda],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → fold with match ──

pub(super) fn try_fuse_filter_map_fold(expr: IrExpr, count: &mut usize, vt: &mut VarTable) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_filter_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];
                    let fm = &inner_args[1];
                    let init = &args[1];
                    let g = &args[2];
                    if let Some(fused_reducer) = compose_filter_map_into_fold(fm, g, vt) {
                        *count += 1;
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![source.clone(), init.clone(), fused_reducer],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── RangeFoldFusion: fold(range(start, end), init, g) → for loop ──

pub(super) fn try_fuse_range_fold(expr: IrExpr, count: &mut usize, vt: &mut VarTable) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, .. } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_range_call(inner_target) && inner_args.len() >= 2 {
                    let start = &inner_args[0];
                    let end = &inner_args[1];
                    let init = &args[1];
                    let g = &args[2];

                    if let IrExprKind::Lambda { params: g_params, body: g_body, .. } = &g.kind {
                        if g_params.len() == 2 {
                            let (g_acc_id, g_acc_ty) = &g_params[0];
                            let (g_elem_id, g_elem_ty) = &g_params[1];

                            let acc_var = vt.alloc("__acc".into(), g_acc_ty.clone(), Mutability::Var, None);
                            let loop_var = vt.alloc("__i".into(), g_elem_ty.clone(), Mutability::Let, None);

                            let acc_ref = IrExpr { kind: IrExprKind::Var { id: acc_var }, ty: g_acc_ty.clone(), span: None };
                            let loop_ref = IrExpr { kind: IrExprKind::Var { id: loop_var }, ty: g_elem_ty.clone(), span: None };

                            let body_subst = substitute_var_in_expr(
                                &substitute_var_in_expr(g_body, *g_acc_id, &acc_ref),
                                *g_elem_id, &loop_ref,
                            );

                            *count += 1;
                            return Some(IrExpr {
                                kind: IrExprKind::Block {
                                    stmts: vec![
                                        IrStmt {
                                            kind: IrStmtKind::Bind {
                                                var: acc_var, mutability: Mutability::Var,
                                                ty: g_acc_ty.clone(), value: init.clone(),
                                            },
                                            span: None,
                                        },
                                        IrStmt {
                                            kind: IrStmtKind::Expr {
                                                expr: IrExpr {
                                                    kind: IrExprKind::ForIn {
                                                        var: loop_var, var_tuple: None,
                                                        iterable: Box::new(IrExpr {
                                                            kind: IrExprKind::Range {
                                                                start: Box::new(start.clone()),
                                                                end: Box::new(end.clone()),
                                                                inclusive: false,
                                                            },
                                                            ty: Ty::Int, span: None,
                                                        }),
                                                        body: vec![IrStmt {
                                                            kind: IrStmtKind::Assign { var: acc_var, value: body_subst },
                                                            span: None,
                                                        }],
                                                    },
                                                    ty: Ty::Unit, span: None,
                                                },
                                            },
                                            span: None,
                                        },
                                    ],
                                    expr: Some(Box::new(acc_ref)),
                                },
                                ty: expr.ty,
                                span: expr.span,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}
