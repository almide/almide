//! Statement and pattern rendering: converts IrStmt and IrPattern nodes
//! to target-specific code strings.

use crate::ir::*;
use crate::types::Ty;
use super::RenderContext;
use super::types::render_type;
use super::expressions::render_expr;
use super::helpers::{template_or, terminate_stmt, ty_has_named_typevar, erase_named_typevars};

pub fn render_stmt(ctx: &RenderContext, stmt: &IrStmt) -> String {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, mutability } => {
            let name_s = ctx.var_name(*var).to_string();
            // Erase Fn types in bindings (Rust can't write `impl Fn` in let position; TS gets `any`)
            let ty = if matches!(ty, Ty::Fn { .. }) { &Ty::Unknown } else { ty };
            // Erase named TypeVars (K, V, B) — not in scope for bindings
            let ty_owned;
            let ty = if ty_has_named_typevar(ty) {
                ty_owned = erase_named_typevars(ty.clone());
                &ty_owned
            } else {
                ty
            };
            let type_s = render_type(ctx, ty);
            let value_s = render_expr(ctx, value);
            let construct = match mutability {
                Mutability::Let => "let_binding",
                Mutability::Var => "var_binding",
            };
            ctx.templates.render_with(construct, None, &[], &[("name", name_s.as_str()), ("type", type_s.as_str()), ("value", value_s.as_str())])
                .unwrap_or_else(|| format!("let _ = _;"))
        }
        IrStmtKind::Assign { var, value } => {
            let target_s = ctx.var_name(*var).to_string();
            let value_s = render_expr(ctx, value);
            ctx.templates.render_with("assignment", None, &[], &[("target", target_s.as_str()), ("value", value_s.as_str())])
                .unwrap_or_else(|| format!("_ = _;"))
        }
        IrStmtKind::Expr { expr } => {
            let rendered = render_expr(ctx, expr);
            terminate_stmt(ctx, rendered)
        }
        IrStmtKind::Guard { cond, else_ } => {
            let cond_str = render_expr(ctx, cond);
            let else_str = render_expr(ctx, else_);
            // Determine action: break for loop guards, return for function guards
            let is_loop_control = matches!(&else_.kind, IrExprKind::Unit | IrExprKind::Break | IrExprKind::Continue)
                || (matches!(&else_.kind, IrExprKind::ResultOk { .. }) && {
                    if let IrExprKind::ResultOk { expr: inner } = &else_.kind {
                        matches!(&inner.kind, IrExprKind::Unit)
                    } else { false }
                });
            let action = if is_loop_control {
                if matches!(&else_.kind, IrExprKind::Continue) { "continue" } else { "break" }
            } else { "return" };
            let neg = ctx.templates.render_with("guard_negate", None, &[], &[("cond", cond_str.as_str())])
                .unwrap_or_else(|| format!("!cond"));
            if action == "break" || action == "continue" {
                format!("if {} {{ {} }}", neg, action)
            } else {
                format!("if {} {{ return {} }}", neg, else_str)
            }
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            let target_str = ctx.var_name(*target).to_string();
            let idx_str = render_expr(ctx, index);
            let val_str = render_expr(ctx, value);
            ctx.templates.render_with("index_assign", None, &[], &[("target", target_str.as_str()), ("index", idx_str.as_str()), ("value", val_str.as_str())])
                .unwrap_or_else(|| "idx[...] = ...;".into())
        }
        IrStmtKind::MapInsert { target, key, value } => {
            let target_str = ctx.var_name(*target).to_string();
            let key_str = render_expr(ctx, key);
            let val_str = render_expr(ctx, value);
            ctx.templates.render_with("map_insert", None, &[], &[("target", target_str.as_str()), ("key", key_str.as_str()), ("value", val_str.as_str())])
                .unwrap_or_else(|| "map_set(...)".into())
        }
        IrStmtKind::FieldAssign { target, field, value } => {
            let target_str = ctx.var_name(*target).to_string();
            let val_str = render_expr(ctx, value);
            format!("{}.{} = {};", target_str, field, val_str)
        }
        IrStmtKind::BindDestructure { pattern, value } => {
            // For record patterns with empty name, resolve from value type
            let pat_str = match pattern {
                IrPattern::RecordPattern { name, fields, rest } if name.is_empty() => {
                    let type_name = match &value.ty {
                        Ty::Named(n, _) => n.to_string(),
                        Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                            let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
                            names.sort();
                            ctx.ann.named_records.get(&names).cloned()
                                .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                                .unwrap_or_else(|| names.join("_"))
                        }
                        _ => "_".into(),
                    };
                    let qualified = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(&type_name) {
                        ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str())])
                            .unwrap_or_else(|| format!("{}::{}", enum_name, type_name))
                    } else {
                        type_name
                    };
                    let fields_str = fields.iter()
                        .map(|f| match &f.pattern {
                            Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
                            None => f.name.clone(),
                        })
                        .collect::<Vec<_>>().join(", ");
                    if *rest {
                        let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                        ctx.templates.render_with(construct, None, &[], &[("name", qualified.as_str()), ("fields", fields_str.as_str())])
                            .unwrap_or_else(|| format!("{} {{ {} }}", qualified, fields_str))
                    } else {
                        ctx.templates.render_with("destructure_pattern", None, &[], &[("name", qualified.as_str()), ("fields", fields_str.as_str())])
                            .unwrap_or_else(|| format!("{} {{ {} }}", qualified, fields_str))
                    }
                }
                _ => render_pattern(ctx, pattern),
            };
            let val_str = render_expr(ctx, value);
            ctx.templates.render_with("bind_destructure", None, &[], &[("pattern", pat_str.as_str()), ("value", val_str.as_str())])
                .unwrap_or_else(|| format!("let _ = _;"))
        }
        IrStmtKind::Comment { text } => format!("// {}", text),
    }
}

// ── Match arm rendering ──

pub fn render_match_arm(ctx: &RenderContext, arm: &IrMatchArm) -> String {
    let pattern = render_pattern(ctx, &arm.pattern);
    let body = render_expr(ctx, &arm.body);
    // Append guard to pattern if present
    let full_pattern = if let Some(ref guard) = arm.guard {
        let guard_str = render_expr(ctx, guard);
        format!("{} if {}", pattern, guard_str)
    } else {
        pattern
    };
    ctx.templates.render_with("match_arm_inline", None, &[], &[("pattern", full_pattern.as_str()), ("body", body.as_str())])
        .unwrap_or_else(|| format!("_ => _,"))
}

pub fn render_pattern(ctx: &RenderContext, pat: &IrPattern) -> String {
    match pat {
        IrPattern::Wildcard => template_or(ctx, "pattern_wildcard", &[], "_"),
        IrPattern::Bind { var, .. } => ctx.var_name(*var).to_string(),
        IrPattern::Literal { expr } => {
            // In patterns, literals must be bare (no .to_string(), no i64 suffix for match)
            match &expr.kind {
                IrExprKind::LitStr { value } => {
                    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("\"{}\"", escaped)
                }
                IrExprKind::LitInt { value } => format!("{}", value),
                IrExprKind::LitFloat { value } => format!("{}", value),
                IrExprKind::LitBool { value } => format!("{}", value),
                _ => render_expr(ctx, expr),
            }
        }
        IrPattern::Some { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_some", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Some(_)"))
        }
        IrPattern::None => template_or(ctx, "pattern_none", &[], "None"),
        IrPattern::Ok { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_ok", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Ok(_)"))
        }
        IrPattern::Err { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_err", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Err(_)"))
        }
        IrPattern::Constructor { name, args } => {
            let qualified = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name) {
                ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", name.as_str())])
                    .unwrap_or_else(|| format!("{}::{}", enum_name, name))
            } else {
                name.clone()
            };
            if args.is_empty() {
                qualified
            } else {
                let args_str = args.iter().map(|a| render_pattern(ctx, a)).collect::<Vec<_>>().join(", ");
                format!("{}({})", qualified, args_str)
            }
        }
        IrPattern::Tuple { elements } => {
            let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrPattern::RecordPattern { name, fields, rest } => {
            // Qualify enum variant record patterns: Circle → Shape::Circle
            let qualified_name = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name) {
                format!("{}::{}", enum_name, name)
            } else {
                name.clone()
            };
            let fields_str = fields.iter()
                .map(|f| match &f.pattern {
                    Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
                    None => f.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            if *rest {
                let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                ctx.templates.render_with(construct, None, &[], &[("name", qualified_name.as_str()), ("fields", fields_str.as_str())])
                    .unwrap_or_else(|| format!("{} {{ {} }}", qualified_name, fields_str))
            } else {
                format!("{} {{ {} }}", qualified_name, fields_str)
            }
        }
    }
}
