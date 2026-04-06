//! Statement and pattern rendering: converts IrStmt and IrPattern nodes
//! to target-specific code strings.

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::types::render_type;
use super::expressions::render_expr;
use super::helpers::{template_or, terminate_stmt, ty_has_named_typevar, erase_named_typevars};

/// Check if an expression references a specific variable (any depth).
pub fn render_stmt(ctx: &RenderContext, stmt: &IrStmt) -> String {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, mutability } => {
            let name_s = ctx.var_name(*var).to_string();
            // List[Fn] Rc wrapping is now handled by RustLoweringPass
            // which inserts RcWrap nodes into the IR.
            // Erase Fn types in bindings (Rust can't write `impl Fn` in let position; TS gets `any`)
            // Also resolve aliases first — `type Handler = Fn(String) -> String` should erase too
            let resolved_owned;
            let ty = if matches!(ty, Ty::Fn { .. }) {
                &Ty::Unknown
            } else if let Ty::Named(name, args) = ty {
                if args.is_empty() {
                    if let Some(target) = ctx.type_aliases.get(name) {
                        if matches!(target, Ty::Fn { .. }) {
                            &Ty::Unknown
                        } else {
                            resolved_owned = target.clone();
                            &resolved_owned
                        }
                    } else {
                        ty
                    }
                } else {
                    ty
                }
            } else {
                ty
            };
            // Erase named TypeVars (K, V, B) — not in scope for bindings
            let ty_owned;
            let ty = if ty_has_named_typevar(ty) {
                ty_owned = erase_named_typevars(ty.clone());
                &ty_owned
            } else {
                ty
            };
            let type_s = render_type(ctx, ty);
            // When binding a lambda to a Fn-typed variable (e.g. type alias Handler = (String) -> String),
            // the let type is erased to `_` but the lambda params have no type annotations either,
            // causing Rust type inference failure. Render lambda params with explicit types in this case.
            let value_s = if matches!(ty, Ty::Unknown) {
                if let IrExprKind::Lambda { params, body, .. } = &value.kind {
                    let has_typed_params = params.iter().any(|(_, t)| !matches!(t, Ty::Unknown));
                    if has_typed_params {
                        let params_str = params.iter()
                            .map(|(id, pty)| {
                                let name = ctx.var_name(*id).to_string();
                                if matches!(pty, Ty::Unknown) {
                                    name
                                } else {
                                    let ty_str = super::types::render_type(ctx, pty);
                                    format!("{}: {}", name, ty_str)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let body_str = render_expr(ctx, body);
                        ctx.templates.render_with("lambda_single", None, &[], &[("params", params_str.as_str()), ("body", body_str.as_str())])
                            .unwrap_or_else(|| format!("move |{}| {}", params_str, body_str))
                    } else {
                        render_expr(ctx, value)
                    }
                } else {
                    render_expr(ctx, value)
                }
            } else {
                render_expr(ctx, value)
            };
            let construct = match mutability {
                Mutability::Let => "let_binding",
                Mutability::Var => "var_binding",
            };
            ctx.templates.render_with(construct, None, &[], &[("name", name_s.as_str()), ("type", type_s.as_str()), ("value", value_s.as_str())])
                .unwrap_or_else(|| format!("let _ = _;"))
        }
        IrStmtKind::Assign { var, value } => {
            // Push optimization (xs = xs + [v] → xs.push(v)) is now handled
            // by RustLoweringPass which rewrites to a Call(Method("push")).
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
            // Determine action: break for loop guards, return for function guards.
            // Check both the expression kind (for direct Unit/Break/Continue/ResultOk(Unit))
            // and the expression type (for LICM-hoisted vars whose kind is Var but type is Result[Unit,_]).
            let is_loop_control = matches!(&else_.kind, IrExprKind::Unit | IrExprKind::Break | IrExprKind::Continue)
                || (matches!(&else_.kind, IrExprKind::ResultOk { .. }) && {
                    if let IrExprKind::ResultOk { expr: inner } = &else_.kind {
                        matches!(&inner.kind, IrExprKind::Unit)
                    } else { false }
                })
                // Block wrapping Continue/Break: { continue } has ty=Unit but action=continue
                || (matches!(&else_.kind, IrExprKind::Block { .. }) && {
                    if let IrExprKind::Block { stmts, expr: None } = &else_.kind {
                        stmts.len() == 1 && matches!(&stmts[0].kind, IrStmtKind::Expr { expr } if matches!(&expr.kind, IrExprKind::Continue | IrExprKind::Break))
                    } else { false }
                })
                // LICM-hoisted ok(()) → Var with Result[Unit,_] type
                || (matches!(&else_.kind, IrExprKind::Var { .. }) &&
                    matches!(&else_.ty, Ty::Applied(TypeConstructorId::Result, args) if args.first().is_some_and(|t| matches!(t, Ty::Unit))));
            let has_continue = matches!(&else_.kind, IrExprKind::Continue)
                || matches!(&else_.kind, IrExprKind::Block { stmts, expr: None }
                    if stmts.len() == 1 && matches!(&stmts[0].kind, IrStmtKind::Expr { expr } if matches!(&expr.kind, IrExprKind::Continue)));
            let action = if is_loop_control {
                if has_continue { "continue" } else { "break" }
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
            // Borrow conflict (xs[f(xs)] = v) is now resolved by RustLoweringPass
            // which lifts the index expression to a let binding at the IR level.
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
                    // Determine the total field count of the value type so we
                    // can automatically insert `..` when the pattern only
                    // destructures a subset (otherwise Rust complains with
                    // E0027 "pattern does not mention field X").
                    let total_fields: Option<usize> = match &value.ty {
                        Ty::Named(n, _) => ctx.ann.record_field_counts.get(n.as_str()).copied(),
                        Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } =>
                            Some(ty_fields.len()),
                        _ => None,
                    };
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
                    let needs_rest = *rest
                        || total_fields.map_or(false, |n| fields.len() < n);
                    if needs_rest {
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
        IrStmtKind::ListSwap { target, a, b } => {
            let t = ctx.var_name(*target).to_string();
            let a_s = render_expr(ctx, a);
            let b_s = render_expr(ctx, b);
            ctx.templates.render_with("peep_swap", None, &[], &[("target", &t), ("a", &a_s), ("b", &b_s)])
                .unwrap_or_else(|| format!("{}.swap({}, {});", t, a_s, b_s))
        }
        IrStmtKind::ListReverse { target, end } => {
            let t = ctx.var_name(*target).to_string();
            let e = render_expr(ctx, end);
            ctx.templates.render_with("peep_reverse", None, &[], &[("target", &t), ("end", &e)])
                .unwrap_or_else(|| format!("{}[..={}].reverse();", t, e))
        }
        IrStmtKind::ListRotateLeft { target, end } => {
            let t = ctx.var_name(*target).to_string();
            let e = render_expr(ctx, end);
            ctx.templates.render_with("peep_rotate_left", None, &[], &[("target", &t), ("end", &e)])
                .unwrap_or_else(|| format!("{}[..={}].rotate_left(1);", t, e))
        }
        IrStmtKind::ListCopySlice { dst, src, len } => {
            let d = ctx.var_name(*dst).to_string();
            let s = ctx.var_name(*src).to_string();
            let n = render_expr(ctx, len);
            ctx.templates.render_with("peep_copy_slice", None, &[], &[("dst", &d), ("src", &s), ("n", &n)])
                .unwrap_or_else(|| format!("{}[..{}].copy_from_slice(&{}[..{}]);", d, n, s, n))
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

/// Check if any match arm uses a list pattern.
pub fn arms_have_list_pattern(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::List { .. }))
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
        IrPattern::List { elements } => {
            if elements.is_empty() {
                "[]".to_string()
            } else {
                let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
                format!("[{}]", elems)
            }
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
