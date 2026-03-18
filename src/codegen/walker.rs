//! IR Walker: traverses typed IR and renders using templates.
//!
//! This is the shared engine for all targets. It walks each IrExprKind,
//! recursively renders sub-expressions, and uses TemplateSet for output.
//!
//! The walker does NOT make semantic decisions — those are handled by
//! Nanopass passes (Layer 2) which annotate the IR before the walker runs.

use std::collections::HashMap;
use crate::ir::*;
use crate::types::Ty;
use super::annotations::CodegenAnnotations;
use super::pass::Target;
use super::template::TemplateSet;

/// Render context: carries the variable table, target, and annotations.
/// The walker NEVER checks types — all codegen decisions come from annotations.
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    pub in_effect_fn: bool,
    pub ann: CodegenAnnotations,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, in_effect_fn: false, ann: CodegenAnnotations::default() }
    }

    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    pub fn with_annotations(mut self, ann: CodegenAnnotations) -> Self {
        self.ann = ann;
        self
    }



    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn var_name(&self, id: VarId) -> &str {
        &self.var_table.get(id).name
    }

    fn bindings(&self) -> HashMap<&'static str, String> {
        HashMap::new()
    }
}

// ── Type rendering ──

pub fn render_type(ctx: &RenderContext, ty: &Ty) -> String {
    match ty {
        Ty::Int => template_or(ctx, "type_int", &[], "i64"),
        Ty::Float => template_or(ctx, "type_float", &[], "f64"),
        Ty::String => template_or(ctx, "type_string", &[], "String"),
        Ty::Bool => template_or(ctx, "type_bool", &[], "bool"),
        Ty::Unit => template_or(ctx, "type_unit", &[], "()"),
        Ty::Option(inner) => {
            let mut b = HashMap::new();
            b.insert("inner", render_type(ctx, inner));
            ctx.templates.render("type_option", None, &[], &b)
                .unwrap_or_else(|| format!("Option<{}>", render_type(ctx, inner)))
        }
        Ty::Result(ok, err) => {
            let mut b = HashMap::new();
            b.insert("ok", render_type(ctx, ok));
            b.insert("err", render_type(ctx, err));
            ctx.templates.render("type_result", None, &[], &b)
                .unwrap_or_else(|| format!("Result<{}, {}>", render_type(ctx, ok), render_type(ctx, err)))
        }
        Ty::List(inner) => {
            let mut b = HashMap::new();
            b.insert("inner", render_type(ctx, inner));
            ctx.templates.render("type_list", None, &[], &b)
                .unwrap_or_else(|| format!("Vec<{}>", render_type(ctx, inner)))
        }
        Ty::Named(name, args) => {
            if args.is_empty() {
                name.clone()
            } else {
                let args_str = args.iter().map(|a| render_type(ctx, a)).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, args_str)
            }
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            // Check named records first (user-defined types)
            if let Some(n) = ctx.ann.named_records.get(&names) {
                return n.clone();
            }
            // Check anonymous records
            if let Some(n) = ctx.ann.anon_records.get(&names) {
                // Generic anonymous record: AlmdRec0<Type0, Type1, ...>
                let mut sorted_fields: Vec<_> = fields.iter().collect();
                sorted_fields.sort_by(|a, b| a.0.cmp(&b.0));
                let args: Vec<String> = sorted_fields.iter().map(|(_, t)| render_type(ctx, t)).collect();
                if args.is_empty() {
                    n.clone()
                } else {
                    format!("{}<{}>", n, args.join(", "))
                }
            } else {
                // Fallback: sorted field names
                names.join("_")
            }
        }
        Ty::Map(k, v) => {
            let mut b = HashMap::new();
            b.insert("key", render_type(ctx, k));
            b.insert("value", render_type(ctx, v));
            ctx.templates.render("type_map", None, &[], &b)
                .unwrap_or_else(|| format!("HashMap<{}, {}>", render_type(ctx, k), render_type(ctx, v)))
        }
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            // Nested Fn return may need boxing (Rust: Box<dyn Fn>; TS: identity)
            let ret_str = if matches!(ret.as_ref(), Ty::Fn { .. }) {
                render_type_boxed_fn(ctx, ret)
            } else {
                render_type(ctx, ret)
            };
            let mut b = HashMap::new();
            b.insert("params", params_str);
            b.insert("return", ret_str);
            ctx.templates.render("type_fn", None, &[], &b)
                .unwrap_or_else(|| format!("Fn({})", b.get("params").unwrap()))
        }
        Ty::Tuple(elems) => {
            let parts = elems.iter().map(|t| render_type(ctx, t)).collect::<Vec<_>>().join(", ");
            format!("({})", parts)
        }
        Ty::TypeVar(n) => {
            if n.starts_with('?') {
                template_or(ctx, "typevar_infer", &[], "_")
            } else {
                n.clone()
            }
        }
        Ty::Unknown | Ty::Union(_) => {
            template_or(ctx, "unknown_type", &[], "_")
        }
        Ty::Variant { name, .. } => name.clone(),
        // Fallback
        #[allow(unreachable_patterns)]
        _ => format!("{}", ty.display()),
    }
}

// ── Expression rendering ──

pub fn render_expr(ctx: &RenderContext, expr: &IrExpr) -> String {
    match &expr.kind {
        // ── Literals ──
        IrExprKind::LitInt { value } => {
            let mut b = HashMap::new();
            b.insert("value", value.to_string());
            ctx.templates.render("int_literal", None, &[], &b)
                .unwrap_or_else(|| value.to_string())
        }
        IrExprKind::LitFloat { value } => {
            let mut b = HashMap::new();
            b.insert("value", format!("{}", value));
            ctx.templates.render("float_literal", None, &[], &b)
                .unwrap_or_else(|| format!("{}", value))
        }
        IrExprKind::LitStr { value } => {
            let mut b = HashMap::new();
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r");
            b.insert("value", escaped);
            ctx.templates.render("string_literal", None, &[], &b)
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
            // Deref/clone via templates (Rust: (*name), .clone(); TS: identity)
            let base = if ctx.ann.lazy_vars.contains(id) {
                let mut b = HashMap::new();
                b.insert("name", name.to_uppercase());
                ctx.templates.render("deref_lazy", None, &[], &b)
                    .unwrap_or_else(|| name.to_uppercase())
            } else if ctx.ann.deref_vars.contains(id) {
                let mut b = HashMap::new();
                b.insert("name", name.clone());
                ctx.templates.render("deref_var", None, &[], &b)
                    .unwrap_or(name)
            } else {
                name
            };
            if ctx.ann.clone_vars.contains(id) {
                let mut b = HashMap::new();
                b.insert("expr", base.clone());
                ctx.templates.render("clone_expr", None, &[], &b)
                    .unwrap_or(base)
            } else {
                base
            }
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
            let mut b = HashMap::new();
            b.insert("cond", render_expr(ctx, cond));
            b.insert("then", render_expr(ctx, then));
            b.insert("else", render_expr(ctx, else_));
            ctx.templates.render("if_expr", None, &[], &b)
                .unwrap_or_else(|| format!("if {} {{ {} }} else {{ {} }}",
                    render_expr(ctx, cond), render_expr(ctx, then), render_expr(ctx, else_)))
        }

        IrExprKind::Match { subject, arms } => {
            let mut subj = render_expr(ctx, subject);
            // String subjects may need transformation for pattern matching (e.g., .as_str() in Rust)
            if matches!(&subject.ty, Ty::String) {
                let has_str_pat = arms.iter().any(|a| matches!(&a.pattern, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. })));
                if has_str_pat {
                    let mut b = HashMap::new();
                    b.insert("subject", subj.clone());
                    subj = ctx.templates.render("string_match_subject", None, &[], &b)
                        .unwrap_or(subj);
                }
            }
            let arms_str = arms.iter()
                .map(|arm| render_match_arm(ctx, arm))
                .collect::<Vec<_>>()
                .join("\n");
            let mut b = HashMap::new();
            b.insert("subject", subj);
            let fallback = format!("match {{ {} }}", &arms_str);
            b.insert("arms", arms_str);
            ctx.templates.render("match_expr", None, &[], &b)
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
                    && !matches!(&e.kind, IrExprKind::Unit | IrExprKind::Break);
                if needs_return && !rendered.starts_with("break") {
                    parts.push(format!("return {}", rendered));
                } else {
                    parts.push(rendered);
                }
            }
            let mut b = HashMap::new();
            b.insert("body", parts.join("\n"));
            ctx.templates.render("loop_block", None, &[], &b)
                .unwrap_or_else(|| format!("loop {{ ... }}"))
        }

        IrExprKind::Block { stmts, expr } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| terminate_stmt(ctx, render_stmt(ctx, s)))
                .collect();
            if let Some(e) = expr {
                let expr_str = render_expr(ctx, e);
                let mut b = HashMap::new();
                b.insert("expr", expr_str);
                // Template decides: Rust uses bare expr, TS uses "return expr"
                parts.push(ctx.templates.render("block_result_expr", None, &[], &b)
                    .unwrap_or_else(|| b.get("expr").unwrap().clone()));
            }
            let mut b = HashMap::new();
            b.insert("body", parts.join("\n"));
            ctx.templates.render("block_expr", None, &[], &b)
                .unwrap_or_else(|| format!("{{\n{}\n}}", parts.join("\n")))
        }

        // ── Loops ──
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            let var_name = if let Some(tuple_vars) = var_tuple {
                let names: Vec<String> = tuple_vars.iter().map(|id| ctx.var_name(*id).to_string()).collect();
                let mut b = HashMap::new();
                b.insert("vars", names.join(", "));
                ctx.templates.render("for_tuple_destructure", None, &[], &b)
                    .unwrap_or_else(|| format!("({})", names.join(", ")))
            } else {
                ctx.var_name(*var).to_string()
            };
            let iter = render_expr(ctx, iterable);
            let body_str = body.iter().map(|s| render_stmt(ctx, s)).collect::<Vec<_>>().join("\n");
            let mut b = HashMap::new();
            b.insert("var", var_name);
            b.insert("iter", iter);
            b.insert("body", body_str);
            ctx.templates.render("for_loop", None, &[], &b)
                .unwrap_or_else(|| format!("for _ in _ {{ }}"))
        }

        IrExprKind::While { cond, body } => {
            let cond_str = render_expr(ctx, cond);
            let body_str = body.iter().map(|s| render_stmt(ctx, s)).collect::<Vec<_>>().join("\n");
            let mut b = HashMap::new();
            b.insert("cond", cond_str);
            b.insert("body", body_str);
            ctx.templates.render("while_loop", None, &[], &b)
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
                    // Module calls: use template (TS/other) or should have been
                    // converted to RenderedCall by StdlibLoweringPass (Rust)
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    let mut b = HashMap::new();
                    b.insert("module", module.clone());
                    b.insert("func", func.clone());
                    b.insert("args", args_str);
                    ctx.templates.render("module_call", None, &[], &b)
                        .unwrap_or_else(|| format!("__almd_{}.{}()", module, func))
                }
                _ => {
                    let callee = match target {
                        CallTarget::Named { name } => {
                            // Qualify enum constructors (Red → Color::Red)
                            if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name.as_str()) {
                                // Box-wrap args for recursive enum constructors
                                let boxed_args: Vec<String> = args.iter().map(|a| {
                                    let rendered = render_expr(ctx, a);
                                    if ctx.ann.recursive_enums.contains(enum_name) && ty_contains_name(&a.ty, enum_name) {
                                        format!("Box::new({})", rendered)
                                    } else {
                                        rendered
                                    }
                                }).collect();
                                let args_str = boxed_args.join(", ");
                                let mut b = HashMap::new();
                                b.insert("enum_name", enum_name.clone());
                                b.insert("ctor_name", name.clone());
                                b.insert("args", args_str.clone());
                                if args.is_empty() {
                                    return ctx.templates.render("ctor_unit", None, &[], &b)
                                        .unwrap_or_else(|| format!("{}::{}", enum_name, name));
                                } else {
                                    return ctx.templates.render("ctor_call", None, &[], &b)
                                        .unwrap_or_else(|| format!("{}::{}({})", enum_name, name, args_str));
                                }
                            }
                            name.clone()
                        }
                        CallTarget::Method { object, method } => {
                            // User-defined UFCS: plain method name (no dots) that isn't
                            // a Rust intrinsic method → convert to func(object, args)
                            let is_rust_intrinsic = matches!(method.as_str(),
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
                            if !method.contains('.') && !is_rust_intrinsic {
                                // User-defined function: f(obj, args)
                                let obj_str = render_expr(ctx, object);
                                let mut all_args = vec![obj_str];
                                all_args.extend(args.iter().map(|a| render_expr(ctx, a)));
                                let all_args_str = all_args.join(", ");
                                return format!("{}({})", method, all_args_str);
                            }
                            format!("{}.{}", render_expr(ctx, object), method)
                        }
                        CallTarget::Computed { callee } => {
                            let s = render_expr(ctx, callee);
                            // Lambdas need parens when immediately invoked
                            if matches!(&callee.kind, IrExprKind::Lambda { .. }) {
                                format!("({})", s)
                            } else {
                                s
                            }
                        }
                        CallTarget::Module { .. } => unreachable!(),
                    };
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    let mut b = HashMap::new();
                    b.insert("callee", callee);
                    b.insert("args", args_str);
                    ctx.templates.render("call_expr", None, &[], &b)
                        .unwrap_or_else(|| format!("call(...)"))
                }
            }
        }

        // ── Collections ──
        IrExprKind::List { elements } => {
            // Empty list: use typed template (Rust needs Vec::<T>::new(), TS uses [])
            if elements.is_empty() {
                let inner_ty = match &expr.ty {
                    Ty::List(inner) => render_type(ctx, inner),
                    _ => "_".into(),
                };
                let mut b = HashMap::new();
                b.insert("inner_type", inner_ty);
                if let Some(rendered) = ctx.templates.render("empty_list", None, &[], &b) {
                    return rendered;
                }
            }
            let elems = elements.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            let mut b = HashMap::new();
            b.insert("elements", elems);
            ctx.templates.render("list_literal", None, &[], &b)
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
                let mut b = HashMap::new();
                b.insert("name", k.clone());
                b.insert("value", val_str.clone());
                field_strs.push(ctx.templates.render("record_field", None, &[], &b)
                    .unwrap_or_else(|| format!("{}: {}", k, val_str)));
            }
            // Fill in default fields that were not explicitly provided
            let default_keys: Vec<(String, String)> = ctx.ann.default_fields.keys()
                .filter(|(cn, _)| cn == ctor_name_str)
                .cloned()
                .collect();
            for (_, field_name) in &default_keys {
                if !explicit_names.contains(field_name.as_str()) {
                    if let Some(default_expr) = ctx.ann.default_fields.get(&(ctor_name_str.to_string(), field_name.clone())) {
                        let mut val_str = render_expr(ctx, default_expr);
                        if let Some(cn) = name {
                            if ctx.ann.boxed_fields.contains(&(cn.clone(), field_name.clone())) {
                                val_str = format!("Box::new({})", val_str);
                            }
                        }
                        let mut b = HashMap::new();
                        b.insert("name", field_name.clone());
                        b.insert("value", val_str.clone());
                        field_strs.push(ctx.templates.render("record_field", None, &[], &b)
                            .unwrap_or_else(|| format!("{}: {}", field_name, val_str)));
                    }
                }
            }
            let fields_str = field_strs.join(", ");
            // Resolve type name: explicit name, or from expr.ty
            // For record literals, use bare struct name (no generics — Rust infers them)
            let mut type_name = name.clone().unwrap_or_else(|| {
                match &expr.ty {
                    Ty::Named(n, _) => n.clone(),
                    Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                        let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.clone()).collect();
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
                let mut b = HashMap::new();
                b.insert("enum_name", enum_name.clone());
                b.insert("ctor_name", type_name.clone());
                type_name = ctx.templates.render("ctor_qualify", None, &[], &b)
                    .unwrap_or_else(|| format!("{}::{}", enum_name, type_name));
            }
            let mut b = HashMap::new();
            b.insert("type_name", type_name);
            let fallback = format!("{{ {} }}", &fields_str);
            b.insert("fields", fields_str);
            ctx.templates.render("record_literal", None, &[], &b)
                .unwrap_or(fallback)
        }

        // ── Access ──
        IrExprKind::Member { object, field } => {
            let mut b = HashMap::new();
            b.insert("expr", render_expr(ctx, object));
            b.insert("field", field.clone());
            ctx.templates.render("field_access", None, &[], &b)
                .unwrap_or_else(|| format!("{}.{}", render_expr(ctx, object), field))
        }

        // ── Option / Result ──
        IrExprKind::OptionSome { expr: inner } => {
            let mut b = HashMap::new();
            b.insert("inner", render_expr(ctx, inner));
            ctx.templates.render("some_expr", None, &[], &b)
                .unwrap_or_else(|| format!("Some({})", render_expr(ctx, inner)))
        }
        IrExprKind::OptionNone => {
            // Typed None: pass inner type via bindings + attribute for template guard
            if let Ty::Option(inner) = &expr.ty {
                if !matches!(inner.as_ref(), Ty::Unknown | Ty::TypeVar(_)) {
                    let mut b = HashMap::new();
                    b.insert("type_hint", render_type(ctx, inner));
                    return ctx.templates.render("none_expr", None, &["none_type_hint"], &b)
                        .unwrap_or_else(|| "None".into());
                }
            }
            template_or(ctx, "none_expr", &[], "None")
        }
        IrExprKind::ResultOk { expr: inner } => {
            let mut b = HashMap::new();
            b.insert("inner", render_expr(ctx, inner));
            ctx.templates.render("ok_expr", None, &[], &b)
                .unwrap_or_else(|| format!("Ok({})", render_expr(ctx, inner)))
        }
        IrExprKind::ResultErr { expr: inner } => {
            let inner_str = render_expr(ctx, inner);
            let mut b = HashMap::new();
            b.insert("inner", inner_str);
            let construct = if matches!(&inner.ty, Ty::String) { "err_inner_string" } else { "err_inner_other" };
            ctx.templates.render(construct, None, &[], &b)
                .or_else(|| ctx.templates.render("err_expr", None, &[], &b))
                .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
        }

        // ── Lambda ──
        IrExprKind::Lambda { params, body } => {
            let params_str = params.iter()
                .map(|(id, _ty)| ctx.var_name(*id).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut body_str = render_expr(ctx, body);
            // Nested lambda: wrap in Box for languages that need it (template returns identity in TS)
            if matches!(&body.kind, IrExprKind::Lambda { .. }) {
                let mut b = HashMap::new();
                b.insert("inner", body_str.clone());
                body_str = ctx.templates.render("box_wrap", None, &[], &b)
                    .unwrap_or(body_str);
            }
            let mut b = HashMap::new();
            b.insert("params", params_str);
            b.insert("body", body_str);
            ctx.templates.render("lambda_single", None, &[], &b)
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
                        // Escape { and } for format!-style templates (safe for all targets)
                        fmt_parts.push(value.replace('{', "{{").replace('}', "}}"));
                    }
                    IrStringPart::Expr { expr } => {
                        fmt_parts.push("{}".to_string());
                        arg_parts.push(render_expr(ctx, expr));
                    }
                }
            }
            let mut b = HashMap::new();
            b.insert("format_str", fmt_parts.join(""));
            b.insert("args", arg_parts.join(", "));
            b.insert("template_str", {
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
            });
            ctx.templates.render("string_interp", None, &[], &b)
                .unwrap_or_else(|| format!("\"...\""))
        }

        // ── Range ──
        IrExprKind::Range { start, end, inclusive } => {
            let s = render_expr(ctx, start);
            let e = render_expr(ctx, end);
            let mut b = HashMap::new();
            b.insert("start", s);
            b.insert("end", e);
            let construct = if *inclusive { "range_inclusive" } else { "range_expr" };
            ctx.templates.render(construct, None, &[], &b)
                .unwrap_or_else(|| "range(...)".into())
        }

        // ── Tuple ──
        IrExprKind::Tuple { elements } => {
            let parts = elements.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            format!("({})", parts)
        }
        IrExprKind::TupleIndex { object, index } => {
            format!("{}.{}", render_expr(ctx, object), index)
        }
        IrExprKind::IndexAccess { object, index } => {
            let obj_str = render_expr(ctx, object);
            let idx = render_expr(ctx, index);
            let mut b = HashMap::new();
            if matches!(&object.ty, Ty::Map(_, _)) {
                b.insert("object", obj_str);
                b.insert("key", idx);
                ctx.templates.render("map_get", None, &[], &b)
                    .unwrap_or_else(|| "map_get(...)".into())
            } else {
                b.insert("object", obj_str);
                b.insert("index", idx);
                ctx.templates.render("index_access", None, &[], &b)
                    .unwrap_or_else(|| "idx[...]".into())
            }
        }

        // ── Map ──
        IrExprKind::MapLiteral { entries } => {
            let entry_template = ctx.templates.render("map_entry", None, &[], &HashMap::new())
                .unwrap_or_else(|| "({key}, {value})".into());
            let parts: Vec<String> = entries.iter()
                .map(|(k, v)| {
                    entry_template.replace("{key}", &render_expr(ctx, k))
                        .replace("{value}", &render_expr(ctx, v))
                })
                .collect();
            let mut b = HashMap::new();
            b.insert("entries", parts.join(", "));
            ctx.templates.render("map_literal", None, &[], &b)
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
                Ty::Named(n, _) => n.clone(),
                Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                    let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.clone()).collect();
                    names.sort();
                    ctx.ann.named_records.get(&names).cloned()
                        .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                        .unwrap_or_else(|| names.join("_"))
                }
                _ => render_type(ctx, &expr.ty),
            };
            let mut b = HashMap::new();
            b.insert("type_name", type_name);
            b.insert("fields", fields_str);
            b.insert("base", base_str);
            ctx.templates.render("spread_record", None, &[], &b)
                .unwrap_or_else(|| "{ ...spread }".into())
        }

        // ── Try / Await ──
        IrExprKind::Try { expr: inner } => {
            let s = render_expr(ctx, inner);
            let mut b = HashMap::new();
            b.insert("inner", s);
            ctx.templates.render("try_expr", None, &[], &b)
                .unwrap_or_else(|| "try(...)".into())
        }
        IrExprKind::Await { expr: inner } => {
            let s = render_expr(ctx, inner);
            let mut b = HashMap::new();
            b.insert("inner", s);
            ctx.templates.render("await_expr", None, &[], &b)
                .unwrap_or_else(|| "await(...)".into())
        }

        // ── Codegen nodes (inserted by passes — walker just renders) ──
        IrExprKind::Clone { expr: inner } => {
            format!("{}.clone()", render_expr(ctx, inner))
        }
        IrExprKind::Deref { expr: inner } => {
            format!("(*{})", render_expr(ctx, inner))
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

        // ── Fan (concurrency) — target-specific runtime model ──
        IrExprKind::Fan { exprs } => {
            if ctx.target == Target::Rust {
                let n = exprs.len();
                let handles: Vec<String> = (0..n).map(|i| format!("__fan_h{}", i)).collect();

                // Spawn threads
                let spawns: Vec<String> = exprs.iter().enumerate().map(|(i, e)| {
                    let mut body = render_expr(ctx, e);
                    let is_result = matches!(&e.ty, Ty::Result(_, _));
                    // Strip trailing ? from body (fan closures return raw Result)
                    if is_result && body.ends_with('?') {
                        body.pop();
                    }
                    format!("let {} = __s.spawn(move || {{ {} }});", handles[i], body)
                }).collect();

                // Join threads
                let any_result = exprs.iter().any(|e| matches!(&e.ty, Ty::Result(_, _)));
                let joins: Vec<String> = exprs.iter().enumerate().map(|(i, e)| {
                    let is_result = matches!(&e.ty, Ty::Result(_, _));
                    if is_result {
                        // In effect fn: use ? for propagation. In test/pure fn: use .unwrap()
                        if ctx.in_effect_fn {
                            format!("{}.join().unwrap()?", handles[i])
                        } else {
                            format!("{}.join().unwrap().unwrap()", handles[i])
                        }
                    } else {
                        format!("{}.join().unwrap()", handles[i])
                    }
                }).collect();

                let join_expr = if n == 1 {
                    joins[0].clone()
                } else {
                    format!("({})", joins.join(", "))
                };

                if any_result && ctx.in_effect_fn {
                    format!("std::thread::scope(|__s| -> Result<_, String> {{ {} Ok({}) }})",
                        spawns.join(" "), join_expr)
                } else {
                    format!("std::thread::scope(|__s| {{ {} {} }})",
                        spawns.join(" "), join_expr)
                }
            } else {
                // TS: Promise.all
                let parts: Vec<String> = exprs.iter().map(|e| render_expr(ctx, e)).collect();
                format!("await Promise.all([{}])", parts.join(", "))
            }
        }

        // ── Fallback ──
        // _ => format!("/* TODO: unhandled IR node */"),
    }
}

// ── Binary operator rendering ──

fn render_binop(ctx: &RenderContext, op: BinOp, left: &IrExpr, right: &IrExpr, ty: &Ty) -> String {
    let l = render_expr(ctx, left);
    let r = render_expr(ctx, right);

    // Type-dispatched operators
    match op {
        BinOp::ConcatStr | BinOp::ConcatList => {
            let mut b = HashMap::new();
            b.insert("left", l);
            b.insert("right", r);
            let ty_tag = if op == BinOp::ConcatStr { "String" } else { "List" };
            ctx.templates.render("concat_expr", Some(ty_tag), &[], &b)
                .unwrap_or_else(|| format!("concat(_, _)"))
        }
        BinOp::Eq => {
            let mut b = HashMap::new();
            b.insert("left", l);
            b.insert("right", r);
            ctx.templates.render("eq_expr", None, &[], &b)
                .unwrap_or_else(|| format!("_ == _"))
        }
        BinOp::Neq => {
            let mut b = HashMap::new();
            b.insert("left", l);
            b.insert("right", r);
            ctx.templates.render("ne_expr", None, &[], &b)
                .unwrap_or_else(|| format!("_ != _"))
        }
        BinOp::PowFloat => {
            let mut b = HashMap::new();
            b.insert("left", l);
            b.insert("right", r);
            let ty_tag = match &left.ty {
                Ty::Int => "Int",
                _ => "Float",
            };
            ctx.templates.render("power_expr", Some(ty_tag), &[], &b)
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
            let mut b = HashMap::new();
            b.insert("left", l);
            b.insert("op", op_str.to_string());
            b.insert("right", r);
            ctx.templates.render("binary_op", None, &[], &b)
                .unwrap_or_else(|| format!("({} {} {})", "l", op_str, "r"))
        }
    }
}

// ── Match arm rendering ──

fn render_match_arm(ctx: &RenderContext, arm: &IrMatchArm) -> String {
    let pattern = render_pattern(ctx, &arm.pattern);
    let body = render_expr(ctx, &arm.body);
    let mut b = HashMap::new();
    // Append guard to pattern if present
    let full_pattern = if let Some(ref guard) = arm.guard {
        let guard_str = render_expr(ctx, guard);
        format!("{} if {}", pattern, guard_str)
    } else {
        pattern
    };
    b.insert("pattern", full_pattern);
    b.insert("body", body);
    ctx.templates.render("match_arm_inline", None, &[], &b)
        .unwrap_or_else(|| format!("_ => _,"))
}

fn render_pattern(ctx: &RenderContext, pat: &IrPattern) -> String {
    match pat {
        IrPattern::Wildcard => template_or(ctx, "pattern_wildcard", &[], "_"),
        IrPattern::Bind { var } => ctx.var_name(*var).to_string(),
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
            let mut b = HashMap::new();
            b.insert("binding", render_pattern(ctx, inner));
            ctx.templates.render("pattern_some", None, &[], &b)
                .unwrap_or_else(|| format!("Some(_)"))
        }
        IrPattern::None => template_or(ctx, "pattern_none", &[], "None"),
        IrPattern::Ok { inner } => {
            let mut b = HashMap::new();
            b.insert("binding", render_pattern(ctx, inner));
            ctx.templates.render("pattern_ok", None, &[], &b)
                .unwrap_or_else(|| format!("Ok(_)"))
        }
        IrPattern::Err { inner } => {
            let mut b = HashMap::new();
            b.insert("binding", render_pattern(ctx, inner));
            ctx.templates.render("pattern_err", None, &[], &b)
                .unwrap_or_else(|| format!("Err(_)"))
        }
        IrPattern::Constructor { name, args } => {
            let qualified = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(name) {
                let mut b = HashMap::new();
                b.insert("enum_name", enum_name.clone());
                b.insert("ctor_name", name.clone());
                ctx.templates.render("ctor_qualify", None, &[], &b)
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
            format!("({})", elems)
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
                let mut b = HashMap::new();
                b.insert("name", qualified_name.clone());
                b.insert("fields", fields_str.clone());
                let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                ctx.templates.render(construct, None, &[], &b)
                    .unwrap_or_else(|| format!("{} {{ {} }}", qualified_name, fields_str))
            } else {
                format!("{} {{ {} }}", qualified_name, fields_str)
            }
        }
    }
}

// ── Statement rendering ──

pub fn render_stmt(ctx: &RenderContext, stmt: &IrStmt) -> String {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, mutability } => {
            let mut b = HashMap::new();
            b.insert("name", ctx.var_name(*var).to_string());
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
            b.insert("type", render_type(ctx, ty));
            b.insert("value", render_expr(ctx, value));
            let construct = match mutability {
                Mutability::Let => "let_binding",
                Mutability::Var => "var_binding",
            };
            ctx.templates.render(construct, None, &[], &b)
                .unwrap_or_else(|| format!("let _ = _;"))
        }
        IrStmtKind::Assign { var, value } => {
            let mut b = HashMap::new();
            b.insert("target", ctx.var_name(*var).to_string());
            b.insert("value", render_expr(ctx, value));
            ctx.templates.render("assignment", None, &[], &b)
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
            let is_loop_break = matches!(&else_.kind, IrExprKind::Unit | IrExprKind::Break)
                || (matches!(&else_.kind, IrExprKind::ResultOk { .. }) && {
                    if let IrExprKind::ResultOk { expr: inner } = &else_.kind {
                        matches!(&inner.kind, IrExprKind::Unit)
                    } else { false }
                });
            let action = if is_loop_break { "break" } else { "return" };
            let mut b = HashMap::new();
            b.insert("cond", cond_str);
            let neg = ctx.templates.render("guard_negate", None, &[], &b)
                .unwrap_or_else(|| format!("!cond"));
            if action == "break" {
                format!("if {} {{ break }}", neg)
            } else {
                format!("if {} {{ return {} }}", neg, else_str)
            }
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            let target_str = ctx.var_name(*target).to_string();
            let idx_str = render_expr(ctx, index);
            let val_str = render_expr(ctx, value);
            let target_ty = &ctx.var_table.get(*target).ty;
            let mut b = HashMap::new();
            if matches!(target_ty, Ty::Map(_, _)) {
                b.insert("target", target_str);
                b.insert("key", idx_str);
                b.insert("value", val_str);
                ctx.templates.render("map_insert", None, &[], &b)
                    .unwrap_or_else(|| "map_set(...)".into())
            } else {
                b.insert("target", target_str);
                b.insert("index", idx_str);
                b.insert("value", val_str);
                ctx.templates.render("index_assign", None, &[], &b)
                    .unwrap_or_else(|| "idx[...] = ...;".into())
            }
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
                        Ty::Named(n, _) => n.clone(),
                        Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                            let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.clone()).collect();
                            names.sort();
                            ctx.ann.named_records.get(&names).cloned()
                                .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                                .unwrap_or_else(|| names.join("_"))
                        }
                        _ => "_".into(),
                    };
                    let qualified = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(&type_name) {
                        format!("{}::{}", enum_name, type_name)
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
                        let mut b = HashMap::new();
                        b.insert("name", qualified.clone());
                        b.insert("fields", fields_str.clone());
                        let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                        ctx.templates.render(construct, None, &[], &b)
                            .unwrap_or_else(|| format!("{} {{ {} }}", qualified, fields_str))
                    } else {
                        format!("{} {{ {} }}", qualified, fields_str)
                    }
                }
                _ => render_pattern(ctx, pattern),
            };
            let val_str = render_expr(ctx, value);
            format!("let {} = {};", pat_str, val_str)
        }
        IrStmtKind::Comment { text } => format!("// {}", text),
    }
}

// ── Function rendering ──

pub fn render_function(ctx: &RenderContext, func: &IrFunction) -> String {
    // Set effect fn context for auto-? insertion
    let fn_ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        in_effect_fn: func.is_effect && !func.is_test,
        ann: ctx.ann.clone(),
    };

    // Extern fn: emit import/use via template
    if !func.extern_attrs.is_empty() {
        let target_str = match ctx.target {
            Target::Rust => "rs",
            Target::TypeScript => "ts",
            _ => "",
        };
        for attr in &func.extern_attrs {
            if attr.target == target_str {
                let mut b = HashMap::new();
                b.insert("module", attr.module.clone());
                b.insert("function", attr.function.clone());
                b.insert("name", func.name.clone());
                return ctx.templates.render("extern_fn", None, &[], &b)
                    .unwrap_or_else(|| format!("// extern: {}.{}", attr.module, attr.function));
            }
        }
    }

    let params_str = func.params.iter()
        .map(|p| {
            let mut b = HashMap::new();
            b.insert("name", p.name.clone());
            b.insert("type", render_type(&fn_ctx, &p.ty));
            fn_ctx.templates.render("fn_param", None, &[], &b)
                .unwrap_or_else(|| format!("{}: {}", p.name, render_type(&fn_ctx, &p.ty)))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let body_str = render_expr(&fn_ctx, &func.body);
    let ret_str = render_type(ctx, &func.ret_ty);

    // Build generics string for functions
    let fn_generics = if let Some(generics) = &func.generics {
        if generics.is_empty() {
            String::new()
        } else {
            let params = generics.iter().map(|g| {
                let mut b = HashMap::new();
                b.insert("name", g.name.clone());
                ctx.templates.render("generic_bound_full", None, &[], &b)
                    .unwrap_or_else(|| g.name.clone())
            }).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        }
    } else {
        String::new()
    };

    // Sanitize function name: spaces/dots/hyphens → underscores
    let mut safe_name = func.name.replace(' ', "_").replace('-', "_").replace('.', "_")
        .replace('+', "_plus_").replace('/', "_div_").replace('*', "_mul_")
        .replace('(', "").replace(')', "").replace(',', "_").replace(':', "_")
        .replace('=', "_eq_").replace('!', "_bang_").replace('?', "_q_")
        .replace('<', "_lt_").replace('>', "_gt_").replace('[', "_").replace(']', "_")
        .replace('|', "_pipe_").replace('&', "_amp_").replace('%', "_mod_");
    // Escape target-specific keywords via template
    let target_keywords = ["while", "for", "if", "else", "match", "loop", "break", "continue",
        "return", "fn", "let", "mut", "use", "mod", "pub", "struct", "enum", "impl", "trait",
        "type", "where", "as", "in", "ref", "self", "super", "crate", "const", "static",
        "unsafe", "async", "await", "dyn", "move", "true", "false"];
    if target_keywords.contains(&safe_name.as_str()) {
        let mut b = HashMap::new();
        b.insert("name", safe_name.clone());
        safe_name = ctx.templates.render("keyword_escape", None, &[], &b)
            .unwrap_or(safe_name);
    }
    let safe_name = format!("{}{}", safe_name, fn_generics);

    let mut b = HashMap::new();
    b.insert("name", safe_name.clone());
    b.insert("params", params_str);
    b.insert("return_type", ret_str);
    b.insert("body", body_str);
    b.insert("name", safe_name);

    let construct = if func.is_test {
        "test_block"
    } else if func.is_effect {
        "effect_fn_decl"
    } else {
        "fn_decl"
    };
    fn_ctx.templates.render(construct, None, &[], &b)
        .unwrap_or_else(|| format!("fn {}() {{ }}", func.name))
}

// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    // Build constructor → enum name map
    let mut ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        in_effect_fn: ctx.in_effect_fn,
        ann: ctx.ann.clone(),
    };
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases {
                ctx.ann.ctor_to_enum.insert(c.name.clone(), td.name.clone());
            }
        }
    }

    // Build anonymous record maps (populated by target-specific pipeline)
    ctx.ann.named_records = collect_named_records(program);
    ctx.ann.anon_records = collect_anon_records(program, &ctx.ann.named_records);

    let mut parts = Vec::new();

    // Anonymous record struct definitions (only if anon_records is populated)
    if !ctx.ann.anon_records.is_empty() {
        for (field_names, struct_name) in &ctx.ann.anon_records {
            let generics: Vec<String> = (0..field_names.len())
                .map(|i| {
                    let mut b = HashMap::new();
                    b.insert("name", format!("T{}", i));
                    ctx.templates.render("generic_bound_full", None, &[], &b)
                        .unwrap_or_else(|| format!("T{}", i))
                })
                .collect();
            let fields: Vec<String> = field_names.iter().enumerate()
                .map(|(i, name)| {
                    let mut b = HashMap::new();
                    b.insert("name", name.clone());
                    b.insert("type", format!("T{}", i));
                    ctx.templates.render("struct_field", None, &[], &b)
                        .unwrap_or_else(|| format!("{}: T{}", name, i))
                })
                .collect();
            let fields_str = fields.join("\n");
            let full_name = format!("{}<{}>", struct_name, generics.join(", "));
            let mut b = HashMap::new();
            b.insert("name", full_name);
            b.insert("fields", fields_str.clone());
            parts.push(ctx.templates.render("struct_decl", None, &[], &b)
                .unwrap_or_else(|| format!("struct {} {{ {} }}", struct_name, fields_str)));
        }
    }

    // Type declarations
    for td in &program.type_decls {
        parts.push(render_type_decl(&ctx, td));
    }

    // Top-level lets
    for tl in &program.top_lets {
        let name = ctx.var_table.get(tl.var).name.clone();
        let ty_str = render_type(&ctx, &tl.ty);
        let val_str = render_expr(&ctx, &tl.value);
        if matches!(tl.kind, TopLetKind::Lazy) {
            ctx.ann.lazy_vars.insert(tl.var);
        }
        let construct = match tl.kind {
            TopLetKind::Const => "top_let_const",
            TopLetKind::Lazy => "top_let_lazy",
        };
        let mut b = HashMap::new();
        b.insert("name", name.to_uppercase());
        b.insert("type", ty_str);
        b.insert("value", val_str.clone());
        let rendered = ctx.templates.render(construct, None, &[], &b)
            .unwrap_or_else(|| format!("const {} = {};", name, val_str));
        parts.push(rendered);
    }

    // Functions (non-test)
    for func in program.functions.iter().filter(|f| !f.is_test) {
        parts.push(render_function(&ctx, func));
    }

    // Test functions
    let test_fns: Vec<&IrFunction> = program.functions.iter().filter(|f| f.is_test).collect();
    if !test_fns.is_empty() {
        let test_parts: Vec<String> = test_fns.iter()
            .map(|f| render_function(&ctx, f))
            .collect();
        let mut b = HashMap::new();
        b.insert("tests", test_parts.join("\n\n"));
        let wrapped = ctx.templates.render("test_module", None, &[], &b)
            .unwrap_or_else(|| test_parts.join("\n\n"));
        parts.push(wrapped);
    }

    // Imported modules: render their type decls and functions
    for module in &program.modules {
        let mod_ctx = RenderContext {
            templates: ctx.templates,
            var_table: &module.var_table,
            indent: ctx.indent,
            target: ctx.target,
            in_effect_fn: false,
            ann: ctx.ann.clone(),
        };
        // Module type decls
        for td in &module.type_decls {
            parts.push(render_type_decl(&mod_ctx, td));
        }
        // Module functions (prefixed with module name)
        for func in &module.functions {
            let rendered = render_function(&mod_ctx, func);
            // Rename: fn name → fn modulename_name (to match almide_rt_module_func naming)
            let prefixed = rendered.replacen(
                &format!("fn {}", func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                &format!("fn almide_rt_{}_{}", module.name, func.name.replace(' ', "_").replace('-', "_").replace('.', "_")),
                1
            );
            parts.push(prefixed);
        }
    }

    parts.join("\n\n")
}

fn render_type_decl(ctx: &RenderContext, td: &IrTypeDecl) -> String {
    // Build generics string e.g. "<T>" or "<T, U>"
    let generics_str = if let Some(generics) = &td.generics {
        if generics.is_empty() {
            String::new()
        } else {
            let params = generics.iter().map(|g| {
                let mut b = HashMap::new();
                b.insert("name", g.name.clone());
                ctx.templates.render("generic_bound", None, &[], &b)
                    .unwrap_or_else(|| g.name.clone())
            }).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        }
    } else {
        String::new()
    };

    match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            let fields_str = fields.iter()
                .map(|f| {
                    let mut b = HashMap::new();
                    b.insert("name", f.name.clone());
                    b.insert("type", render_type(ctx, &f.ty));
                    ctx.templates.render("struct_field", None, &[], &b)
                        .unwrap_or_else(|| format!("{}: {},", f.name, render_type(ctx, &f.ty)))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut b = HashMap::new();
            let full_name = format!("{}{}", td.name, generics_str);
            b.insert("name", full_name.clone());
            let fallback = format!("struct {} {{ {} }}", full_name, &fields_str);
            b.insert("fields", fields_str);
            ctx.templates.render("struct_decl", None, &[], &b)
                .unwrap_or(fallback)
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            let variants_str = cases.iter()
                .map(|v| match &v.kind {
                    IrVariantKind::Unit => {
                        let mut b = HashMap::new();
                        b.insert("name", v.name.clone());
                        ctx.templates.render("enum_variant_unit", None, &[], &b)
                            .unwrap_or_else(|| v.name.clone())
                    }
                    IrVariantKind::Tuple { fields } => {
                        let fields_str = fields.iter().map(|t| {
                            let rendered = render_type(ctx, t);
                            if ctx.ann.recursive_enums.contains(&td.name) && ty_contains_name(t, &td.name) {
                                format!("Box<{}>", rendered)
                            } else {
                                rendered
                            }
                        }).collect::<Vec<_>>().join(", ");
                        let mut b = HashMap::new();
                        b.insert("name", v.name.clone());
                        let fallback = format!("{}({})", v.name, &fields_str);
                        b.insert("fields", fields_str);
                        ctx.templates.render("enum_variant", None, &[], &b)
                            .unwrap_or(fallback)
                    }
                    IrVariantKind::Record { fields } => {
                        let fields_str = fields.iter()
                            .map(|f| {
                                let rendered = render_type(ctx, &f.ty);
                                let boxed = if ctx.ann.recursive_enums.contains(&td.name) && ty_contains_name(&f.ty, &td.name) {
                                    format!("Box<{}>", rendered)
                                } else {
                                    rendered
                                };
                                format!("{}: {}", f.name, boxed)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{} {{ {} }}", v.name, fields_str)
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n");
            let mut b = HashMap::new();
            let full_name = format!("{}{}", td.name, generics_str);
            b.insert("name", full_name.clone());
            let fallback = format!("enum {} {{ {} }}", full_name, &variants_str);
            b.insert("variants", variants_str);
            ctx.templates.render("enum_decl", None, &[], &b)
                .unwrap_or(fallback)
        }
        IrTypeDeclKind::Alias { target } => {
            let mut b = HashMap::new();
            b.insert("name", td.name.clone());
            b.insert("type", render_type(ctx, target));
            ctx.templates.render("type_alias", None, &[], &b)
                .unwrap_or_else(|| format!("type {} = {};", td.name, render_type(ctx, target)))
        }
    }
}

// ── Helpers ──

/// Try to render via template, fallback to default string.
/// Render a Fn type as Box<dyn Fn(...) -> T> (for nested impl Trait in Rust)
fn render_type_boxed_fn(ctx: &RenderContext, ty: &Ty) -> String {
    match ty {
        Ty::Fn { params, ret } => {
            let params_str = params.iter().map(|p| render_type(ctx, p)).collect::<Vec<_>>().join(", ");
            let ret_str = if matches!(ret.as_ref(), Ty::Fn { .. }) {
                render_type_boxed_fn(ctx, ret)
            } else {
                render_type(ctx, ret)
            };
            let mut b = HashMap::new();
            b.insert("params", params_str);
            b.insert("return", ret_str);
            ctx.templates.render("type_fn_boxed", None, &[], &b)
                .unwrap_or_else(|| ctx.templates.render("type_fn", None, &[], &b)
                    .unwrap_or_else(|| "BoxFn".into()))
        }
        _ => render_type(ctx, ty),
    }
}

fn template_or(ctx: &RenderContext, construct: &str, attrs: &[&str], fallback: &str) -> String {
    let b = HashMap::new();
    ctx.templates.render(construct, None, attrs, &b)
        .unwrap_or_else(|| fallback.to_string())
}

/// Add statement terminator (`;` in Rust, `;` in TS) if the rendered string doesn't already end with one
fn terminate_stmt(ctx: &RenderContext, rendered: String) -> String {
    let term = template_or(ctx, "stmt_terminator", &[], ";");
    if !term.is_empty() && !rendered.ends_with(';') && !rendered.ends_with('}') {
        format!("{}{}", rendered, term)
    } else {
        rendered
    }
}

// ── Rust-specific helpers ──

/// Check if a type contains a reference to a named type (for recursive Box detection).
pub fn ty_contains_name(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::Named(n, args) => n == name || args.iter().any(|a| ty_contains_name(a, name)),
        Ty::Variant { name: vn, .. } => vn == name,
        Ty::List(inner) | Ty::Option(inner) => ty_contains_name(inner, name),
        Ty::Result(a, b) | Ty::Map(a, b) => ty_contains_name(a, name) || ty_contains_name(b, name),
        Ty::Tuple(elems) => elems.iter().any(|e| ty_contains_name(e, name)),
        Ty::Fn { params, ret } => params.iter().any(|p| ty_contains_name(p, name)) || ty_contains_name(ret, name),
        _ => false,
    }
}

/// Check if a type tree contains any named TypeVars (non-? prefix)
fn ty_has_named_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(n) => !n.starts_with('?'),
        Ty::List(inner) | Ty::Option(inner) => ty_has_named_typevar(inner),
        Ty::Result(a, b) | Ty::Map(a, b) => ty_has_named_typevar(a) || ty_has_named_typevar(b),
        Ty::Tuple(elems) => elems.iter().any(ty_has_named_typevar),
        Ty::Named(_, args) => args.iter().any(ty_has_named_typevar),
        Ty::Fn { params, ret } => params.iter().any(ty_has_named_typevar) || ty_has_named_typevar(ret),
        _ => false,
    }
}

/// Replace named TypeVars with Ty::Unknown (rendered as _)
fn erase_named_typevars(ty: Ty) -> Ty {
    match ty {
        Ty::TypeVar(n) if !n.starts_with('?') => Ty::Unknown,
        Ty::List(inner) => Ty::List(Box::new(erase_named_typevars(*inner))),
        Ty::Option(inner) => Ty::Option(Box::new(erase_named_typevars(*inner))),
        Ty::Result(a, b) => Ty::Result(Box::new(erase_named_typevars(*a)), Box::new(erase_named_typevars(*b))),
        Ty::Map(a, b) => Ty::Map(Box::new(erase_named_typevars(*a)), Box::new(erase_named_typevars(*b))),
        Ty::Tuple(elems) => Ty::Tuple(elems.into_iter().map(erase_named_typevars).collect()),
        Ty::Named(n, args) => Ty::Named(n, args.into_iter().map(erase_named_typevars).collect()),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.into_iter().map(erase_named_typevars).collect(),
            ret: Box::new(erase_named_typevars(*ret)),
        },
        other => other,
    }
}

/// Does this type need .clone() when used as a variable in Rust?
fn needs_clone(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Record { .. } | Ty::Named(_, _) | Ty::Option(_) | Ty::Result(_, _))
}

/// Render stdlib function arguments with Rust-specific decorations.
/// - List args → `({arg}).to_vec()`
/// - String args used as &str → `&*{arg}`
/// - Lambda args → `|params| { clone_bindings; body }`
fn render_stdlib_args_rust(ctx: &RenderContext, args: &[IrExpr]) -> String {
    args.iter().map(|arg| {
        match &arg.kind {
            // Lambda: render with clone bindings for captured variables
            IrExprKind::Lambda { params, body } => {
                let params_str = params.iter()
                    .map(|(id, _)| ctx.var_name(*id).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                // Clone all parameters inside the closure
                let clone_bindings: Vec<String> = params.iter()
                    .map(|(id, _)| format!("let {} = {}.clone();", ctx.var_name(*id), ctx.var_name(*id)))
                    .collect();
                let body_str = render_expr(ctx, body);
                if clone_bindings.is_empty() {
                    format!("|{}| {{ {} }}", params_str, body_str)
                } else {
                    format!("|{}| {{ {} {} }}", params_str, clone_bindings.join(" "), body_str)
                }
            }
            _ => {
                let rendered = render_expr(ctx, arg);
                match &arg.ty {
                    // List → &(expr).to_vec() — pass as reference to owned copy
                    Ty::List(_) => format!("&({}).to_vec()", rendered),
                    // String → &* — pass as &str
                    Ty::String => format!("&*{}", rendered),
                    _ => rendered,
                }
            }
        }
    }).collect::<Vec<_>>().join(", ")
}

// ── Anonymous record collection ──
// Simplified version of emit_rust::lower_types logic, directly in codegen.

use std::collections::HashSet;

fn collect_named_records(program: &IrProgram) -> HashMap<Vec<String>, String> {
    let mut map = HashMap::new();
    for td in &program.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            names.sort();
            map.insert(names, td.name.clone());
        }
    }
    map
}

fn collect_anon_records(program: &IrProgram, named: &HashMap<Vec<String>, String>) -> HashMap<Vec<String>, String> {
    let named_set: HashSet<Vec<String>> = named.keys().cloned().collect();
    let mut seen: HashSet<Vec<String>> = HashSet::new();

    // Collect from all types AND expressions in the program
    for func in &program.functions {
        for p in &func.params { collect_anon_from_ty(&p.ty, &named_set, &mut seen); }
        collect_anon_from_ty(&func.ret_ty, &named_set, &mut seen);
        collect_anon_from_expr(&func.body, &named_set, &mut seen);
    }
    for tl in &program.top_lets {
        collect_anon_from_ty(&tl.ty, &named_set, &mut seen);
        collect_anon_from_expr(&tl.value, &named_set, &mut seen);
    }

    let mut map = HashMap::new();
    let mut keys: Vec<Vec<String>> = seen.into_iter().collect();
    keys.sort();
    for (i, key) in keys.into_iter().enumerate() {
        map.insert(key, format!("AlmdRec{}", i));
    }
    map
}

fn collect_anon_from_expr(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    collect_anon_from_ty(&expr.ty, named, seen);
    match &expr.kind {
        IrExprKind::Block { stmts, expr: e } | IrExprKind::DoBlock { stmts, expr: e } => {
            for s in stmts { collect_anon_from_stmt(s, named, seen); }
            if let Some(e) = e { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(then, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrExprKind::Match { subject, arms } => {
            collect_anon_from_expr(subject, named, seen);
            for arm in arms { collect_anon_from_expr(&arm.body, named, seen); }
        }
        IrExprKind::Call { args, target, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
                collect_anon_from_expr(object, named, seen);
            }
            for a in args { collect_anon_from_expr(a, named, seen); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_anon_from_expr(left, named, seen);
            collect_anon_from_expr(right, named, seen);
        }
        IrExprKind::UnOp { operand, .. } => collect_anon_from_expr(operand, named, seen),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::Lambda { body, .. } => collect_anon_from_expr(body, named, seen),
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, v) in fields { collect_anon_from_expr(v, named, seen); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            collect_anon_from_expr(object, named, seen);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_anon_from_expr(iterable, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::While { cond, body } => {
            collect_anon_from_expr(cond, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr } => {
            collect_anon_from_expr(expr, named, seen);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { collect_anon_from_expr(expr, named, seen); }
            }
        }
        // Codegen-specific nodes
        IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            collect_anon_from_expr(expr, named, seen);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { collect_anon_from_expr(a, named, seen); }
        }
        _ => {}
    }
}

fn collect_anon_from_stmt(stmt: &IrStmt, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            collect_anon_from_ty(ty, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_anon_from_expr(index, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::Guard { cond, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrStmtKind::Expr { expr } => collect_anon_from_expr(expr, named, seen),
        _ => {}
    }
}

fn collect_anon_from_ty(ty: &Ty, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if !named.contains(&names) { seen.insert(names); }
            for (_, t) in fields { collect_anon_from_ty(t, named, seen); }
        }
        Ty::List(inner) | Ty::Option(inner) => collect_anon_from_ty(inner, named, seen),
        Ty::Result(a, b) | Ty::Map(a, b) => { collect_anon_from_ty(a, named, seen); collect_anon_from_ty(b, named, seen); }
        Ty::Tuple(elems) => { for e in elems { collect_anon_from_ty(e, named, seen); } }
        Ty::Fn { params, ret } => { for p in params { collect_anon_from_ty(p, named, seen); } collect_anon_from_ty(ret, named, seen); }
        _ => {}
    }
}
