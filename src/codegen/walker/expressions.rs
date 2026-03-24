//! Expression rendering: converts IrExpr nodes to target-specific code strings.

use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::types::render_type;
use super::statements::{render_stmt, render_match_arm};
use super::helpers::{template_or, terminate_stmt, contains_loop_control, ty_has_named_typevar, erase_named_typevars, ty_contains_name};

pub fn render_expr(ctx: &RenderContext, expr: &IrExpr) -> String {
    match &expr.kind {
        // ── Literals ──
        IrExprKind::LitInt { value } => {
            let value_s = value.to_string();
            ctx.templates.render_with("int_literal", None, &[], &[("value", value_s.as_str())])
                .unwrap_or_else(|| value.to_string())
        }
        IrExprKind::LitFloat { value } => {
            let value_s = format!("{}", value);
            ctx.templates.render_with("float_literal", None, &[], &[("value", value_s.as_str())])
                .unwrap_or_else(|| format!("{}", value))
        }
        IrExprKind::LitStr { value } => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r");
            ctx.templates.render_with("string_literal", None, &[], &[("value", escaped.as_str())])
                .unwrap_or_else(|| format!("\"{}\"", value))
        }
        IrExprKind::LitBool { value } => {
            let key = if *value { "bool_literal_true" } else { "bool_literal_false" };
            template_or(ctx, key, &[], &value.to_string())
        }
        IrExprKind::Unit => template_or(ctx, "unit_literal", &[], "()"),

        // ── Variables ──
        IrExprKind::Var { id } => {
            let name = ctx.var_name(*id).to_string();
            // Lazy vars need deref via template
            if ctx.ann.lazy_vars.contains(id) {
                let upper = name.to_uppercase();
                ctx.templates.render_with("deref_lazy", None, &[], &[("name", upper.as_str())])
                    .unwrap_or_else(|| name.to_uppercase())
            } else {
                name
            }
            // Clone/Deref are now IR nodes (CloneInsertionPass / BoxDerefPass)
        }
        IrExprKind::FnRef { name } => name.clone(),

        // ── Operators ──
        IrExprKind::BinOp { op, left, right } => {
            render_binop(ctx, *op, left, right, &expr.ty)
        }
        IrExprKind::UnOp { op, operand } => {
            let inner = render_expr(ctx, operand);
            match op {
                UnOp::NegInt | UnOp::NegFloat => format!("(-{})", inner),
                UnOp::Not => format!("(!{})", inner),
            }
        }

        // ── Control flow ──
        IrExprKind::If { cond, then, else_ } => {
            // If branches contain break/continue, use statement form (not ternary/IIFE)
            if contains_loop_control(then) || contains_loop_control(else_) {
                let cond_str = render_expr(ctx, cond);
                let then_str = render_expr(ctx, then);
                let else_str = render_expr(ctx, else_);
                format!("if ({}) {{ {} }} else {{ {} }}", cond_str, then_str, else_str)
            } else {
                let cond_s = render_expr(ctx, cond);
                let then_s = render_expr(ctx, then);
                let else_s = render_expr(ctx, else_);
                ctx.templates.render_with("if_expr", None, &[], &[("cond", cond_s.as_str()), ("then", then_s.as_str()), ("else", else_s.as_str())])
                    .unwrap_or_else(|| format!("if {} {{ {} }} else {{ {} }}",
                        render_expr(ctx, cond), render_expr(ctx, then), render_expr(ctx, else_)))
            }
        }

        IrExprKind::Match { subject, arms } => {
            // Match subject transforms (.as_str(), .as_deref()) are handled by
            // MatchSubjectPass nanopass — walker just renders what's in the IR.
            let subj = render_expr(ctx, subject);
            let arms_str = arms.iter()
                .map(|arm| render_match_arm(ctx, arm))
                .collect::<Vec<_>>()
                .join("\n");
            let fallback = format!("match {{ {} }}", &arms_str);
            ctx.templates.render_with("match_expr", None, &[], &[("subject", subj.as_str()), ("arms", arms_str.as_str())])
                .unwrap_or(fallback)
        }

        IrExprKind::DoBlock { stmts, expr } => {
            // DoBlock with guard → loop { body }
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| {
                    let rendered = render_stmt(ctx, s);
                    terminate_stmt(ctx, rendered)
                })
                .collect();
            if let Some(e) = expr {
                let rendered = render_expr(ctx, e);
                // In a loop (DoBlock), non-Unit final expressions need `return` to exit
                let needs_return = !matches!(&e.ty, Ty::Unit)
                    && !matches!(&e.kind, IrExprKind::Unit | IrExprKind::Break | IrExprKind::Continue);
                if needs_return && !rendered.starts_with("break") && !rendered.starts_with("continue") {
                    parts.push(format!("return {}", rendered));
                } else {
                    parts.push(rendered);
                }
            }
            let body_s = parts.join("\n");
            ctx.templates.render_with("loop_block", None, &[], &[("body", body_s.as_str())])
                .unwrap_or_else(|| format!("loop {{ ... }}"))
        }

        IrExprKind::Block { stmts, expr } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| terminate_stmt(ctx, render_stmt(ctx, s)))
                .collect();
            if let Some(e) = expr {
                let expr_str = render_expr(ctx, e);
                // break/continue are statements — don't wrap in return
                let is_control_flow = matches!(&e.kind, IrExprKind::Break | IrExprKind::Continue);
                if is_control_flow {
                    parts.push(expr_str);
                } else {
                    parts.push(ctx.templates.render_with("block_result_expr", None, &[], &[("expr", expr_str.as_str())])
                        .unwrap_or_else(|| expr_str.clone()));
                }
            }
            let body = parts.join("\n");
            // If block contains break/continue, don't wrap in IIFE — use bare block
            let has_control = stmts.iter().any(|s| match &s.kind {
                IrStmtKind::Expr { expr } => contains_loop_control(expr),
                IrStmtKind::Bind { value, .. } => contains_loop_control(value),
                _ => false,
            }) || expr.as_ref().map_or(false, |e| contains_loop_control(e));
            if has_control {
                format!("{{\n{}\n}}", body)
            } else {
                ctx.templates.render_with("block_expr", None, &[], &[("body", body.as_str())])
                    .unwrap_or_else(|| format!("{{\n{}\n}}", body))
            }
        }

        // ── Loops ──
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            let var_name = if let Some(tuple_vars) = var_tuple {
                let names: Vec<String> = tuple_vars.iter().map(|id| ctx.var_name(*id).to_string()).collect();
                let vars_s = names.join(", ");
                ctx.templates.render_with("for_tuple_destructure", None, &[], &[("vars", vars_s.as_str())])
                    .unwrap_or_else(|| format!("({})", names.join(", ")))
            } else {
                ctx.var_name(*var).to_string()
            };
            let iter = render_expr(ctx, iterable);
            let body_str = body.iter().map(|s| render_stmt(ctx, s)).collect::<Vec<_>>().join("\n");
            ctx.templates.render_with("for_loop", None, &[], &[("var", var_name.as_str()), ("iter", iter.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("for _ in _ {{ }}"))
        }

        IrExprKind::While { cond, body } => {
            let cond_str = render_expr(ctx, cond);
            let body_str = body.iter().map(|s| render_stmt(ctx, s)).collect::<Vec<_>>().join("\n");
            ctx.templates.render_with("while_loop", None, &[], &[("cond", cond_str.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("while _ {{ }}"))
        }

        IrExprKind::Break => template_or(ctx, "break_stmt", &[], "break"),
        IrExprKind::Continue => template_or(ctx, "continue_stmt", &[], "continue"),

        // ── Codegen pre-rendered call ──
        IrExprKind::RenderedCall { code } => code.clone(),

        // ── Calls ──
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Module { module, func } => {
                    // Module calls: use template (TS/JS) or runtime function (Rust)
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    ctx.templates.render_with("module_call", None, &[], &[("module", module.as_str()), ("func", func.as_str()), ("args", args_str.as_str())])
                        .unwrap_or_else(|| format!("almide_rt_{}_{}({})", module, func, args_str))
                }
                _ => render_generic_call(ctx, target, args)
            }
        }

        // ── Collections ──
        IrExprKind::List { elements } => {
            // Empty list: use typed template (Rust needs Vec::<T>::new(), TS uses [])
            if elements.is_empty() {
                let inner_ty = match &expr.ty {
                    Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
                        let inner = &args[0];
                        let ty = if ty_has_named_typevar(inner) {
                            erase_named_typevars(inner.clone())
                        } else {
                            inner.clone()
                        };
                        render_type(ctx, &ty)
                    }
                    _ => "_".into(),
                };
                if let Some(rendered) = ctx.templates.render_with("empty_list", None, &[], &[("inner_type", inner_ty.as_str())]) {
                    return rendered;
                }
            }
            let elems = elements.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("list_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| format!("[...]"))
        }

        IrExprKind::Record { name, fields } => {
            // Build field strings (explicit + defaults for missing)
            let ctor_name_str = name.as_ref().map(|s| s.as_str()).unwrap_or("");
            let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
            let mut field_strs: Vec<String> = Vec::new();
            // Render explicit fields
            for (k, v) in fields.iter() {
                let mut val_str = render_expr(ctx, v);
                // Box recursive fields (annotation is target-aware — empty for non-Rust)
                if let Some(cn) = name {
                    if ctx.ann.boxed_fields.contains(&(cn.clone(), k.clone())) {
                        val_str = format!("Box::new({})", val_str);
                    }
                }
                field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", k.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("{}: {}", k, val_str)));
            }
            // Fill in default fields that were not explicitly provided
            let default_keys: Vec<(String, String)> = ctx.ann.default_fields.keys()
                .filter(|(cn, _)| cn == ctor_name_str)
                .cloned()
                .collect();
            for (_, field_name) in &default_keys {
                if explicit_names.contains(field_name.as_str()) { continue; }
                let Some(default_expr) = ctx.ann.default_fields.get(&(ctor_name_str.to_string(), field_name.clone())) else { continue; };
                let mut val_str = render_expr(ctx, default_expr);
                let needs_box = name.as_ref()
                    .map_or(false, |cn| ctx.ann.boxed_fields.contains(&(cn.clone(), field_name.clone())));
                if needs_box { val_str = format!("Box::new({})", val_str); }
                field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", field_name.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("{}: {}", field_name, val_str)));
            }
            let fields_str = field_strs.join(", ");
            // Resolve type name: explicit name, or from expr.ty
            // For record literals, use bare struct name (no generics — Rust infers them)
            let mut type_name = name.clone().unwrap_or_else(|| {
                match &expr.ty {
                    Ty::Named(n, _) => n.to_string(),
                    Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                        let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
                        names.sort();
                        if let Some(n) = ctx.ann.named_records.get(&names) {
                            n.clone()
                        } else if let Some(n) = ctx.ann.anon_records.get(&names) {
                            n.clone() // bare name, no generics
                        } else {
                            names.join("_")
                        }
                    }
                    _ => render_type(ctx, &expr.ty),
                }
            });
            // Qualify enum variant constructors via template
            if let Some(enum_name) = ctx.ann.ctor_to_enum.get(&type_name) {
                // Try ctor_record template first (TS: function call), fallback to record_literal
                if let Some(rendered) = ctx.templates.render_with("ctor_record", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str()), ("fields", fields_str.as_str())]) {
                    return rendered;
                }
                type_name = ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str()), ("fields", fields_str.as_str())])
                    .unwrap_or_else(|| format!("{}::{}", enum_name, type_name));
            }
            let fallback = format!("{{ {} }}", &fields_str);
            ctx.templates.render_with("record_literal", None, &[], &[("type_name", type_name.as_str()), ("fields", fields_str.as_str())])
                .unwrap_or(fallback)
        }

        // ── Access ──
        IrExprKind::Member { object, field } => {
            let expr_s = render_expr(ctx, object);
            ctx.templates.render_with("field_access", None, &[], &[("expr", expr_s.as_str()), ("field", field.as_str())])
                .unwrap_or_else(|| format!("{}.{}", render_expr(ctx, object), field))
        }

        // ── Option / Result ──
        IrExprKind::OptionSome { expr: inner } => {
            let inner_s = render_expr(ctx, inner);
            ctx.templates.render_with("some_expr", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Some({})", render_expr(ctx, inner)))
        }
        IrExprKind::OptionNone => {
            // Typed None: pass inner type via bindings + attribute for template guard
            if let Ty::Applied(TypeConstructorId::Option, args) = &expr.ty {
                if args.len() == 1 && !matches!(&args[0], Ty::Unknown | Ty::TypeVar(_)) {
                    let type_hint_s = render_type(ctx, &args[0]);
                    return ctx.templates.render_with("none_expr", None, &["none_type_hint"], &[("type_hint", type_hint_s.as_str())])
                        .unwrap_or_else(|| "None".into());
                }
            }
            template_or(ctx, "none_expr", &[], "None")
        }
        IrExprKind::ResultOk { expr: inner } => {
            let inner_s = render_expr(ctx, inner);
            ctx.templates.render_with("ok_expr", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Ok({})", render_expr(ctx, inner)))
        }
        IrExprKind::ResultErr { expr: inner } => {
            let inner_str = render_expr(ctx, inner);
            let construct = if matches!(&inner.ty, Ty::String) { "err_inner_string" } else { "err_inner_other" };
            ctx.templates.render_with(construct, None, &[], &[("inner", inner_str.as_str())])
                .or_else(|| ctx.templates.render_with("err_expr", None, &[], &[("inner", inner_str.as_str())]))
                .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
        }

        // ── Lambda ──
        IrExprKind::Lambda { params, body, .. } => {
            let params_str = params.iter()
                .map(|(id, _ty)| ctx.var_name(*id).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut body_str = render_expr(ctx, body);
            // Nested lambda: wrap in Box for languages that need it (template returns identity in TS)
            if matches!(&body.kind, IrExprKind::Lambda { .. }) {
                body_str = ctx.templates.render_with("box_wrap", None, &[], &[("inner", body_str.as_str())])
                    .unwrap_or(body_str);
            }
            ctx.templates.render_with("lambda_single", None, &[], &[("params", params_str.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("|_| {{ }}"))
        }

        // ── String interpolation ──
        IrExprKind::StringInterp { parts } => {
            // Collect format string and args separately
            let mut fmt_parts = Vec::new();
            let mut arg_parts = Vec::new();
            for part in parts {
                match part {
                    IrStringPart::Lit { value } => {
                        // Escape special chars for format!-style templates
                        fmt_parts.push(value
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('{', "{{")
                            .replace('}', "}}"));
                    }
                    IrStringPart::Expr { expr } => {
                        fmt_parts.push("{}".to_string());
                        arg_parts.push(render_expr(ctx, expr));
                    }
                }
            }
            let format_str_s = fmt_parts.join("");
            let args_s = arg_parts.join(", ");
            let template_str_s = {
                // For TS-style template literals: `${expr}`
                let mut s = String::new();
                for part in parts {
                    match part {
                        IrStringPart::Lit { value } => s.push_str(value),
                        IrStringPart::Expr { expr } => {
                            s.push_str("${");
                            s.push_str(&render_expr(ctx, expr));
                            s.push('}');
                        }
                    }
                }
                s
            };
            ctx.templates.render_with("string_interp", None, &[], &[("format_str", format_str_s.as_str()), ("args", args_s.as_str()), ("template_str", template_str_s.as_str())])
                .unwrap_or_else(|| format!("\"...\""))
        }

        // ── Range ──
        IrExprKind::Range { start, end, inclusive } => {
            let s = render_expr(ctx, start);
            let e = render_expr(ctx, end);
            let construct = if *inclusive { "range_inclusive" } else { "range_expr" };
            ctx.templates.render_with(construct, None, &[], &[("start", s.as_str()), ("end", e.as_str())])
                .unwrap_or_else(|| "range(...)".into())
        }

        // ── Tuple ──
        IrExprKind::Tuple { elements } => {
            let parts = elements.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", parts.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrExprKind::TupleIndex { object, index } => {
            let object_s = render_expr(ctx, object);
            let index_s = format!("{}", index);
            ctx.templates.render_with("tuple_index", None, &[], &[("object", object_s.as_str()), ("index", index_s.as_str())])
                .unwrap_or_else(|| format!("{}.{}", render_expr(ctx, object), index))
        }
        IrExprKind::IndexAccess { object, index } => {
            let obj_str = render_expr(ctx, object);
            let idx = render_expr(ctx, index);
            ctx.templates.render_with("index_access", None, &[], &[("object", obj_str.as_str()), ("index", idx.as_str())])
                .unwrap_or_else(|| "idx[...]".into())
        }
        IrExprKind::MapAccess { object, key } => {
            let obj_str = render_expr(ctx, object);
            let key_str = render_expr(ctx, key);
            ctx.templates.render_with("map_get", None, &[], &[("object", obj_str.as_str()), ("key", key_str.as_str())])
                .unwrap_or_else(|| "map_get(...)".into())
        }

        // ── Map ──
        IrExprKind::MapLiteral { entries } => {
            let entry_template = ctx.templates.render_with("map_entry", None, &[], &[])
                .unwrap_or_else(|| "({key}, {value})".into());
            let parts: Vec<String> = entries.iter()
                .map(|(k, v)| {
                    entry_template.replace("{key}", &render_expr(ctx, k))
                        .replace("{value}", &render_expr(ctx, v))
                })
                .collect();
            let entries_s = parts.join(", ");
            ctx.templates.render_with("map_literal", None, &[], &[("entries", entries_s.as_str())])
                .unwrap_or_else(|| format!("map([{}])", parts.join(", ")))
        }
        IrExprKind::EmptyMap => {
            template_or(ctx, "empty_map", &[], "HashMap::new()")
        }

        // ── SpreadRecord ──
        IrExprKind::SpreadRecord { base, fields } => {
            let base_str = render_expr(ctx, base);
            let fields_str = fields.iter()
                .map(|(k, v)| format!("{}: {}", k, render_expr(ctx, v)))
                .collect::<Vec<_>>()
                .join(", ");
            let type_name = match &expr.ty {
                Ty::Named(n, _) => n.to_string(),
                Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                    let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
                    names.sort();
                    ctx.ann.named_records.get(&names).cloned()
                        .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                        .unwrap_or_else(|| names.join("_"))
                }
                _ => render_type(ctx, &expr.ty),
            };
            ctx.templates.render_with("spread_record", None, &[], &[("type_name", type_name.as_str()), ("fields", fields_str.as_str()), ("base", base_str.as_str())])
                .unwrap_or_else(|| "{ ...spread }".into())
        }

        // ── Try / Await ──
        IrExprKind::Try { expr: inner } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("try_expr", None, &[], &[("inner", s.as_str())])
                .unwrap_or_else(|| "try(...)".into())
        }
        IrExprKind::Await { expr: inner } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("await_expr", None, &[], &[("inner", s.as_str())])
                .unwrap_or_else(|| "await(...)".into())
        }

        // ── Codegen nodes (inserted by passes — walker just renders) ──
        IrExprKind::Clone { expr: inner } => {
            let expr_s = render_expr(ctx, inner);
            ctx.templates.render_with("clone_expr", None, &[], &[("expr", expr_s.as_str())])
                .unwrap_or_else(|| format!("{}.clone()", expr_s))
        }
        IrExprKind::Deref { expr: inner } => {
            let name_s = render_expr(ctx, inner);
            ctx.templates.render_with("deref_var", None, &[], &[("name", name_s.as_str())])
                .unwrap_or_else(|| format!("(*{})", name_s))
        }
        IrExprKind::Borrow { expr: inner, as_str } => {
            if *as_str {
                format!("&*{}", render_expr(ctx, inner))
            } else {
                format!("&{}", render_expr(ctx, inner))
            }
        }
        IrExprKind::BoxNew { expr: inner } => {
            format!("Box::new({})", render_expr(ctx, inner))
        }
        IrExprKind::RustMacro { name, args } => {
            // Render macro args — LitStr rendered as bare &str (no .to_string())
            let args_str = args.iter().map(|a| {
                match &a.kind {
                    IrExprKind::LitStr { value } => {
                        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                        format!("\"{}\"", escaped)
                    }
                    _ => render_expr(ctx, a),
                }
            }).collect::<Vec<_>>().join(", ");
            format!("{}!({})", name, args_str)
        }
        IrExprKind::ToVec { expr: inner } => {
            format!("({}).to_vec()", render_expr(ctx, inner))
        }

        // ── Hole / Todo ──
        IrExprKind::Hole => template_or(ctx, "hole", &[], "todo!()"),
        IrExprKind::Todo { message } => {
            template_or(ctx, "todo", &[], &format!("todo!(\"{}\")", message))
        }

        // ── Fan (concurrency) — fully template-driven ──
        IrExprKind::Fan { exprs } => {
            let rendered: Vec<String> = exprs.iter().map(|e| {
                let mut body = render_expr(ctx, e);
                // Strip trailing ? from body (fan closures return raw Result)
                if e.ty.is_result() && body.ends_with('?') {
                    body.pop();
                }
                body
            }).collect();
            let exprs_s = rendered.join(", ");
            let count_s = format!("{}", exprs.len());
            // Build spawn/join parts for thread-based template
            let handles: Vec<String> = (0..exprs.len()).map(|i| format!("__fan_h{}", i)).collect();
            let spawns: Vec<String> = rendered.iter().enumerate()
                .map(|(i, body)| format!("let {} = __s.spawn(move || {{ {} }});", handles[i], body))
                .collect();
            let any_result = exprs.iter().any(|e| e.ty.is_result());
            let joins: Vec<String> = exprs.iter().enumerate().map(|(i, e)| {
                if e.ty.is_result() {
                    if ctx.auto_unwrap {
                        format!("{}.join().unwrap()?", handles[i])
                    } else {
                        format!("{}.join().unwrap().unwrap()", handles[i])
                    }
                } else {
                    format!("{}.join().unwrap()", handles[i])
                }
            }).collect();
            let join_expr = if joins.len() == 1 { joins[0].clone() }
                else { format!("({})", joins.join(", ")) };
            let spawns_s = spawns.join(" ");
            // Select template variant
            let construct = if any_result && ctx.auto_unwrap { "fan_effect" } else { "fan_expr" };
            ctx.templates.render_with(construct, None, &[], &[("exprs", exprs_s.as_str()), ("count", count_s.as_str()), ("spawns", spawns_s.as_str()), ("join_expr", join_expr.as_str())])
                .unwrap_or_else(|| format!("fan({})", rendered.join(", ")))
        }

        // ── Fallback ──
        // _ => format!("/* TODO: unhandled IR node */"),
    }
}

// ── Binary operator rendering ──

fn render_binop(ctx: &RenderContext, op: BinOp, left: &IrExpr, right: &IrExpr, _ty: &Ty) -> String {
    let l = render_expr(ctx, left);
    let r = render_expr(ctx, right);

    // Type-dispatched operators
    match op {
        BinOp::ConcatStr | BinOp::ConcatList => {
            let ty_tag = if op == BinOp::ConcatStr { "String" } else { "List" };
            ctx.templates.render_with("concat_expr", Some(ty_tag), &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("concat(_, _)"))
        }
        BinOp::Eq => {
            ctx.templates.render_with("eq_expr", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("_ == _"))
        }
        BinOp::Neq => {
            ctx.templates.render_with("ne_expr", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("_ != _"))
        }
        BinOp::PowInt => {
            ctx.templates.render_with("power_expr", Some("Int"), &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("pow(_, _)"))
        }
        BinOp::PowFloat => {
            ctx.templates.render_with("power_expr", Some("Float"), &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("pow(_, _)"))
        }
        _ => {
            let op_str = match op {
                BinOp::AddInt | BinOp::AddFloat => "+",
                BinOp::SubInt | BinOp::SubFloat => "-",
                BinOp::MulInt | BinOp::MulFloat => "*",
                BinOp::DivInt | BinOp::DivFloat => "/",
                BinOp::ModInt | BinOp::ModFloat => "%",
                BinOp::XorInt => "^",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Lte => "<=",
                BinOp::Gte => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                _ => "??",
            };
            let op_s = op_str.to_string();
            ctx.templates.render_with("binary_op", None, &[], &[("left", l.as_str()), ("op", op_s.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("({} {} {})", "l", op_str, "r"))
        }
    }
}

/// Render a generic call expression (Named, Method, or Computed target).
fn render_generic_call(ctx: &RenderContext, target: &CallTarget, args: &[IrExpr]) -> String {
    let callee = match target {
        CallTarget::Named { name } => {
            if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name.as_str()) {
                return render_enum_constructor(ctx, name, enum_name, args);
            }
            // Convention methods: "Type.method" → "Type_method" (free functions in all targets)
            if name.contains('.') {
                name.replace('.', "_")
            } else {
                name.clone()
            }
        }
        CallTarget::Method { object, method } => {
            if let Some(full) = render_method_call_full(ctx, object, method, args) {
                return full;
            }
            format!("{}.{}", render_expr(ctx, object), method)
        }
        CallTarget::Computed { callee } => {
            let s = render_expr(ctx, callee);
            if matches!(&callee.kind, IrExprKind::Lambda { .. }) { format!("({})", s) } else { s }
        }
        CallTarget::Module { .. } => unreachable!(),
    };
    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
    ctx.templates.render_with("call_expr", None, &[], &[("callee", callee.as_str()), ("args", args_str.as_str())])
        .unwrap_or_else(|| format!("call(...)"))
}

/// Render a method call as a full expression for UFCS and module.func patterns.
/// Returns Some(full_expr) if the method call was handled, None for normal obj.method calls.
fn render_method_call_full(ctx: &RenderContext, object: &IrExpr, method: &str, args: &[IrExpr]) -> Option<String> {
    let is_rust_intrinsic = matches!(method,
        "clone" | "is_some" | "is_none" | "unwrap" | "unwrap_or"
        | "to_string" | "len" | "push" | "pop" | "insert" | "remove"
        | "contains" | "iter" | "into_iter" | "collect" | "map"
        | "filter" | "to_vec" | "join" | "split" | "trim"
        | "starts_with" | "ends_with" | "replace" | "chars"
        | "as_str" | "get" | "keys" | "values" | "abs" | "powi"
        | "is_empty" | "contains_key" | "entry" | "or_insert"
        | "expect" | "ok" | "err" | "and_then" | "map_err"
        | "unwrap_or_else" | "ok_or" | "flatten" | "as_ref"
    );
    // User-defined UFCS: plain method name (no dots) → func(object, args)
    if !method.contains('.') && !is_rust_intrinsic {
        let obj_str = render_expr(ctx, object);
        let mut all_args = vec![obj_str];
        all_args.extend(args.iter().map(|a| render_expr(ctx, a)));
        return Some(format!("{}({})", method, all_args.join(", ")));
    }
    // Module.func or Convention method UFCS
    if let Some(dot_pos) = method.find('.') {
        let obj_str = render_expr(ctx, object);
        let mut all_args = vec![obj_str];
        all_args.extend(args.iter().map(|a| render_expr(ctx, a)));
        let module = &method[..dot_pos];
        let func = &method[dot_pos+1..];
        // Convention methods (Type.method): first char uppercase → emit as Type_method(args)
        let is_convention = module.chars().next().map_or(false, |c| c.is_uppercase());
        if is_convention {
            let callee = format!("{}_{}", module, func);
            return Some(format!("{}({})", callee, all_args.join(", ")));
        }
        return Some(ctx.templates.render_with("module_call", None, &[], &[("module", module), ("func", func), ("args", all_args.join(", ").as_str())])
            .unwrap_or_else(|| format!("{}.{}()", module, func)));
    }
    None
}

/// Render an enum constructor call with optional Box wrapping for recursive types.
fn render_enum_constructor(ctx: &RenderContext, ctor_name: &str, enum_name: &str, args: &[IrExpr]) -> String {
    let boxed_args: Vec<String> = args.iter().map(|a| {
        let rendered = render_expr(ctx, a);
        if ctx.ann.recursive_enums.contains(enum_name) && ty_contains_name(&a.ty, enum_name) {
            format!("Box::new({})", rendered)
        } else {
            rendered
        }
    }).collect();
    let args_str = boxed_args.join(", ");
    if args.is_empty() {
        ctx.templates.render_with("ctor_unit", None, &[], &[("enum_name", enum_name), ("ctor_name", ctor_name), ("args", args_str.as_str())])
            .unwrap_or_else(|| format!("{}::{}", enum_name, ctor_name))
    } else {
        ctx.templates.render_with("ctor_call", None, &[], &[("enum_name", enum_name), ("ctor_name", ctor_name), ("args", args_str.as_str())])
            .unwrap_or_else(|| format!("{}::{}({})", enum_name, ctor_name, args_str))
    }
}
