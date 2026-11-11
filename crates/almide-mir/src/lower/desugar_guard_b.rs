
/// LOOP EARLY-RETURN desugar (a pre-lowering program pass, shared chain like the
/// guard passes above): a `guard c else E` INSIDE a `while` body where `E` has the
/// FUNCTION's Result type is a function-level early return from the loop — v1 has
/// no mid-function Return, so it walls (the walled-real baseline 6). Rewrite it to
/// the FLAG+RESULT form the proven machinery already runs:
///
///   var __lr_set = false
///   var __lr_val: Ret = err("")               // seed, only read when set
///   while (if __lr_set then false else cond) {
///     …; if c then { <rest of body> }
///        else { __lr_val = E; __lr_set = true }
///   }
///   if __lr_set then __lr_val else { <post>; <tail> }
///
/// The containing loop and every enclosing loop check the flag. Guards
/// inside statement blocks, if arms and match arms share it; statements
/// following an exited scope are skipped. A while condition is evaluated
/// only while the function has not returned (Almide's `and` is eager).
/// The guard result must have the function's return type; unsupported
/// result expressions and loop-control guards leave the original IR intact.
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
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
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
    fn unit_block(stmts: Vec<IrStmt>) -> IrExpr {
        IrExpr {
            kind: IrExprKind::Block { stmts, expr: None },
            ty: Ty::Unit, span: None, def_id: None,
        }
    }

    fn flag(set: almide_ir::VarId) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id: set }, ty: Ty::Bool, span: None, def_id: None }
    }

    /// Guards in a value-producing branch still return from this function.
    /// Lambda bodies have their own return boundary.
    fn has_scope_guard(value: &IrExpr) -> bool {
        struct Find(bool);
        impl almide_ir::IrVisitor for Find {
            fn visit_expr(&mut self, e: &IrExpr) {
                if !matches!(e.kind, IrExprKind::Lambda { .. }) {
                    almide_ir::walk_expr(self, e);
                }
            }
            fn visit_stmt(&mut self, s: &IrStmt) {
                self.0 |= matches!(s.kind, IrStmtKind::Guard { .. });
                almide_ir::walk_stmt(self, s);
            }
        }
        let mut find = Find(false);
        almide_ir::IrVisitor::visit_expr(&mut find, value);
        find.0
    }

    /// Move a binding and its continuation into each value-producing leaf.
    /// The initializer's block statements stay before the binding, exposing
    /// their guards to the ordinary statement-scope early-return rewrite.
    /// No placeholder value is bound on a path that returns early.
    fn bind_continuation(value: &IrExpr, binding: &IrStmt, rest: &[IrStmt]) -> Option<IrExpr> {
        let mut out = value.clone();
        match &mut out.kind {
            IrExprKind::If { then, else_, .. } => {
                **then = bind_continuation(then, binding, rest)?;
                **else_ = bind_continuation(else_, binding, rest)?;
            }
            IrExprKind::Match { arms, .. } => {
                for arm in arms { arm.body = bind_continuation(&arm.body, binding, rest)?; }
            }
            IrExprKind::Block { stmts, expr } => {
                let tail = expr.as_deref().cloned().unwrap_or(IrExpr {
                    kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None,
                });
                stmts.push(IrStmt { span: tail.span, kind: IrStmtKind::Expr {
                    expr: bind_continuation(&tail, binding, rest)?,
                } });
                *expr = None;
            }
            _ => {
                // A guard hidden in a call operand needs its own evaluation-
                // order transform; do not leave it as a value-shaped exit.
                if has_scope_guard(value) { return None; }
                if almide_ir::diverge::diverges(value) {
                    return Some(unit_block(vec![IrStmt { span: value.span,
                        kind: IrStmtKind::Expr { expr: value.clone() } }]));
                }
                let mut bound = binding.clone();
                let IrStmtKind::Bind { value: initializer, .. } = &mut bound.kind else { return None };
                *initializer = value.clone();
                return Some(unit_block(std::iter::once(bound).chain(rest.iter().cloned()).collect()));
            }
        }
        out.ty = Ty::Unit;
        Some(out)
    }

    /// Walk statement scopes, never closure bodies. Once any nested scope
    /// returns, every following statement is skipped, including pure stores:
    /// they may change the already captured result through shared state.
    fn rewrite_body(body: &mut Vec<IrStmt>, ret_ty: &Ty, set: almide_ir::VarId, val: almide_ir::VarId) -> Option<bool> {
        let mut rest = Vec::new();
        let mut changed = false;
        for mut stmt in std::mem::take(body).into_iter().rev() {
            if let IrStmtKind::Bind { value, .. } = &stmt.kind {
                if has_scope_guard(value) {
                    let mut continuation = bind_continuation(value, &stmt, &rest)?;
                    if !rewrite_statement_scope(&mut continuation, ret_ty, set, val)? { return None; }
                    rest = vec![IrStmt { span: stmt.span, kind: IrStmtKind::Expr { expr: continuation } }];
                    changed = true;
                    continue;
                }
            }
            if let IrStmtKind::Guard { cond, mut else_ } = stmt.kind {
                // A bare `none` carries no payload type of its own; in an Option
                // fn it is the fn's `none` (the certified native render).
                if crate::lower::owned_tco_slots()
                    && matches!(else_.kind, IrExprKind::OptionNone)
                    && matches!(ret_ty, Ty::Applied(TypeConstructorId::Option, _))
                {
                    else_.ty = ret_ty.clone();
                }
                if else_.ty != *ret_ty || contains_call(&else_) { return None; }
                let exits = vec![
                    IrStmt { kind: IrStmtKind::Assign { var: val, value: else_ }, span: None },
                    IrStmt { kind: IrStmtKind::Assign { var: set, value: IrExpr {
                        kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None, def_id: None,
                    } }, span: None },
                ];
                // `guard false else E` (what the front end leaves of a `guard let`
                // else in a loop) always exits: on the certified native render the
                // exit stands alone, with no constant `if` around it and none of
                // the statements it can never reach.
                let always = crate::lower::owned_tco_slots() && matches!(cond.kind, IrExprKind::LitBool { value: false });
                stmt.kind = IrStmtKind::Expr { expr: if always { unit_block(exits) } else { IrExpr {
                    kind: IrExprKind::If { cond: Box::new(cond), then: Box::new(unit_block(rest)), else_: Box::new(unit_block(exits)) },
                    ty: Ty::Unit, span: stmt.span, def_id: None,
                } } };
                rest = vec![stmt];
                changed = true;
            } else {
                let nested = match &mut stmt.kind {
                    IrStmtKind::Expr { expr } => rewrite_statement_scope(expr, ret_ty, set, val)?,
                    _ => false,
                };
                if nested && !rest.is_empty() {
                    rest = vec![IrStmt { kind: IrStmtKind::Expr { expr: IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(flag(set)), then: Box::new(unit_block(Vec::new())), else_: Box::new(unit_block(rest)),
                        }, ty: Ty::Unit, span: None, def_id: None,
                    } }, span: None }];
                }
                rest.insert(0, stmt);
                changed |= nested;
            }
        }
        *body = rest;
        Some(changed)
    }

    fn rewrite_statement_scope(e: &mut IrExpr, ret_ty: &Ty, set: almide_ir::VarId, val: almide_ir::VarId) -> Option<bool> {
        match &mut e.kind {
            IrExprKind::While { cond, body } => {
                let changed = rewrite_body(body, ret_ty, set, val)?;
                if changed { conjoin_flag(cond, set); }
                Some(changed)
            }
            IrExprKind::ForIn { body, .. } => {
                let changed = rewrite_body(body, ret_ty, set, val)?;
                if changed { guard_for_body(body, set); }
                Some(changed)
            }
            IrExprKind::Block { stmts, expr } => {
                // A block's tail in statement position is also discarded;
                // keep its effects but do not evaluate it after an exit.
                if let Some(tail) = expr.take() {
                    stmts.push(IrStmt { span: tail.span, kind: IrStmtKind::Expr { expr: *tail } });
                }
                let changed = rewrite_body(stmts, ret_ty, set, val)?;
                Some(changed)
            }
            IrExprKind::If { then, else_, .. } => {
                let a = rewrite_statement_scope(then, ret_ty, set, val)?;
                let b = rewrite_statement_scope(else_, ret_ty, set, val)?;
                Some(a || b)
            }
            IrExprKind::Match { arms, .. } => {
                let mut changed = false;
                for arm in arms { changed |= rewrite_statement_scope(&mut arm.body, ret_ty, set, val)?; }
                Some(changed)
            }
            _ => Some(false),
        }
    }

    fn conjoin_flag(cond: &mut Box<IrExpr>, set: almide_ir::VarId) {
        let old = (**cond).clone();
        // `and` is eager in Almide: an already returned loop must not
        // evaluate its condition again, even when that condition has effects.
        **cond = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(flag(set)),
                then: Box::new(IrExpr { kind: IrExprKind::LitBool { value: false }, ty: Ty::Bool, span: None, def_id: None }),
                else_: Box::new(old),
            }, ty: Ty::Bool, span: None, def_id: None,
        };
    }

    fn guard_for_body(body: &mut Vec<IrStmt>, set: almide_ir::VarId) {
        let rest = std::mem::take(body);
        body.push(IrStmt { kind: IrStmtKind::Expr { expr: IrExpr {
            kind: IrExprKind::If { cond: Box::new(flag(set)), then: Box::new(unit_block(Vec::new())), else_: Box::new(unit_block(rest)) },
            ty: Ty::Unit, span: None, def_id: None,
        } }, span: None });
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
            // The certified native render also seeds an Option return with `none`.
            Ty::Applied(TypeConstructorId::Option, _) if crate::lower::owned_tco_slots() => lit(IrExprKind::OptionNone, ret_ty.clone()),
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
        guard_for_body(&mut wb, set);
        *wbody = wb;
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
            let mut else_block = if post.is_empty() {
                old_tail
            } else {
                IrExpr {
                    kind: IrExprKind::Block { stmts: post, expr: Some(Box::new(old_tail)) },
                    ty: ret_ty.clone(),
                    span: None,
                    def_id: None,
                }
            };
            rewrite_fn(&mut else_block, ret_ty, raw, vt);
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
            return; // later loops were processed in the post-loop continuation
        }
        // No loop of this block exits. On the certified native render the fn's
        // value may still come from a loop further down its tail chain — in an
        // arm of a tail branch, where the exit lift (arm_exit.rs) puts the rest
        // of the block.
        if crate::lower::owned_tco_slots() {
            rewrite_tail_arms(tail_e, ret_ty, raw, vt);
        }
    }

    fn rewrite_tail_arms(e: &mut IrExpr, ret_ty: &Ty, raw: bool, vt: &mut VarTable) {
        match &mut e.kind {
            IrExprKind::Block { .. } => rewrite_fn(e, ret_ty, raw, vt),
            IrExprKind::If { then, else_, .. } => {
                rewrite_tail_arms(then, ret_ty, raw, vt);
                rewrite_tail_arms(else_, ret_ty, raw, vt);
            }
            IrExprKind::Match { arms, .. } => arms.iter_mut().for_each(|a| rewrite_tail_arms(&mut a.body, ret_ty, raw, vt)),
            _ => {}
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
