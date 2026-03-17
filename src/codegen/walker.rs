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
use super::template::TemplateSet;

/// Render context: carries the variable table and indentation state.
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
}

impl<'a> RenderContext<'a> {
    pub fn new(templates: &'a TemplateSet, var_table: &'a VarTable) -> Self {
        Self { templates, var_table, indent: 0 }
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
        // Fallback for complex types not yet templated
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
            b.insert("value", value.clone());
            ctx.templates.render("string_literal", None, &[], &b)
                .unwrap_or_else(|| format!("\"{}\"", value))
        }
        IrExprKind::LitBool { value } => {
            let key = if *value { "bool_literal_true" } else { "bool_literal_false" };
            template_or(ctx, key, &[], &value.to_string())
        }
        IrExprKind::Unit => template_or(ctx, "unit_literal", &[], "()"),

        // ── Variables ──
        IrExprKind::Var { id } => ctx.var_name(*id).to_string(),
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
            let subj = render_expr(ctx, subject);
            let arms_str = arms.iter()
                .map(|arm| render_match_arm(ctx, arm))
                .collect::<Vec<_>>()
                .join("\n");
            let mut b = HashMap::new();
            b.insert("subject", subj);
            b.insert("arms", arms_str);
            ctx.templates.render("match_expr", None, &[], &b)
                .unwrap_or_else(|| format!("match {{ {} }}", arms_str))
        }

        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            let mut parts: Vec<String> = stmts.iter()
                .map(|s| render_stmt(ctx, s))
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
            let args_str = args.iter().map(|a| render_expr(ctx, a)).collect::<Vec<_>>().join(", ");
            let callee = match target {
                CallTarget::Named { name } => name.clone(),
                CallTarget::Module { module, func } => format!("{}_{}", module, func),
                CallTarget::Method { object, method } => {
                    format!("{}.{}", render_expr(ctx, object), method)
                }
                CallTarget::Computed { callee } => render_expr(ctx, callee),
            };
            let mut b = HashMap::new();
            b.insert("callee", callee);
            b.insert("args", args_str);
            ctx.templates.render("call_expr", None, &[], &b)
                .unwrap_or_else(|| format!("call(...)"))
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
            let fields_str = fields.iter()
                .map(|(k, v)| {
                    let mut b = HashMap::new();
                    b.insert("name", k.clone());
                    b.insert("value", render_expr(ctx, v));
                    ctx.templates.render("record_field", None, &[], &b)
                        .unwrap_or_else(|| format!("{}: {}", k, render_expr(ctx, v)))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut b = HashMap::new();
            b.insert("type_name", name.clone().unwrap_or_default());
            b.insert("fields", fields_str);
            ctx.templates.render("record_literal", None, &[], &b)
                .unwrap_or_else(|| format!("{{ {} }}", fields_str))
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
            let mut b = HashMap::new();
            b.insert("inner", render_expr(ctx, inner));
            ctx.templates.render("err_expr", None, &[], &b)
                .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
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
                    IrStringPart::Lit { value } => fmt_parts.push(value.clone()),
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

        // ── Fallback for not-yet-templated nodes ──
        _ => format!("/* TODO: {} */", std::any::type_name::<IrExprKind>()),
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
        IrPattern::Literal { expr } => render_expr(ctx, expr),
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
            let args_str = args.iter().map(|a| render_pattern(ctx, a)).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, args_str)
        }
        IrPattern::Tuple { elements } => {
            let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
            format!("({})", elems)
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let fields_str = fields.iter()
                .map(|f| match &f.pattern {
                    Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
                    None => f.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {} }}", name, fields_str)
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
        IrStmtKind::Expr { expr } => render_expr(ctx, expr),
        IrStmtKind::Comment { text } => format!("// {}", text),
        _ => format!("/* TODO: stmt */"),
    }
}

// ── Function rendering ──

pub fn render_function(ctx: &RenderContext, func: &IrFunction) -> String {
    let params_str = func.params.iter()
        .map(|p| {
            let mut b = HashMap::new();
            b.insert("name", p.name.clone());
            b.insert("type", render_type(ctx, &p.ty));
            ctx.templates.render("fn_param", None, &[], &b)
                .unwrap_or_else(|| format!("{}: {}", p.name, render_type(ctx, &p.ty)))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let body_str = render_expr(ctx, &func.body);
    let ret_str = render_type(ctx, &func.ret_ty);

    let mut b = HashMap::new();
    b.insert("name", func.name.clone());
    b.insert("params", params_str);
    b.insert("return_type", ret_str);
    b.insert("body", body_str);

    let construct = if func.is_effect { "effect_fn_decl" } else { "fn_decl" };
    ctx.templates.render(construct, None, &[], &b)
        .unwrap_or_else(|| format!("fn {}() {{ }}", func.name))
}

// ── Full program rendering ──

pub fn render_program(ctx: &RenderContext, program: &IrProgram) -> String {
    let mut parts = Vec::new();

    // Type declarations
    for td in &program.type_decls {
        parts.push(render_type_decl(ctx, td));
    }

    // Top-level lets
    for tl in &program.top_lets {
        let mut b = HashMap::new();
        b.insert("name", ctx.var_table.get(tl.var).name.clone());
        b.insert("type", render_type(ctx, &tl.ty));
        b.insert("value", render_expr(ctx, &tl.value));
        let rendered = ctx.templates.render("let_binding", None, &[], &b)
            .unwrap_or_else(|| format!("let _ = _;"));
        parts.push(rendered);
    }

    // Functions
    for func in &program.functions {
        parts.push(render_function(ctx, func));
    }

    parts.join("\n\n")
}

fn render_type_decl(ctx: &RenderContext, td: &IrTypeDecl) -> String {
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
            b.insert("name", td.name.clone());
            b.insert("fields", fields_str);
            ctx.templates.render("struct_decl", None, &[], &b)
                .unwrap_or_else(|| format!("struct {} {{ {} }}", td.name, fields_str))
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
                        let fields_str = fields.iter().map(|t| render_type(ctx, t)).collect::<Vec<_>>().join(", ");
                        let mut b = HashMap::new();
                        b.insert("name", v.name.clone());
                        b.insert("fields", fields_str);
                        ctx.templates.render("enum_variant", None, &[], &b)
                            .unwrap_or_else(|| format!("{}({})", v.name, fields_str))
                    }
                    IrVariantKind::Record { fields } => {
                        let fields_str = fields.iter()
                            .map(|f| format!("{}: {}", f.name, render_type(ctx, &f.ty)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{} {{ {} }}", v.name, fields_str)
                    }
                })
                .collect::<Vec<_>>()
                .join(",\n");
            let mut b = HashMap::new();
            b.insert("name", td.name.clone());
            b.insert("variants", variants_str);
            ctx.templates.render("enum_decl", None, &[], &b)
                .unwrap_or_else(|| format!("enum {} {{ {} }}", td.name, variants_str))
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
