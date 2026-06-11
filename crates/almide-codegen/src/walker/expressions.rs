//! Expression rendering: converts IrExpr nodes to target-specific code strings.

use almide_ir::*;
use almide_ir::annotations::VarStorage;
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

/// Mangle a type into the monomorphization suffix form (mirrors mono/utils.rs).
fn mangle_ty_for_mono(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Int8 => "Int8".into(),
        Ty::Int16 => "Int16".into(),
        Ty::Int32 => "Int32".into(),
        Ty::UInt8 => "UInt8".into(),
        Ty::UInt16 => "UInt16".into(),
        Ty::UInt32 => "UInt32".into(),
        Ty::UInt64 => "UInt64".into(),
        Ty::Float32 => "Float32".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Unit => "Unit".into(),
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else { format!("{}_{}", name, args.iter().map(mangle_ty_for_mono).collect::<Vec<_>>().join("_")) }
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 =>
            format!("List_{}", mangle_ty_for_mono(&args[0])),
        Ty::Applied(id, args) => {
            let name = format!("{:?}", id);
            if args.is_empty() { name } else {
                format!("{}_{}", name, args.iter().map(mangle_ty_for_mono).collect::<Vec<_>>().join("_"))
            }
        }
        _ => "Unknown".into(),
    }
}

/// Render an expression ensuring an owned value (not RcCow wrapper).
/// For RcCow vars, produces `(*var).clone()` to yield the unwrapped T.
/// Used at sites that need owned T: function args, record fields, concat operands.
pub(crate) fn render_expr_owned(ctx: &RenderContext, expr: &IrExpr) -> String {
    if let IrExprKind::Var { id } = &expr.kind {
        if ctx.ann.is_rc_cow(id) {
            return format!("(*{}).clone()", ctx.var_name(*id));
        }
    }
    render_expr(ctx, expr)
}

/// Render a closure literal (`move |params| body`). `annotate` adds explicit
/// param types: a BOXED closure (`Rc::new(move |k| …) as Rc<dyn Fn(String)>`)
/// needs them because the `as` cast does NOT back-infer the closure's param type
/// (`|k|` alone is E0282). A bare combinator-consumed lambda passes
/// `annotate = false` — a fused iterator adapter infers `&T` and an explicit `T`
/// would mismatch.
fn render_lambda(ctx: &RenderContext, params: &[(VarId, Ty)], body: &IrExpr, annotate: bool) -> String {
    let params_str = params.iter()
        .map(|(id, ty)| {
            let name = ctx.var_name(*id).to_string();
            if annotate && !ty.has_unresolved_deep() {
                format!("{}: {}", name, render_type(ctx, ty))
            } else {
                name
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut body_str = if let IrExprKind::Var { id } = &body.kind {
        if ctx.ann.is_rc_cow(id) {
            format!("(*{}).clone()", ctx.var_name(*id))
        } else { render_expr(ctx, body) }
    } else { render_expr(ctx, body) };
    // Nested (curried) lambda: box the returned closure as `Rc<dyn Fn>` with an
    // explicit cast so `Rc<concrete-closure>` coerces to the trait object the
    // outer closure's return type demands (E0271 without the cast).
    if matches!(&body.kind, IrExprKind::Lambda { .. }) {
        let wrapped = ctx.templates.render_with("box_wrap", None, &[], &[("inner", body_str.as_str())])
            .unwrap_or_else(|| format!("std::rc::Rc::new({})", body_str));
        let cast = super::helpers::render_type_rc_fn(ctx, &body.ty);
        body_str = format!("{} as {}", wrapped, cast);
    }
    ctx.templates.render_with("lambda_single", None, &[], &[("params", params_str.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| "|_| { }".to_string())
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
            // Non-finite floats (a const-fold can produce inf/NaN, e.g.
            // `1e300 * 1e300`) have no Rust literal form: `format!("{}", inf)`
            // is the bare identifier `inf`, which the `{value}f64` template
            // turns into the undefined `inff64` (E0425). Emit the associated
            // constant directly — it needs no numeric suffix and is valid for
            // both f64 and f32.
            if !value.is_finite() {
                let f32_suffix = matches!(expr.ty, Ty::Float32);
                let assoc = if f32_suffix { "f32" } else { "f64" };
                return if value.is_nan() {
                    format!("{}::NAN", assoc)
                } else if *value > 0.0 {
                    format!("{}::INFINITY", assoc)
                } else {
                    format!("{}::NEG_INFINITY", assoc)
                };
            }
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
            let raw_name = ctx.var_name(*id).to_string();
            // Shared-mut local (`Rc<Cell<T>>`): read the cell. (Closure v2, P3.)
            if ctx.ann.is_shared_mut(id) {
                return format!("{}.get()", raw_name);
            }
            // §4 Stage 2: a module global is decided by ONE alias-resolved
            // lookup — storage class AND emitted name come from the same
            // GlobalInfo the agreement verifier checks. This replaces the
            // former five-predicate chain (storage-by-name, lazy_vars,
            // lazy_top_let_names, const_top_let_vars, module_origin
            // prefixing), whose per-use re-derivation was the #486/#500
            // drift surface. It also removes the lazy_vars mid-emission
            // ordering hazard: a Lazy global ALWAYS derefs, regardless of
            // declaration/use render order.
            if let Some(info) = ctx.ann.global(*id) {
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                return match info.storage {
                    Tls::Cell => format!("{}.with(|c| c.get())", info.static_name),
                    Tls::RcRefCell => format!("{}.with(|c| (**c.borrow()).clone())", info.static_name),
                    Tls::Lazy { .. } => ctx.templates
                        .render_with("deref_lazy", None, &[], &[("name", info.static_name.as_str())])
                        .unwrap_or_else(|| info.static_name.clone()),
                    Tls::Const => info.static_name.clone(),
                };
            }
            raw_name
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
            // Rust's grammar forbids a bare struct literal in match-subject
            // position ("struct literals are not allowed here") — a record/
            // variant brace construction must be parenthesized (#490).
            let subj = if matches!(&subject.kind, IrExprKind::Record { name: Some(_), .. } | IrExprKind::SpreadRecord { .. }) {
                format!("({})", subj)
            } else {
                subj
            };
            let arms_raw = arms.iter()
                .map(|arm| render_match_arm(ctx, arm, &expr.ty, &subject.ty))
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
                _ => {
                    let base = render_expr(ctx, iterable);
                    // List types: .iter().cloned() works for both RcCow<Vec<T>>
                    // (via Deref) and plain Vec<T>, giving owned T values.
                    match &iterable.ty {
                        Ty::Applied(TypeConstructorId::List, _) => format!("{}.iter().cloned()", base),
                        _ => base,
                    }
                }
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
            // Rewrite legacy AlmdRec{N} references to field-name-based names.
            // @inline_rust templates in dependency packages may contain hardcoded
            // AlmdRec0, AlmdRec1 etc. that no longer match the current naming.
            // Extract all AlmdRec{digits} tokens, then resolve each by matching
            // the constructor's field names against the anon_records map.
            if out.contains("AlmdRec") {
                let mut legacy_names: Vec<String> = Vec::new();
                let bytes = out.as_bytes();
                let prefix = b"AlmdRec";
                let mut pos = 0;
                while pos + prefix.len() < bytes.len() {
                    if bytes[pos..].starts_with(prefix) {
                        let start = pos;
                        pos += prefix.len();
                        // Consume trailing digits
                        let digit_start = pos;
                        while pos < bytes.len() && bytes[pos].is_ascii_digit() { pos += 1; }
                        if pos > digit_start {
                            let name = std::str::from_utf8(&bytes[start..pos]).unwrap().to_string();
                            // Skip names that are already field-based (contain '_' after "AlmdRec")
                            if !name.contains('_') {
                                legacy_names.push(name);
                            }
                        }
                    } else {
                        pos += 1;
                    }
                }
                legacy_names.sort();
                legacy_names.dedup();
                for legacy in &legacy_names {
                    let matched = ctx.ann.anon_records.iter().find(|(field_names, _)| {
                        field_names.iter().all(|f| {
                            let pattern = format!("{} {{ {}: ", legacy, f);
                            let pattern2 = format!(", {}: ", f);
                            out.contains(&pattern) || out.contains(&pattern2)
                        })
                    });
                    if let Some((_, struct_name)) = matched {
                        out = out.replace(legacy, struct_name);
                    }
                }
            }
            out
        }

        // ── Pre-resolved runtime call (from @intrinsic / NormalizeRuntimeCalls) ──
        IrExprKind::RuntimeCall { symbol, args } => render_runtime_call(ctx, symbol, args),

        // ── Calls ──
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            match target {
                CallTarget::Module { module, func, .. } => {
                    // Module calls: use template (TS/JS) or runtime function (Rust)
                    let args_str = args.iter().map(|a| render_expr_owned(ctx, a)).collect::<Vec<_>>().join(", ");
                    let mod_ident = module.replace('.', "_");
                    let func_ident = func.replace('.', "_");
                    ctx.templates.render_with("module_call", None, &[], &[("module", mod_ident.as_str()), ("func", func_ident.as_str()), ("args", args_str.as_str())])
                        .unwrap_or_else(|| {
                            format!("almide_rt_{}_{}({})", mod_ident, func_ident, args_str)
                        })
                }
                _ => render_generic_call(ctx, target, args, &expr.ty)
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
                        // A `List[Fn]` empty literal must not render
                        // `Vec::<impl Fn>::new()` (E0562: impl Trait in path).
                        // Closures stored in a list are boxed to `Rc<dyn Fn>`
                        // (see the closure-boxing pass), so the turbofish element
                        // type must match.
                        if matches!(&ty, Ty::Fn { .. }) {
                            super::helpers::render_type_field_fn(ctx, &ty)
                        } else {
                            render_type(ctx, &ty)
                        }
                    }
                    _ => "_".into(),
                };
                if let Some(rendered) = ctx.templates.render_with("empty_list", None, &[], &[("inner_type", inner_ty.as_str())]) {
                    return rendered;
                }
            }
            let elems = elements.iter().map(|e| render_expr_owned(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("list_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| format!("[...]"))
        }

        IrExprKind::Record { name, fields } => {
            // Build field strings (explicit + defaults for missing)
            let ctor_name_str = name.as_ref().map(|s| s.as_str()).unwrap_or("");
            let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(k, _)| &**k).collect();
            let mut field_strs: Vec<String> = Vec::new();
            // Render explicit fields (owned: RcCow vars unwrapped to T)
            for (k, v) in fields.iter() {
                let mut val_str = render_expr_owned(ctx, v);
                // Box recursive fields (annotation is target-aware — empty for non-Rust)
                if let Some(cn) = name {
                    if ctx.ann.boxed_fields.contains(&(cn.to_string(), k.to_string())) {
                        val_str = format!("std::boxed::Box::new({})", val_str);
                    }
                }
                // A closure stored in a struct field is `Rc<dyn Fn>`; the
                // box-by-default pass already boxed the field value, so no
                // field-side `Rc::new` — that double-boxed it (this site had no
                // `RcWrap` guard at all).
                field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", k.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("{}: {}", k, val_str)));
            }
            // Fill in default fields that were not explicitly provided.
            // default_fields is keyed by both bare name ("Msg") and module-qualified
            // name ("dep_pkg.Msg"), so we try the exact ctor_name_str first.
            let mut default_keys: Vec<(String, String)> = ctx.ann.default_fields.keys()
                .filter(|(cn, _)| cn == ctor_name_str)
                .cloned()
                .collect();
            // `default_fields` is a HashMap, so `.keys()` iteration order is
            // per-process (RandomState). Sort the default fields we append so
            // the emitted struct literal is host-deterministic. Explicit fields
            // are already in IR order; defaults follow in (ctor, field) order.
            // Determinism Belt — Rust-target emit.
            default_keys.sort();
            for (_, field_name) in &default_keys {
                if explicit_names.contains(field_name.as_str()) { continue; }
                let Some(default_expr) = ctx.ann.default_fields.get(&(ctor_name_str.to_string(), field_name.clone())) else { continue; };
                let mut val_str = render_expr(ctx, default_expr);
                let needs_box = name.as_ref()
                    .map_or(false, |cn| ctx.ann.boxed_fields.contains(&(cn.to_string(), field_name.clone())));
                if needs_box { val_str = format!("std::boxed::Box::new({})", val_str); }
                field_strs.push(ctx.templates.render_with("record_field", None, &[], &[("name", field_name.as_str()), ("value", val_str.as_str())])
                    .unwrap_or_else(|| format!("{}: {}", field_name, val_str)));
            }
            let fields_str = field_strs.join(", ");
            // Resolve type name: explicit name, or from expr.ty
            // For record literals, use bare struct name (no generics — Rust infers them)
            // Strip module qualifier from names like "module.TypeName" → "TypeName"
            // since all modules are flattened into one file in generated Rust.
            let mut type_name = name.map(|n| {
                let s = n.as_str();
                s.rsplit('.').next().unwrap_or(s).to_string()
            }).unwrap_or_else(|| {
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
            let inner_s = render_expr_owned(ctx, inner);
            ctx.templates.render_with("some_expr", None, &[], &[("inner", inner_s.as_str())])
                .unwrap_or_else(|| format!("Some({})", inner_s))
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
            let inner_s = render_expr_owned(ctx, inner);
            // A bare `Ok(x)` is `Result<TyOf(x), _>` — the error type stays open
            // for rustc to infer from context. That fails (E0282) when the
            // surrounding call leaves E unconstrained, e.g.
            // `result.unwrap_or(ok(10), -1)`: `unwrap_or<T, E>` names E only in its
            // `Result<T, E>` parameter, so nothing pins it. The checker already
            // resolved the full Result type (`expr.ty`), so emit a turbofish that
            // carries it — defaulting a still-unconstrained error to String
            // (Almide's conventional error type; mirrors the render_type fix).
            if let Some((ok_s, err_s)) = result_turbofish_args(ctx, &expr.ty) {
                format!("Ok::<{}, {}>({})", ok_s, err_s, inner_s)
            } else {
                ctx.templates.render_with("ok_expr", None, &[], &[("inner", inner_s.as_str())])
                    .unwrap_or_else(|| format!("Ok({})", inner_s))
            }
        }
        IrExprKind::ResultErr { expr: inner } => {
            let inner_str = render_expr(ctx, inner);
            let construct = if matches!(&inner.ty, Ty::String) { "err_inner_string" } else { "err_inner_other" };
            ctx.templates.render_with(construct, None, &[], &[("inner", inner_str.as_str())])
                .or_else(|| ctx.templates.render_with("err_expr", None, &[], &[("inner", inner_str.as_str())]))
                .unwrap_or_else(|| format!("Err({})", render_expr(ctx, inner)))
        }

        // ── Lambda ──
        // A bare (combinator-consumed) lambda leaves its params UNANNOTATED — a
        // fused iterator adapter infers `&T`, and annotating `T` would mismatch.
        IrExprKind::Lambda { params, body, .. } => render_lambda(ctx, params, body, false),

        // ── String interpolation ──
        IrExprKind::StringInterp { parts } => render_string_interp(ctx, parts),

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
            let parts = elements.iter().map(|e| render_expr_owned(ctx, e)).collect::<Vec<_>>().join(", ");
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
            let base = ctx.templates.render_with("index_access", None, &[], &[("object", obj_str.as_str()), ("index", idx.as_str())])
                .unwrap_or_else(|| "idx[...]".into());
            if matches!(object.ty, Ty::Bytes) {
                format!("{} as i64", base)
            } else {
                base
            }
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
            // Render `AlmideMap::<K, V>::new()` with the key/value types from the
            // literal's resolved type, so an annotated empty map (`[:]: Map[K,V]`)
            // routed through `almide_repr` infers (native E0282 otherwise). When
            // the types are unknown they erase to `_` (inference fills them from
            // the surrounding context, as before this turbofish).
            let (key_ty, value_ty) = match &expr.ty {
                Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
                    (render_map_type_arg(ctx, &args[0]), render_map_type_arg(ctx, &args[1]))
                }
                _ => ("_".to_string(), "_".to_string()),
            };
            ctx.templates.render_with("empty_map", None, &[], &[("key_type", key_ty.as_str()), ("value_type", value_ty.as_str())])
                .unwrap_or_else(|| format!("AlmideMap::<{}, {}>::new()", key_ty, value_ty))
        }

        // ── SpreadRecord ──
        IrExprKind::SpreadRecord { base, fields } => {
            let mut base_str = render_expr(ctx, base);
            // Spread requires an owned value. If base is a LazyLock deref
            // (module top_let), clone to get owned.
            if let IrExprKind::Var { id } = &base.kind {
                // §4 Stage 2: a Lazy global derefs a LazyLock and every
                // cross-module use renders through an accessor — both need
                // an owned clone for the spread. Attribute equivalent of the
                // former (module_origin || lazy_vars) probe.
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                let needs_clone = ctx.ann.global_alias.contains_key(id)
                    || matches!(ctx.ann.global(*id).map(|i| i.storage), Some(Tls::Lazy { .. }));
                if needs_clone && !base_str.ends_with(".clone()") {
                    base_str = format!("{}.clone()", base_str);
                }
            }
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
            // Val-wrapped var: deref then clone to get T. Bind handler re-wraps in RcCow::new().
            if let IrExprKind::Var { id } = &inner.kind {
                if ctx.ann.is_rc_cow(id) {
                    let var_name = ctx.var_name(*id).to_string();
                    return format!("(*{}).clone()", var_name);
                }
            }
            let expr_s = render_expr(ctx, inner);
            // Special case: cloning a String-typed Var that's a fn param
            // (so it's actually emitted as `&str` in Rust). `.clone()` on
            // `&str` returns `&str`, not `String`. Use `.to_string()` so
            // the surrounding context (which expects an owned `String`)
            // type-checks.
            let is_borrowed_string_param = matches!(ctx.target, super::super::pass::Target::Rust)
                && matches!(inner.ty, Ty::String)
                && match &inner.kind {
                    IrExprKind::Var { id } => ctx.ref_params.contains(id),
                    _ => false,
                };
            if is_borrowed_string_param {
                return format!("{}.to_string()", expr_s);
            }
            ctx.templates.render_with("clone_expr", None, &[], &[("expr", expr_s.as_str())])
                .unwrap_or_else(|| format!("{}.clone()", expr_s))
        }
        IrExprKind::Deref { expr: inner } => {
            let name_s = render_expr(ctx, inner);
            ctx.templates.render_with("deref_var", None, &[], &[("name", name_s.as_str())])
                .unwrap_or_else(|| format!("(*{})", name_s))
        }
        IrExprKind::Borrow { expr: inner, as_str, mutable } => {
            // Shared-mut non-Copy var (`SharedMut`, Closure v2 P6): borrow through the
            // `RefCell` rather than the `.get()` clone a bare Var read would emit, so a
            // mutating call (`list.push(acc, …)` → `&mut *acc.borrow_mut()`) writes the
            // ONE shared cell the closure also holds. A shared read uses `&*acc.borrow()`
            // (no clone). Copy shared-mut vars stay on the `Cell` `.get()` path below.
            if let IrExprKind::Var { id } = &inner.kind {
                if ctx.ann.is_shared_mut(id)
                    && !almide_ir::top_let_storage::capture_copy_cell(&ctx.var_table.get(*id).ty)
                {
                    let var_name = ctx.var_name(*id).to_string();
                    return if *mutable {
                        // In-place mutation writes the one shared cell.
                        format!("&mut *{}.borrow_mut()", var_name)
                    } else {
                        // A shared read borrows an owned snapshot (`.get()` clones the
                        // cell's value). Unlike `&*x.borrow()`, this owned temporary has
                        // no lifetime tie to `x`, so it is also safe in tail position
                        // where `x` is a block-local (`let outer = () => { var a = …; …; a })`.
                        format!("&{}.get()", var_name)
                    };
                }
            }
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
                // Val-wrapped var: .make_mut() for COW semantics
                if let IrExprKind::Var { id } = &inner.kind {
                    if ctx.ann.is_rc_cow(id) {
                        let var_name = ctx.var_name(*id).to_string();
                        return format!("{}.make_mut()", var_name);
                    }
                }
                format!("&mut {}", render_expr(ctx, inner))
            } else if *as_str {
                // String literal → bare &str in Rust, skip .to_string() allocation
                if let IrExprKind::LitStr { value } = &inner.kind {
                    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"")
                        .replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r");
                    return format!("\"{}\"", escaped);
                }
                format!("&*{}", render_expr(ctx, inner))
            } else {
                format!("&{}", render_expr(ctx, inner))
            }
        }
        IrExprKind::BoxNew { expr: inner } => {
            format!("std::boxed::Box::new({})", render_expr(ctx, inner))
        }
        IrExprKind::RcWrap { expr: inner, cast_ty, wrap } => {
            // A BOXED closure literal needs annotated params (the `as` cast does
            // not back-infer them). Wrap in parens so a boxed-then-CALLED closure
            // `(Rc::new(..) as T)(args)` parses (a cast can't be followed by `(`).
            let s = if let IrExprKind::Lambda { params, body, .. } = &inner.kind {
                render_lambda(ctx, params, body, true)
            } else {
                render_expr(ctx, inner)
            };
            match wrap {
                // fan.race/any/settle thunk: `Box<dyn Fn + Send + Sync>` is itself
                // `Fn + Send + Sync`, so heterogeneous capturing thunks unify in the
                // runtime's `Vec<impl Fn() -> _ + Send + Sync>` (fixes E0308).
                almide_ir::FnBox::BoxSendSync => {
                    let ty = cast_ty.as_deref().expect("fan thunk RcWrap always carries a Fn cast_ty");
                    let box_type = super::helpers::render_type_box_fn(ctx, ty, "Send + Sync");
                    format!("(std::boxed::Box::new({}) as {})", s, box_type)
                }
                almide_ir::FnBox::Rc => {
                    if let Some(ty) = cast_ty {
                        let rc_type = super::helpers::render_type_rc_fn(ctx, ty);
                        format!("(std::rc::Rc::new({}) as {})", s, rc_type)
                    } else {
                        format!("std::rc::Rc::new({})", s)
                    }
                }
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
        IrExprKind::Fan { exprs } => render_fan(ctx, exprs),

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
            // Unwrap RcCow operands to owned T for concat
            let lo = render_expr_owned(ctx, left);
            let ro = render_expr_owned(ctx, right);
            ctx.templates.render_with("concat_expr", Some(ty_tag), &[], &[("left", lo.as_str()), ("right", ro.as_str())])
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
        // Integer `/` and `%` are total: a zero divisor or signed MIN/-1 overflow
        // aborts via the almide_div!/almide_mod! prelude macros (`Error: <msg>\n` +
        // exit 1) instead of panicking. The macro is generic over all int widths and
        // is not const-evaluable, so a literal `10 / 0` compiles (no rustc
        // `unconditional_panic`) and aborts at runtime — matching the WASM trap.
        // Float div/mod keep IEEE semantics and fall through to the bare-operator arm.
        BinOp::DivInt => {
            ctx.templates.render_with("div_int", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_div!({}, {})", l, r))
        }
        BinOp::ModInt => {
            ctx.templates.render_with("mod_int", None, &[], &[("left", l.as_str()), ("right", r.as_str())])
                .unwrap_or_else(|| format!("almide_mod!({}, {})", l, r))
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
                // DivInt/ModInt are matched above (totality macros) and must never
                // reach this bare-operator fallback — fall to "??" loudly if they do.
                BinOp::DivFloat => "/",
                BinOp::ModFloat => "%",
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
fn render_generic_call(ctx: &RenderContext, target: &CallTarget, args: &[IrExpr], result_ty: &almide_lang::types::Ty) -> String {
    let callee = match target {
        CallTarget::Named { name } => {
            // Invariant: NormalizeRuntimeCallsPass collapses every
            // `Named { "almide_rt_*" }` into `RuntimeCall { symbol }`.
            // A `Named` target reaching the walker therefore must
            // refer to a user-defined or external function — never
            // a runtime helper. If this assertion fires, a generator
            // produced a `Named { "almide_rt_*" }` after the
            // normalize pass, or the pass was removed from the
            // pipeline.
            assert!(
                !name.as_str().starts_with("almide_rt_"),
                "walker received Named call with reserved runtime prefix: {} \
                 (expected RuntimeCall — see pass_normalize_runtime_calls)",
                name.as_str()
            );
            if let Some(mapped) = ctx.ann.ctor_to_enum.get(name.as_str()) {
                // `name` is a variant constructor. The global `ctor_to_enum` map
                // collapses a constructor name shared across packages to the
                // last-registered enum (#413). When the construction's RESOLVED
                // type (`.ty`, disambiguated by the type checker) names a DIFFERENT
                // but valid enum, prefer it; otherwise keep the mapped enum (no
                // change for the common, non-colliding case — and for non-variant
                // ctors like newtypes where `.ty` isn't a known enum).
                let enum_name = match result_ty {
                    almide_lang::types::Ty::Named(n, _)
                        if n.as_str() != mapped.as_str()
                           && ctx.ann.ctor_to_enum.values().any(|e| e.as_str() == n.as_str())
                        => n.to_string(),
                    _ => mapped.clone(),
                };
                return render_enum_constructor(ctx, name, &enum_name, args);
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
            // Val-wrapped var: mutating method calls need .make_mut()
            if let IrExprKind::Var { id } = &object.kind {
                if ctx.ann.is_rc_cow(id) {
                    let is_mutating_method = matches!(method.as_str(),
                        "push" | "pop" | "clear" | "extend" | "insert" | "remove"
                        | "sort" | "sort_by" | "reverse" | "truncate" | "retain");
                    if is_mutating_method {
                        let var_name = ctx.var_name(*id).to_string();
                        let args_str = args.iter().map(|a| render_expr(ctx, a))
                            .collect::<Vec<_>>().join(", ");
                        if args_str.is_empty() {
                            return format!("{}.make_mut().{}()", var_name, method);
                        } else {
                            return format!("{}.make_mut().{}({})", var_name, method, args_str);
                        }
                    }
                }
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
            } else {
                // Parenthesize ANY computed callee: a `Member` (h.run)("x"), a
                // `??`/`match`/`if` that yields a closure — `match … {}(x)` is
                // invalid Rust ("expected ;"), `(match … {})(x)` is correct — and a
                // bare `(f)(x)` is harmless.
                format!("({})", render_expr(ctx, callee))
            }
        }
        CallTarget::Module { .. } => unreachable!(),
    };
    // A closure ARG is already `Rc<dyn Fn>`: the box-by-default pass boxed every
    // closure literal where it sits — including a Named user-HOF arg, whose param
    // is `Rc<dyn Fn>` under the uniform repr (top-level fns no longer take
    // `impl Fn`). No call-site boxing here — re-wrapping a capture-clone
    // `{ let __cap; lambda }` arg (kind `Block`, not `RcWrap`) double-boxed it.
    let args_str = args.iter().map(|a| render_expr_owned(ctx, a))
        .collect::<Vec<_>>().join(", ");
    ctx.templates.render_with("call_expr", None, &[], &[("callee", callee.as_str()), ("args", args_str.as_str())])
        .unwrap_or_else(|| format!("call(...)"))
}

/// Render a method call as a full expression for UFCS and module.func patterns.
/// Returns Some(full_expr) if the method call was handled, None for normal obj.method calls.
///
/// Dispatch strategy (type-driven):
///   1. join fallback (StdlibLowering miss)
///   2. Dot-qualified → module/convention
///   3. User-defined type (Ty::Named) → ALWAYS UFCS free function
///   4. Builtin type + native Rust method → None (emit obj.method())
///   5. Builtin type + non-native method → UFCS (user fn on builtin type)
fn render_method_call_full(ctx: &RenderContext, object: &IrExpr, method: &str, args: &[IrExpr]) -> Option<String> {
    // 1. join fallback: route to almide_rt_list_join when StdlibLowering missed it
    if method == "join" && args.len() == 1 {
        let obj_str = render_expr(ctx, object);
        let sep_str = render_expr(ctx, &args[0]);
        return Some(format!("almide_rt_list_join(&{}, &*{})", obj_str, sep_str));
    }

    // 2. Dot-qualified: handled after type-based dispatch (below)

    // 3. User-defined types: ALWAYS UFCS — Rust structs have no methods.
    //    Derive monomorphized name from type args when generic.
    if let Ty::Named(_type_name, type_args) = &object.ty {
        if !method.contains('.') {
            let obj_str = render_expr_owned(ctx, object);
            let mut all_args = vec![obj_str];
            all_args.extend(args.iter().map(|a| render_expr_owned(ctx, a)));
            let func_name = if type_args.is_empty() {
                method.to_string()
            } else {
                let suffix = type_args.iter()
                    .map(|t| mangle_ty_for_mono(t))
                    .collect::<Vec<_>>().join("_");
                format!("{}__{}", method, suffix)
            };
            return Some(format!("{}({})", func_name, all_args.join(", ")));
        }
    }

    // 4+5. Builtin types: distinguish native Rust methods from user UFCS.
    //    Native methods exist on the Rust type and should remain as obj.method().
    //    Non-native methods are user-defined UFCS → func(obj, args).
    let is_native_rust_method = matches!(method,
        // Universal traits (Clone, Display)
        "clone" | "to_string"
        // Vec<T>
        | "len" | "push" | "pop" | "insert" | "remove" | "contains"
        | "iter" | "into_iter" | "collect" | "is_empty" | "to_vec" | "get"
        // HashMap<K,V>
        | "keys" | "values" | "contains_key" | "entry" | "or_insert"
        // String
        | "split" | "trim" | "starts_with" | "ends_with" | "replace" | "chars"
        | "to_owned" | "as_str"
        // Option<T> / Result<T,E>
        | "is_some" | "is_none" | "unwrap" | "unwrap_or" | "expect"
        | "ok" | "err" | "and_then" | "map_err" | "unwrap_or_else"
        | "ok_or" | "flatten" | "as_ref" | "as_deref"
        // Iterator adapters (from collect/filter chains)
        | "map" | "filter"
        // f64 math
        | "abs" | "powi" | "powf" | "sqrt" | "floor" | "ceil" | "round"
        | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
        | "exp" | "ln" | "log2" | "log10" | "is_nan" | "is_infinite"
    );

    if !method.contains('.') && !is_native_rust_method {
        // 5. Non-native method on builtin type → UFCS
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
/// For a `Result<Ok, Err>` type, render the `(ok, err)` turbofish arguments used
/// to pin a `Ok(..)` / `Err(..)` constructor's phantom type parameters. The error
/// type defaults to `String` when still unresolved (`Unknown` or an inference
/// typevar), exactly as the `render_type` Result arm does for type positions
/// (dv_13). Returns `None` for a non-Result type, leaving the bare constructor.
fn result_turbofish_args(ctx: &RenderContext, ty: &Ty) -> Option<(String, String)> {
    match ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            let ok_s = render_type(ctx, &args[0]);
            let err_s = match &args[1] {
                Ty::Unknown => "String".to_string(),
                Ty::TypeVar(n) if n.starts_with('?') => "String".to_string(),
                _ => render_type(ctx, &args[1]),
            };
            Some((ok_s, err_s))
        }
        _ => None,
    }
}

fn render_enum_constructor(ctx: &RenderContext, ctor_name: &str, enum_name: &str, args: &[IrExpr]) -> String {
    let boxed_args: Vec<String> = args.iter().enumerate().map(|(i, a)| {
        let rendered = render_expr(ctx, a);
        let needs_box = ctx.ann.recursive_enums.contains(enum_name)
            && (ty_contains_name(&a.ty, enum_name)
                || ctx.ann.boxed_fields.contains(&(ctor_name.to_string(), format!("{}", i))));
        // Unwrap RcCow for var bindings used as variant constructor args.
        let is_rc_cow_var = matches!(&a.kind, IrExprKind::Var { id } if ctx.ann.is_rc_cow(id));
        let rendered = if is_rc_cow_var {
            format!("{}.into_inner()", rendered)
        } else { rendered };
        // A closure payload field is `Rc<dyn Fn>`; the box-by-default pass already
        // boxed the closure value (a direct lambda → `RcWrap`, a capture-clone
        // `{ let __cap; lambda }` → boxes the tail, a `Var` is already `Rc`), so no
        // ctor-side boxing — wrapping again here double-boxed `Block`-shaped args.
        if needs_box {
            format!("std::boxed::Box::new({})", rendered)
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

// ── Extracted sub-functions (reduce render_expr complexity) ──

fn render_runtime_call(ctx: &RenderContext, symbol: &almide_base::intern::Sym, args: &[IrExpr]) -> String {
    // Inline numeric casts
    match symbol.as_str() {
        "almide_rt_float_from_int" | "almide_rt_int_to_float" if args.len() == 1 => {
            return format!("({} as f64)", render_expr(ctx, &args[0]));
        }
        "almide_rt_float_to_int" if args.len() == 1 => {
            return format!("({} as i64)", render_expr(ctx, &args[0]));
        }
        _ => {}
    }
    // Mutating stdlib calls on a module-level (`ModuleRc`) or `RcCow` var: route
    // through `Rc::make_mut`/`.make_mut()` so the mutation hits the shared backing
    // store, not a clone. The mutator set is the one source of truth in
    // `pass_closure_conversion` (list/map/string/bytes &mut-on-args[0] fns); before
    // it was only list push/pop/clear, so `map.insert`/`bytes.push`/… on a global
    // silently mutated a discarded `(**c.borrow()).clone()`.
    if !args.is_empty() {
        let is_mutating = crate::pass_closure_conversion::is_inplace_mutator(symbol.as_str());
        if is_mutating {
            if let IrExprKind::Borrow { expr: inner, .. } | IrExprKind::Clone { expr: inner } = &args[0].kind {
                if let IrExprKind::Var { id } = &inner.kind {
                    let name = ctx.var_name(*id).to_string();
                    // §4 Stage 2: a global target dispatches on the attribute.
                    if let Some(info) = ctx.ann.global(*id) {
                        use almide_ir::top_let_storage::TopLetStorage as Tls;
                        if matches!(info.storage, Tls::RcRefCell) {
                            let rest_args = args[1..].iter().map(|a| render_expr(ctx, a))
                                .collect::<Vec<_>>().join(", ");
                            let rc_mut = "std::rc::Rc::make_mut(&mut *c.borrow_mut())";
                            let call_args = if rest_args.is_empty() {
                                rc_mut.to_string()
                            } else {
                                format!("{}, {}", rc_mut, rest_args)
                            };
                            return format!("{}.with(|c| {}({}))", info.static_name, symbol.as_str(), call_args);
                        }
                    }
                    match ctx.ann.get_var_storage(id) {
                        VarStorage::RcCow => {
                            let rest_args = args[1..].iter().map(|a| render_expr(ctx, a))
                                .collect::<Vec<_>>().join(", ");
                            let call_args = if rest_args.is_empty() {
                                format!("{}.make_mut()", name)
                            } else {
                                format!("{}.make_mut(), {}", name, rest_args)
                            };
                            return format!("{}({})", symbol.as_str(), call_args);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    // Default: render all args as owned
    let args_str = args.iter().map(|a| render_expr_owned(ctx, a))
        .collect::<Vec<_>>().join(", ");
    format!("{}({})", symbol.as_str(), args_str)
}

/// A COMPOUND interpolation part — one whose value has no `Display` and must be
/// rendered via `AlmideRepr` to its Almide-literal form. Scalars (numbers,
/// `Bool`, `Unit`) and bare `String` keep their plain `{}` Display path so the
/// emitted code for existing programs is byte-identical.
///
/// Scope: the types backed by an `AlmideRepr` impl on BOTH targets, recursively
/// composed — the structural containers (`List`, `Map`, `Set`, `Option`,
/// `Result`, `Tuple`) plus user-defined records and variants. A record/variant
/// is identified by name via `ctx.repr_named_types` (the set of types that got a
/// generated `AlmideRepr` impl); an anonymous record (`Ty::Record`/`OpenRecord`)
/// is always repr-backed. Closure-bearing types are excluded from the set, so a
/// value with an `Fn` field stays on the Display path (it has no repr).
/// Render a key/value type for an empty-map turbofish, erasing unresolved
/// named typevars to `_` exactly like the empty-list `inner_type` path.
fn render_map_type_arg(ctx: &RenderContext, ty: &Ty) -> String {
    let ty = if ty_has_named_typevar(ty) { erase_named_typevars(ty.clone()) } else { ty.clone() };
    render_type(ctx, &ty)
}

fn ty_needs_repr(ctx: &RenderContext, ty: &Ty) -> bool {
    use TypeConstructorId::{List, Map, Set, Option as OptionId, Result as ResultId};
    match ty {
        // Backed container constructors → repr.
        Ty::Applied(id, _) => matches!(id, List | Map | Set | OptionId | ResultId),
        // Tuples → repr.
        Ty::Tuple(..) => true,
        // Anonymous records get an inline literal repr.
        Ty::Record { .. } | Ty::OpenRecord { .. } => true,
        // Named records/variants → repr only when a generated impl exists.
        Ty::Named(name, _) => ctx.repr_named_types.contains(name),
        // Everything else (scalars, String, Bool, Unit, Fn, Unknown, …) stays on
        // the Display path.
        _ => false,
    }
}

fn render_string_interp(ctx: &RenderContext, parts: &[IrStringPart]) -> String {
    let mut fmt_parts = Vec::new();
    let mut arg_parts = Vec::new();
    for part in parts {
        match part {
            IrStringPart::Lit { value } => {
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
                // A COMPOUND part (List/Map/Set/Tuple/Option/Result/record/variant)
                // has no `Display`; route it through the `AlmideRepr` trait so it
                // renders to its Almide-literal form (`[1, 2, 3]`, `["a": 1]`, …).
                // Bare String/Int/Float/Bool keep the plain `{}` Display path so
                // existing programs' emitted code is byte-identical. `almide_repr`
                // takes `&T` and has ref-forwarding impls, so `&(...)` is correct
                // whether the part renders to an owned value or an existing borrow.
                if ty_needs_repr(ctx, &expr.ty) {
                    arg_parts.push(format!("almide_repr(&({}))", render_expr(ctx, expr)));
                } else {
                    arg_parts.push(render_expr(ctx, expr));
                }
            }
        }
    }
    let format_str_s = fmt_parts.join("");
    let args_s = arg_parts.join(", ");
    let template_str_s = {
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
        .unwrap_or_else(|| "\"...\"".into())
}

fn render_fan(ctx: &RenderContext, exprs: &[IrExpr]) -> String {
    let rendered: Vec<String> = exprs.iter().map(|e| {
        let mut body = render_expr(ctx, e);
        if e.ty.is_result() && body.ends_with('?') { body.pop(); }
        body
    }).collect();
    let exprs_s = rendered.join(", ");
    let count_s = format!("{}", exprs.len());
    let handles: Vec<String> = (0..exprs.len()).map(|i| format!("__fan_h{}", i)).collect();
    let spawns: Vec<String> = rendered.iter().enumerate()
        .map(|(i, body)| format!("let {} = __s.spawn(move || {{ {} }});", handles[i], body))
        .collect();
    let any_result = exprs.iter().any(|e| e.ty.is_result());
    let joins: Vec<String> = exprs.iter().enumerate().map(|(i, e)| {
        if e.ty.is_result() {
            if ctx.auto_unwrap { format!("{}.join().unwrap()?", handles[i]) }
            else { format!("{}.join().unwrap().unwrap()", handles[i]) }
        } else { format!("{}.join().unwrap()", handles[i]) }
    }).collect();
    let join_expr = if joins.len() == 1 { joins[0].clone() }
        else { format!("({})", joins.join(", ")) };
    let spawns_s = spawns.join(" ");
    let construct = if any_result && ctx.auto_unwrap { "fan_effect" } else { "fan_expr" };
    ctx.templates.render_with(construct, None, &[], &[("exprs", exprs_s.as_str()), ("count", count_s.as_str()), ("spawns", spawns_s.as_str()), ("join_expr", join_expr.as_str())])
        .unwrap_or_else(|| format!("fan({})", rendered.join(", ")))
}
