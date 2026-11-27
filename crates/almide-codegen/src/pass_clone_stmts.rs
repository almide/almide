// ── part of pass_clone.rs, include!-spliced (split for the 800-line budget) ──

/// Account an in-place-mutation `target` as a use, mirroring the +1 that
/// `SyntacticCounter::visit_stmt` recorded. Only `eligible` (last-use-move) vars
/// track `remaining`; `always`/move-by-default vars don't appear there.
fn count_target_use(target: VarId, eligible: &HashSet<VarId>, remaining: &mut HashMap<VarId, u32>) {
    if eligible.contains(&target) {
        if let Some(r) = remaining.get_mut(&target) {
            *r = r.saturating_sub(1);
        }
    }
}

pub(crate) fn insert_clone_stmts_live(stmts: Vec<IrStmt>, ctx: &mut CloneCtx) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones_live(value, ctx),
            },
            IrStmtKind::Assign { var, value } => super::pass_clone_places::insert_clones_reassign(var, value, ctx),
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones_live(expr, ctx) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones_live(cond, ctx), else_: insert_clones_live(else_, ctx),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones_live(value, ctx),
            },
            // In-place mutations: process the sub-exprs first (they may consume
            // vars), THEN account the target as a use of `target` itself —
            // `count_target_use` decrements `remaining[target]` to match the +1
            // that `SyntacticCounter::visit_stmt` added, keeping last-use tracking
            // consistent for any later use of `target`. The target binding is NOT
            // cloned/moved (the statement writes through it in place); this is a
            // pure counter decrement.
            IrStmtKind::IndexAssign { target, index, value } => {
                let index = insert_clones_live(index, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::IndexAssign { target, index, value }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let value = super::pass_clone_places::insert_clones_field_reassign(target, field, value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::FieldAssign { target, field, value }
            }
            // Value before key when the key is a plain var (see
            // `map_insert_value_first`): the key position becomes the var's
            // last use, so `m[w] = f(&w)` moves `w` instead of cloning it.
            IrStmtKind::MapInsert { target, key, value } if map_insert_value_first(&key, &value) => {
                let value = insert_clones_live(value, ctx);
                let key = insert_clones_live(key, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::MapInsert { target, key, value }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                let key = insert_clones_live(key, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::MapInsert { target, key, value }
            }
            // Default: recurse every expr child via the exhaustive `map_exprs`
            // chokepoint so no un-listed stmt kind (`ListSwap`/`ListReverse`/… —
            // which `count_syntactic` already counts) drops its expr subtree.
            other => IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| insert_clones_live(e, ctx))
                .kind,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
