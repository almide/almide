// The `fan { … }` block renderers: the threaded form, the one-arm form
// (#3341) and the inline form (#3459).

fn render_fan(ctx: &RenderContext, span: Option<almide_base::span::Span>, exprs: &[IrExpr]) -> String {
    let rendered: Vec<String> = exprs.iter().map(|e| {
        let mut body = render_expr(ctx, e);
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
    ctx.templates.render_with(construct, None, &[], &[("exprs", exprs_s.as_str()), ("count", count_s.as_str()), ("spawns", spawns_s.as_str()), ("join_expr", join_expr.as_str())])
        .unwrap_or_else(|| format!("fan({})", rendered.join(", ")))
}

/// An arm's settled value with the join's `?` / `.unwrap()` for a Result arm.
fn fan_join_tail(ctx: &RenderContext, e: &IrExpr, settled: String) -> String {
    match (e.ty.is_result(), ctx.auto_unwrap) {
        (true, true) => format!("{settled}?"),
        (true, false) => format!("{settled}.unwrap()"),
        _ => settled,
    }
}

/// #3341: a one-arm fan runs its arm inline (`fan_single`), with the join's `?` / `.unwrap()`.
fn render_fan_single(ctx: &RenderContext, e: &IrExpr, body: &str) -> String {
    let tail = match (e.ty.is_result(), ctx.auto_unwrap) { (true, true) => "?", (true, false) => ".unwrap()", _ => "" };
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
