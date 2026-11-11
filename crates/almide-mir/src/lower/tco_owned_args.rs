// Native owned loop slots permit argument snapshots of every checked type.
// Evaluate in source order before replacing any carried slot. A mutable
// heap binding requests a real owned Dup, so a later slot replacement cannot
// change or release an earlier argument. The ordinary MIR ownership checker
// sees those copies and their releases; no new borrowing rule is introduced.
fn tco_stage_owned_args(args: &[IrExpr], shape: TcoShape<'_>, counters: &mut TcoCounters<'_>) -> IrExpr {
    let mut stmts = Vec::new();
    let mut commits = Vec::new();
    for (i, p) in shape.params.iter().enumerate() {
        if !shape.carried[i] || matches!(&args[i].kind, IrExprKind::Var { id } if *id == p.var) { continue; }
        let temp = VarId(*counters.next_var);
        *counters.next_var += 1;
        stmts.push(IrStmt { kind: IrStmtKind::Bind {
            var: temp, mutability: almide_ir::Mutability::Var,
            ty: p.ty.clone(), value: args[i].clone(),
        }, span: None });
        commits.push(IrStmt { kind: IrStmtKind::Assign {
            var: p.var, value: tco_ir(IrExprKind::Var { id: temp }, p.ty.clone()),
        }, span: None });
    }
    stmts.extend(commits);
    if let Some(idx) = shape.idx {
        stmts.push(IrStmt { kind: IrStmtKind::Assign {
            var: idx, value: tco_ir(IrExprKind::BinOp {
                op: almide_ir::BinOp::AddInt,
                left: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
                right: Box::new(tco_ir(IrExprKind::LitInt { value: 1 }, Ty::Int)),
            }, Ty::Int),
        }, span: None });
    }
    tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
}

/// One rewritten SELF-CALL leaf: assign each carried param its new value with
/// SIMULTANEOUS-UPDATE staging (the loop carries all params at once). A
/// self-call arg may read ANOTHER carried param (`acc + [string.slice(s, pos,
/// …)]` reads `pos`; `start = pos + 1` reads `pos`), so a plain sequential
/// assign would see already-updated values — an off-by-one. Stage every
/// carried SCALAR's new value in a fresh temp (reading OLD params), THEN do the HEAP
/// accumulator assigns (which read the still-OLD scalar locals), THEN commit the scalar temps.
/// An IDENTITY arg (`acc` passed unchanged) is skipped (the stable local already holds it).
fn tco_rewrite_self_call(
    args: &[IrExpr],
    shape: TcoShape<'_>,
    counters: &mut TcoCounters<'_>,
) -> IrExpr {
    if crate::lower::owned_tco_slots() {
        return tco_stage_owned_args(args, shape, counters);
    }
    let TcoShape { params, carried, idx, .. } = shape;
    let changed = |i: usize| {
        carried[i] && !matches!(&args[i].kind, IrExprKind::Var { id } if *id == params[i].var)
    };
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut finals: Vec<(VarId, VarId, Ty)> = Vec::new();
    // Phase 1: stage carried SCALAR args in temps (read OLD params).
    for i in 0..params.len() {
        if changed(i) && !is_heap_ty(&params[i].ty) {
            let t = VarId(*counters.next_var);
            *counters.next_var += 1;
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: t,
                    mutability: almide_ir::Mutability::Let,
                    ty: params[i].ty.clone(),
                    value: args[i].clone(),
                },
                span: None,
            });
            finals.push((params[i].var, t, params[i].ty.clone()));
        }
    }
    // Phase 2: HEAP append/reset accumulator(s) — `acc = acc + [x]` reads the still-OLD scalar
    // locals. Emit in READ-DEPENDENCY order so a heap accumulator that reads ANOTHER heap
    // accumulator (`rows = rows + [cur]` alongside `cur = []`) is assigned BEFORE that one is
    // updated — the reader must observe the old value. try_tco_rewrite already walled the
    // cyclic case, so the order always exists (the unwrap_or is a defensive param-order
    // fallback).
    let heap_changed: Vec<usize> = (0..params.len())
        .filter(|&i| changed(i) && is_heap_ty(&params[i].ty))
        .collect();
    let heap_order = order_heap_accs_by_read_dep(&heap_changed, args, params)
        .unwrap_or(heap_changed);
    for i in heap_order {
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign { var: params[i].var, value: args[i].clone() },
            span: None,
        });
    }
    // Phase 3: commit the staged scalar updates.
    for (p, t, ty) in finals {
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: p,
                value: tco_ir(IrExprKind::Var { id: t }, ty),
            },
            span: None,
        });
    }
    // LIST-ITERATOR self-call: the consumed list param is INVARIANT (carried[ci]=false), so
    // advancing it `list.drop(cs,1)` becomes `idx = idx + 1` — the cert-clean iterator bump.
    if let Some(iv) = idx {
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: iv,
                value: tco_ir(
                    IrExprKind::BinOp {
                        op: almide_ir::BinOp::AddInt,
                        left: Box::new(tco_ir(IrExprKind::Var { id: iv }, Ty::Int)),
                        right: Box::new(tco_ir(IrExprKind::LitInt { value: 1 }, Ty::Int)),
                    },
                    Ty::Int,
                ),
            },
            span: None,
        });
    }
    tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
}


impl LowerCtx {
    /// Materialize a checked expression through the ordinary mutable-bind
    /// path. The fresh binding owns its value, including nested constructor
    /// payloads; the caller transfers this handle into an existing loop slot.
    fn lower_owned_binding(&mut self, value: &IrExpr) -> Option<ValueId> {
        self.speculate_bindings(|ctx| {
            let var = ctx.fresh_synth_var();
            ctx.lower_stmt(&IrStmt { kind: IrStmtKind::Bind {
                var, mutability: almide_ir::Mutability::Var,
                ty: value.ty.clone(), value: value.clone(),
            }, span: value.span }).ok()?;
            let owned = ctx.value_for(var).ok()?;
            (ctx.live_heap_handles.contains(&owned)
                && !ctx.param_values.contains(&owned)
                && !ctx.deferred_opaque_binds.contains(&owned)).then_some(owned)
        })
    }

    /// The left list of a `+` read in place: a live local slot of this fn that
    /// the right operand neither reads nor rebinds (the append then grows it,
    /// with no copy to free).
    fn concat_left_in_place(&self, left: &IrExpr, right: &IrExpr) -> Option<ValueId> {
        let IrExprKind::Var { id } = &left.kind else { return None };
        let mut rebound = Vec::new();
        crate::lower::collect_heap_reassign_vars_in_expr(right, &mut rebound);
        if rebound.contains(id) || expr_reads_var(right, *id) {
            return None;
        }
        self.value_of.get(id).copied()
            .filter(|v| self.live_heap_handles.contains(v) && !self.param_values.contains(v))
    }

    /// A Result delivered to a loop slot. A heap-Ok Result, and every Result
    /// off the native owned slots, takes the block builders first; a scalar-Ok
    /// Result on the native owned slots keeps its declared carrier, which only
    /// the ordinary bind builds (the len-as-tag block is not it).
    fn lower_loop_slot_result(&mut self, value: &IrExpr, payload: &IrExpr) -> Option<ValueId> {
        let declared = crate::lower::owned_tco_slots() && !Self::is_heap_ok_result(&value.ty);
        (!declared)
            .then(|| self.speculate(|ctx| ctx.lower_scalar_loop_result_delivery(value, payload)))
            .flatten()
            .or_else(|| self.lower_owned_binding(value))
    }

    /// Any other checked expression delivered to a loop slot: the field
    /// builders, and on the native owned slots the ordinary mutable bind for
    /// what they decline (a declined builder leaves nothing behind).
    fn lower_loop_slot_value(&mut self, value: &IrExpr) -> Option<ValueId> {
        if !crate::lower::owned_tco_slots() {
            return self.lower_owned_heap_field(value);
        }
        self.speculate(|ctx| ctx.lower_owned_heap_field(value)).or_else(|| self.lower_owned_binding(value))
    }

    /// An attempted expression may replace source bindings before a later
    /// operand declines. Restore those bindings along with its emitted ops.
    fn speculate_bindings<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let values = self.value_of.clone();
        let declarations = self.var_decl_tys.clone();
        let result = self.speculate(f);
        if result.is_none() {
            self.value_of = values;
            self.var_decl_tys = declarations;
        }
        result
    }
}
