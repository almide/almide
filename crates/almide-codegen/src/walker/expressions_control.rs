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

// ── render_expr arm extraction (cog>100 decomposition, pattern 2) ──
//
// The following helpers are 1:1 text-moves of the largest `render_expr`
// match arms. Each re-narrows `expr.kind` via `let-else` and returns the
// exact String the inline arm used to produce — no behavior change.

fn render_expr_var(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Var { id } = &expr.kind else { unreachable!() };
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
        let read = match info.storage {
            Tls::Cell => format!("{}.with(|c| c.get())", info.static_name),
            Tls::RcRefCell => format!("{}.with(|c| (**c.borrow()).clone())", info.static_name),
            Tls::Lazy { .. } => ctx.templates
                .render_with("deref_lazy", None, &[], &[("name", info.static_name.as_str())])
                .unwrap_or_else(|| info.static_name.clone()),
            Tls::Const => info.static_name.clone(),
        };
        // #617: a STATIC stores the RAW Bytes/Matrix shape (an `Rc` inside a
        // shared static is not Sync, and fan threads read globals). A BARE
        // read stays raw — borrow positions coerce against the raw runtime
        // signatures directly; OWNING positions go through `Clone{Var}` nodes
        // (CloneInsertion), whose render re-wraps into the RcCow shape.
        return read;
    }
    raw_name
}

fn render_expr_if(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::If { cond, then, else_ } = &expr.kind else { unreachable!() };
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

fn render_expr_block(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Block { stmts, expr: tail } = &expr.kind else { unreachable!() };
    let mut parts: Vec<String> = render_stmts(ctx, stmts).into_iter()
        .map(|s| terminate_stmt(ctx, s))
        .collect();
    if let Some(e) = tail {
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
    }) || tail.as_ref().map_or(false, |e| contains_loop_control(e));
    if has_control {
        format!("{{\n{}\n}}", indented_body)
    } else {
        ctx.templates.render_with("block_expr", None, &[], &[("body", indented_body.as_str())])
            .unwrap_or_else(|| format!("{{\n{}\n}}", indented_body))
    }
}

fn render_expr_for_in(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::ForIn { var, var_tuple, iterable, body } = &expr.kind else { unreachable!() };
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

/// Rewrite legacy AlmdRec{N} references to field-name-based names.
/// @inline_rust templates in dependency packages may contain hardcoded
/// AlmdRec0, AlmdRec1 etc. that no longer match the current naming.
/// Extract all AlmdRec{digits} tokens, then resolve each by matching
/// the constructor's field names against the anon_records map. Extracted
/// from `render_expr_inline_rust` (cog>30 decomposition): a `&mut String`
/// output param that's only written to, never read back to change its own
/// branching — safe to thread out.
fn rewrite_legacy_almd_rec_names(ctx: &RenderContext, out: &mut String) {
    if !out.contains("AlmdRec") { return; }
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
            *out = out.replace(legacy, struct_name);
        }
    }
}

fn render_expr_inline_rust(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::InlineRust { template, args } = &expr.kind else { unreachable!() };
    let mut out = template.clone();
    for (name, arg) in args {
        let rendered = render_expr(ctx, arg);
        let placeholder = format!("{{{}}}", name.as_str());
        out = out.replace(&placeholder, &rendered);
    }
    rewrite_legacy_almd_rec_names(ctx, &mut out);
    out
}

fn render_expr_call(ctx: &RenderContext, expr: &IrExpr) -> String {
    let (IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args }) = &expr.kind else { unreachable!() };
    match target {
        CallTarget::Module { module, func, .. } => {
            // Module calls: use template (TS/JS) or runtime function (Rust)
            let args_str = args.iter().map(|a| render_expr_owned(ctx, a)).collect::<Vec<_>>().join(", ");
            let mod_ident = module.replace('.', "_");
            let func_ident = func.replace('.', "_");
            let call = ctx.templates.render_with("module_call", None, &[], &[("module", mod_ident.as_str()), ("func", func_ident.as_str()), ("args", args_str.as_str())])
                .unwrap_or_else(|| {
                    format!("almide_rt_{}_{}({})", mod_ident, func_ident, args_str)
                });
            // #617: same raw-runtime-result → RcCow boundary as RuntimeCall —
            // but ONLY for STDLIB modules (native runtime signatures). A USER
            // module fn's generated signature already carries the mapped RcCow
            // types, so gluing it would double-wrap (E0283 in the nn repo).
            if almide_lang::stdlib_info::is_stdlib_module(module.as_str()) {
                rc_cow_result_glue(call, &expr.ty)
            } else {
                call
            }
        }
        _ => render_generic_call(ctx, target, args, &expr.ty)
    }
}

fn render_expr_list(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::List { elements } = &expr.kind else { unreachable!() };
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

fn render_expr_record(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Record { name, fields } = &expr.kind else { unreachable!() };
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
    if default_keys.is_empty() && std::env::var("ALMIDE_DEFAULTS_DEBUG").is_ok() {
        let all: Vec<&String> = ctx.ann.default_fields.keys().map(|(c, _)| c).collect();
        eprintln!("[defaults-miss] ctor={:?} known={:?}", ctor_name_str, all);
    }
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

fn render_expr_spread_record(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::SpreadRecord { base, fields } = &expr.kind else { unreachable!() };
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

/// `render_expr_unwrap`'s error-coercion-template-attr computation,
/// extracted verbatim (cog>30 decomposition) — `Option<&'static str>`
/// return, no state, safe to hoist out of the surrounding function.
/// For Result with a non-String error, choose the `map_err` variant the
/// template needs (the template decides whether/how to coerce —
/// target-agnostic). `None` for `Option` unwraps (no error to coerce).
fn unwrap_err_coerce_attr(ctx: &RenderContext, inner_ty: &Ty) -> Option<&'static str> {
    if inner_ty.is_option() { return None; }
    let (_, err_ty) = inner_ty.inner2()?;
    // When the source error type already matches the enclosing fn's
    // propagated error type, `?` carries it through unchanged — no
    // coercion. This preserves a custom variant error (e.g.
    // `Result[_, AppErr]` !-ed inside a fn that also returns
    // `Result[_, AppErr]`) instead of stringifying it via Debug (which
    // would need `From<String>` and fail to compile). #630
    let matches_fn_err = ctx.fn_err_ty.as_ref() == Some(err_ty);
    if matches_fn_err {
        None
    } else if matches!(err_ty, Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 && matches!(args[0], Ty::String)) {
        Some("map_err_join")
    } else if !matches!(err_ty, Ty::String) {
        Some("map_err_debug")
    } else {
        None
    }
}

fn render_expr_unwrap(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Unwrap { expr: inner } = &expr.kind else { unreachable!() };
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
        let err_coerce_attr = unwrap_err_coerce_attr(ctx, &inner.ty);
        let attrs: Vec<&str> = err_coerce_attr.into_iter().collect();
        ctx.templates.render_with("unwrap_expr", when_type, &attrs, &[("inner", s.as_str())])
            .unwrap_or_else(|| format!("({})?", s))
    }
}

fn render_expr_clone(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Clone { expr: inner } = &expr.kind else { unreachable!() };
    // Val-wrapped var: deref then clone to get T. Bind handler re-wraps in RcCow::new().
    if let IrExprKind::Var { id } = &inner.kind {
        if ctx.ann.is_rc_cow(id) {
            let var_name = ctx.var_name(*id).to_string();
            return format!("(*{}).clone()", var_name);
        }
        // #617: cloning a GLOBAL out of its raw static (Bytes/Matrix shapes)
        // produces the raw value — re-wrap into the RcCow value shape the
        // surrounding code stores. Locals are already RcCow (their clone is
        // the O(1) Rc bump) — no glue.
        if ctx.ann.global(*id).is_some() && rc_cow_needs_glue(&inner.ty) {
            let read = render_expr(ctx, inner);
            return rc_cow_result_glue(format!("{read}.clone()"), &inner.ty);
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

/// Shared-mut non-Copy var (`SharedMut`, Closure v2 P6): borrow through the
/// `RefCell` rather than the `.get()` clone a bare Var read would emit, so a
/// mutating call (`list.push(acc, …)` → `&mut *acc.borrow_mut()`) writes the
/// ONE shared cell the closure also holds. A shared read uses `&*acc.borrow()`
/// (no clone). Copy shared-mut vars stay on the `Cell` `.get()` path below.
/// Extracted from `render_expr_borrow` (cog>30 decomposition): `Some`
/// mirrors the original's early `return`, `None` falls through.
fn try_render_borrow_shared_mut(ctx: &RenderContext, inner: &IrExpr, mutable: bool) -> Option<String> {
    let IrExprKind::Var { id } = &inner.kind else { return None; };
    if !ctx.ann.is_shared_mut(id) { return None; }
    if almide_ir::top_let_storage::capture_copy_cell(&ctx.var_table.get(*id).ty) { return None; }
    let var_name = ctx.var_name(*id).to_string();
    Some(if mutable {
        // In-place mutation writes the one shared cell.
        format!("&mut *{}.borrow_mut()", var_name)
    } else {
        // A shared read borrows an owned snapshot (`.get()` clones the
        // cell's value). Unlike `&*x.borrow()`, this owned temporary has
        // no lifetime tie to `x`, so it is also safe in tail position
        // where `x` is a block-local (`let outer = () => { var a = …; …; a })`.
        format!("&{}.get()", var_name)
    })
}

/// If the borrowed operand is a Var referencing a fn param already emitted
/// as a reference (`&T`, `&[T]`, `&str` for a shared borrow; `&mut T` for a
/// mutable one), skip the outer `&`/`&mut` — Rust auto-reborrows when you
/// pass the naked var, so this avoids a `&&T`/`&mut &mut T` double-borrow.
/// Extracted from `render_expr_borrow`.
fn try_render_borrow_already_ref_param(ctx: &RenderContext, inner: &IrExpr, as_str: bool, mutable: bool) -> Option<String> {
    let IrExprKind::Var { id } = &inner.kind else { return None; };
    if !as_str && !mutable && ctx.ref_params.contains(id) {
        return Some(render_expr(ctx, inner));
    }
    if mutable && ctx.ref_mut_params.contains(id) {
        return Some(render_expr(ctx, inner));
    }
    None
}

fn render_expr_borrow(ctx: &RenderContext, expr: &IrExpr) -> String {
    let IrExprKind::Borrow { expr: inner, as_str, mutable } = &expr.kind else { unreachable!() };
    if let Some(rendered) = try_render_borrow_shared_mut(ctx, inner, *mutable) {
        return rendered;
    }
    if let Some(rendered) = try_render_borrow_already_ref_param(ctx, inner, *as_str, *mutable) {
        return rendered;
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
