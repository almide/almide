// The `fan { … }` block renderers: the threaded form, the one-arm form
// (#3341) and the inline form (#3459).

fn render_fan(ctx: &RenderContext, span: Option<almide_base::span::Span>, exprs: &[IrExpr]) -> String {
    // #3467: a Result arm's body propagates into the ARM's own error type (a
    // scoped arm's `!`s, ADR-0021 D1), as a closure body does.
    let rendered: Vec<String> = exprs.iter().map(|e| {
        let mut body = match e.ty.result_err_ty() {
            Some(err) => render_expr(&super::with_fn_err_ty(ctx, Some(err)), e),
            None => render_expr(ctx, e),
        };
        if e.ty.is_result() && body.ends_with('?') { body.pop(); }
        body
    }).collect();
    if let ([e], [body]) = (exprs, rendered.as_slice()) {
        return render_fan_single(ctx, e, body);
    }
    if ctx.ann.is_inline_fan(span) {
        return render_fan_inline(ctx, exprs, &rendered);
    }
    let exprs_s = rendered.join(", ");
    let count_s = format!("{}", exprs.len());
    let handles: Vec<String> = (0..exprs.len()).map(|i| format!("__almide_fan_h{}", i)).collect();
    let spawns: Vec<String> = rendered.iter().enumerate()
        .map(|(i, body)| format!("let {} = __almide_s.spawn(move || {{ let __almide_fan_e = almide_fan_enter(__almide_fan_g, {}); {} }});", handles[i], i, body))
        .collect();
    let any_result = exprs.iter().any(|e| e.ty.is_result());
    let joins: Vec<String> = exprs.iter().enumerate()
        .map(|(i, e)| fan_join_tail(ctx, e, format!("almide_fan_join({})", handles[i])))
        .collect();
    let join_expr = if joins.len() == 1 { joins[0].clone() }
        else { format!("({})", joins.join(", ")) };
    let spawns_s = spawns.join(" ");
    let construct = if any_result && ctx.auto_unwrap { "fan_effect" } else { "fan_expr" };
    let err_s = render_type(ctx, &fan_err_ty(ctx));
    ctx.templates.render_with(construct, None, &[], &[("exprs", exprs_s.as_str()), ("count", count_s.as_str()), ("spawns", spawns_s.as_str()), ("join_expr", join_expr.as_str()), ("err_ty", err_s.as_str())])
        .unwrap_or_else(|| format!("fan({})", rendered.join(", ")))
}

/// The error a fan block's join propagates into: the enclosing fn's (#3467:
/// a `-> Result[_, E]` fn's `E`, not always `String`).
fn fan_err_ty(ctx: &RenderContext) -> Ty {
    ctx.fn_err_ty.clone().unwrap_or(Ty::String)
}

/// The join's `?` / `.unwrap()` for a Result arm: a typed arm error joined
/// into a `String` channel takes its repr text, as a `!` there does (#2725).
fn fan_join_suffix(ctx: &RenderContext, e: &IrExpr) -> &'static str {
    match (e.ty.result_err_ty(), ctx.auto_unwrap) {
        (Some(err), true) if err != fan_err_ty(ctx) && fan_err_ty(ctx) == Ty::String => ".map_err(|e| almide_repr(&e))?",
        (Some(_), true) => "?",
        (Some(_), false) => ".unwrap()",
        _ => "",
    }
}

/// An arm's settled value with the join's `?` / `.unwrap()` for a Result arm.
fn fan_join_tail(ctx: &RenderContext, e: &IrExpr, settled: String) -> String {
    format!("{settled}{}", fan_join_suffix(ctx, e))
}

/// #3341: a one-arm fan runs its arm inline (`fan_single`), with the join's `?` / `.unwrap()`.
fn render_fan_single(ctx: &RenderContext, e: &IrExpr, body: &str) -> String {
    let tail = fan_join_suffix(ctx, e);
    let body = if tail.is_empty() { body.to_string() } else { format!("({body}){tail}") };
    ctx.templates.render_with("fan_single", None, &[], &[("body", body.as_str())]).unwrap_or(body)
}

/// #3459: a fan `FanLowering` marked inline (an arm captures an `Rc`-backed
/// value, which cannot move onto a spawned thread) runs its arms on the
/// calling thread, in arm order, each as the same `move` closure the threaded
/// form spawns, called in place, and every arm runs before the first join.
/// This is the sequential evaluation the threaded form reproduces (the wasm
/// leg's form): output in arm order, every arm runs, the lowest-index `Err`
/// is the block's. Only the overlap is given up.
fn render_fan_inline(ctx: &RenderContext, exprs: &[IrExpr], rendered: &[String]) -> String {
    let arms: Vec<String> = rendered.iter().enumerate()
        .map(|(i, body)| format!("let __almide_fan_r{i} = (move || {{ {body} }})();"))
        .collect();
    let joins: Vec<String> = exprs.iter().enumerate()
        .map(|(i, e)| fan_join_tail(ctx, e, format!("__almide_fan_r{i}")))
        .collect();
    let join_expr = format!("({})", joins.join(", "));
    let arms_s = arms.join(" ");
    ctx.templates.render_with("fan_inline", None, &[], &[("arms", arms_s.as_str()), ("join_expr", join_expr.as_str())])
        .unwrap_or_else(|| format!("{{ {arms_s} {join_expr} }}"))
}
