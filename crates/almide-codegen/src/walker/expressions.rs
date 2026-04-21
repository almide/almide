//! Expression rendering: converts IrExpr nodes to target-specific code strings.

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::super::pass::Target;
use super::types::render_type;
use super::statements::{render_stmt, render_match_arm};
use super::helpers::{template_or, terminate_stmt, indent_lines, render_body_content, contains_loop_control, ty_has_named_typevar, erase_named_typevars, ty_contains_name};

/// Render a statement list. Peephole patterns are detected at IR level
/// by PeepholePass; this just renders the resulting IR nodes.
fn render_stmts(ctx: &RenderContext, stmts: &[IrStmt]) -> Vec<String> {
    stmts.iter().map(|s| render_stmt(ctx, s)).collect()
}

pub fn render_expr(ctx: &RenderContext, expr: &IrExpr) -> String {
    match &expr.kind {
        // ── Literals ──
        IrExprKind::LitInt { value } => {
            let value_s = value.to_string();
            // Pick the Rust literal suffix from `expr.ty` so sized
            // numeric types (Stage 1a/1b) emit the right width:
            // `Ty::Int32` → `i32`, `Ty::UInt8` → `u8`, and the
            // canonical `Ty::Int` keeps the legacy `i64`. Falls
            // through to the `int_literal` template for backward
            // compatibility when ty is Int / Unknown.
            match &expr.ty {
                Ty::Int8 => format!("{}i8", value_s),
                Ty::Int16 => format!("{}i16", value_s),
                Ty::Int32 => format!("{}i32", value_s),
                Ty::UInt8 => format!("{}u8", value_s),
                Ty::UInt16 => format!("{}u16", value_s),
                Ty::UInt32 => format!("{}u32", value_s),
                Ty::UInt64 => format!("{}u64", value_s),
                _ => ctx.templates.render_with("int_literal", None, &[], &[("value", value_s.as_str())])
                    .unwrap_or_else(|| value.to_string()),
            }
        }
        IrExprKind::LitFloat { value } => {
            let value_s = format!("{}", value);
            if matches!(expr.ty, Ty::Float32) {
                format!("{}f32", value_s)
            } else {
                ctx.templates.render_with("float_literal", None, &[], &[("value", value_s.as_str())])
                    .unwrap_or_else(|| format!("{}", value))
            }
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
            // Lazy vars need deref via template. Cross-module top_let synthetic
            // vars carry an `ALMIDE_RT_<MOD>_<NAME>` name and reference a static
            // LazyLock — auto-deref them too, BUT only if the target top_let's
            // kind is Lazy. Scalar `Const` top_lets (plain `const NAME: i64 = 42;`)
            // must NOT be dereferenced. The synthetic Var carries a fresh
            // VarId so `lazy_vars` misses it; cross-reference by uppercased
            // name against `lazy_top_let_names` instead.
            let upper = name.to_uppercase();
            let is_synthetic_lazy = name.starts_with("ALMIDE_RT_")
                && ctx.ann.lazy_top_let_names.contains(&upper);
            if ctx.ann.lazy_vars.contains(id) || is_synthetic_lazy {
                ctx.templates.render_with("deref_lazy", None, &[], &[("name", upper.as_str())])
                    .unwrap_or_else(|| upper.clone())
            } else if name.starts_with("ALMIDE_RT_") {
                upper
            } else {
                name
            }
            // Clone/Deref are now IR nodes (CloneInsertionPass / BoxDerefPass)
        }
        IrExprKind::FnRef { name } => name.to_string(),

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
            let cond_s = render_expr(ctx, cond);
            let then_content = render_body_content(ctx, then);
            let else_content = render_body_content(ctx, else_);

            // Coerce bare Var branches to `String` when the If's type is
            // String. Function params of type String are emitted as `&str`
            // in Rust, so `if ... { "lit".to_string() } else { param }` would
            // mix `String` and `&str`. `.to_string()` unifies both.
            let (then_content, else_content) = if matches!(ctx.target, super::super::pass::Target::Rust)
                && matches!(expr.ty, Ty::String) {
                (
                    coerce_to_owned_string(&then_content, then),
                    coerce_to_owned_string(&else_content, else_),
                )
            } else {
                (then_content, else_content)
            };

            if then_content.contains('\n') || else_content.contains('\n') {
                // Multi-line: indent branch bodies
                let indented_then = indent_lines(&then_content, 4);
                let indented_else = indent_lines(&else_content, 4);
                format!("if {} {{\n{}\n}} else {{\n{}\n}}", cond_s, indented_then, indented_else)
            } else {
                ctx.templates.render_with("if_expr", None, &[], &[("cond", cond_s.as_str()), ("then", then_content.as_str()), ("else", else_content.as_str())])
                    .unwrap_or_else(|| format!("if {} {{ {} }} else {{ {} }}", cond_s, then_content, else_content))
            }
        }

        IrExprKind::Match { subject, arms } => {
            // Match subject transforms (.as_str(), .as_deref()) are handled by
            // MatchSubjectPass nanopass — walker just renders what's in the IR.
            let subj = render_expr(ctx, subject);
            let arms_raw = arms.iter()
                .map(|arm| render_match_arm(ctx, arm))
                .collect::<Vec<_>>()
                .join("\n");
            let arms_str = indent_lines(&arms_raw, 4);
            let fallback = format!("match {} {{\n{}\n}}", &subj, &arms_str);
            ctx.templates.render_with("match_expr", None, &[], &[("subject", subj.as_str()), ("arms", arms_str.as_str())])
                .unwrap_or(fallback)
        }

        IrExprKind::Block { stmts, expr } => {
            let mut parts: Vec<String> = render_stmts(ctx, stmts).into_iter()
                .map(|s| terminate_stmt(ctx, s))
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
            let indented_body = indent_lines(&body, 4);
            // If block contains break/continue, don't wrap in IIFE — use bare block
            let has_control = stmts.iter().any(|s| match &s.kind {
                IrStmtKind::Expr { expr } => contains_loop_control(expr),
                IrStmtKind::Bind { value, .. } => contains_loop_control(value),
                _ => false,
            }) || expr.as_ref().map_or(false, |e| contains_loop_control(e));
            if has_control {
                format!("{{\n{}\n}}", indented_body)
            } else {
                ctx.templates.render_with("block_expr", None, &[], &[("body", indented_body.as_str())])
                    .unwrap_or_else(|| format!("{{\n{}\n}}", indented_body))
            }
        }

        // ── Loops ──
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            // Optimize: for loop over empty list literal → skip entirely
            if let IrExprKind::List { elements } = &iterable.kind {
                if elements.is_empty() {
                    return "{}".to_string();
                }
            }
            let var_name = if let Some(tuple_vars) = var_tuple {
                let names: Vec<String> = tuple_vars.iter().map(|id| ctx.var_name(*id).to_string()).collect();
                let vars_s = names.join(", ");
                ctx.templates.render_with("for_tuple_destructure", None, &[], &[("vars", vars_s.as_str())])
                    .unwrap_or_else(|| format!("({})", names.join(", ")))
            } else {
                ctx.var_name(*var).to_string()
            };
            // Rust's `for i in 0..n` consumes a Range directly; the default
            // `range_expr` template wraps ranges in `.collect::<Vec<_>>()`
            // so they're usable as Vec values elsewhere, but that allocates
            // a Vec every time a plain range appears as a ForIn iterable.
            // A 2 M-weight inner loop inside a 16 k outer loop was paying
            // ~16 MB/tensor of throwaway Vec allocations. Render Ranges
            // that appear as a loop iterable with the bare `start..end`
            // form to skip the alloc.
            let iter = match &iterable.kind {
                IrExprKind::Range { start, end, inclusive } => {
                    let s = render_expr(ctx, start);
                    let e = render_expr(ctx, end);
                    let op = if *inclusive { "..=" } else { ".." };
                    format!("{}{}{}", s, op, e)
                }
                _ => render_expr(ctx, iterable),
            };
            let body_raw = render_stmts(ctx, body).join("\n");
            let body_str = indent_lines(&body_raw, 4);
            ctx.templates.render_with("for_loop", None, &[], &[("var", var_name.as_str()), ("iter", iter.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("for _ in _ {{ }}"))
        }

        IrExprKind::While { cond, body } => {
            let cond_str = render_expr(ctx, cond);
            let body_raw = render_stmts(ctx, body).join("\n");
            let body_str = indent_lines(&body_raw, 4);
            ctx.templates.render_with("while_loop", None, &[], &[("cond", cond_str.as_str()), ("body", body_str.as_str())])
                .unwrap_or_else(|| format!("while _ {{ }}"))
        }

        IrExprKind::Break => template_or(ctx, "break_stmt", &[], "break"),
        IrExprKind::Continue => template_or(ctx, "continue_stmt", &[], "continue"),

        // ── Codegen pre-rendered call ──
        IrExprKind::RenderedCall { code } => code.clone(),

        // ── @inline_rust template dispatch (Stdlib Unification Stage 1) ──
        // Produced by `pass_stdlib_lowering` for calls to bundled stdlib
        // fns whose IrFunction carries an `@inline_rust("...")` attribute.
        // Render each arg into the param-keyed placeholder.
        IrExprKind::InlineRust { template, args } => {
            let mut out = template.clone();
            for (name, arg) in args {
                let rendered = render_expr(ctx, arg);
                let placeholder = format!("{{{}}}", name.as_str());
                out = out.replace(&placeholder, &rendered);
            }
            out
        }

        // ── Pre-resolved runtime call (from @intrinsic) ──
        IrExprKind::RuntimeCall { symbol, args } => {
            // BorrowInsertion wraps args with Borrow / Clone IR nodes
            // based on the `@intrinsic` fn's derived signature
            // (`intrinsic_borrow_mode`). The walker just renders.
            let args_str = args.iter().map(|a| render_expr(ctx, a))
                .collect::<Vec<_>>().join(", ");
            format!("{}({})", symbol.as_str(), args_str)
        }

        // ── Calls ──
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            match target {
                CallTarget::Module { module, func } => {
                    // Module calls: use template (TS/JS) or runtime function (Rust)
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    let mod_ident = module.replace('.', "_");
                    let func_ident = func.replace('.', "_");
                    ctx.templates.render_with("module_call", None, &[], &[("module", mod_ident.as_str()), ("func", func_ident.as_str()), ("args", args_str.as_str())])
                        .unwrap_or_else(|| {
                            format!("almide_rt_{}_{}({})", mod_ident, func_ident, args_str)
                        })
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
            let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(k, _)| &**k).collect();
            let mut field_strs: Vec<String> = Vec::new();
            // Render explicit fields
            for (k, v) in fields.iter() {
                let mut val_str = render_expr(ctx, v);
                // Box recursive fields (annotation is target-aware — empty for non-Rust)
                if let Some(cn) = name {
                    if ctx.ann.boxed_fields.contains(&(cn.to_string(), k.to_string())) {
                        val_str = format!("Box::new({})", val_str);
                    }
                }
                // Rc-wrap Fn-typed fields: closures stored in struct fields use Rc<dyn Fn>
                if matches!(&v.ty, Ty::Fn { .. }) {
                    val_str = format!("std::rc::Rc::new({})", val_str);
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
                    .map_or(false, |cn| ctx.ann.boxed_fields.contains(&(cn.to_string(), field_name.clone())));
                if needs_box { val_str = format!("Box::new({})", val_str); }
                field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", field_name.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("{}: {}", field_name, val_str)));
            }
            let fields_str = field_strs.join(", ");
            // Resolve type name: explicit name, or from expr.ty
            // For record literals, use bare struct name (no generics — Rust infers them)
            let mut type_name = name.map(|n| n.to_string()).unwrap_or_else(|| {
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
                if args.len() == 1 && !args[0].is_unresolved() {
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
                        // Escape special chars for format!-style templates.
                        // Include control chars so they don't land as real
                        // source newlines in the format string — otherwise
                        // rustfmt continuation indent leaks into runtime
                        // output. Bug seen with "foo\n" + "${var}" chains.
                        fmt_parts.push(value
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\n")
                            .replace('\t', "\\t")
                            .replace('\r', "\\r")
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

        // ── Try / Await / Unwrap / ToOption ──
        IrExprKind::Try { expr: inner } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("try_expr", None, &[], &[("inner", s.as_str())])
                .unwrap_or_else(|| "try(...)".into())
        }
        IrExprKind::Unwrap { expr: inner } => {
            // Short-circuit: ok(x)! = x, some(x)! = x — the unwrap is a no-op.
            if matches!(&inner.kind, IrExprKind::ResultOk { .. } | IrExprKind::OptionSome { .. }) {
                let inner_expr = match &inner.kind {
                    IrExprKind::ResultOk { expr } | IrExprKind::OptionSome { expr } => expr,
                    _ => unreachable!(),
                };
                return render_expr(ctx, inner_expr);
            }
            let s = render_expr(ctx, inner);
            // In test functions, ? cannot be used (return type is ()).
            // Use .unwrap() instead.
            if ctx.is_test {
                format!("({}).unwrap()", s)
            } else {
                // Determine the right template variant based on inner type.
                // For Result with non-String error, use map_err variants
                // (the template decides whether/how to coerce — target-agnostic).
                let when_type = if inner.ty.is_option() { Some("Option") } else { None };
                let err_coerce_attr = if !inner.ty.is_option() {
                    if let Some((_, err_ty)) = inner.ty.inner2() {
                        if matches!(err_ty, Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 && matches!(args[0], Ty::String)) {
                            Some("map_err_join")
                        } else if !matches!(err_ty, Ty::String) {
                            Some("map_err_debug")
                        } else {
                            None
                        }
                    } else { None }
                } else { None };
                let attrs: Vec<&str> = err_coerce_attr.into_iter().collect();
                ctx.templates.render_with("unwrap_expr", when_type, &attrs, &[("inner", s.as_str())])
                    .unwrap_or_else(|| format!("({})?", s))
            }
        }
        IrExprKind::UnwrapOr { expr: inner, fallback } => {
            let s = render_expr(ctx, inner);
            let f = render_expr(ctx, fallback);
            // Rc wrapping for List[Fn] fallback is now handled by RustLoweringPass
            // which inserts RcWrap nodes into the IR.
            let when_type = if inner.ty.is_option() { Some("Option") } else { None };
            // When inner.ty is Unknown, defaults to Result template.
            // This is correct if type inference produced Unknown due to a bug;
            // the Rust compiler will catch any mismatch.
            ctx.templates.render_with("unwrap_or_expr", when_type, &[], &[("inner", s.as_str()), ("fallback", f.as_str())])
                .unwrap_or_else(|| format!("{}.unwrap_or({})", s, f))
        }
        IrExprKind::ToOption { expr: inner } => {
            if inner.ty.is_option() {
                render_expr(ctx, inner)
            } else {
                let s = render_expr(ctx, inner);
                ctx.templates.render_with("to_option_expr", None, &[], &[("inner", s.as_str())])
                    .unwrap_or_else(|| format!("({}).ok()", s))
            }
        }
        IrExprKind::OptionalChain { expr: inner, field } => {
            let s = render_expr(ctx, inner);
            ctx.templates.render_with("optional_chain_expr", None, &[], &[("inner", s.as_str()), ("field", field)])
                .unwrap_or_else(|| format!("{}.as_ref().map(|__v| __v.{}.clone())", s, field))
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
        IrExprKind::Borrow { expr: inner, as_str, mutable } => {
            // If the borrowed operand is a Var referencing a fn param
            // already emitted as a reference (`&T`, `&[T]`, `&str`),
            // skip the outer `&` to avoid `&&T` double-borrow. The
            // `&*` (deref-then-ref) decoration still renders because
            // it rewraps via `Deref`.
            if !*as_str && !*mutable {
                if let IrExprKind::Var { id } = &inner.kind {
                    if ctx.ref_params.contains(id) {
                        return render_expr(ctx, inner);
                    }
                }
            }
            // Same idea for `&mut b` against a `b: &mut T` param:
            // Rust auto-reborrows when you pass the naked var, so
            // dropping the outer `&mut` here keeps the callee's
            // `&mut T` slot filled without a `&mut &mut T` layer.
            if *mutable {
                if let IrExprKind::Var { id } = &inner.kind {
                    if ctx.ref_mut_params.contains(id) {
                        return render_expr(ctx, inner);
                    }
                }
            }
            if *mutable {
                format!("&mut {}", render_expr(ctx, inner))
            } else if *as_str {
                format!("&*{}", render_expr(ctx, inner))
            } else {
                format!("&{}", render_expr(ctx, inner))
            }
        }
        IrExprKind::BoxNew { expr: inner } => {
            format!("Box::new({})", render_expr(ctx, inner))
        }
        IrExprKind::RcWrap { expr: inner, cast_ty } => {
            let s = render_expr(ctx, inner);
            if let Some(ty) = cast_ty {
                let rc_type = super::helpers::render_type_rc_fn(ctx, ty);
                format!("std::rc::Rc::new({}) as {}", s, rc_type)
            } else {
                format!("std::rc::Rc::new({})", s)
            }
        }
        IrExprKind::RustMacro { name, args } => {
            // Render macro args — LitStr rendered as bare &str (no .to_string()).
            // Control chars must be escaped here too, otherwise they land as
            // real source newlines and the Rust source formatter's continuation
            // indent leaks into the string literal at runtime (same failure
            // mode as StringInterp's Lit parts).
            let args_str = args.iter().map(|a| {
                match &a.kind {
                    IrExprKind::LitStr { value } => {
                        let escaped = value
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\n")
                            .replace('\t', "\\t")
                            .replace('\r', "\\r");
                        format!("\"{}\"", escaped)
                    }
                    _ => render_expr(ctx, a),
                }
            }).collect::<Vec<_>>().join(", ");
            format!("{}!({})", name, args_str)
        }
        IrExprKind::ToVec { expr: inner } => {
            if matches!(&inner.kind, IrExprKind::Range { .. }) {
                format!("({}).collect::<Vec<_>>()", render_expr(ctx, inner))
            } else {
                format!("({}).to_vec()", render_expr(ctx, inner))
            }
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

        // ── Iterator chain (Rust-only) ──
        IrExprKind::IterChain { source, consume, steps, collector } => {
            render_iter_chain(ctx, source, *consume, steps, collector)
        }

        // ── Closure conversion nodes (WASM-only, never reached by Rust walker) ──
        IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => {
            unreachable!("ClosureCreate/EnvLoad should only appear in WASM pipeline")
        }
    }
}

fn render_iter_chain(ctx: &RenderContext, source: &IrExpr, consume: bool, steps: &[IterStep], collector: &IterCollector) -> String {
    let src = render_expr(ctx, source);
    let mut chain = if consume {
        format!("({}).into_iter()", src)
    } else {
        format!("({}).iter()", src)
    };

    for step in steps {
        match step {
            IterStep::Map { lambda } => chain = format!("{}.map({})", chain, render_expr(ctx, lambda)),
            IterStep::Filter { lambda } => chain = format!("{}.filter({})", chain, render_expr(ctx, lambda)),
            IterStep::FlatMap { lambda } => chain = format!("{}.flat_map({})", chain, render_expr(ctx, lambda)),
            IterStep::FilterMap { lambda } => chain = format!("{}.filter_map({})", chain, render_expr(ctx, lambda)),
        }
    }

    match collector {
        IterCollector::Collect => format!("{}.collect::<Vec<_>>()", chain),
        IterCollector::Fold { init, lambda } => format!("{}.fold({}, {})", chain, render_expr(ctx, init), render_expr(ctx, lambda)),
        IterCollector::Any { lambda } => format!("{}.any({})", chain, render_expr(ctx, lambda)),
        IterCollector::All { lambda } => format!("{}.all({})", chain, render_expr(ctx, lambda)),
        IterCollector::Find { lambda } => format!("{}.find({})", chain, render_expr(ctx, lambda)),
        IterCollector::Count { lambda } => format!("{}.filter({}).count() as i64", chain, render_expr(ctx, lambda)),
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
        BinOp::MulMatrix => {
            ctx.templates.render_with("matrix_mul", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_rt_matrix_mul(&{}, &{})", l, r))
        }
        BinOp::AddMatrix => {
            ctx.templates.render_with("matrix_add", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_rt_matrix_add(&{}, &{})", l, r))
        }
        BinOp::SubMatrix => {
            ctx.templates.render_with("matrix_sub", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_rt_matrix_sub(&{}, &{})", l, r))
        }
        BinOp::ScaleMatrix => {
            // Ensure matrix is first arg, scalar is second
            let (mat, scalar) = if matches!(&left.ty, Ty::Matrix) {
                (l.as_str(), r.as_str())
            } else {
                (r.as_str(), l.as_str())
            };
            ctx.templates.render_with("matrix_scale", None, &[], &[("left", mat), ("right", scalar)])
                .unwrap_or_else(|| format!("almide_rt_matrix_scale(&{}, {})", mat, scalar))
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
            // Inline numeric casts: runtime function → Rust `as` cast
            match name.as_str() {
                "almide_rt_float_from_int" | "almide_rt_int_to_float" if args.len() == 1 => {
                    return format!("({} as f64)", render_expr(ctx, &args[0]));
                }
                "almide_rt_float_to_int" if args.len() == 1 => {
                    return format!("({} as i64)", render_expr(ctx, &args[0]));
                }
                _ => {}
            }
            if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name.as_str()) {
                return render_enum_constructor(ctx, name, enum_name, args);
            }
            // Convention methods: "Type.method" → "Type_method" (free functions in all targets)
            if name.contains('.') {
                name.replace('.', "_")
            } else {
                name.to_string()
            }
        }
        CallTarget::Method { object, method } => {
            if let Some(full) = render_method_call_full(ctx, object, method, args) {
                return full;
            }
            {
                let obj_str = render_expr(ctx, object);
                // Wrap in parens if the object expression needs it for method call precedence
                let needs_parens = matches!(&object.kind,
                    IrExprKind::UnOp { .. } | IrExprKind::BinOp { .. }
                    | IrExprKind::If { .. } | IrExprKind::Match { .. }
                ) || matches!(&object.kind, IrExprKind::LitFloat { value } if *value < 0.0)
                  || matches!(&object.kind, IrExprKind::LitInt { value } if *value < 0);
                if needs_parens {
                    format!("({}).{}", obj_str, method)
                } else {
                    format!("{}.{}", obj_str, method)
                }
            }
        }
        CallTarget::Computed { callee } => {
            // Pipe terminus case: `expr |> (lambda)` lowers to `(lambda)(expr)`.
            // The lambda is the computed callee. Here — and ONLY here — we
            // annotate the lambda's params so rustc can infer types.
            // (Lambda elsewhere, e.g. as arg to `.filter(...)`, must stay
            // unannotated because iterator adapters want `&T` not `T`.)
            if let IrExprKind::Lambda { params, body, .. } = &callee.kind {
                let params_str = params.iter()
                    .map(|(id, ty)| {
                        let name = ctx.var_name(*id).to_string();
                        if ty.has_unresolved_deep() {
                            name
                        } else {
                            let ty_str = super::types::render_type(ctx, ty);
                            format!("{}: {}", name, ty_str)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let body_str = render_expr(ctx, body);
                format!("(move |{}| {})", params_str, body_str)
            } else if matches!(&callee.kind, IrExprKind::Member { .. }) {
                // Member: (h.run)("hello") — required in Rust to call Fn-typed fields
                format!("({})", render_expr(ctx, callee))
            } else {
                render_expr(ctx, callee)
            }
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
        | "as_str" | "get" | "keys" | "values" | "abs" | "powi" | "powf"
        | "is_empty" | "contains_key" | "entry" | "or_insert"
        | "expect" | "ok" | "err" | "and_then" | "map_err"
        | "unwrap_or_else" | "ok_or" | "flatten" | "as_ref" | "as_deref"
        // math intrinsics (inlined by StdlibLowering)
        | "sqrt" | "floor" | "ceil" | "round" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "atan2" | "exp" | "ln" | "log2" | "log10"
        | "is_nan" | "is_infinite"
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
    let boxed_args: Vec<String> = args.iter().enumerate().map(|(i, a)| {
        let rendered = render_expr(ctx, a);
        let needs_box = ctx.ann.recursive_enums.contains(enum_name)
            && (ty_contains_name(&a.ty, enum_name)
                || ctx.ann.boxed_fields.contains(&(ctor_name.to_string(), format!("{}", i))));
        if needs_box {
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

/// Wrap a rendered branch with `.to_string()` when it likely evaluates to
/// `&str` at the Rust level but the surrounding context needs an owned
/// `String`. Used by the If walker to avoid type mismatches between a
/// literal branch (already `String` via `"...".to_string()`) and a bare
/// `Var` branch whose param was emitted as `&str`.
///
/// Applied only when:
///   - the expression is a bare `Var` of `Ty::String`, or
///   - the expression is a block whose tail is such a Var.
///
/// Rust's `.to_string()` is idempotent on owned `String` (it clones), so
/// even when the branch is already owned the result is correct, just with
/// one redundant allocation.
fn coerce_to_owned_string(rendered: &str, expr: &IrExpr) -> String {
    let is_bare_string_var = match &expr.kind {
        IrExprKind::Var { .. } => matches!(expr.ty, Ty::String),
        IrExprKind::Block { stmts, expr: tail } if stmts.is_empty() => {
            if let Some(t) = tail {
                matches!(t.kind, IrExprKind::Var { .. }) && matches!(t.ty, Ty::String)
            } else {
                false
            }
        }
        _ => false,
    };
    if is_bare_string_var {
        format!("{}.to_string()", rendered)
    } else {
        rendered.to_string()
    }
}
