//! Utility functions for the walker: template fallback, statement termination,
//! type inspection helpers, and loop control detection.

use almide_ir::*;
use almide_lang::types::Ty;
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

/// Does `ty` reference ANY type in `recursive` (a set of names that participate
/// in a recursion cycle)? Generalizes `ty_contains_name` from direct
/// self-recursion to MUTUAL recursion (`type A = A(B); type B = B(A)`), where a
/// field references a *different* cycle member rather than the enclosing type's
/// own name — without an indirection there, native rustc rejects the pair as
/// infinitely sized (E0072) (#656). Over-inclusion only adds a harmless extra Box.
pub fn ty_contains_any_recursive(ty: &Ty, recursive: &std::collections::HashSet<String>) -> bool {
    ty.any_child_recursive(&|t| match t {
        Ty::Named(n, _) => recursive.contains(n.as_str()),
        Ty::Variant { name: vn, .. } => recursive.contains(vn.as_str()),
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

/// Replace every Fn type (at any depth) with Ty::Unknown (rendered as `_`).
///
/// Rust forbids `impl Trait` in the type of a variable binding (E0562), so a
/// `let`/`var` whose type *contains* a closure type — e.g. a tuple element
/// `(impl Fn() -> () + Clone, i64)`, a map value `HashMap<String, impl Fn..>`,
/// or a record field — cannot be written literally. Erasing the Fn subtree to
/// `_` lets Rust infer the concrete (unnameable) closure type from the RHS,
/// while preserving the surrounding container shape (`(_, i64)`,
/// `HashMap<String, _>`). The closure value itself is emitted unchanged and is
/// `Clone` (captures are `Rc`/`Copy` clones), so later `g.clone()()` call sites
/// keep working.
///
/// A top-level Fn is already mapped to `Ty::Unknown` by the binding code before
/// this runs, so this only rewrites Fn types nested inside containers.
pub fn erase_fn_types(ty: Ty) -> Ty {
    match &ty {
        Ty::Fn { .. } => Ty::Unknown,
        _ => ty.map_children(&|child| erase_fn_types(child.clone())),
    }
}

/// Indent each non-empty line by the given number of spaces.
pub fn indent_lines(s: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    s.lines()
        .map(|line| if line.is_empty() { String::new() } else { format!("{}{}", prefix, line) })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render the body of an expression as flat lines (unwrapping Block if present).
/// Used by if/else branches and other contexts that provide their own `{ }` wrapping.
pub fn render_body_content(ctx: &RenderContext, expr: &almide_ir::IrExpr) -> String {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| terminate_stmt(ctx, super::statements::render_stmt(ctx, s)))
                .collect();
            if let Some(e) = tail {
                let expr_str = super::expressions::render_expr_owned(ctx, e);
                let is_control = matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue);
                if is_control {
                    parts.push(expr_str);
                } else {
                    parts.push(ctx.templates.render_with("block_result_expr", None, &[], &[("expr", expr_str.as_str())])
                        .unwrap_or_else(|| expr_str.clone()));
                }
            }
            parts.join("\n")
        }
        _ => super::expressions::render_expr_owned(ctx, expr),
    }
}

/// Render a Fn type as Rc<dyn Fn(...) -> T> (for Fn types inside collections — cloneable)
pub fn render_type_rc_fn(ctx: &RenderContext, ty: &Ty) -> String {
    // A boxed closure VALUE (RcWrap) uses the same all-`Rc<dyn Fn>` rendering as a
    // field: a nested returned closure must be `Rc<dyn Fn>` (Clone), not
    // `Box<dyn Fn>` — a closure-returning-closure result is cloned at call sites,
    // and `Box<dyn Fn>` is not `Clone` (E0599).
    render_type_field_fn(ctx, ty)
}

/// Render a `fan.*` thread-thunk's boxed trait-object type:
/// `Box<dyn Fn(params) -> ret + {bounds}>` (`bounds` = `"Send + Sync"`).
///
/// `Box<dyn Fn + Send + Sync>` *itself* implements `Fn + Send + Sync`, so a boxed
/// thunk slots into the runtime's `Vec<impl Fn() -> _ + Send + Sync>` parameter
/// with no signature change — while distinct CAPTURING closures, which cannot
/// share one `impl Fn` type (E0308), unify as one trait-object element type.
pub fn render_type_box_fn(ctx: &RenderContext, ty: &Ty, bounds: &str) -> String {
    match ty {
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| super::types::render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            let ret_str = super::types::render_type(ctx, ret);
            format!("std::boxed::Box<dyn Fn({}) -> {} + {}>", params_str, ret_str, bounds)
        }
        _ => super::types::render_type(ctx, ty),
    }
}

/// True if `ty` mentions a function type anywhere (directly or nested).
pub(super) fn ty_mentions_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(ty_mentions_fn),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| ty_mentions_fn(t)),
        Ty::Applied(_, args) | Ty::Named(_, args) => args.iter().any(ty_mentions_fn),
        _ => false,
    }
}

/// Render a type for a STORAGE position (struct/record field, variant payload),
/// rendering every `Fn` — directly or nested inside a `List`/`Map`/`Tuple`/
/// `Option`/`Result` — as `Rc<dyn Fn(...) -> T>` rather than `impl Fn` (which is
/// illegal in field types, E0562) or `Box<dyn Fn>` (not `Clone`). Fn-free
/// subtrees fall through to the normal `render_type`.
pub fn render_type_field_fn(ctx: &RenderContext, ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    // Fast path: no closure anywhere → render normally.
    if !ty_mentions_fn(ty) {
        return super::types::render_type(ctx, ty);
    }
    let rf = |t: &Ty| render_type_field_fn(ctx, t);
    let tmpl = |name: &str, kv: &[(&str, &str)], fb: String| {
        ctx.templates.render_with(name, None, &[], kv).unwrap_or(fb)
    };
    match ty {
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(&rf).collect::<Vec<_>>().join(", ");
            let ret_str = rf(ret);
            tmpl("type_fn_field", &[("params", params_str.as_str()), ("return", ret_str.as_str())],
                format!("std::rc::Rc<dyn Fn({}) -> {}>", params_str, ret_str))
        }
        Ty::Tuple(elems) => {
            let parts = elems.iter().map(&rf).collect::<Vec<_>>().join(", ");
            tmpl("type_tuple", &[("elements", parts.as_str())], format!("({})", parts))
        }
        Ty::Applied(TCI::List, args) if args.len() == 1 => {
            let inner = rf(&args[0]);
            tmpl("type_list", &[("inner", inner.as_str())], format!("Vec<{}>", inner))
        }
        Ty::Applied(TCI::Map, args) if args.len() == 2 => {
            let (k, v) = (rf(&args[0]), rf(&args[1]));
            tmpl("type_map", &[("key", k.as_str()), ("value", v.as_str())], format!("HashMap<{}, {}>", k, v))
        }
        Ty::Applied(TCI::Option, args) if args.len() == 1 => {
            let inner = rf(&args[0]);
            tmpl("type_option", &[("inner", inner.as_str())], format!("Option<{}>", inner))
        }
        Ty::Applied(TCI::Result, args) if args.len() == 2 => {
            let (ok, err) = (rf(&args[0]), rf(&args[1]));
            tmpl("type_result", &[("ok", ok.as_str()), ("err", err.as_str())], format!("Result<{}, {}>", ok, err))
        }
        // Set[Fn] is rejected at typecheck (E016); render conservatively if reached.
        // Other Fn-containing shapes (e.g. a user generic over a closure) are rare —
        // fall back to render_type, which at least keeps the container correct.
        _ => super::types::render_type(ctx, ty),
    }
}
