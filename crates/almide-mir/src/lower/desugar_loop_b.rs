
/// See the module comment above: rewrite the FIRST `for` loop (in a `Result[T, E]`-returning block)
/// whose body contains an effect-`!` into the loop-carried error-flag form.
pub fn desugar_loop_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // The enclosing result must be `Result[T, E]`. `E` is gated to `String`: the accumulator's
    // owned-copy (`$x ++ ""`, see `loop_uw_err_arm`) and `""` seed are String-specific, and a
    // String error is the effect-fn norm (it covers every porta wall). A non-String `E` declines
    // (the `!` is left to WALL — never a silent miscompile).
    let (ok_scalar_ty, err_ty) = match &body.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String) => {
            (a[0].clone(), a[1].clone())
        }
        _ => return None,
    };
    let empty_err = tco_empty_for(&err_ty)?;
    // The FIRST `for`/`while` loop whose body holds an `!` OR a VALUE early-exit (`guard c
    // else ok(n)` — the heterogeneous-else form desugar_guard leaves in a loop body). A
    // `while` needs its FLAGS INJECTED INTO THE CONDITION (the body-guard alone would spin
    // forever once an exit fires — the induction update lives in the now-skipped body; this
    // is exactly why `while` was originally excluded).
    let body_has_value_exit = |lbody: &[IrStmt]| -> bool {
        // BOTH forms: the raw `guard c else <value>` statement (this pass can run BEFORE
        // desugar_guard's loop-body rewrite in the shared fixpoint), and its desugared
        // heterogeneous-else `if` form.
        fn stmt_guard_value_exit(s: &IrStmt, ret_ty: &Ty) -> bool {
            match &s.kind {
                IrStmtKind::Guard { else_, .. } => else_.ty == *ret_ty,
                IrStmtKind::Expr { expr } | IrStmtKind::Bind { value: expr, .. } => {
                    expr_has_value_exit(
                        expr,
                        Some((VarId(u32::MAX), VarId(u32::MAX), ret_ty)),
                    )
                }
                _ => false,
            }
        }
        lbody.iter().any(|s| stmt_guard_value_exit(s, &body.ty))
    };
    // Detection fires ONLY on `!`-bearing bodies (the legacy criterion): a value-exit-only
    // loop has nothing this pass can rewrite while the value-exit delivery is disabled, and
    // firing on it would make the desugar fixpoint re-enter forever (the entry fast-path
    // returns an unchanged clone as `Some` — a probe-confirmed stack overflow).
    let loop_idx = stmts.iter().position(|s| match &s.kind {
        IrStmtKind::Expr { expr } => match &expr.kind {
            IrExprKind::ForIn { body: lbody, .. } | IrExprKind::While { body: lbody, .. } => {
                lbody.iter().any(stmt_has_unwrap)
            }
            _ => false,
        },
        _ => false,
    })?;
    let IrStmtKind::Expr { expr: loop_expr } = &stmts[loop_idx].kind else {
        return None;
    };
    let (for_parts, while_parts, lbody) = match &loop_expr.kind {
        IrExprKind::ForIn { var, var_tuple, iterable, body: lbody } => {
            (Some((var, var_tuple, iterable)), None, lbody)
        }
        IrExprKind::While { cond, body: lbody } => (None, Some(cond), lbody),
        _ => return None,
    };
    // The VALUE-exit pair — allocated ONLY when the body carries one (existing `!`-only
    // loops keep their exact prior shape, zero churn). ENABLED now that both B127-recorded
    // lower-layer gaps are closed: (a) the lp5 conditional-heap-reassign silent drop is
    // fixed by `desugar_unit_if_heap_reassign` (the post-loop `if __vf then { __r1 =
    // ok(__vn) }` delivery below is EXACTLY that shape — the SSA pass rewrites it to a
    // let-bound value-`if` the proven heap-result-`if` machinery lowers); (b) the
    // statement-if fold below already keeps the terminal dispatch ONE level (no nested
    // per-arm `__ev` release — the CBranch-expressible shape). GATED `tail_foldable`:
    // the loop must be the block's FINAL statement and the tail CALL-FREE (moving it
    // into the `__r1` init below is then count- and effect-invariant), the Ok payload
    // SCALAR (the `__vn` slot is an i64 local). Anything else keeps the honest wall.
    let has_vx = body_has_value_exit(lbody);
    if has_vx {
        let tail_foldable = stmts[loop_idx + 1..].is_empty()
            && tail.as_deref().is_some_and(|t| !crate::lower::expr_contains_call(t))
            && !is_heap_ty(&ok_scalar_ty);
        if !tail_foldable {
            return None;
        }
    }
    let ef = VarId(*next_var);
    let ev = VarId(*next_var + 1);
    *next_var += 2;
    let (vf, vres): (Option<VarId>, Option<VarId>) = if has_vx {
        let f = VarId(*next_var);
        let r = VarId(*next_var + 1);
        *next_var += 2;
        (Some(f), Some(r))
    } else {
        (None, None)
    };
    // Rewrite the loop body's `!`s (declining the whole pass if any cannot be cleanly placed).
    let body_block =
        loop_uw_node(IrExprKind::Block { stmts: lbody.clone(), expr: None }, Ty::Unit);
    let vx = match (vf, vres) {
        (Some(f), Some(r)) => Some((f, r, &body.ty)),
        _ => None,
    };
    let rewritten = loop_uw_rewrite(&body_block, ef, ev, &err_ty, vx, next_var)?;
    // The combined not-exited condition: `not __ef` (and `not __vf` when the value pair
    // exists) — the ForIn body-guard / the While condition injection.
    let not_flag = |v: VarId| {
        loop_uw_node(
            IrExprKind::UnOp {
                op: almide_ir::UnOp::Not,
                operand: Box::new(loop_uw_node(IrExprKind::Var { id: v }, Ty::Bool)),
            },
            Ty::Bool,
        )
    };
    // Combine via the BRANCH-FREE 0/1 product (`MulInt` over Bool bits), NOT `and`:
    // the short-circuit `and` lowers to nested IfThen merges, and a merge nested
    // inside the loop's certificate region flushes as the always-rejecting poison
    // `{i|}` (flush_branch's conservative nested-delimiter rule) — the corpus-wall
    // unbacked-`+1` breach. Every factor here is a PURE flag/comparison, so eager
    // evaluation is effect-identical.
    let bool_prod = |a: IrExpr, b: IrExpr| {
        loop_uw_node(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::MulInt,
                left: Box::new(a),
                right: Box::new(b),
            },
            Ty::Bool,
        )
    };
    let mut not_exited = not_flag(ef);
    if let Some(f) = vf {
        not_exited = bool_prod(not_exited, not_flag(f));
    }
    let new_loop = if let Some((var, var_tuple, iterable)) = for_parts {
        // ForIn: guard the iteration body — `if <not-exited> then { <rewritten> } else ()`
        // (a finite iterable, so the remaining no-op iterations terminate).
        let guard_if = loop_uw_node(
            IrExprKind::If {
                cond: Box::new(not_exited),
                then: Box::new(rewritten),
                else_: Box::new(loop_uw_node(IrExprKind::Unit, Ty::Unit)),
            },
            Ty::Unit,
        );
        loop_uw_node(
            IrExprKind::ForIn {
                var: *var,
                var_tuple: var_tuple.clone(),
                iterable: iterable.clone(),
                body: vec![IrStmt { kind: IrStmtKind::Expr { expr: guard_if }, span: None }],
            },
            Ty::Unit,
        )
    } else {
        // While: INJECT the flags into the condition (`<not-exited> and cond`) — the body
        // holds the induction update, so a body-guard alone would never terminate after an
        // exit fires.
        let cond = while_parts.expect("for/while dichotomy");
        let new_cond = bool_prod(not_exited.clone(), (**cond).clone());
        loop_uw_node(
            IrExprKind::While {
                cond: Box::new(new_cond),
                body: vec![IrStmt { kind: IrStmtKind::Expr { expr: rewritten }, span: None }],
            },
            Ty::Unit,
        )
    };
    // `<stmts before loop>; var __ef=false; var __ev=<empty>; <new_loop>`.
    let mut new_stmts: Vec<IrStmt> = stmts[..loop_idx].to_vec();
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ef,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Bool,
            value: loop_uw_node(IrExprKind::LitBool { value: false }, Ty::Bool),
        },
        span: None,
    });
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ev,
            mutability: almide_ir::Mutability::Var,
            ty: err_ty.clone(),
            value: empty_err,
        },
        span: None,
    });
    // The VALUE-exit slot: `var __vres: <RetTy> = err("")` — a valid len-tag placeholder
    // (never read unless `__vf` was set, which always assigns first). Bound BEFORE the loop.
    if let (Some(f), Some(r)) = (vf, vres) {
        new_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: f,
                mutability: almide_ir::Mutability::Var,
                ty: Ty::Bool,
                value: loop_uw_node(IrExprKind::LitBool { value: false }, Ty::Bool),
            },
            span: None,
        });
        // `var __vn: <T> = 0` — the SCALAR Ok-payload slot (never read unless `__vf`).
        new_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: r,
                mutability: almide_ir::Mutability::Var,
                ty: ok_scalar_ty.clone(),
                value: loop_uw_node(IrExprKind::LitInt { value: 0 }, ok_scalar_ty.clone()),
            },
            span: None,
        });
    }
    new_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: new_loop }, span: None });
    // Post-loop dispatch. LEGACY (no value-exit): `if __ef then err(__ev) else { <post> }` —
    // the shipped one-level shape, untouched. VALUE-exit variant: the SECOND dispatch level
    // must NOT nest inside the terminal branch (each nested terminal arm re-releases the
    // fn-scope `__ev` slot per-path — a per-object event pattern the v4 certificate's
    // CBranch cannot express; `flush_branch` then emits its designed rejecting poison
    // `{i|}` = the corpus-wall unbacked-`+1` breach). Fold it as a STATEMENT-if slot
    // assignment instead (dst-less branches open NO certificate frame — the same reason the
    // in-loop unwrap matches stay clean):
    //   `var __r1 = <tail>; if __vf then { __r1 = ok(__vn) } else (); if __ef then err(__ev) else __r1`
    // Gated by `tail_foldable` (call-free tail, loop is the final stmt), so moving the tail
    // into the init is count- and effect-invariant.
    let post = if let (Some(f), Some(r)) = (vf, vres) {
        let r1 = VarId(*next_var);
        *next_var += 1;
        new_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: r1,
                mutability: almide_ir::Mutability::Var,
                ty: body.ty.clone(),
                value: (**tail.as_ref().expect("tail_foldable gate")).clone(),
            },
            span: None,
        });
        let assign_ok = IrStmt {
            kind: IrStmtKind::Assign {
                var: r1,
                value: loop_uw_node(
                    IrExprKind::ResultOk {
                        expr: Box::new(loop_uw_node(
                            IrExprKind::Var { id: r },
                            ok_scalar_ty.clone(),
                        )),
                    },
                    body.ty.clone(),
                ),
            },
            span: None,
        };
        new_stmts.push(IrStmt {
            kind: IrStmtKind::Expr {
                expr: loop_uw_node(
                    IrExprKind::If {
                        cond: Box::new(loop_uw_node(IrExprKind::Var { id: f }, Ty::Bool)),
                        then: Box::new(loop_uw_node(
                            IrExprKind::Block { stmts: vec![assign_ok], expr: None },
                            Ty::Unit,
                        )),
                        else_: Box::new(loop_uw_node(IrExprKind::Unit, Ty::Unit)),
                    },
                    Ty::Unit,
                ),
            },
            span: None,
        });
        loop_uw_node(IrExprKind::Var { id: r1 }, body.ty.clone())
    } else {
        loop_uw_node(
            IrExprKind::Block { stmts: stmts[loop_idx + 1..].to_vec(), expr: tail.clone() },
            body.ty.clone(),
        )
    };
    // VALUE-exit variant: the err payload is an OWNED COPY (`__ev ++ ""` — the same
    // trick `loop_uw_err_arm` uses for the slot assign). The tail-duplicated dispatch
    // nests `err(__ev)` TWICE (once per `__vf` arm); a raw Var payload made the nested
    // instance's release-parity sweep double-release `__ev` on the (vf=0, ef=1) path
    // (probe: rc_dec memory fault on the error string's bytes). A fresh owned payload
    // removes `__ev` from every arm's parity set — the fn-scope slot is released exactly
    // once at scope end on every path. Legacy `!`-only loops keep the raw Var (their
    // one-level dispatch is the proven shipped shape).
    let err_payload = if has_vx {
        loop_uw_node(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(loop_uw_node(IrExprKind::Var { id: ev }, err_ty.clone())),
                right: Box::new(loop_uw_node(
                    IrExprKind::LitStr { value: String::new() },
                    Ty::String,
                )),
            },
            err_ty.clone(),
        )
    } else {
        loop_uw_node(IrExprKind::Var { id: ev }, err_ty.clone())
    };
    let err_result = loop_uw_node(
        IrExprKind::ResultErr { expr: Box::new(err_payload) },
        body.ty.clone(),
    );
    let new_tail = loop_uw_node(
        IrExprKind::If {
            cond: Box::new(loop_uw_node(IrExprKind::Var { id: ef }, Ty::Bool)),
            then: Box::new(err_result),
            else_: Box::new(post),
        },
        body.ty.clone(),
    );
    Some(loop_uw_node(
        IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_tail)) },
        body.ty.clone(),
    ))
}


/// BREAK elimination — rewrite the FIRST `for`/`while` loop whose body carries a `break` into
/// the flag form: `var __bk = false` before the loop; each `break` (admitted ONLY as a WHOLE
/// `if` arm — the shape `guard c else break` desugars to, with the iteration's remainder nested
/// in the opposite arm, so nothing in the same iteration follows the flag-set) becomes
/// `{ __bk = true }`; a ForIn guards its body on `not __bk` (finite iterable — the remaining
/// no-op iterations terminate, the `desugar_loop_unwrap` precedent), a While injects
/// `not __bk and cond` (the body holds the induction update, so a body-guard alone would spin).
/// A `break`/`continue` anywhere else declines — the loop keeps its honest wall. Count-invariant
/// (flag literals only), so the shared-desugar caps accounting holds.
pub fn desugar_loop_break(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    fn scan_breaks(e: &IrExpr, any: &mut bool, bad: &mut bool) {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct S<'a> {
            any: &'a mut bool,
            bad: &'a mut bool,
        }
        impl IrVisitor for S<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    // A whole-arm break is consumed by the rewrite WITHOUT descending, so a
                    // visit reaching a BARE Break/Continue here is an unadmitted position.
                    IrExprKind::If { cond, then, else_ } => {
                        self.visit_expr(cond);
                        for arm in [then, else_] {
                            if matches!(&arm.kind, IrExprKind::Break) {
                                *self.any = true;
                            } else {
                                self.visit_expr(arm);
                            }
                        }
                    }
                    IrExprKind::Break | IrExprKind::Continue => *self.bad = true,
                    IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {} // own scope
                    _ => walk_expr(self, e),
                }
            }
        }
        S { any, bad }.visit_expr(e);
    }
    let has_admissible_break = |lbody: &[IrStmt]| -> Option<bool> {
        let blk = loop_uw_node(IrExprKind::Block { stmts: lbody.to_vec(), expr: None }, Ty::Unit);
        let (mut any, mut bad) = (false, false);
        scan_breaks(&blk, &mut any, &mut bad);
        if bad {
            return None; // unadmitted break/continue position — decline the whole pass
        }
        Some(any)
    };
    let mut loop_idx = None;
    for (i, s) in stmts.iter().enumerate() {
        if let IrStmtKind::Expr { expr } = &s.kind {
            if let IrExprKind::ForIn { body: lbody, .. } | IrExprKind::While { body: lbody, .. } =
                &expr.kind
            {
                match has_admissible_break(lbody) {
                    Some(true) => {
                        loop_idx = Some(i);
                        break;
                    }
                    Some(false) => continue,
                    Option::None => return None,
                }
            }
        }
    }
    let loop_idx = loop_idx?;
    let IrStmtKind::Expr { expr: loop_expr } = &stmts[loop_idx].kind else { return None };
    let bk = VarId(*next_var);
    *next_var += 1;
    fn rewrite_breaks(e: &IrExpr, bk: VarId) -> IrExpr {
        let mut out = e.clone();
        out = out.map_children(&mut |c| match &c.kind {
            IrExprKind::ForIn { .. } | IrExprKind::While { .. } => c, // own scope
            _ => rewrite_breaks(&c, bk),
        });
        if let IrExprKind::If { cond, then, else_ } = &out.kind {
            let fix = |arm: &IrExpr| -> IrExpr {
                if matches!(&arm.kind, IrExprKind::Break) {
                    loop_uw_node(
                        IrExprKind::Block {
                            stmts: vec![IrStmt {
                                kind: IrStmtKind::Assign {
                                    var: bk,
                                    value: loop_uw_node(
                                        IrExprKind::LitBool { value: true },
                                        Ty::Bool,
                                    ),
                                },
                                span: None,
                            }],
                            expr: None,
                        },
                        Ty::Unit,
                    )
                } else {
                    arm.clone()
                }
            };
            return loop_uw_node(
                IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(fix(then)),
                    else_: Box::new(fix(else_)),
                },
                out.ty.clone(),
            );
        }
        out
    }
    let not_bk = loop_uw_node(
        IrExprKind::UnOp {
            op: almide_ir::UnOp::Not,
            operand: Box::new(loop_uw_node(IrExprKind::Var { id: bk }, Ty::Bool)),
        },
        Ty::Bool,
    );
    let new_loop = match &loop_expr.kind {
        IrExprKind::ForIn { var, var_tuple, iterable, body: lbody } => {
            let blk = loop_uw_node(
                IrExprKind::Block { stmts: lbody.clone(), expr: None },
                Ty::Unit,
            );
            let rewritten = rewrite_breaks(&blk, bk);
            let guard_if = loop_uw_node(
                IrExprKind::If {
                    cond: Box::new(not_bk),
                    then: Box::new(rewritten),
                    else_: Box::new(loop_uw_node(IrExprKind::Unit, Ty::Unit)),
                },
                Ty::Unit,
            );
            loop_uw_node(
                IrExprKind::ForIn {
                    var: *var,
                    var_tuple: var_tuple.clone(),
                    iterable: iterable.clone(),
                    body: vec![IrStmt { kind: IrStmtKind::Expr { expr: guard_if }, span: None }],
                },
                Ty::Unit,
            )
        }
        IrExprKind::While { cond, body: lbody } => {
            let blk = loop_uw_node(
                IrExprKind::Block { stmts: lbody.clone(), expr: None },
                Ty::Unit,
            );
            let rewritten = rewrite_breaks(&blk, bk);
            let new_cond = loop_uw_node(
                IrExprKind::BinOp {
                    op: almide_ir::BinOp::And,
                    left: Box::new(not_bk),
                    right: Box::new((**cond).clone()),
                },
                Ty::Bool,
            );
            loop_uw_node(
                IrExprKind::While {
                    cond: Box::new(new_cond),
                    body: vec![IrStmt { kind: IrStmtKind::Expr { expr: rewritten }, span: None }],
                },
                Ty::Unit,
            )
        }
        _ => return None,
    };
    let mut new_stmts: Vec<IrStmt> = stmts[..loop_idx].to_vec();
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: bk,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Bool,
            value: loop_uw_node(IrExprKind::LitBool { value: false }, Ty::Bool),
        },
        span: None,
    });
    new_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: new_loop }, span: None });
    new_stmts.extend_from_slice(&stmts[loop_idx + 1..]);
    Some(loop_uw_node(
        IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
        body.ty.clone(),
    ))
}
