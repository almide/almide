//! Does the rhs of an Assign spend the assigned var's own credit? (#2616,
//! #3127) — the predicate `lower_assign` and `lower_field_assign` settle the
//! old occupant by, split from stmts.rs for the file budget.

use almide_ir::{IrExpr, IrExprKind, VarId};

use crate::emitter::Emitter;

impl Emitter<'_> {
    /// Does the rhs of `var = rhs` spend `var`'s own credit, so the Assign
    /// must NOT release the old occupant? (#2616)
    ///
    /// * A NON-call rhs never does: `data = data + [x]` (the append loop's
    ///   ConcatList) READS the old block and builds a fresh one; skipping
    ///   the dec leaked every outgrown generation, and a 65k-append loop
    ///   exhausted the 4 GiB address space in a quarter second (#1729).
    /// * A MODULE call never does (arm-aware, #2010 item 4): a native arm
    ///   declares Borrow (reads it) or Retain (+1 share), the registry route
    ///   incs an owned position and passes a borrowed one as is — and the
    ///   RC-5 inc already made an aliasing result (`s = set.insert(s, x)`'s
    ///   present path) its own credit.
    /// * A program-fn call spends it only through a `mut` parameter: the
    ///   C-132 write-back hands the var to a callee that may reallocate it
    ///   in place and returns the buffer the var is rebound to. Any other
    ///   position leaves the local's credit where it was — a BORROWED param
    ///   (#2028) takes no share and the callee releases nothing, an OWNED
    ///   one takes the site's +1 (`rc_arg_guard`) and its exit plan releases
    ///   exactly that. Skipping the dec there leaked every old value of
    ///   `m = g(m, r)`: 1.6 GB over 20k steps of a list accumulator.
    ///   A var mentioned INSIDE an argument is read the same way (#3127):
    ///   under a nested program-fn call (`g(h(m), r)`) by that call's own
    ///   `mut` flags — its site takes the same share discipline, so a
    ///   borrowed or owned position leaves the credit where it was — and a
    ///   scalar read out of it (`v3(rd.x, 0.0, 0.0)`) carries no credit at
    ///   all. Skipping the dec there leaked the old block of every
    ///   `rd = vnorm(vreflect(rd, n))`. Any other mention (a closure, a
    ///   container literal, a control funnel) keeps the conservative skip.
    /// * A runtime helper (`xs = $push(xs, v)`) consumes its operand.
    pub(super) fn assign_rhs_spends_var(&self, value: &IrExpr, var: VarId) -> bool {
        use almide_ir::CallTarget;
        let call = match &value.kind {
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => expr.as_ref(),
            _ => value,
        };
        let mentions = || crate::rc_ownership::rc_mentions_var(call, var);
        match &call.kind {
            IrExprKind::Call { target: CallTarget::Module { .. }, .. } => false,
            IrExprKind::Call { target: CallTarget::Named { .. }, .. } => self.named_call_spends_var(call, var).unwrap_or_else(mentions),
            // A CLOSURE call shares a Var argument (`rc_arg_guard`, calls.rs)
            // and the lifted body releases its params at its own exit plan,
            // so a direct Var argument leaves the local's credit where it
            // was: `acc = f(acc, x)` releases the old `acc` like any Assign
            // (#2977 — every step of a closure accumulator loop kept it).
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } => {
                crate::rc_ownership::rc_mentions_var(callee, var)
                    || args.iter().any(|a| match &a.kind {
                        IrExprKind::Var { id } if *id == var => false,
                        _ => self.arg_spends_var(a, var),
                    })
            }
            IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } => mentions(),
            _ => false,
        }
    }

    /// A call to a resolved program fn spends `var` only through a `mut`
    /// parameter it is passed to directly, or through an argument that
    /// spends it (`arg_spends_var`). `None` when the call is not one: a
    /// variant constructor or an unresolved name.
    fn named_call_spends_var(&self, call: &IrExpr, var: VarId) -> Option<bool> {
        let IrExprKind::Call { target: almide_ir::CallTarget::Named { name }, args, .. } = &call.kind else {
            return None;
        };
        let name = name.as_str();
        let i = if self.is_variant_ctor(name, call) { None } else { self.resolve_named_fn(name) }?;
        let param_mut = &self.table.infos[i].param_mut;
        Some(args.iter().enumerate().any(|(k, a)| match &a.kind {
            IrExprKind::Var { id } if *id == var => param_mut.get(k).copied().unwrap_or(true),
            _ => self.arg_spends_var(a, var),
        }))
    }

    /// Can evaluating this call ARGUMENT spend `var`'s credit? A nested
    /// program-fn call by its own convention, a scalar projection of the
    /// var never (`rd.x`), a concat operand or a write-back block by the
    /// rules below (#3130), anything else that mentions it conservatively.
    fn arg_spends_var(&self, a: &IrExpr, var: VarId) -> bool {
        if let Some(spends) = self.named_call_spends_var(a, var) {
            return spends;
        }
        // `keep(keep(xs) + [4])` (#3130): `$concat` READS both operands
        // and answers a FRESH block (runtime.rs `emit_concat`: it allocates
        // `la + lb` and copies; it releases neither operand and never hands
        // one back), so the concat spends `var` only where an operand does —
        // the var itself is a read, anything else is judged as an argument.
        if let IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr, left, right } = &a.kind {
            return [left, right].into_iter().any(|o| match &o.kind {
                IrExprKind::Var { id } if *id == var => false,
                _ => self.arg_spends_var(o, var),
            });
        }
        if let IrExprKind::Block { stmts, expr } = &a.kind {
            if write_back_settles_var(stmts, expr.as_deref(), var) {
                return false;
            }
            if let Some(spends) = self.let_block_spends_var(stmts, expr.as_deref(), var) {
                return spends;
            }
        }
        mentions_var_beyond_scalar_reads(a, var)
    }

    /// A block of plain `let` temporaries and a tail — the ANF form an
    /// argument arrives in (`keep(xs) + [4]` is
    /// `{ let t = keep(xs); let u = [4]; t + u }`, #3130) — spends `var`
    /// only where a bound value or the tail does, each judged as an
    /// argument: a `let` binds a NEW local that takes its own credit (RC-5
    /// share, or the owned result's) and leaves `var`'s where it was. The
    /// var itself as a bound value or as the tail stays conservative.
    /// `None` when any statement is not a plain `let`.
    fn let_block_spends_var(&self, stmts: &[almide_ir::IrStmt], tail: Option<&IrExpr>, var: VarId) -> Option<bool> {
        let judge = |e: &IrExpr| match &e.kind {
            IrExprKind::Var { id } if *id == var => true,
            _ => self.arg_spends_var(e, var),
        };
        let mut spends = false;
        for s in stmts {
            let almide_ir::IrStmtKind::Bind { value, .. } = &s.kind else { return None };
            spends = spends || judge(value);
        }
        Some(spends || tail.is_some_and(judge))
    }
}

/// Is this block (statements and tail) one whose LAST statement assigns `var`, with a tail that
/// mentions it nowhere (beyond scalar reads)? The C-132 write-back of a
/// `mut` call nested in an argument is exactly this shape (#3130):
/// `ys = keep(grow(ys))` arrives as
/// `ys = keep({ let t = grow(ys); let (r, b) = t; ys = b; r })`.
/// Whatever the statements before it did with the old occupant's credit,
/// that inner `Assign` settles it under its own judgement and leaves the
/// local holding exactly ONE credit on the written-back block (RC-5: an
/// inc, an owned result, or a moved temp). The outer Assign's release
/// reads the local AFTER the whole rhs is evaluated, so it releases that
/// credit — unless something evaluated after the write-back spends it,
/// which is why the tail must not mention the var; a later sibling
/// argument is judged on its own by the caller's `any`.
fn write_back_settles_var(stmts: &[almide_ir::IrStmt], tail: Option<&IrExpr>, var: VarId) -> bool {
    matches!(stmts.last().map(|s| &s.kind), Some(almide_ir::IrStmtKind::Assign { var: v, .. }) if *v == var)
        && !tail.is_some_and(|t| mentions_var_beyond_scalar_reads(t, var))
}

/// Does the expression mention `var` other than through a SCALAR
/// projection of it (`rd.x`, `p.c.y`, `t.0` typed Int/Float/Bool)? A
/// scalar read out of the var's block carries no credit anywhere, so it
/// can never spend the var's own — the argument-position refinement of the
/// Assign dec-old suppressor (#3127): `rd = vnorm(v3(rd.x, 0.0, 0.0))`
/// builds a fresh block from scalars of the old occupant, and counting the
/// mention as a spend skipped the release and leaked one block per such
/// assign. Every other mention (the var itself, a droppable field of it)
/// keeps the conservative reading.
fn mentions_var_beyond_scalar_reads(e: &IrExpr, var: VarId) -> bool {
    let mut f = Finder { var, found: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut f, e);
    f.found
}

fn projection_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => projection_root(object),
        _ => None,
    }
}

/// The walk behind [`mentions_var_beyond_scalar_reads`]: finds a mention of
/// `var` other than a scalar projection of it.
struct Finder {
    var: VarId,
    found: bool,
}

impl almide_ir::visit::IrVisitor for Finder {
    fn visit_expr(&mut self, e: &IrExpr) {
        if self.found {
            return;
        }
        match &e.kind {
            IrExprKind::Var { id } if *id == self.var => self.found = true,
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if matches!(e.ty, almide_types::types::Ty::Int | almide_types::types::Ty::Float | almide_types::types::Ty::Bool)
                    && projection_root(e) == Some(self.var) => {}
            _ => almide_ir::visit::walk_expr(self, e),
        }
    }
}
