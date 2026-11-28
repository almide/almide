// ── part of pass_tco_loop_rewrite.rs, include!-spliced (split for the 800-line budget) ──

fn emit_tail_call_replacement(args: Vec<IrExpr>, f: &TailFrame<'_>) -> IrExpr {
    let TailFrame { params, temps, dec_params, owned_params, borrowed_params, .. } = *f;
    let mut stmts: Vec<IrStmt> = Vec::new();

    // F5 (#527): an IDENTITY CARRY — argument i is the bare Var of param i —
    // is a semantic no-op; emitting the temp anyway gave the temp bind a
    // Rule-1 alias-Inc with no Dec anywhere (the temp is Dec-exempt by name,
    // an unmanaged param has no per-iteration Dec): +1 rc per iteration, an
    // immortal param block and rc creep toward wrap on long loops. Skip both
    // the bind and the assign for those positions.
    //
    // Read through the `Borrow` wrapper (`carried_var`): a slot that KEEPS its
    // borrow reaches here as `Borrow(Var p)`, and binding that would emit
    // `let tmp = &mut p; p = tmp;` — a reference to a reference, not what the
    // slot holds. Seeing it as the identity carry it is also drops the
    // pre-existing no-op reborrow for a carried `Bytes` buffer. This agrees by
    // construction with [`tco_slot_args`], which decides from the same
    // helper which slots keep a borrow at all.
    let identity_carry: Vec<bool> = args.iter().enumerate().map(|(i, arg)| {
        carried_var(arg) == Some(params[i].0)
    }).collect();

    // Strip Borrow from arg unless this param position is kept-borrowed
    // (e.g. Bytes borrow preserved across iterations).
    let args: Vec<Option<IrExpr>> = args.into_iter().enumerate().map(|(i, arg)| {
        if identity_carry[i] { return None; }
        Some(if borrowed_params.contains(&i) { arg } else { strip_borrow(arg) })
    }).collect();

    let (moved, moved_sources) = movable_accumulators(&args, params, &identity_carry, owned_params);

    // Bind temporaries to argument expressions. A scalar argument that only
    // reads and computes (`acc + n.v`) has no effect to order, so it binds
    // FIRST: a heap argument after it is then the last read of the binder it
    // projects (`n.next`), and moves instead of copying the rest of the value
    // every iteration (#4249).
    let mut args: Vec<(usize, Option<IrExpr>)> = args.into_iter().enumerate().collect();
    args.sort_by_key(|(_, a)| !a.as_ref().is_some_and(|a| almide_ir::top_let_storage::clone_free(&a.ty) && reads_only(a)));
    for (i, arg) in args {
        let Some(arg) = arg else { continue };
        let (tmp_var, tmp_ty) = &temps[i];
        let value = move_either_way_sources(wrap_owned_reads_except(arg, owned_params, &moved), &moved_sources);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tmp_var,
                mutability: Mutability::Let,
                ty: tmp_ty.clone(),
                value,
            },
            span: None,
        });
    }

    // Free the OLD value of each dec-managed heap param BEFORE overwriting it.
    // The new values are already captured in the temps above (which only borrowed
    // the old params), so the old values are now dead and unaliased — Dec-ing them
    // is safe and is exactly the per-iteration free that closes the accumulator
    // leak. (Done after ALL temp binds so an arg that borrows several params still
    // reads their live old values first.)
    for p in dec_params {
        stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
    }

    // Assign params from temporaries (identity carries skipped — the param
    // already holds its value).
    for (i, (param_var, _)) in params.iter().enumerate() {
        if identity_carry[i] { continue; }
        let (tmp_var, tmp_ty) = &temps[i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: *param_var,
                value: IrExpr {
                    kind: IrExprKind::Var { id: *tmp_var },
                    ty: tmp_ty.clone(),
                    span: None, def_id: None,
                },
            },
            span: None,
        });
    }

    // Continue the loop
    let continue_expr = IrExpr {
        kind: IrExprKind::Continue,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(continue_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

/// Emit the base case: assign to result variable and break.
/// ```text
/// __tco_result = expr
/// break
/// ```
fn emit_base_case(expr: IrExpr, result_var: VarId, dec_params: &[VarId], owned_params: &HashSet<VarId>) -> IrExpr {
    // A heap-typed base-case result may MOVE a managed param out to the caller
    // (e.g. `then s` returns the accumulator). Freeing the param here would then
    // be a use-after-free on the returned value, so suppress the exit Dec for a
    // heap result and let the caller own it. A non-heap result (Int/Bool/…) cannot
    // carry a param out, so the params are dead after the assign and freeing the
    // final loop value here balances the entry `Inc`.
    let result_is_heap = tco_is_heap(&expr.ty);

    // The base case ends the loop (`break` follows; only `__tco_result` is
    // read afterwards), so it is a terminal block like the temp block above:
    // an owned param whose sole read here is a bare consuming Var MOVES into
    // the result — no reassignment requirement, the params die with the loop.
    // (`__tco_result = acc` was previously an O(n) parting clone.)
    let mut census: HashMap<VarId, OwnedReadCensus> = HashMap::new();
    census_owned_reads(&expr, owned_params, false, &mut census);
    let moved_by = |pred: fn(&OwnedReadCensus) -> bool| -> HashSet<VarId> {
        owned_params.iter().copied().filter(|p| census.get(p).is_some_and(pred)).collect()
    };
    let expr = wrap_owned_reads_except(expr, owned_params, &moved_by(OwnedReadCensus::only_bare));
    let expr = move_either_way_sources(expr, &moved_by(OwnedReadCensus::only_source));

    let assign = IrStmt {
        kind: IrStmtKind::Assign {
            var: result_var,
            value: expr,
        },
        span: None,
    };

    let mut stmts = vec![assign];
    if !result_is_heap {
        for p in dec_params {
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
        }
    }

    let break_expr = IrExpr {
        kind: IrExprKind::Break,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(break_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

// ── TCO-owned clone/move decisions ──────────────────────────────────
//
// CloneInsertion's blanket rule — "inside a loop, every consuming read
// clones" — is correct for a general loop but pessimal for the loop THIS
// pass constructs: every iteration path ends by either reassigning all
// non-identity params (`continue`) or leaving the loop for good (`break`),
// so a param's final read on a path is provably dead afterwards and may
// MOVE. On a list/string accumulator (`build(n - 1, acc + [x])` — the
// idiomatic Almide tail recursion) the per-iteration clone is O(len),
// turning the whole loop O(n²); the move makes it O(n) (AlmideConcat
// extends in place). For each param it can prove safe, this pass takes
// over ALL clone decisions (`tco_owned_params` — CloneInsertion skips the
// id): reads it cannot prove dead get an explicit `Clone` here, the one
// provably-final read stays a bare move. rustc's borrow checker re-proves
// every move — a wrong decision is a loud E0382/E0505 at codegen-output
// compile time, never a silent wrong value.

include!("pass_tco_owned_reads.rs");

/// An expression with no effect and no abort: variable and field reads,
/// literals, and the arithmetic, comparisons and logic that cannot trap (Int
/// `+ - *` wrap; `/` and `%` can abort, a call can do anything). Its value
/// does not depend on when it is evaluated among other such reads.
fn reads_only(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Var { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit => true,
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => reads_only(object),
        IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, mutable: false, .. } | IrExprKind::Clone { expr } => reads_only(expr),
        IrExprKind::UnOp { operand, .. } => reads_only(operand),
        IrExprKind::BinOp { op, left, right } => !matches!(
            op,
            BinOp::DivInt | BinOp::ModInt | BinOp::DivFloat | BinOp::ModFloat | BinOp::PowInt | BinOp::PowFloat
        ) && matches!(
            op,
            BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat
                | BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte | BinOp::And | BinOp::Or
        ) && reads_only(left) && reads_only(right),
        _ => false,
    }
}
