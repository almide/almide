
/// LOOP EARLY-RETURN desugar (a pre-lowering program pass, shared chain like the
/// guard passes above): a `guard c else E` INSIDE a `while` body where `E` has the
/// FUNCTION's Result type is a function-level early return from the loop — v1 has
/// no mid-function Return, so it walls (the walled-real baseline 6). Rewrite it to
/// the FLAG+RESULT form the proven machinery already runs:
///
///   var __lr_set = false
///   var __lr_val: Ret = err("")               // seed, only read when set
///   while (__lr_set == false) and cond {
///     …; if c then { <rest of body> }
///        else { __lr_val = E; __lr_set = true }
///   }
///   if __lr_set then __lr_val else { <post>; <tail> }
///
/// The guard's containing while AND every ANCESTOR while get the flag conjunct, so
/// a nested-loop early return (first_duplicate) unwinds both levels. SOUNDNESS
/// GUARDS (decline = keep the honest wall): the fn returns `Result[_, _]`; every
/// statement that can still run after the flag is set (the stmts AFTER a nested
/// while inside an outer body) is call-free (pure Assign/arith — running them once
/// more is unobservable); the guard's `E` type equals the fn's Result type.
///
/// #3058 widens it along both axes the corpus walled on. The LOOP: a top-level
/// `for x in xs` has no condition to conjoin, so its body is wrapped as
/// `if __lr_set == false { <body'> }` — the remaining iterations run empty, which
/// is unobservable. The RETURN: a fn whose body yields its value RAW — a pure fn,
/// or an effect fn the effect-ABI facts prove never errs — seeds `__lr_val` with
/// a literal of its scalar / Unit / String return (read only once the flag is
/// set, so the seed never escapes). A `guard .. else break` / `else continue` is
/// not a value return and is declined (its else holds a loop exit, which in a
/// Unit fn the type alone would not tell apart).
pub fn desugar_loop_early_returns(program: &mut almide_ir::IrProgram) {
    use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;

    fn contains_call(e: &IrExpr) -> bool {
        let mut found = false;
        struct C<'a> {
            found: &'a mut bool,
        }
        impl<'a> almide_ir::visit::IrVisitor for C<'a> {
            fn visit_expr(&mut self, e: &IrExpr) {
                // A loop exit counts too (#3058): a `guard .. else break` /
                // `else continue` is not a value return, and in a Unit fn its
                // type alone would not tell them apart.
                if matches!(e.kind, IrExprKind::Call { .. } | IrExprKind::Break | IrExprKind::Continue) {
                    *self.found = true;
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut C { found: &mut found }, e);
        found
    }
    fn stmt_has_call(s: &IrStmt) -> bool {
        match &s.kind {
            IrStmtKind::Bind { value, .. }
            | IrStmtKind::Assign { value, .. }
            | IrStmtKind::Expr { expr: value } => contains_call(value),
            _ => true, // anything unusual — decline conservatively
        }
    }

    /// Rewrite the guard inside `body` (recursing into nested whiles). Returns
    /// true iff a guard was rewritten somewhere below; conjoins the flag onto
    /// every while on the path. `set`/`val` are the flag/result VarIds.
    fn rewrite_body(
        body: &mut Vec<IrStmt>,
        ret_ty: &Ty,
        set: almide_ir::VarId,
        val: almide_ir::VarId,
    ) -> Option<bool> {
        // find a top-level guard with E : ret_ty
        let gpos = body.iter().position(|s| matches!(&s.kind,
            IrStmtKind::Guard { else_, .. } if else_.ty == *ret_ty && !contains_call(else_)));
        if let Some(gi) = gpos {
            let rest: Vec<IrStmt> = body.split_off(gi + 1);
            let Some(IrStmt { kind: IrStmtKind::Guard { cond, else_ }, span }) = body.pop().map(|s| s)
            else {
                unreachable!()
            };
            let set_stmts = vec![
                IrStmt {
                    kind: IrStmtKind::Assign { var: val, value: else_ },
                    span: None,
                },
                IrStmt {
                    kind: IrStmtKind::Assign {
                        var: set,
                        value: IrExpr {
                            kind: IrExprKind::LitBool { value: true },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                },
            ];
            let mk_block = |stmts: Vec<IrStmt>| IrExpr {
                kind: IrExprKind::Block {
                    stmts,
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::Unit,
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    })),
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            };
            body.push(IrStmt {
                kind: IrStmtKind::Expr {
                    expr: IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(mk_block(rest)),
                            else_: Box::new(mk_block(set_stmts)),
                        },
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    },
                },
                span,
            });
            return Some(true);
        }
        // recurse into ONE nested while (the first that rewrites)
        for wi in 0..body.len() {
            let IrStmtKind::Expr { expr } = &mut body[wi].kind else { continue };
            let IrExprKind::While { cond, body: inner } = &mut expr.kind else { continue };
            if let Some(true) = rewrite_body(inner, ret_ty, set, val) {
                conjoin_flag(cond, set);
                // Every stmt after the nested while may run ONCE MORE with the
                // flag set — decline unless all are call-free (pure updates).
                if body[wi + 1..].iter().any(stmt_has_call) {
                    return None;
                }
                return Some(true);
            }
        }
        Some(false)
    }

    fn conjoin_flag(cond: &mut Box<IrExpr>, set: almide_ir::VarId) {
        let not_set = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: set },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                }),
                right: Box::new(IrExpr {
                    kind: IrExprKind::LitBool { value: false },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                }),
            },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        };
        let old = std::mem::replace(
            cond,
            Box::new(IrExpr {
                kind: IrExprKind::Unit,
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }),
        );
        *cond = Box::new(IrExpr {
            kind: IrExprKind::BinOp { op: BinOp::And, left: Box::new(not_set), right: old },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        });
    }

    /// The `__lr_val` seed, read only once the flag is set: `err("")` for a
    /// `Result[_, String]` fn (#3058: and, for a fn whose body returns its
    /// value RAW — pure, or a never-err effect fn — the zero of a scalar,
    /// `()`, or `""`).
    fn seed_for(ret_ty: &Ty, raw: bool) -> Option<IrExpr> {
        let lit = |kind: IrExprKind, ty: Ty| Some(IrExpr { kind, ty, span: None, def_id: None });
        match ret_ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String) => lit(
                IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: String::new() },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
                ret_ty.clone(),
            ),
            _ if !raw => None,
            Ty::Int => lit(IrExprKind::LitInt { value: 0 }, Ty::Int),
            Ty::Bool => lit(IrExprKind::LitBool { value: false }, Ty::Bool),
            Ty::Float => lit(IrExprKind::LitFloat { value: 0.0 }, Ty::Float),
            Ty::Unit => lit(IrExprKind::Unit, Ty::Unit),
            Ty::String => lit(IrExprKind::LitStr { value: String::new() }, Ty::String),
            _ => None,
        }
    }

    /// `for x in xs { body }` → `for x in xs { if __lr_set == false { body' } }`:
    /// a for-in has no condition to conjoin, so the flag guards the body and
    /// the remaining iterations run empty.
    fn rewrite_for_body(wbody: &mut Vec<IrStmt>, ret_ty: &Ty, set: almide_ir::VarId, val: almide_ir::VarId) -> bool {
        let mut wb = wbody.clone();
        if rewrite_body(&mut wb, ret_ty, set, val) != Some(true) {
            return false;
        }
        let mut cond = Box::new(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None, def_id: None });
        conjoin_flag(&mut cond, set);
        let IrExprKind::BinOp { left: not_set, .. } = cond.kind else { return false };
        let guarded = IrExpr {
            kind: IrExprKind::If {
                cond: not_set,
                then: Box::new(IrExpr {
                    kind: IrExprKind::Block {
                        stmts: wb,
                        expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None })),
                    },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                }),
                else_: Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }),
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        };
        *wbody = vec![IrStmt { kind: IrStmtKind::Expr { expr: guarded }, span: None }];
        true
    }

    fn rewrite_fn(body: &mut IrExpr, ret_ty: &Ty, raw: bool, vt: &mut VarTable) {
        let Some(seed) = seed_for(ret_ty, raw) else { return };
        let IrExprKind::Block { stmts, expr: tail } = &mut body.kind else { return };
        let Some(tail_e) = tail.as_deref_mut() else { return };
        for wi in 0..stmts.len() {
            let IrStmtKind::Expr { expr } = &stmts[wi].kind else { continue };
            if !matches!(&expr.kind, IrExprKind::While { .. } | IrExprKind::ForIn { .. }) {
                continue;
            }
            let set = vt.alloc(
                almide_lang::intern::sym("__lr_set"),
                Ty::Bool,
                Mutability::Var,
                None,
            );
            let val = vt.alloc(
                almide_lang::intern::sym("__lr_val"),
                ret_ty.clone(),
                Mutability::Var,
                None,
            );
            // try the rewrite on a CLONE — commit only on success.
            let IrStmtKind::Expr { expr } = &mut stmts[wi].kind else { unreachable!() };
            match &mut expr.kind {
                IrExprKind::While { cond, body: wbody } => {
                    let mut wb = wbody.clone();
                    match rewrite_body(&mut wb, ret_ty, set, val) {
                        Some(true) => {
                            *wbody = wb;
                            conjoin_flag(cond, set);
                        }
                        _ => continue,
                    }
                }
                IrExprKind::ForIn { body: wbody, .. } => {
                    if !rewrite_for_body(wbody, ret_ty, set, val) {
                        continue;
                    }
                }
                _ => continue,
            }
            // binds BEFORE the loop
            stmts.insert(
                wi,
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: set,
                        mutability: Mutability::Var,
                        ty: Ty::Bool,
                        value: IrExpr {
                            kind: IrExprKind::LitBool { value: false },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                },
            );
            stmts.insert(
                wi + 1,
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: val,
                        mutability: Mutability::Var,
                        ty: ret_ty.clone(),
                        value: seed,
                    },
                    span: None,
                },
            );
            // post-loop continuation → the else of the flag dispatch
            let post: Vec<IrStmt> = stmts.split_off(wi + 3);
            let old_tail = tail_e.clone();
            let else_block = if post.is_empty() {
                old_tail
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts: post, expr: Some(Box::new(old_tail)) },
                    ty: ret_ty.clone(),
                    span: None,
                    def_id: None,
                }
            };
            *tail_e = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: set },
                        ty: Ty::Bool,
                        span: None,
                        def_id: None,
                    }),
                    then: Box::new(IrExpr { kind: IrExprKind::Var { id: val }, ty: ret_ty.clone(), span: None, def_id: None }),
                    else_: Box::new(else_block),
                },
                ty: ret_ty.clone(),
                span: None,
                def_id: None,
            };
            return; // one loop per fn (the corpus shape); later loops keep the wall
        }
    }

    // #3058: a fn whose body returns its value RAW — a pure fn, or an effect
    // fn that can never err (its ABI is the raw value) — takes a raw seed.
    let facts = almide_ir::effect_abi::effect_abi_facts(program);
    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for (module, func) in functions
        .iter_mut()
        .map(|f| (None, f))
        .chain(modules.iter_mut().flat_map(|m| {
            let name = m.name.as_str().to_string();
            m.functions.iter_mut().map(move |f| (Some(name.clone()), f))
        }))
    {
        let key = match &module {
            None => func.name.as_str().to_string(),
            Some(m) => format!("{m}.{}", func.name.as_str()),
        };
        let raw = !func.is_effect || facts.never_err_lifted.contains(&key);
        let ret_ty = func.ret_ty.clone();
        rewrite_fn(&mut func.body, &ret_ty, raw, var_table);
    }
}

/// SPREAD-BASE HOIST (a pre-lowering program pass, shared chain like the passes
/// above): a record spread whose BASE is a fn CALL (`let c = { ...toplib.mk(),
/// name: "w" }` — #502) had no faithful inline lowering: the strict path emitted
/// the callee as a dst-less bare call (its i32 result REMAINED ON THE WASM STACK
/// — invalid wasm) and deferred `c` to an Opaque. Hoist the base to its own bind
/// — `let __sb = toplib.mk(); let c = { ...__sb, … }` — so the call result is a
/// MATERIALIZED record (the binds_p2 aggregate seeding) and the spread takes the
/// proven spread-of-var path. Bind/Assign statement positions, every block depth;
/// call-count-invariant (the call node MOVES, never duplicates).
pub fn hoist_spread_call_bases(program: &mut almide_ir::IrProgram) {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};

    /// Per-statement worker, extracted out of `rewrite_block`'s loop body (the loop
    /// SKELETON — index bookkeeping + the insert — stays in `rewrite_block`; only the
    /// "does this ONE statement need a spread-base hoist, and if so what" decision
    /// moves here). A pure function of one `&mut IrStmt`, no state shared across
    /// statements, so the split changes nothing observable.
    fn compute_spread_base_hoist(
        stmt: &mut IrStmt,
        vt: &mut VarTable,
    ) -> Option<(almide_ir::VarId, Ty, IrExpr)> {
        match &mut stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                // recurse into nested blocks first
                rewrite_expr(value, vt);
                let IrExprKind::SpreadRecord { base, .. } = &mut value.kind else { return None };
                if !matches!(base.kind, IrExprKind::Call { .. }) {
                    return None;
                }
                let bty = base.ty.clone();
                let sb = vt.alloc(
                    almide_lang::intern::sym("__spread_base"),
                    bty.clone(),
                    Mutability::Let,
                    None,
                );
                let call = std::mem::replace(
                    &mut **base,
                    IrExpr { kind: IrExprKind::Var { id: sb }, ty: bty.clone(), span: None, def_id: None },
                );
                Some((sb, bty, call))
            }
            IrStmtKind::Expr { expr } => {
                rewrite_expr(expr, vt);
                None
            }
            _ => None,
        }
    }

    fn rewrite_block(stmts: &mut Vec<IrStmt>, vt: &mut VarTable) {
        let mut i = 0;
        while i < stmts.len() {
            let hoist = compute_spread_base_hoist(&mut stmts[i], vt);
            if let Some((sb, bty, call)) = hoist {
                stmts.insert(
                    i,
                    IrStmt {
                        kind: IrStmtKind::Bind {
                            var: sb,
                            mutability: Mutability::Let,
                            ty: bty,
                            value: call,
                        },
                        span: None,
                    },
                );
                i += 1; // skip the inserted bind; the rewritten stmt is next
            }
            i += 1;
        }
    }

    fn rewrite_expr(e: &mut IrExpr, vt: &mut VarTable) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => {
                rewrite_block(stmts, vt);
                if let Some(t) = expr.as_deref_mut() {
                    rewrite_expr(t, vt);
                }
            }
            IrExprKind::If { cond, then, else_ } => {
                rewrite_expr(cond, vt);
                rewrite_expr(then, vt);
                rewrite_expr(else_, vt);
            }
            IrExprKind::While { cond, body } => {
                rewrite_expr(cond, vt);
                rewrite_block(body, vt);
            }
            _ => {}
        }
    }

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        rewrite_expr(&mut func.body, var_table);
    }
}
