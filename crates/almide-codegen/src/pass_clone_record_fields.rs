//! A final record rebuild can move distinct fields of an owned input.
use std::collections::{HashMap, HashSet};
use almide_base::Sym;
use almide_ir::*;
use super::pass_clone::{CloneCtx, insert_clones_live};
use super::use_kind::{ExplicitBorrows, Site, UseSites};

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
        *unique && (!ctx.in_loop || (ctx.fresh.contains(id) && !ctx.loops.binders.contains(id)))
            && ctx.owned.contains(id) && !ctx.always.contains(id)
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

/// `{ ...b, f: g(b.f), n: b.n + 1 }` where `b` is an owned value whose last
/// use is this update (#3404): move `b.f` into `g` instead of cloning it.
///
/// Rust evaluates the field initializers first and then takes the fields the
/// literal does not name from the base, so a partial move of `b.f` is legal
/// when (1) the base itself moves (the walk leaves it a bare `Var` — `b` is
/// owned, not captured, and this is its last use), (2) `f` is one of the
/// fields the literal names, so the base never supplies it, (3) `b.f` is read
/// exactly once across the initializers, and (4) every other occurrence of
/// `b` there is a field read too — a whole-`b` read after the move is E0382.
/// No occurrence may sit under a closure, a loop or an iterator chain (it
/// would repeat) or a `&mut`. Before this the update copied the whole field
/// on every call, which made `b = add(b, i)` growth loops quadratic.
pub(super) fn rewrite_spread(base: IrExpr, fields: Vec<(Sym, IrExpr)>, ctx: &mut CloneCtx) -> IrExprKind {
    let movable = spread_movable_fields(&base, &fields, ctx);
    // Fields are evaluated before the spread base in Rust struct literals.
    let mut new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, ctx))).collect();
    let new_base = insert_clones_live(base, ctx);
    if let Some((id, movable)) = movable
        && matches!(new_base.kind, IrExprKind::Var { id: b } if b == id)
    {
        for (_, v) in &mut new_fields {
            strip_field_clones(v, id, &movable);
        }
    }
    IrExprKind::SpreadRecord { base: Box::new(new_base), fields: new_fields }
}

/// `x = { ...x, f: g(x.f), n: x.n + 1 }` (#4261): the update is the rhs of
/// `x`'s own reassignment, so its base read is `x`'s last use before the
/// statement overwrites it — even inside a loop, where the liveness walk sees
/// the next iteration's read and clones the base (the whole record, every
/// collection in it) and every field read off it. When every occurrence of
/// `x` in the initializers is a plain field read (the rule
/// [`rewrite_spread`] applies to a dying base), the base moves and each
/// named field read exactly once moves into its initializer:
/// `r = { ...r, xs: r.xs + [i] }` appends to the moved `xs` in place, as
/// the plain-var `xs = xs + [i]` does. `None`: not that shape.
pub(super) fn rewrite_spread_reassign(var: VarId, value: &IrExpr, ctx: &mut CloneCtx) -> Option<IrExpr> {
    let IrExprKind::SpreadRecord { base, fields } = &value.kind else { return None };
    if !matches!(base.kind, IrExprKind::Var { id } if id == var) {
        return None;
    }
    let movable = movable_fields_of(var, fields, ctx)?;
    let IrExprKind::SpreadRecord { base, fields } = value.clone().kind else { unreachable!() };
    let mut new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, ctx))).collect();
    for (_, v) in &mut new_fields {
        strip_field_clones(v, var, &movable);
    }
    // The walk still visits the base (it counts `x`'s remaining uses), but
    // the base stays the bare var: the reassignment overwrites it right after.
    let walked = insert_clones_live(*base, ctx);
    let base = match walked.kind {
        IrExprKind::Clone { expr } => expr,
        _ => Box::new(walked),
    };
    let kind = IrExprKind::SpreadRecord { base, fields: new_fields };
    Some(IrExpr { kind, ty: value.ty.clone(), span: value.span, def_id: value.def_id })
}

fn spread_movable_fields(base: &IrExpr, fields: &[(Sym, IrExpr)], ctx: &CloneCtx) -> Option<(VarId, HashSet<Sym>)> {
    let IrExprKind::Var { id } = base.kind else { return None };
    if ctx.loops.binders.contains(&id) {
        return None;
    }
    let movable = movable_fields_of(id, fields, ctx)?;
    (!movable.is_empty()).then_some((id, movable))
}

/// The named fields of `{ ...id, … }` whose single plain read `id.f` may move
/// once the base moves — `None` when `id` is not an owned, uncaptured local or
/// some occurrence of it in the initializers is not a plain field read.
fn movable_fields_of(id: VarId, fields: &[(Sym, IrExpr)], ctx: &CloneCtx) -> Option<HashSet<Sym>> {
    if !ctx.owned.contains(&id) || ctx.always.contains(&id) || ctx.captured.contains(&id) {
        return None;
    }
    let mut reads: HashMap<Sym, u32> = HashMap::new();
    for (_, e) in fields {
        let all_plain_field_reads = UseSites::of_expr(e, Site::Result, &ExplicitBorrows).of(id).all(|u| {
            u.site == Site::Member && u.depth == 0 && !u.in_chain && !u.in_loop && !u.in_mut && !u.in_guard
        });
        if !all_plain_field_reads {
            return None;
        }
        count_field_reads(e, id, &mut reads);
    }
    let named: HashSet<Sym> = fields.iter().map(|(k, _)| *k).collect();
    Some(reads.into_iter()
        .filter(|(f, n)| *n == 1 && named.contains(f))
        .map(|(f, _)| f)
        .collect())
}

/// How many `b.f` nodes (the field read directly off the variable) each
/// field `f` has in `e`. A deeper read `b.f.g` counts as a read of `f`.
fn count_field_reads(e: &IrExpr, id: VarId, reads: &mut HashMap<Sym, u32>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Count<'a> { id: VarId, reads: &'a mut HashMap<Sym, u32> }
    impl IrVisitor for Count<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let Some((v, f)) = projection(e) && v == self.id {
                *self.reads.entry(f).or_insert(0) += 1;
            }
            walk_expr(self, e);
        }
    }
    Count { id, reads }.visit_expr(e);
}

/// `Clone(b.f)` → `b.f` for every `f` in `movable`.
fn strip_field_clones(e: &mut IrExpr, id: VarId, movable: &HashSet<Sym>) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    struct Strip<'a> { id: VarId, movable: &'a HashSet<Sym> }
    impl IrMutVisitor for Strip<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if let IrExprKind::Clone { expr } = &mut e.kind
                && projection(expr).is_some_and(|(v, f)| v == self.id && self.movable.contains(&f))
            {
                let inner = std::mem::replace(expr.as_mut(), IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None, def_id: None });
                *e = inner;
                return;
            }
            walk_expr_mut(self, e);
        }
    }
    Strip { id, movable }.visit_expr_mut(e);
}
