//! A final record rebuild can move distinct fields of an owned input.
use std::collections::{HashMap, HashSet};
use almide_base::Sym;
use almide_ir::*;
use super::pass_clone::{CloneCtx, insert_clones_live};

fn projection(e: &IrExpr) -> Option<(VarId, Sym)> {
    let IrExprKind::Member { object, field } = &e.kind else { return None; };
    let IrExprKind::Var { id } = object.kind else { return None; };
    Some((id, *field))
}

pub(super) fn rewrite(fields: Vec<(Sym, IrExpr)>, ctx: &mut CloneCtx) -> Vec<(Sym, IrExpr)> {
    let mut candidates: HashMap<VarId, (HashSet<Sym>, bool)> = HashMap::new();
    for (_, value) in &fields {
        if let Some((id, field)) = projection(value) {
            let (seen, unique) = candidates.entry(id).or_insert_with(|| (HashSet::new(), true));
            *unique &= seen.insert(field);
        }
    }
    candidates.retain(|id, (seen, unique)| {
        *unique && !ctx.in_loop && ctx.owned.contains(id) && !ctx.always.contains(id)
            && ctx.remaining.get(id).copied().unwrap_or(1) <= seen.len() as u32
            && fields.iter().all(|(_, e)| projection(e).is_some_and(|(v, _)| v == *id)
                || !almide_ir::free_vars::free_vars(e, &HashSet::new()).contains(id))
    });
    fields.into_iter().map(|(field, value)| {
        let transfer = projection(&value).is_some_and(|(id, _)| candidates.contains_key(&id));
        let value = insert_clones_live(value, ctx);
        let value = if transfer {
            match value.kind {
                IrExprKind::Clone { expr } => *expr,
                _ => value,
            }
        } else { value };
        (field, value)
    }).collect()
}
