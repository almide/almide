// Loop rendering consumes ownership decisions recorded by nanopasses.
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
        let vars_s = super::helpers::tuple_elems_join(&names);
        ctx.templates.render_with("for_tuple_destructure", None, &[], &[("vars", vars_s.as_str())])
            .unwrap_or_else(|| format!("({})", vars_s))
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
        // #1857: a `let`-bound range whose every read is a head was bound as
        // a bare `Range<i64>` (`try_render_bind_counting_range`); iterate a
        // clone of the two scalars so a second, nested, or captured head
        // reads the same bounds — the counting loop, never a Vec.
        IrExprKind::Var { id } if ctx.ann.range_counting_vars.contains(id) => {
            format!("{}.clone()", ctx.var_name(*id))
        }
        _ => {
            let base = render_expr(ctx, iterable);
            // List types: .iter().cloned() works for both AlmideRcCow<Vec<T>>
            // (via Deref) and plain Vec<T>, giving owned T values. A binder
            // the body only borrows (`borrowed_loop_vars`, #1673) skips the
            // per-element copy: `.iter()` binds `&T`.
            match &iterable.ty {
                Ty::Applied(TypeConstructorId::List, _) if ctx.ann.borrowed_loop_vars.contains(var) => format!("{}.iter()", base),
                Ty::Applied(TypeConstructorId::List, _) if ctx.ann.consumed_loop_vars.contains(var) => format!("{}.into_iter()", base),
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
