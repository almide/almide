//! Utility functions for the walker: template fallback, statement termination,
//! type inspection helpers, and loop control detection.

use crate::ir::*;
use crate::types::Ty;
use super::RenderContext;

/// Try to render via template, fallback to default string.
pub fn template_or(ctx: &RenderContext, construct: &str, attrs: &[&str], fallback: &str) -> String {
    ctx.templates.render_with(construct, None, attrs, &[])
        .unwrap_or_else(|| fallback.to_string())
}

/// Add statement terminator (`;` in Rust, `;` in TS) if the rendered string doesn't already end with one
pub fn terminate_stmt(ctx: &RenderContext, rendered: String) -> String {
    let term = template_or(ctx, "stmt_terminator", &[], ";");
    if !term.is_empty() && !rendered.ends_with(';') && !rendered.ends_with('}') {
        format!("{}{}", rendered, term)
    } else {
        rendered
    }
}

/// Check if a type contains a reference to a named type (for recursive Box detection).
/// Uses Ty::any_child_recursive for uniform traversal.
pub fn ty_contains_name(ty: &Ty, name: &str) -> bool {
    ty.any_child_recursive(&|t| match t {
        Ty::Named(n, _) => n == name,
        Ty::Variant { name: vn, .. } => vn == name,
        _ => false,
    })
}

/// Check if an expression tree contains break or continue (for IIFE avoidance)
pub fn contains_loop_control(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Break | IrExprKind::Continue => true,
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| match &s.kind {
                IrStmtKind::Expr { expr } => contains_loop_control(expr),
                IrStmtKind::Bind { value, .. } => contains_loop_control(value),
                IrStmtKind::Assign { value, .. } => contains_loop_control(value),
                _ => false,
            }) || expr.as_ref().map_or(false, |e| contains_loop_control(e))
        }
        IrExprKind::If { cond, then, else_ } =>
            contains_loop_control(cond) || contains_loop_control(then) || contains_loop_control(else_),
        _ => false,
    }
}

/// Check if a type tree contains any named TypeVars (non-? prefix).
/// Uses Ty::any_child_recursive for uniform traversal.
pub fn ty_has_named_typevar(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| matches!(t, Ty::TypeVar(n) if !n.starts_with('?')))
}

/// Replace named TypeVars with Ty::Unknown (rendered as _).
/// Uses Ty::map_children for uniform recursive traversal.
pub fn erase_named_typevars(ty: Ty) -> Ty {
    match &ty {
        Ty::TypeVar(n) if !n.starts_with('?') => Ty::Unknown,
        _ => ty.map_children(&|child| erase_named_typevars(child.clone())),
    }
}

/// Render a Fn type as Box<dyn Fn(...) -> T> (for nested impl Trait in Rust)
pub fn render_type_boxed_fn(ctx: &RenderContext, ty: &Ty) -> String {
    match ty {
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| super::types::render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            let ret_str = if matches!(ret.as_ref(), Ty::Fn { .. }) {
                render_type_boxed_fn(ctx, ret)
            } else {
                super::types::render_type(ctx, ret)
            };
            ctx.templates.render_with("type_fn_boxed", None, &[], &[("params", params_str.as_str()), ("return", ret_str.as_str())])
                .unwrap_or_else(|| ctx.templates.render_with("type_fn", None, &[], &[("params", params_str.as_str()), ("return", ret_str.as_str())])
                    .unwrap_or_else(|| "BoxFn".into()))
        }
        _ => super::types::render_type(ctx, ty),
    }
}

/// Render a Fn type as Rc<dyn Fn(...) -> T> for struct fields (cloneable, no impl Trait)
pub fn render_type_field_fn(ctx: &RenderContext, ty: &Ty) -> String {
    match ty {
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| super::types::render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            let ret_str = if matches!(ret.as_ref(), Ty::Fn { .. }) {
                render_type_field_fn(ctx, ret)
            } else {
                super::types::render_type(ctx, ret)
            };
            ctx.templates.render_with("type_fn_field", None, &[], &[("params", params_str.as_str()), ("return", ret_str.as_str())])
                .unwrap_or_else(|| format!("std::rc::Rc<dyn Fn({}) -> {}>", params_str, ret_str))
        }
        _ => super::types::render_type(ctx, ty),
    }
}
