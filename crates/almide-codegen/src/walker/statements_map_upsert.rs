// `m[k] = e(map.get_or(m, k, d))` as ONE keyed operation (the word-count
// update). Spelled literally it is a lookup (`get_or`) followed by an insert
// that looks the same key up again — two hashes, two probes, two key
// compares per update. When `e` is plain arithmetic over the one `get_or`
// read, the statement is rendered through the `map_upsert` template
// (`AlmideMap::upsert_with`): one probe finds the slot, `e` runs on its
// current value (or on `d` when the key is absent, which then appends), and
// the result is stored in place.
//
// Equivalence with the two-step form: `e` may only contain literals, plain
// variables other than `m` and `k`, unary/binary operators, and the single
// `get_or(m, k, d)` read (whose `d` obeys the same rule). Nothing in it can
// observe or change `m`, so running it before or after the probe cannot be
// told apart; an operator that aborts (`/ 0`) ends the process either way.
// The key is a plain variable — moved in, exactly as the insert moved it.
// Anything else (a call, a `!`, a second read of `m`, a key expression)
// keeps the literal two-step rendering.

/// The `upsert_with` rendering of a `MapInsert`, when the statement has the
/// shape above and the target's storage takes a plain method call.
fn try_render_map_upsert(ctx: &RenderContext, target: VarId, key: &IrExpr, value: &IrExpr) -> Option<String> {
    let key_id = upsert_key_var(key)?;
    let receiver = upsert_receiver(ctx, target)?;
    let mut default: Option<IrExpr> = None;
    let body = upsert_body(value, target, key_id, &mut default)?;
    let default = default?;
    let key_str = render_expr(ctx, key);
    let default_str = render_expr(ctx, &default);
    let body_str = render_expr(ctx, &body);
    ctx.templates.render_with("map_upsert", None, &[], &[
        ("target", receiver.as_str()),
        ("key", key_str.as_str()),
        ("default", default_str.as_str()),
        ("old", UPSERT_OLD),
        ("value", body_str.as_str()),
    ])
}

/// The name the rendered update closure binds the slot's current value to.
const UPSERT_OLD: &str = "__almide_old";

/// `k` / `k.clone()` — a key that is a plain variable.
fn upsert_key_var(key: &IrExpr) -> Option<VarId> {
    match &key.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Clone { expr } => match &expr.kind {
            IrExprKind::Var { id } => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// The receiver the update is called on: the binding itself, or its
/// copy-on-write handle's `make_mut()`. A shared cell or a global keeps the
/// two-step form (their insert goes through a borrow guard).
fn upsert_receiver(ctx: &RenderContext, target: VarId) -> Option<String> {
    if ctx.ann.is_shared_mut(&target) || ctx.ann.global(target).is_some() {
        return None;
    }
    let name = ctx.var_name(target).to_string();
    Some(match ctx.ann.get_var_storage(&target) {
        VarStorage::RcCow => format!("{name}.make_mut()"),
        _ => name,
    })
}

/// A read of variable `id`, through any `&` / clone the passes wrapped it in.
fn reads_var(e: &IrExpr, id: VarId) -> bool {
    match &e.kind {
        IrExprKind::Var { id: v } => *v == id,
        IrExprKind::Borrow { expr, .. } | IrExprKind::Clone { expr } => reads_var(expr, id),
        _ => false,
    }
}

/// `value` with its one `get_or(target, key, d)` replaced by the closure
/// parameter; `d` is moved out into `default`. `None` when `value` is not
/// the admitted shape (see the header).
fn upsert_body(value: &IrExpr, target: VarId, key: VarId, default: &mut Option<IrExpr>) -> Option<IrExpr> {
    let kind = match &value.kind {
        IrExprKind::RuntimeCall { symbol, args } if symbol.as_str() == "almide_rt_map_get_or" => {
            let [m, k, d] = args.as_slice() else { return None };
            if default.is_some() || !reads_var(m, target) || !reads_var(k, key) {
                return None;
            }
            *default = Some(upsert_operand(d, target, key)?);
            IrExprKind::InlineRust { template: UPSERT_OLD.to_string(), args: Vec::new() }
        }
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op: *op,
            left: Box::new(upsert_body(left, target, key, default)?),
            right: Box::new(upsert_body(right, target, key, default)?),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op: *op,
            operand: Box::new(upsert_body(operand, target, key, default)?),
        },
        _ => return upsert_operand(value, target, key),
    };
    Some(IrExpr { kind, ty: value.ty.clone(), span: value.span, def_id: value.def_id })
}

/// A leaf of the update: a literal, or a variable that is neither the map
/// nor the key.
fn upsert_operand(e: &IrExpr, target: VarId, key: VarId) -> Option<IrExpr> {
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } => Some(e.clone()),
        IrExprKind::Var { id } if *id != target && *id != key => Some(e.clone()),
        _ => None,
    }
}
