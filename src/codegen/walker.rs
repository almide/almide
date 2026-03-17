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
use super::pass::Target;
use super::template::TemplateSet;

/// Render context: carries the variable table, target, and scope state.
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    /// Are we inside an effect fn? (for auto-? insertion)
    pub in_effect_fn: bool,
    /// Constructor name → enum name mapping (e.g. "Red" → "Color")
    pub ctor_to_enum: HashMap<String, String>,
    /// Anonymous record field names → struct name (e.g. ["age","name"] → "AlmdRec0")
    pub anon_records: HashMap<Vec<String>, String>,
    /// Named record field names → type name (e.g. ["age","name"] → "User")
    pub named_records: HashMap<Vec<String>, String>,
    /// Top-level lazy let VarIds (need * deref in Rust)
    pub lazy_vars: HashSet<VarId>,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0, target: Target::Rust, in_effect_fn: false, ctor_to_enum: HashMap::new(), anon_records: HashMap::new(), named_records: HashMap::new(), lazy_vars: HashSet::new() }
    }

    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    fn is_rust(&self) -> bool { self.target == Target::Rust }

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
            if let Some(n) = ctx.named_records.get(&names) {
                return n.clone();
            }
            // Check anonymous records
            if let Some(n) = ctx.anon_records.get(&names) {
                if ctx.is_rust() {
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
                    n.clone()
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
            let ret_str = render_type(ctx, ret);
            if ctx.is_rust() {
                format!("impl Fn({}) -> {}", params_str, ret_str)
            } else {
                format!("({}) => {}", params_str, ret_str)
            }
        }
        Ty::Tuple(elems) => {
            let parts = elems.iter().map(|t| render_type(ctx, t)).collect::<Vec<_>>().join(", ");
            format!("({})", parts)
        }
        Ty::TypeVar(n) => {
            // TypeVars like "?23" should be rendered as _ (infer) in Rust
            if n.starts_with('?') {
                if ctx.is_rust() { "_".into() } else { "any".into() }
            } else {
                n.clone()
            }
        }
        Ty::Unknown | Ty::Union(_) => {
            // Unknown types: use a generic placeholder that won't break compilation
            if ctx.is_rust() { "_".into() } else { "any".into() }
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
            // Escape backslashes and special chars for target language
            let escaped = if ctx.is_rust() {
                value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r")
            } else {
                value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")
            };
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
            // Rust: top-level lazy vars need * deref
            if ctx.is_rust() && ctx.lazy_vars.contains(id) {
                let deref_name = format!("(*{})", name.to_uppercase());
                if needs_clone(&expr.ty) {
                    format!("{}.clone()", deref_name)
                } else {
                    deref_name
                }
            // Rust: clone heap types (String, Vec, HashMap, records) when used
            } else if ctx.is_rust() && needs_clone(&expr.ty) {
                format!("{}.clone()", name)
            } else {
                name
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
            // Rust: string subjects need .as_str() for pattern matching
            if ctx.is_rust() && matches!(&subject.ty, Ty::String) {
                // Check if any arm has a string literal pattern
                let has_str_pat = arms.iter().any(|a| matches!(&a.pattern, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. })));
                if has_str_pat {
                    subj = format!("{}.as_str()", subj);
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
                    if ctx.is_rust() && !rendered.ends_with(';') && !rendered.ends_with('}') {
                        format!("{};", rendered)
                    } else {
                        rendered
                    }
                })
                .collect();
            if let Some(e) = expr {
                let rendered = render_expr(ctx, e);
                // In a loop (DoBlock), non-Unit final expressions need `return` to exit the loop
                let needs_return = ctx.is_rust()
                    && !matches!(&e.ty, Ty::Unit)
                    && !matches!(&e.kind, IrExprKind::Unit | IrExprKind::Break);
                if needs_return && !rendered.starts_with("break") {
                    parts.push(format!("return {}", rendered));
                } else {
                    parts.push(rendered);
                }
            }
            if ctx.is_rust() {
                format!("loop {{\n{}\n}}", parts.join("\n"))
            } else {
                format!("while (true) {{\n{}\n}}", parts.join("\n"))
            }
        }

        IrExprKind::Block { stmts, expr } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| {
                    let rendered = render_stmt(ctx, s);
                    // Add ; if the stmt doesn't already end with one
                    if ctx.is_rust() && !rendered.ends_with(';') && !rendered.ends_with('}') {
                        format!("{};", rendered)
                    } else {
                        rendered
                    }
                })
                .collect();
            if let Some(e) = expr {
                parts.push(render_expr(ctx, e));
            }
            format!("{{\n{}\n}}", parts.join("\n"))
        }

        // ── Loops ──
        IrExprKind::ForIn { var, iterable, body, .. } => {
            let var_name = ctx.var_name(*var).to_string();
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

        // ── Calls ──
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Module { module, func } => {
                    // Special cases: io.print → println! in Rust
                    if ctx.is_rust() && module == "io" && func == "print" {
                        let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                        return format!("println!(\"{{}}\", {})", args_str);
                    }

                    if ctx.is_rust() {
                        // If any arg is a FnRef (not a lambda), use simple rendering
                        // gen_generated_call's inline_lambda doesn't handle FnRef
                        let has_fn_ref = args.iter().any(|a| matches!(&a.kind, IrExprKind::FnRef { .. }));
                        if has_fn_ref {
                            let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                            let mut result = format!("almide_rt_{}_{}({})", module, func, args_str);
                            if ctx.in_effect_fn && matches!(&expr.ty, Ty::Result(_, _)) {
                                result.push('?');
                            }
                            return result;
                        }

                        // Use the generated stdlib dispatch (handles &*, .to_vec(), ?, lambda formatting)
                        let rendered_args: Vec<String> = args.iter().map(|a| render_expr(ctx, a)).collect();
                        let inline_lambda = |arg_idx: usize, param_count: usize| -> (Vec<String>, String) {
                            if let Some(arg) = args.get(arg_idx) {
                                if let IrExprKind::Lambda { params, body } = &arg.kind {
                                    let names: Vec<String> = params.iter()
                                        .take(param_count)
                                        .map(|(id, _)| {
                                            let n = ctx.var_name(*id).to_string();
                                            if n == "_" { "__unused".into() } else { n }
                                        })
                                        .collect();
                                    let body_str = render_expr(ctx, body);
                                    return (names, body_str);
                                }
                            }
                            (vec!["__unused".into()], "()".into())
                        };
                        if let Some(mut result) = crate::generated::emit_rust_calls::gen_generated_call(
                            module, func, &rendered_args, ctx.in_effect_fn, &inline_lambda
                        ) {
                            // Fix value module: TOML templates emit value_str() but runtime uses almide_rt_value_str()
                            if module == "value" && result.starts_with("value_") {
                                result = format!("almide_rt_{}", result);
                            }
                            return result;
                        }
                        // Fallback: simple module call
                        let args_str = rendered_args.join(", ");
                        return format!("almide_rt_{}_{}({})", module, func, args_str);
                    }

                    // TS/other: simple module call
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    let mut b = HashMap::new();
                    b.insert("module", module.clone());
                    b.insert("func", func.clone());
                    b.insert("args", args_str);
                    ctx.templates.render("module_call", None, &[], &b)
                        .unwrap_or_else(|| format!("__almd_{}.{}()", module, func))
                }
                _ => {
                    let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
                    let callee = match target {
                        CallTarget::Named { name } => {
                            // Rust: assert_eq/assert_ne → macro invocation
                            if ctx.is_rust() && (name == "assert_eq" || name == "assert_ne") {
                                return format!("{}!({})", name, args_str);
                            }
                            // Rust: assert_some → assert!(x.is_some())
                            if ctx.is_rust() && name == "assert_some" {
                                return format!("assert!(({}).is_some())", args_str);
                            }
                            // Rust: println → println! macro
                            if ctx.is_rust() && name == "println" {
                                return format!("println!(\"{{}}\", {})", args_str);
                            }
                            // Rust: value_* and __encode/__decode functions → almide_rt_* (codec runtime)
                            if ctx.is_rust() && name.starts_with("value_") {
                                return format!("almide_rt_{}({})", name, args_str);
                            }
                            // Rust: __encode_list_Type / __decode_list_Type → runtime or inline
                            if ctx.is_rust() && (name.starts_with("__encode_list_") || name.starts_with("__decode_list_")) {
                                let type_name = if name.starts_with("__encode_list_") {
                                    &name["__encode_list_".len()..]
                                } else {
                                    &name["__decode_list_".len()..]
                                };
                                let primitives = ["string", "int", "float", "bool"];
                                if primitives.contains(&type_name) {
                                    // Primitive: runtime has almide_rt___encode_list_string etc.
                                    return format!("almide_rt_{}({})", name, args_str);
                                } else {
                                    // Custom type: use generic encode/decode with Type_encode/Type_decode
                                    if name.starts_with("__encode") {
                                        return format!("almide_rt_value_encode_list({}, {}_encode)", args_str, type_name);
                                    } else {
                                        return format!("almide_rt_value_decode_list({}, {}_decode)", args_str, type_name);
                                    }
                                }
                            }
                            // Rust: other __ prefixed functions
                            if ctx.is_rust() && name.starts_with("__") {
                                return format!("almide_rt_{}({})", name, args_str);
                            }
                            // Rust: Type.method → Type_method (standalone function)
                            // These are auto-derived codec functions like Person.encode/Person.decode
                            if ctx.is_rust() && name.contains('.') {
                                let flat = name.replace('.', "_");
                                return format!("{}({})", flat, args_str);
                            }
                            // Qualify enum constructors (Red → Color::Red)
                            if let Some(enum_name) = ctx.ctor_to_enum.get(name.as_str()) {
                                if args.is_empty() {
                                    return format!("{}::{}", enum_name, name);
                                } else {
                                    return format!("{}::{}({})", enum_name, name, args_str);
                                }
                            }
                            name.clone()
                        }
                        CallTarget::Method { object, method } => {
                            // Rust: codec encode/decode methods → Type_method(object)
                            // Rust: codec encode/decode methods → Type_method(object, ...)
                            if ctx.is_rust() {
                                let is_codec = method == "encode" || method == "decode"
                                    || method.ends_with(".encode") || method.ends_with(".decode");
                                if is_codec {
                                    // Method might be "encode" or "Color.encode"
                                    let flat_method = method.replace('.', "_");
                                    let call_name = if method.contains('.') {
                                        flat_method // "Color_encode" — type already in method name
                                    } else {
                                        let type_name = match &object.ty {
                                            Ty::Named(n, _) => n.clone(),
                                            Ty::Variant { name, .. } => name.clone(),
                                            _ => render_type(ctx, &object.ty),
                                        };
                                        format!("{}_{}", type_name, method)
                                    };
                                    let obj_str = render_expr(ctx, object);
                                    if args_str.is_empty() {
                                        return format!("{}({})", call_name, obj_str);
                                    } else {
                                        return format!("{}({}, {})", call_name, obj_str, args_str);
                                    }
                                }
                            }
                            format!("{}.{}", render_expr(ctx, object), method)
                        }
                        CallTarget::Computed { callee } => render_expr(ctx, callee),
                        CallTarget::Module { .. } => unreachable!(),
                    };
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
            let elems = elements.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            let mut b = HashMap::new();
            b.insert("elements", elems);
            ctx.templates.render("list_literal", None, &[], &b)
                .unwrap_or_else(|| format!("[...]"))
        }

        IrExprKind::Record { name, fields } => {
            // Detect parent enum for Box wrapping of recursive fields
            let parent_enum = name.as_ref().and_then(|n| ctx.ctor_to_enum.get(n)).cloned();
            let fields_str = fields.iter()
                .map(|(k, v)| {
                    let mut val_str = render_expr(ctx, v);
                    // Box recursive fields (value type contains parent enum name)
                    if ctx.is_rust() {
                        if let Some(ref pe) = parent_enum {
                            if ty_contains_name(&v.ty, pe) {
                                val_str = format!("Box::new({})", val_str);
                            }
                        }
                    }
                    let mut b = HashMap::new();
                    b.insert("name", k.clone());
                    b.insert("value", val_str);
                    ctx.templates.render("record_field", None, &[], &b)
                        .unwrap_or_else(|| format!("{}: {}", k, render_expr(ctx, v)))
                })
                .collect::<Vec<_>>()
                .join(", ");
            // Resolve type name: explicit name, or from expr.ty
            // For record literals, use bare struct name (no generics — Rust infers them)
            let mut type_name = name.clone().unwrap_or_else(|| {
                match &expr.ty {
                    Ty::Named(n, _) => n.clone(),
                    Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                        let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.clone()).collect();
                        names.sort();
                        if let Some(n) = ctx.named_records.get(&names) {
                            n.clone()
                        } else if let Some(n) = ctx.anon_records.get(&names) {
                            n.clone() // bare name, no generics
                        } else {
                            names.join("_")
                        }
                    }
                    _ => render_type(ctx, &expr.ty),
                }
            });
            // Qualify enum variant constructors: Circle → Shape::Circle
            if let Some(enum_name) = ctx.ctor_to_enum.get(&type_name) {
                type_name = format!("{}::{}", enum_name, type_name);
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
            if ctx.is_rust() {
                // If inner type is already String, use .to_string()
                // If not, pass as-is (custom error types)
                match &inner.ty {
                    Ty::String => format!("Err({}.to_string())", inner_str),
                    _ => format!("Err({})", inner_str),
                }
            } else {
                let mut b = HashMap::new();
                b.insert("inner", inner_str);
                ctx.templates.render("err_expr", None, &[], &b)
                    .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
            }
        }

        // ── Lambda ──
        IrExprKind::Lambda { params, body } => {
            let params_str = params.iter()
                .map(|(id, _ty)| ctx.var_name(*id).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let body_str = render_expr(ctx, body);
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
                        // Escape { and } in literal parts for Rust format! strings
                        if ctx.is_rust() {
                            fmt_parts.push(value.replace('{', "{{").replace('}', "}}"));
                        } else {
                            fmt_parts.push(value.clone());
                        }
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
            if *inclusive {
                format!("({}..={})", s, e)
            } else {
                format!("({}..{})", s, e)
            }
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
            format!("{}[{}]", render_expr(ctx, object), render_expr(ctx, index))
        }

        // ── Map ──
        IrExprKind::MapLiteral { entries } => {
            if ctx.is_rust() {
                let parts: Vec<String> = entries.iter()
                    .map(|(k, v)| format!("({}, {})", render_expr(ctx, k), render_expr(ctx, v)))
                    .collect();
                format!("HashMap::from([{}])", parts.join(", "))
            } else {
                let parts: Vec<String> = entries.iter()
                    .map(|(k, v)| format!("[{}, {}]", render_expr(ctx, k), render_expr(ctx, v)))
                    .collect();
                format!("new Map([{}])", parts.join(", "))
            }
        }
        IrExprKind::EmptyMap => {
            if ctx.is_rust() { "HashMap::new()".into() } else { "new Map()".into() }
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
                    ctx.named_records.get(&names).cloned()
                        .or_else(|| ctx.anon_records.get(&names).cloned())
                        .unwrap_or_else(|| names.join("_"))
                }
                _ => render_type(ctx, &expr.ty),
            };
            if ctx.is_rust() {
                format!("{} {{ {}, ..{} }}", type_name, fields_str, base_str)
            } else {
                format!("{{ ...{}, {} }}", base_str, fields_str)
            }
        }

        // ── Try / Await ──
        IrExprKind::Try { expr: inner } => {
            let s = render_expr(ctx, inner);
            if ctx.is_rust() { format!("({})?", s) } else { s }
        }
        IrExprKind::Await { expr: inner } => {
            let s = render_expr(ctx, inner);
            if ctx.is_rust() { format!("{}.await", s) } else { format!("await {}", s) }
        }

        // ── Hole / Todo ──
        IrExprKind::Hole => if ctx.is_rust() { "todo!()".into() } else { "undefined".into() },
        IrExprKind::Todo { message } => {
            if ctx.is_rust() { format!("todo!(\"{}\")", message) } else { format!("throw new Error(\"{}\")", message) }
        }

        // ── Fan (concurrency) ──
        IrExprKind::Fan { exprs } => {
            // Placeholder: render as tuple of expressions
            let parts = exprs.iter().map(|e| render_expr(ctx, e)).collect::<Vec<_>>().join(", ");
            format!("/* fan */ ({})", parts)
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
    b.insert("pattern", pattern);
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
                    let escaped = if ctx.is_rust() {
                        value.replace('\\', "\\\\").replace('"', "\\\"")
                    } else {
                        value.clone()
                    };
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
            let qualified = if let Some(enum_name) = ctx.ctor_to_enum.get(name) {
                format!("{}::{}", enum_name, name)
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
            let qualified_name = if let Some(enum_name) = ctx.ctor_to_enum.get(name) {
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
            if *rest && ctx.is_rust() {
                if fields_str.is_empty() {
                    format!("{} {{ .. }}", qualified_name)
                } else {
                    format!("{} {{ {}, .. }}", qualified_name, fields_str)
                }
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
            if ctx.is_rust() && !rendered.ends_with(';') && !rendered.ends_with('}') {
                format!("{};", rendered)
            } else {
                rendered
            }
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
            if ctx.is_rust() {
                if action == "break" {
                    format!("if !({}) {{ break }}", cond_str)
                } else {
                    format!("if !({}) {{ return {} }}", cond_str, else_str)
                }
            } else {
                if action == "break" {
                    format!("if (!{}) {{ break }}", cond_str)
                } else {
                    format!("if (!{}) {{ return {} }}", cond_str, else_str)
                }
            }
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            let target_str = ctx.var_name(*target).to_string();
            let idx_str = render_expr(ctx, index);
            let val_str = render_expr(ctx, value);
            format!("{}[{}] = {};", target_str, idx_str, val_str)
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
                            ctx.named_records.get(&names).cloned()
                                .or_else(|| ctx.anon_records.get(&names).cloned())
                                .unwrap_or_else(|| names.join("_"))
                        }
                        _ => "_".into(),
                    };
                    let qualified = if let Some(enum_name) = ctx.ctor_to_enum.get(&type_name) {
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
                    if *rest && ctx.is_rust() {
                        if fields_str.is_empty() { format!("{} {{ .. }}", qualified) }
                        else { format!("{} {{ {}, .. }}", qualified, fields_str) }
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
    let mut fn_ctx = RenderContext {
        templates: ctx.templates,
        var_table: ctx.var_table,
        indent: ctx.indent,
        target: ctx.target,
        in_effect_fn: func.is_effect,
        ctor_to_enum: ctx.ctor_to_enum.clone(),
        anon_records: ctx.anon_records.clone(),
        named_records: ctx.named_records.clone(),
        lazy_vars: ctx.lazy_vars.clone(),
    };

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
        } else if ctx.is_rust() {
            let params = generics.iter().map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name)).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        } else {
            let params = generics.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(", ");
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
    // Escape Rust keywords with r# prefix
    let rust_keywords = ["while", "for", "if", "else", "match", "loop", "break", "continue",
        "return", "fn", "let", "mut", "use", "mod", "pub", "struct", "enum", "impl", "trait",
        "type", "where", "as", "in", "ref", "self", "super", "crate", "const", "static",
        "unsafe", "async", "await", "dyn", "move", "true", "false"];
    if ctx.is_rust() && rust_keywords.contains(&safe_name.as_str()) {
        safe_name = format!("r#{}", safe_name);
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
        ctor_to_enum: ctx.ctor_to_enum.clone(),
        anon_records: HashMap::new(),
        named_records: HashMap::new(),
        lazy_vars: HashSet::new(),
    };
    for td in &program.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases {
                ctx.ctor_to_enum.insert(c.name.clone(), td.name.clone());
            }
        }
    }

    // Build anonymous record maps (Rust only)
    if ctx.is_rust() {
        ctx.named_records = collect_named_records(program);
        ctx.anon_records = collect_anon_records(program, &ctx.named_records);
    }

    let mut parts = Vec::new();

    // Anonymous record struct definitions (Rust only)
    if ctx.is_rust() {
        for (field_names, struct_name) in &ctx.anon_records {
            let generics: Vec<String> = (0..field_names.len())
                .map(|i| format!("T{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", i))
                .collect();
            let fields: Vec<String> = field_names.iter().enumerate()
                .map(|(i, name)| format!("pub {}: T{},", name, i))
                .collect();
            parts.push(format!(
                "#[derive(Debug, Clone, PartialEq)]\nstruct {}<{}> {{\n{}\n}}",
                struct_name,
                generics.join(", "),
                fields.join("\n")
            ));
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
        let rendered = if ctx.is_rust() {
            match tl.kind {
                TopLetKind::Const => {
                    format!("const {}: {} = {};", name.to_uppercase(), ty_str, val_str)
                }
                TopLetKind::Lazy => {
                    ctx.lazy_vars.insert(tl.var);
                    format!("static {}: std::sync::LazyLock<{}> = std::sync::LazyLock::new(|| {});", name.to_uppercase(), ty_str, val_str)
                }
            }
        } else {
            format!("const {} = {};", name, val_str)
        };
        parts.push(rendered);
    }

    // Functions
    for func in &program.functions {
        parts.push(render_function(&ctx, func));
    }

    parts.join("\n\n")
}

fn render_type_decl(ctx: &RenderContext, td: &IrTypeDecl) -> String {
    // Build generics string e.g. "<T>" or "<T, U>"
    let generics_str = if let Some(generics) = &td.generics {
        if generics.is_empty() {
            String::new()
        } else if ctx.is_rust() {
            let params = generics.iter().map(|g| format!("{}: Clone", g.name)).collect::<Vec<_>>().join(", ");
            format!("<{}>", params)
        } else {
            let params = generics.iter().map(|g| g.name.clone()).collect::<Vec<_>>().join(", ");
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
                            // Box recursive references (field type contains the enum name)
                            if ctx.is_rust() && ty_contains_name(t, &td.name) {
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
                                let boxed = if ctx.is_rust() && ty_contains_name(&f.ty, &td.name) {
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
fn template_or(ctx: &RenderContext, construct: &str, attrs: &[&str], fallback: &str) -> String {
    let b = HashMap::new();
    ctx.templates.render(construct, None, attrs, &b)
        .unwrap_or_else(|| fallback.to_string())
}

// ── Rust-specific helpers ──

/// Check if a type contains a reference to a named type (for recursive Box detection).
fn ty_contains_name(ty: &Ty, name: &str) -> bool {
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
