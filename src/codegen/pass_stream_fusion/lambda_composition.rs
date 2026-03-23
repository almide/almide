//! Lambda composition helpers for stream fusion.

use crate::ir::*;
use crate::types::Ty;

/// Compose two lambdas: f and g → (x) => g(f(x))
pub(super) fn compose_lambdas(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body, .. },
        IrExprKind::Lambda { params: g_params, body: g_body, .. },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_param_id, _) = &g_params[0];
        let composed_body = substitute_var_in_expr(g_body, *g_param_id, f_body);
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
                lambda_id: None,
            },
            ty: g.ty.clone(),
            span: f.span,
        });
    }
    None
}

/// Compose two predicates: p and q → (x) => p(x) && q(x)
pub(super) fn compose_predicates(p: &IrExpr, q: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: p_params, body: p_body, .. },
        IrExprKind::Lambda { params: q_params, body: q_body, .. },
    ) = (&p.kind, &q.kind) {
        if p_params.len() != 1 || q_params.len() != 1 { return None; }
        let (p_param_id, p_param_ty) = &p_params[0];
        let (q_param_id, _) = &q_params[0];
        let q_body_subst = substitute_var_in_expr(q_body, *q_param_id, &IrExpr {
            kind: IrExprKind::Var { id: *p_param_id },
            ty: p_param_ty.clone(), span: None,
        });
        let composed_body = IrExpr {
            kind: IrExprKind::BinOp {
                op: crate::ir::BinOp::And,
                left: p_body.clone(),
                right: Box::new(q_body_subst),
            },
            ty: crate::types::Ty::Bool, span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*p_param_id, p_param_ty.clone())],
                body: Box::new(composed_body),
                lambda_id: None,
            },
            ty: p.ty.clone(), span: p.span,
        });
    }
    None
}

/// Compose map f into fold reducer g: (acc, x) => g(acc, f(x))
pub(super) fn compose_map_into_fold(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body, .. },
        IrExprKind::Lambda { params: g_params, body: g_body, .. },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 2 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _) = &g_params[1];
        let g_body_subst = substitute_var_in_expr(g_body, *g_elem_id, f_body);
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*g_acc_id, g_acc_ty.clone()), (*f_param_id, f_param_ty.clone())],
                body: Box::new(g_body_subst),
                lambda_id: None,
            },
            ty: g.ty.clone(), span: g.span,
        });
    }
    None
}

/// Compose two flat_map functions: f and g → (x) => flat_map(f(x), g)
pub(super) fn compose_flatmaps(f: &IrExpr, g: &IrExpr, target: &CallTarget, type_args: &[Ty]) -> Option<IrExpr> {
    if let IrExprKind::Lambda { params: f_params, body: f_body, .. } = &f.kind {
        if f_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let inner_call = IrExpr {
            kind: IrExprKind::Call {
                target: target.clone(),
                args: vec![*f_body.clone(), g.clone()],
                type_args: type_args.to_vec(),
            },
            ty: f.ty.clone(), span: f.span,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(inner_call),
                lambda_id: None,
            },
            ty: f.ty.clone(), span: f.span,
        });
    }
    None
}

/// Compose filter_map lambda and fold reducer into a single match-based reducer.
pub(super) fn compose_filter_map_into_fold(fm: &IrExpr, g: &IrExpr, vt: &mut VarTable) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: fm_params, body: fm_body, .. },
        IrExprKind::Lambda { params: g_params, body: g_body, .. },
    ) = (&fm.kind, &g.kind) {
        if fm_params.len() != 1 || g_params.len() != 2 { return None; }
        let (fm_param_id, fm_param_ty) = &fm_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _) = &g_params[1];

        let fm_call = *fm_body.clone();
        let v_var = vt.alloc("__fused_v".into(), g_acc_ty.clone(), Mutability::Let, None);
        let some_arm = IrMatchArm {
            pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: v_var, ty: g_acc_ty.clone() }) },
            guard: None,
            body: substitute_var_in_expr(g_body, *g_elem_id, &IrExpr {
                kind: IrExprKind::Var { id: v_var }, ty: g_acc_ty.clone(), span: None,
            }),
        };
        let none_arm = IrMatchArm {
            pattern: IrPattern::None, guard: None,
            body: IrExpr { kind: IrExprKind::Var { id: *g_acc_id }, ty: g_acc_ty.clone(), span: None },
        };
        let match_expr = IrExpr {
            kind: IrExprKind::Match { subject: Box::new(fm_call), arms: vec![some_arm, none_arm] },
            ty: g_acc_ty.clone(), span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*g_acc_id, g_acc_ty.clone()), (*fm_param_id, fm_param_ty.clone())],
                body: Box::new(match_expr),
                lambda_id: None,
            },
            ty: g.ty.clone(), span: g.span,
        });
    }
    None
}

/// Compose map function f and filter predicate p into a filter_map lambda.
pub(super) fn compose_map_filter(f: &IrExpr, p: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body, .. },
        IrExprKind::Lambda { params: p_params, body: p_body, .. },
    ) = (&f.kind, &p.kind) {
        if f_params.len() != 1 || p_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (p_param_id, _) = &p_params[0];
        let p_applied = substitute_var_in_expr(p_body, *p_param_id, f_body);
        let result_ty = f_body.ty.clone();
        let composed_body = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(p_applied),
                then: Box::new(IrExpr {
                    kind: IrExprKind::OptionSome { expr: f_body.clone() },
                    ty: Ty::option(result_ty.clone()), span: None,
                }),
                else_: Box::new(IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: Ty::option(result_ty), span: None,
                }),
            },
            ty: f_body.ty.clone(), span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
                lambda_id: None,
            },
            ty: f.ty.clone(), span: f.span,
        });
    }
    None
}
