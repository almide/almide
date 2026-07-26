
/// Rewrite tail leaves: a self-call → a Block assigning each CARRIED param to its new arg; a base
/// → `result_kind = <its 1-based kind>` (kinds assigned in `tco_collect`'s left-to-right order).
/// The fixed shape of the function a TCO rewrite is threading through.
///
/// Everything here is decided once, before the walk starts, and read at every
/// node — as opposed to the two counters, which the walk advances.
#[derive(Copy, Clone)]
pub(crate) struct TcoShape<'a> {
    pub fn_name: &'a str,
    pub params: &'a [almide_ir::IrParam],
    /// Which parameters are loop-carried.
    pub carried: &'a [bool],
    /// The variable holding the rewritten call's dispatch kind.
    pub rk: VarId,
    /// The induction variable, when the rewrite introduced one.
    pub idx: Option<VarId>,
    /// The variable the loop's result lands in.
    pub result: Option<VarId>,
}

/// The counters a TCO rewrite advances as it walks.
///
/// Both are `&mut` and were adjacent parameters of different integer widths, so
/// they could not be transposed — but they belong together, and passing them as
/// one makes every recursive call site one argument instead of two.
pub(crate) struct TcoCounters<'a> {
    pub next_kind: &'a mut i64,
    pub next_var: &'a mut u32,
}

fn tco_rewrite(body: &IrExpr, shape: TcoShape<'_>, counters: &mut TcoCounters<'_>) -> IrExpr {
    let TcoShape { fn_name, rk, result, .. } = shape;
    match &body.kind {
        IrExprKind::If { cond, then, else_ } => tco_ir(
            IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(tco_rewrite(then, shape, counters)),
                else_: Box::new(tco_rewrite(else_, shape, counters)),
            },
            Ty::Unit,
        ),
        // A `match` tail: rewrite each arm body (ok-arm → recurse/acc-update, err-arm → base/rk-set),
        // preserving the subject + patterns so the per-iteration match dispatches continue-or-exit.
        IrExprKind::Match { subject, arms } => tco_ir(
            IrExprKind::Match {
                subject: subject.clone(),
                arms: arms
                    .iter()
                    .map(|a| almide_ir::IrMatchArm {
                        pattern: a.pattern.clone(),
                        guard: a.guard.clone(),
                        body: tco_rewrite(&a.body, shape, counters),
                    })
                    .collect(),
            },
            Ty::Unit,
        ),
        IrExprKind::Block { stmts, expr: Some(tail) } => tco_ir(
            IrExprKind::Block {
                stmts: stmts.clone(),
                expr: Some(Box::new(tco_rewrite(tail, shape, counters))),
            },
            Ty::Unit,
        ),
        // The auto-`?`-wrapped tail self-call (`Try{Call self}` / `Unwrap{Call self}`):
        // same-repr effect propagation is the identity on the self-call, and tco_collect's
        // twin arm already classified it as a CALL — rewrite the wrapped call exactly like
        // the bare one. Without this arm the leaf fell to the BASE fallthrough below, so
        // the recursive arm of every err-CAPABLE effect fn compiled to "exit with the last
        // base's kind": `checked(n - 0)` returned ok(0) where native spins (fuzz
        // seed-20260718 index 946), and a carried accumulator delivered its INITIAL value.
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }
            if matches!(&expr.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if name.as_str() == fn_name) =>
        {
            tco_rewrite(expr, shape, counters)
        }
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if name.as_str() == fn_name =>
        {
            tco_rewrite_self_call(args, shape, counters)
        }
        _ => tco_rewrite_base(body, rk, result, counters),
    }
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

/// A BASE case (a non-self tail). Set `rk` to a non-zero kind so the `while rk == 0` loop
/// exits. The base VALUE is delivered one of two ways:
///   • result accumulator (`result = Some`): assign `<base>` to the carried result var HERE,
///     IN the loop — where the base's inputs (carried params AND loop-body-local bindings
///     like a destructured `let (field, _) = pf(…)`) are all live. The post-loop trivially
///     returns the accumulator. This is the only correct place when the base reads a
///     loop-body-local (those are dead in the post-loop dispatch — the parse_rows_rec bug).
///   • post-loop dispatch (`result = None`): just record WHICH base via `rk = k`; the value
///     is recomputed after the loop. Sound ONLY when the base closes over carried params.
fn tco_rewrite_base(
    body: &IrExpr,
    rk: VarId,
    result: Option<VarId>,
    counters: &mut TcoCounters<'_>,
) -> IrExpr {
    let k = *counters.next_kind;
    *counters.next_kind += 1;
    let mut stmts: Vec<IrStmt> = Vec::new();
    if let Some(rv) = result {
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign { var: rv, value: body.clone() },
            span: None,
        });
    }
    stmts.push(IrStmt {
        kind: IrStmtKind::Assign {
            var: rk,
            value: tco_ir(IrExprKind::LitInt { value: k }, Ty::Int),
        },
        span: None,
    });
    tco_ir(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
}

/// Rewrite a tail-self-recursive function body to a scalar loop + post-loop dispatch, or `None`
/// if it is outside the TCO subset (no self-call, a heap loop-carried arg, a self-call in a
/// non-tail position, or no base). The result lowers through the ordinary statements+tail path.
pub(crate) fn try_tco_rewrite(
    fn_name: &str,
    params: &[almide_ir::IrParam],
    body: &IrExpr,
) -> Option<IrExpr> {
    if !is_heap_ty(&body.ty) && has_tuple_destructure(body) {
        // SCALAR-result gate: the loop rewrite mishandles a TUPLE-DESTRUCTURE bind in
        // the body (`let (cp, l) = __trim_cp(addr)` — the rewritten loop spun forever,
        // int.parse via the Unicode trim, 2026-07-03). Decline those; they keep the
        // real-recursion lowering (correct, stack-bound) as before. A destructure-free
        // scalar body (the `__split_fill`/`__chunk_outer` byte-walkers) is admitted.
        return None;
    }
    // A HEAP-result self-rec function (the kind the self-rec guard walls — it returns an
    // Option/Result/Value/String the deep recursion would build then trap on), AND a
    // SCALAR-result one: the latter lowers as REAL recursion (a function-tail self-call)
    // which is correct but STACK-BOUND — the self-host byte-walkers (`__split_fill`,
    // `__chunk_outer`, `__fp_pow10_acc`) exhausted the wasm call stack on large inputs
    // (spec/wasm_cross r5_split / list_count_index_truncation, 2026-07-03). A scalar
    // result is exactly the cert-clean scalar-loop form (`tco_empty_for` has scalar
    // empties since brick 1), so admit it; the collect/carried gates below still decline
    // anything outside the loop subset, falling back to the real recursion as before.
    let n = params.len();
    let max_v = max_var_id(body).max(params.iter().map(|p| p.var.0).max().unwrap_or(0));
    let rk = VarId(max_v + 1);
    // LIST-ITERATOR rewrite (the heap-loop-carried escape): a HEAP carried param `cs` consumed in
    // EVERY self-call ONLY as `list.drop(cs, 1)`, with the body matching on `list.first(cs)`, is a
    // forward list scan. Rewrite it to an INVARIANT borrowed `cs` + a synthetic scalar INDEX `idx`:
    // `match list.first(cs) { none => BASE, some(ch) => BODY }` → `if idx < list.len(cs) then { let
    // ch = cs[idx]; BODY } else BASE`, and each `f(list.drop(cs,1), …)` self-call bumps `idx += 1`
    // (handled in `tco_rewrite`). `cs` becomes invariant, so the loop is the cert-clean scalar form —
    // NO heap back-edge merge, NO cert change. Closes oct_rec/bin_rec. Done BEFORE `tco_collect`
    // (which bails on a `match` body), so the rewritten `if` body is what gets collected + lowered.
    let lit = try_list_iter_rewrite(fn_name, body, params, max_v + 2);
    let work_body: &IrExpr = lit.as_ref().map(|(b, _, _)| b).unwrap_or(body);
    let idx_var = lit.as_ref().map(|(_, iv, _)| *iv);

    // FIRST collection — detect the self-calls + carried params (on the pre-substitution body).
    let (calls0, bases0) = collect_tco_shape(work_body, fn_name, n)?;
    // An `err($x)`-of-a-VAR base is the desugared `e!` early-return — a Result-unwrap INSIDE the
    // recursion (read_basic's `let (ch,np)=read_escape(..)!`). The TCO loop cannot carry that mid-body
    // early-exit: desugar_let_unwrap (run before this) turned the `!` into a `match e { ok($p)=>{..;
    // self-call}, err($x)=>err($x) }`, so the self-call sits in a nested match arm whose heap-accumulator
    // reassign then walls (mod_p3 "heap reassignment in a scalar loop body"). BAIL → the function falls
    // to the now-allowed REAL recursive lowering (a function-tail self-call, control_p4 ~188), which is
    // input-bounded and byte-matches v0. A natural `err("literal")` base (not a Var) is unaffected.
    if bases0.iter().any(|b| {
        matches!(&b.kind, IrExprKind::ResultErr { expr } if matches!(&expr.kind, IrExprKind::Var { .. }))
    }) {
        return None;
    }
    let mut carried0 = carried_flags(&calls0, params);
    if let Some((_, _, ci)) = &lit {
        carried0[*ci] = false;
    }
    let append_accs = detect_append_accs(&calls0, params, &carried0)?;
    drop(calls0);
    drop(bases0);

    // Build the (possibly substituted) working body + params + upfront slot-init binds.
    let mut slot_next = max_v + 3;
    let (upfront, params_v, subst_body) =
        build_append_slot_subst(work_body, params, &append_accs, &mut slot_next);
    let work_ref: &IrExpr = subst_body.as_ref().unwrap_or(work_body);
    let params2: &[almide_ir::IrParam] = &params_v;

    // SECOND collection — on the substituted body, with the slot params.
    let (calls, bases) = collect_tco_shape(work_ref, fn_name, n)?;
    // A param is loop-CARRIED iff some self-call passes a value other than the param itself.
    let mut carried = carried_flags(&calls, params2);
    // The list-iterator param is now INVARIANT — its `list.drop(cs,1)` self-call arg is replaced by
    // the `idx` bump (in `tco_rewrite`), so `cs` is never reassigned in the loop.
    if let Some((_, _, ci)) = &lit {
        carried[*ci] = false;
    }
    // A carried HEAP arg is admitted ONLY as an append-accumulator SLOT (handled below by the in-loop
    // `Assign` lowering as drop-old/alloc-new); any other heap carry needs a general back-edge merge.
    let append_slots: std::collections::BTreeSet<VarId> =
        append_accs.iter().map(|&i| params2[i].var).collect();
    if (0..n)
        .any(|i| carried[i] && is_heap_ty(&params2[i].ty) && !append_slots.contains(&params2[i].var))
    {
        return None;
    }
    if !simultaneous_update_safe(&calls, params2, &carried) {
        return None;
    }
    let base_exprs: Vec<IrExpr> = bases.iter().map(|b| (*b).clone()).collect();
    let ret_ty = body.ty.clone();

    let use_result_acc = tco_result_acc_mode(work_ref, &base_exprs, &ret_ty)?;
    let result_var: Option<VarId> = if use_result_acc {
        tco_empty_for(&ret_ty)?;
        let rv = VarId(slot_next);
        slot_next += 1;
        Some(rv)
    } else {
        None
    };

    let mut next_kind = 1i64;
    // `slot_next` is the next free VarId (after rk / list-iter idx / append slots / result) — tco_rewrite
    // draws its simultaneous-update temps from here.
    let loop_body = tco_rewrite(
        work_ref,
        TcoShape {
            fn_name, params: params2, carried: &carried, rk,
            idx: idx_var, result: result_var,
        },
        &mut TcoCounters { next_kind: &mut next_kind, next_var: &mut slot_next },
    );

    // Post-loop: the accumulator path just READS the result the loop computed; otherwise the dispatch
    // `if rk == 1 then base_1 else if … else base_N` recomputes the hit base from the carried params.
    let post = if let Some(rv) = result_var {
        tco_ir(IrExprKind::Var { id: rv }, ret_ty.clone())
    } else {
        tco_post_dispatch(&base_exprs, rk, &ret_ty)?
    };

    // `{ [let slot = acc + [];]* [var idx = 0;] var rk = 0; while (rk == 0) { <loop_body> }; <post> }`
    // The append-accumulator slot inits (owned copies of the borrowed `acc` params) come FIRST.
    let mut inits: Vec<IrStmt> = upfront;
    inits.extend(tco_state_inits(idx_var, result_var, rk, &ret_ty));
    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr {
            expr: tco_ir(
                IrExprKind::While {
                    cond: Box::new(tco_eq_rk(rk, 0)),
                    body: vec![IrStmt { kind: IrStmtKind::Expr { expr: loop_body }, span: None }],
                },
                Ty::Unit,
            ),
        },
        span: None,
    };
    inits.push(while_stmt);
    Some(tco_ir(
        IrExprKind::Block { stmts: inits, expr: Some(Box::new(post)) },
        ret_ty,
    ))
}

/// Does the body contain a TUPLE-DESTRUCTURE bind (`let (cp, l) = …`)? The
/// scalar-result TCO gate declines those (the rewritten loop spun forever —
/// int.parse via the Unicode trim, 2026-07-03).
fn has_tuple_destructure(e: &IrExpr) -> bool {
    use almide_ir::visit::IrVisitor;
    struct C(bool);
    impl IrVisitor for C {
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            if matches!(&s.kind, almide_ir::IrStmtKind::BindDestructure { .. }) {
                self.0 = true;
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut c = C(false);
    c.visit_expr(e);
    c.0
}

/// Collect the self-calls + base tails of a candidate body, gated on the shape
/// every collection site requires: at least one self-call, at least one base,
/// and every self-call at the declared arity. `None` = outside the TCO subset.
fn collect_tco_shape<'a>(
    body: &'a IrExpr,
    fn_name: &str,
    n: usize,
) -> Option<(Vec<&'a [IrExpr]>, Vec<&'a IrExpr>)> {
    let mut calls: Vec<&[IrExpr]> = Vec::new();
    let mut bases: Vec<&IrExpr> = Vec::new();
    tco_collect(body, fn_name, &mut calls, &mut bases)?;
    if calls.is_empty() || bases.is_empty() {
        return None;
    }
    if calls.iter().any(|c| c.len() != n) {
        return None;
    }
    Some((calls, bases))
}

/// A param is loop-CARRIED iff some self-call passes a value other than the
/// param itself.
fn carried_flags(calls: &[&[IrExpr]], params: &[almide_ir::IrParam]) -> Vec<bool> {
    let n = params.len();
    let mut carried = vec![false; n];
    for c in calls {
        for i in 0..n {
            if !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params[i].var) {
                carried[i] = true;
            }
        }
    }
    carried
}

/// A self-call value that GROWS the accumulator from itself: `acc + [x]` (`ConcatList`) OR
/// `acc + s` (`ConcatStr`, the STRING accumulator — `parse_unquoted_field(text, pos+1, acc + c)`).
/// Both allocate a FRESH owned heap value; the TCO makes the accumulator an owned loop-carried
/// slot (drop-old/alloc-new per iter, cert `i(id)m`).
/// `acc + x`, OR a LEFT-NESTED chain `acc + a + b + …` whose leftmost leaf is `acc` (base64
/// encode_chunks: `enc(.., acc + c0 + c1 + c2 + c3)`). Both GROW the accumulator from itself —
/// a fresh owned heap value the loop-carried slot takes via drop-old/alloc-new; the in-loop
/// general-reassign path materializes the (possibly nested) concat via try_lower_concat_str/list,
/// and the OwnershipChecker `i(id)m` proof covers any fresh-owned producer regardless of nesting.
fn concat_leftmost_is_var(e: &IrExpr, acc: VarId) -> bool {
    match &e.kind {
        IrExprKind::Var { id } => *id == acc,
        IrExprKind::BinOp {
            op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr,
            left,
            ..
        } => concat_leftmost_is_var(left, acc),
        _ => false,
    }
}

fn is_self_append(e: &IrExpr, acc: VarId) -> bool {
    matches!(
        &e.kind,
        IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr, .. }
    ) && concat_leftmost_is_var(e, acc)
}

/// A PURE Module call WRAPPING the growth (`string.take(acc + "x", 8)` — the
/// churn spin): the callee is a pure stdlib fn (fresh-owned result by the
/// calling convention — self-hosts copy, never alias their inputs), and some
/// argument grows from / passes through the accumulator. The loop-carried slot
/// takes the fresh value via the same drop-old/alloc-new; the in-loop reassign
/// materializes the call through the standard pure-module path.
fn is_wrapped_growth(e: &IrExpr, acc: VarId) -> bool {
    matches!(&e.kind,
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if crate::purity::is_pure(module.as_str(), func.as_str())
                && args.iter().any(|a|
                    is_self_append(a, acc)
                        || matches!(&a.kind, IrExprKind::Var { id } if *id == acc)))
}

/// A RESET to a FRESH EMPTY heap value (`cur = []` / `acc = ""`): the parser-row shape resets the
/// current-row accumulator after a delimiter. Like a self-append it is a fresh owned heap value the
/// loop-carried slot takes via drop-old/alloc-new (cert `i(id)m`); the in-loop `Assign` lowering's
/// general `lower_owned_heap_field` path materializes the empty literal.
fn is_reset(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::List { elements } if elements.is_empty())
        || matches!(&e.kind, IrExprKind::LitStr { value } if value.is_empty())
}

/// APPEND ACCUMULATORS (option C producer): a heap carried param whose EVERY self-call value is
/// `acc + [x]` (`BinOp::ConcatList` appending the accumulator to itself). Each becomes an OWNED
/// loop-carried SLOT — a fresh var initialized to `acc + []` (an owned copy: a `__list_concat`
/// Call heap-result, so `of[slot]=slot` and cert `i`), substituted for `acc` throughout, then
/// drop-old/alloc-new per iteration (cert `i(id)m`, accepted by the proven `check_cert_lc`). A heap
/// carried param that is NOT a self-append needs a general heap back-edge merge — still unsupported,
/// so it declines the whole TCO (`None`).
fn detect_append_accs(
    calls: &[&[IrExpr]],
    params: &[almide_ir::IrParam],
    carried: &[bool],
) -> Option<Vec<usize>> {
    let mut append_accs: Vec<usize> = Vec::new();
    for i in 0..params.len() {
        if carried[i] && is_heap_ty(&params[i].ty) {
            // Each self-call passes the accumulator UNCHANGED (`acc`, a pass-through branch), APPENDED
            // (`acc + [x]`), or RESET to a fresh empty (`[]`/`""`); at least one grows/resets it (else
            // not carried). A heap carry outside these needs a general back-edge merge — unsupported.
            if calls.iter().all(|c| {
                matches!(&c[i].kind, IrExprKind::Var { id } if *id == params[i].var)
                    || is_self_append(&c[i], params[i].var)
                    || is_reset(&c[i])
                    || is_wrapped_growth(&c[i], params[i].var)
            }) {
                append_accs.push(i);
            } else {
                return None;
            }
        }
    }
    Some(append_accs)
}

/// Substitute every append accumulator with an OWNED loop-carried slot: an
/// upfront `let slot = acc + <empty>` bind (a fresh OWNED copy of the borrowed
/// accumulator param — the concat always allocates, so the slot never aliases
/// it; a String accumulator copies via `acc + ""`, a list via `acc + []`), the
/// slot substituted for `acc` throughout the body, and the param rebound to the
/// slot. Returns `(upfront binds, substituted params, substituted body)` —
/// body `None` when there is nothing to substitute.
fn build_append_slot_subst(
    work_body: &IrExpr,
    params: &[almide_ir::IrParam],
    append_accs: &[usize],
    slot_next: &mut u32,
) -> (Vec<IrStmt>, Vec<almide_ir::IrParam>, Option<IrExpr>) {
    let mut upfront: Vec<IrStmt> = Vec::new();
    let mut params_v: Vec<almide_ir::IrParam> = params.to_vec();
    if append_accs.is_empty() {
        return (upfront, params_v, None);
    }
    let mut b = work_body.clone();
    for &ai in append_accs {
        let slot = VarId(*slot_next);
        *slot_next += 1;
        let acc_var = params[ai].var;
        let list_ty = params[ai].ty.clone();
        let (empty, concat_op) = if matches!(list_ty, Ty::String) {
            (tco_ir(IrExprKind::LitStr { value: String::new() }, Ty::String), almide_ir::BinOp::ConcatStr)
        } else {
            (tco_ir(IrExprKind::List { elements: vec![] }, list_ty.clone()), almide_ir::BinOp::ConcatList)
        };
        let copy = tco_ir(
            IrExprKind::BinOp {
                op: concat_op,
                left: Box::new(tco_ir(IrExprKind::Var { id: acc_var }, list_ty.clone())),
                right: Box::new(empty),
            },
            list_ty.clone(),
        );
        upfront.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: slot,
                mutability: almide_ir::Mutability::Var,
                ty: list_ty.clone(),
                value: copy,
            },
            span: None,
        });
        let slot_ref = tco_ir(IrExprKind::Var { id: slot }, list_ty);
        b = almide_ir::substitute_var_in_expr(&b, acc_var, &slot_ref);
        params_v[ai].var = slot;
    }
    (upfront, params_v, Some(b))
}

/// SIMULTANEOUS-UPDATE SAFETY. `tco_rewrite` stages scalar updates in temps and runs the heap
/// accumulator assigns BEFORE committing them, so scalar↔scalar and heap-reads-scalar are correct.
/// A HEAP accumulator arg that reads ANOTHER carried HEAP accumulator (`rows = rows + [cur]` while
/// `cur = []`) is handled by emitting the heap assigns in READ-DEPENDENCY order (reader before the
/// accumulator it reads — `order_heap_accs_by_read_dep` in tco_rewrite), so the reader sees the OLD
/// value. WALL only the residual the topological order CANNOT serialize: a CYCLE (`a = a + b`,
/// `b = b + a` — no order sees both olds; needs owned-temp staging, not in this brick).
///
/// PURE-VAR ALIAS HAZARD: a carried scalar whose new value is exactly ANOTHER carried param
/// (`start = pos`) cannot be staged in a copy temp — `let t = pos` ALIASES pos's local, so the
/// later `start = t` reads pos's ALREADY-updated value (off-by-one). A COMPUTED arg (`pos + 1`)
/// stages a fresh value and is fine. Wall the pure-var-aliasing form (rare; the parser loops use
/// computed indices like `pos + 1`).
fn simultaneous_update_safe(
    calls: &[&[IrExpr]],
    params2: &[almide_ir::IrParam],
    carried: &[bool],
) -> bool {
    let n = params2.len();
    for c in calls {
        let changed_heap: Vec<usize> = (0..n)
            .filter(|&i| {
                carried[i]
                    && is_heap_ty(&params2[i].ty)
                    && !matches!(&c[i].kind, IrExprKind::Var { id } if *id == params2[i].var)
            })
            .collect();
        if order_heap_accs_by_read_dep(&changed_heap, c, params2).is_none() {
            return false; // a heap-accumulator read cycle — unsupported
        }
    }
    let carried_scalars: std::collections::BTreeSet<VarId> = (0..n)
        .filter(|&i| carried[i] && !is_heap_ty(&params2[i].ty))
        .map(|i| params2[i].var)
        .collect();
    for c in calls {
        for i in 0..n {
            if carried[i] {
                if let IrExprKind::Var { id } = &c[i].kind {
                    if *id != params2[i].var && carried_scalars.contains(id) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Does the base value have to be carried out through an in-loop RESULT
/// ACCUMULATOR (`Some(true)`), or may the post-loop dispatch recompute it
/// (`Some(false)`)? `None` = the accumulator is REQUIRED but has no sound
/// materializer for this result type — decline the TCO entirely (the function
/// keeps its memory-safe non-TCO form, never the dispatch's use-after-free).
///
/// The accumulator is required when a base reads a LOOP-BODY-LOCAL binding —
/// a `let`/destructure in the loop body (e.g. `let (field, np) = pf(…)`) —
/// which is dead in the post-loop dispatch (the parse_rows_rec
/// use-after-free); `free_vars(base)` excludes anything the base binds
/// internally, so the intersection is exactly the loop-body bindings the base
/// READS from an enclosing scope. brick 2: a Value-CONTAINING tuple result
/// must ALSO route to the accumulator — the dispatch recomputes a tuple base
/// from the carried params, but a tuple whose base holds a
/// `value.object(..)`/`value.str(..)` CALL alongside a sibling scalar carry
/// reads the scalar STALE (pos=0 not the loop's final pos). A Value-FREE
/// tuple (csv `pf`'s `(acc, pos)`) works via the dispatch and routing it
/// regresses parse_rows_rec, so the gate is strictly a Value-containing tuple.
///
/// The in-loop accumulator materializes each base via the cap-as-tag heap-Ok
/// Result block (`materialize_result_str` over `lower_result_str_piece`),
/// which needs a HEAP Ok payload. A `Result[Unit/scalar, String]` (`ok(())`)
/// has no heap Ok piece for that path, and routing it through the len-as-tag
/// materializers instead would drift from the empty-init / drop repr (the
/// Result-repr-drift hole). A `Result[Option[<nested-heap>], String]` Ok
/// payload (read_message: `Option[JsonRpcRequest]`) is also unsound there —
/// `materialize_result_str` would mask the Option block with the flat
/// `DropListStr`, LEAKING the nested record/Value inside the Some
/// (`Option[String]`, a flat 0-or-1 `DropListStr` block, is EXCLUDED — its
/// accumulator IS sound, so it keeps TCO). For both shapes the function falls
/// to the proven REAL recursive lowering (control_p4 self-call arm;
/// input-bounded, byte-matches v0).
fn tco_result_acc_mode(work_ref: &IrExpr, base_exprs: &[IrExpr], ret_ty: &Ty) -> Option<bool> {
    use almide_lang::types::constructor::TypeConstructorId;
    let loop_lets = almide_ir::free_vars::bound_vars(work_ref);
    let base_reads_loop_local = base_exprs.iter().any(|b| {
        almide_ir::free_vars::free_vars(b, &std::collections::HashSet::new())
            .iter()
            .any(|v| loop_lets.contains(v))
    });
    let tuple_with_value = matches!(ret_ty, Ty::Tuple(tys) if tys.iter().any(is_value_ty));
    let non_heap_ok_result = matches!(ret_ty,
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && !is_heap_ty(&a[0]));
    let option_nested_ok_result = matches!(ret_ty,
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(&a[1], Ty::String)
            && matches!(&a[0], Ty::Applied(TypeConstructorId::Option, oa)
                if oa.len() == 1 && is_heap_ty(&oa[0]) && !matches!(&oa[0], Ty::String)));
    if (base_reads_loop_local || tuple_with_value)
        && (non_heap_ok_result || option_nested_ok_result)
    {
        return None;
    }
    Some(base_reads_loop_local || tuple_with_value)
}

/// `rk == k` (the loop guard uses `rk == 0`; the post-loop dispatch uses `rk == <base kind>`).
fn tco_eq_rk(rk: VarId, k: i64) -> IrExpr {
    tco_ir(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::Eq,
            left: Box::new(tco_ir(IrExprKind::Var { id: rk }, Ty::Int)),
            right: Box::new(tco_ir(IrExprKind::LitInt { value: k }, Ty::Int)),
        },
        Ty::Bool,
    )
}

/// The post-loop dispatch `if rk == 1 then base_1 else if … else base_N` —
/// recomputes the hit base from the carried params.
fn tco_post_dispatch(base_exprs: &[IrExpr], rk: VarId, ret_ty: &Ty) -> Option<IrExpr> {
    let mut post = base_exprs.last()?.clone();
    for (idx, base) in base_exprs.iter().enumerate().rev().skip(1) {
        post = tco_ir(
            IrExprKind::If {
                cond: Box::new(tco_eq_rk(rk, (idx + 1) as i64)),
                then: Box::new(base.clone()),
                else_: Box::new(post),
            },
            ret_ty.clone(),
        );
    }
    Some(post)
}

/// The loop-state init binds, in order: `[var idx = 0;] [var result = <empty>;]
/// var rk = 0;`. The result accumulator (when used) starts at an empty value of
/// the result type — a placeholder the first base case overwrites IN the loop;
/// declared mutable so the in-loop base assigns it.
fn tco_state_inits(
    idx_var: Option<VarId>,
    result_var: Option<VarId>,
    rk: VarId,
    ret_ty: &Ty,
) -> Vec<IrStmt> {
    let mut inits: Vec<IrStmt> = Vec::new();
    if let Some(iv) = idx_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: iv,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Int,
                value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
            },
            span: None,
        });
    }
    if let Some(rv) = result_var {
        inits.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: rv,
                mutability: almide_ir::Mutability::Var,
                ty: ret_ty.clone(),
                value: tco_empty_for(ret_ty).expect("checked Some above"),
            },
            span: None,
        });
    }
    inits.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: rk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Int,
            value: tco_ir(IrExprKind::LitInt { value: 0 }, Ty::Int),
        },
        span: None,
    });
    inits
}

