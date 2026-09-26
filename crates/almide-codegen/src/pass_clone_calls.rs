//! Call argument borrow lifetimes during clone insertion.
use super::*;

/// The E0505 guard's borrow scan (#809/#866): the vars passed BY BORROW at
/// the top level of a call's arguments (or its method receiver). Such a var
/// stays borrowed until the call itself executes, so a MOVE of it anywhere in
/// a SIBLING argument conflicts — rustc's borrow live-range, not the flat
/// last-use count, is the authority INSIDE one call.
fn call_borrowed_vars(args: &[IrExpr], target: Option<&CallTarget>) -> HashSet<VarId> {
    let mut borrowed: HashSet<VarId> = HashSet::new();
    for a in args {
        if let IrExprKind::Borrow { expr, .. } = &a.kind {
            // `&x`, and `&*x` of a box-deref'd pattern binder (#2582): the
            // Borrow arm strips a clone under either, so both stay borrowed
            // until the call runs.
            let expr = match &expr.kind { IrExprKind::Deref { expr: d } => d, _ => expr };
            if let IrExprKind::Var { id } = &expr.kind {
                borrowed.insert(*id);
            }
        }
    }
    if let Some(CallTarget::Method { object, .. }) = target {
        if let IrExprKind::Borrow { expr, .. } = &object.kind {
            if let IrExprKind::Var { id } = &expr.kind {
                borrowed.insert(*id);
            }
        }
    }
    borrowed
}

/// `RuntimeCall { symbol, args }` arm of [`insert_clones_live`]: the same
/// E0505 guard as [`insert_clones_call`]. An intrinsic-lowered stdlib call
/// (`map.fold` → `almide_rt_map_fold`) reaches this pass as a `RuntimeCall`,
/// and its borrowed subject conflicts with a sibling-arg move exactly the
/// same way — `map.fold(acc, (if … else acc), λ)` moved `acc` in the seed
/// while `&acc` from the subject argument was still live (#866).
pub(super) fn insert_clones_runtime_call(args: Vec<IrExpr>, ctx: &mut CloneCtx) -> Vec<IrExpr> {
    let borrowed = call_borrowed_vars(&args, None);
    if borrowed.is_empty() {
        return args.into_iter().map(|a| insert_clones_live(a, ctx)).collect();
    }
    let merged: HashSet<VarId> = ctx.always.union(&borrowed).copied().collect();
    let mut call_ctx = CloneCtx {
        always: &merged,
        eligible: ctx.eligible,
        remaining: ctx.remaining,
        in_loop: ctx.in_loop,
        memo: ctx.memo,
        fresh: ctx.fresh,
        owned: ctx.owned,
        loops: ctx.loops, captured: ctx.captured,
    };
    args.into_iter().map(|a| insert_clones_live(a, &mut call_ctx)).collect()
}

/// `Call { target, args, type_args }` arm of [`insert_clones_live`].
pub(super) fn insert_clones_call(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<Ty>, ctx: &mut CloneCtx) -> IrExprKind {
    // E0505 guard (#809): see `call_borrowed_vars`. Force-clone every var
    // borrowed at the top level of an argument (or the method receiver) for
    // the duration of this call's transform (`map.fold(acc, acc, (…) => acc)`
    // moved `acc` into the closure's capture bind while `&acc` from the first
    // argument was still live). The Borrow arm strips any clone inserted
    // directly under it, so the borrowed occurrence itself stays a plain `&x`.
    let borrowed = call_borrowed_vars(&args, Some(&target));
    if !borrowed.is_empty() {
        let merged: HashSet<VarId> = ctx.always.union(&borrowed).copied().collect();
        let mut call_ctx = CloneCtx {
            always: &merged,
            eligible: ctx.eligible,
            remaining: ctx.remaining,
            in_loop: ctx.in_loop,
            memo: ctx.memo,
            fresh: ctx.fresh,
            owned: ctx.owned,
            loops: ctx.loops, captured: ctx.captured,
        };
        let args = args.into_iter().map(|a| insert_clones_live(a, &mut call_ctx)).collect();
        let target = match target {
            CallTarget::Method { object, method } => CallTarget::Method {
                object: Box::new(insert_clones_live(*object, &mut call_ctx)),
                method,
            },
            CallTarget::Computed { callee } => CallTarget::Computed {
                callee: Box::new(insert_clones_live(*callee, &mut call_ctx)),
            },
            other => other,
        };
        return IrExprKind::Call { target, args, type_args };
    }
    let args = args.into_iter().map(|a| insert_clones_live(a, ctx)).collect();
    let target = match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(insert_clones_live(*object, ctx)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(insert_clones_live(*callee, ctx)),
        },
        other => other,
    };
    IrExprKind::Call { target, args, type_args }
}

/// `IterChain` arm of [`insert_clones_live`], with the chain's E0505 guard
/// (#2686). A step or collector closure borrows what it captures from the
/// moment the chain is built until the terminal runs, so a MOVE of a captured
/// var in a non-closure part of the chain — a fold's `init`, evaluated after
/// the steps' closures exist — conflicts, whatever the flat last-use count
/// says. Unfused, `list.map` ran to completion before the init was evaluated;
/// fused, it has not started. Such a var is force-cloned for the chain's
/// transform, the same way [`insert_clones_call`] treats a borrowed argument.
pub(super) fn insert_clones_iter_chain(chain: IrExpr, ctx: &mut CloneCtx) -> IrExpr {
    super::super::pass_clone_loops::note_chain_element_binders(&chain, ctx.loops);
    let captured = chain_captured_vars(&chain, ctx.eligible);
    let mut chain = if captured.is_empty() {
        chain.map_children(&mut |child| insert_clones_live(child, ctx))
    } else {
        let merged: HashSet<VarId> = ctx.always.union(&captured).copied().collect();
        let mut chain_ctx = CloneCtx {
            always: &merged,
            eligible: ctx.eligible,
            remaining: ctx.remaining,
            in_loop: ctx.in_loop,
            memo: ctx.memo,
            fresh: ctx.fresh,
            owned: ctx.owned,
            loops: ctx.loops, captured: ctx.captured,
        };
        chain.map_children(&mut |child| insert_clones_live(child, &mut chain_ctx))
    };
    super::super::pass_clone_loops::mark_chain_element_binders(&mut chain, ctx.loops);
    chain
}

/// The last-use-eligible vars a chain's step or collector closures capture.
fn chain_captured_vars(chain: &IrExpr, eligible: &HashSet<VarId>) -> HashSet<VarId> {
    let IrExprKind::IterChain { steps, collector, .. } = &chain.kind else { return HashSet::new() };
    let lambdas = steps.iter().filter_map(IterStep::lambda).chain(collector.lambda());
    let mut captured = HashSet::new();
    for lam in lambdas {
        if let IrExprKind::Lambda { params, body, .. } = &lam.kind {
            let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
            captured.extend(almide_ir::free_vars::free_vars(body, &bound).into_iter().filter(|v| eligible.contains(v)));
        }
    }
    captured
}
