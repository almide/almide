// The thread boundary under ADR-0013's shared string buffer (design point 3,
// Koka's thread-shared flip). Every value a
// scoped thread can reach is marked before the threads start, so the count
// of each `AlmideStr` block it holds turns atomic and only those do. The
// values are the free variables of the threaded code: a fan block's arms, or
// the arguments of a runtime helper whose signature bounds `Send` (the
// runtime marks its own data arguments).

/// `v.almide_share();` for every variable `exprs` read from outside.
fn thread_share_marks(ctx: &RenderContext, exprs: &[&IrExpr]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = String::new();
    for e in exprs {
        for id in almide_ir::free_vars::free_vars(e, &std::collections::HashSet::new()) {
            if !seen.insert(id) { continue; }
            let ty = ctx.var_table.get(id).ty.clone();
            let var = IrExpr { kind: IrExprKind::Var { id }, ty, ..Default::default() };
            out.push_str(&format!("({}).almide_share(); ", render_expr(ctx, &var)));
        }
    }
    out
}

/// A runtime call that hands work to scoped threads, with its captured
/// variables marked first.
fn render_thread_crossing_call(ctx: &RenderContext, symbol: &Sym, args: &[IrExpr], call: String) -> String {
    if !crate::generated::runtime_fn_modes::crosses_threads(symbol.as_str()) {
        return call;
    }
    let marks = thread_share_marks(ctx, &args.iter().collect::<Vec<_>>());
    if marks.is_empty() { call } else { format!("{{ {marks}{call} }}") }
}
