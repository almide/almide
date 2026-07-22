
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
                if matches!(e.kind, IrExprKind::Call { .. }) {
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

    fn rewrite_fn(body: &mut IrExpr, ret_ty: &Ty, vt: &mut VarTable) {
        // fn Result type with a String err — the err("") seed is exact.
        let is_res_str = matches!(ret_ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String));
        if !is_res_str {
            return;
        }
        let IrExprKind::Block { stmts, expr: tail } = &mut body.kind else { return };
        let Some(tail_e) = tail.as_deref_mut() else { return };
        for wi in 0..stmts.len() {
            let IrStmtKind::Expr { expr } = &stmts[wi].kind else { continue };
            let IrExprKind::While { .. } = &expr.kind else { continue };
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
            let IrExprKind::While { cond, body: wbody } = &mut expr.kind else { unreachable!() };
            let mut wb = wbody.clone();
            match rewrite_body(&mut wb, ret_ty, set, val) {
                Some(true) => {
                    *wbody = wb;
                    conjoin_flag(cond, set);
                }
                _ => continue,
            }
            // binds BEFORE the while
            let seed = IrExpr {
                kind: IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: String::new() },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                },
                ty: ret_ty.clone(),
                span: None,
                def_id: None,
            };
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
                    then: Box::new(IrExpr {
                        kind: IrExprKind::Var { id: val },
                        ty: ret_ty.clone(),
                        span: None,
                        def_id: None,
                    }),
                    else_: Box::new(else_block),
                },
                ty: ret_ty.clone(),
                span: None,
                def_id: None,
            };
            return; // one loop per fn (the corpus shape); later loops keep the wall
        }
    }

    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let ret_ty = func.ret_ty.clone();
        rewrite_fn(&mut func.body, &ret_ty, var_table);
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

/// RECORD-LITERAL ARG HOIST (a pre-lowering program pass, shared chain): a
/// SCALAR-result call carrying a RECORD-LITERAL argument (`10.0 |>
/// letlib.box_left({ top: 0.0, left: letlib.GAP })` — #785's shape) walls in the
/// scalar-bind route (the literal needs aggregate materialization the scalar
/// path cannot do). Hoist the literal to its own bind — `let __arg = { … };
/// letlib.box_left(__arg, 10.0)` — so it builds through the PROVEN record-bind
/// machinery and the call sees a materialized Var. Scoped EXACTLY to the walled
/// set (Bind/Assign value = a Named call with a scalar type and ≥1 record-literal
/// arg) so no already-lowering call path changes. Call-count-invariant.
///
/// `hoist_record_literal_args_in_fn` is the SINGLE-FUNCTION entry: the pipeline
/// re-runs it AFTER the pure-call global substitution (the ceangal/`#785` bridge
/// inlines `letlib.GAP` → `default_gap()` INTO record fields at that later stage
/// — the program-pass run cannot see those calls yet).
pub fn hoist_record_literal_args_in_fn(
    body: &mut almide_ir::IrExpr,
    vt: &mut almide_ir::VarTable,
) {
    hoist_rewrite_expr(body, vt);
}

pub fn hoist_record_literal_args(program: &mut almide_ir::IrProgram) {
    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions
        .iter_mut()
        .chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        hoist_rewrite_expr(&mut func.body, var_table);
    }
}

mod hoist_impl {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
    use almide_lang::types::Ty;

    fn is_scalar_ty(ty: &Ty) -> bool {
        matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
            || crate::lower::calls_p4_is_small_int(ty)
    }

    fn rewrite_block(stmts: &mut Vec<IrStmt>, vt: &mut VarTable) {
        let mut i = 0;
        while i < stmts.len() {
            let mut hoists: Vec<IrStmt> = Vec::new();
            match &mut stmts[i].kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                    rewrite_expr(value, vt);
                    // Guard-clause flattening of the former 2-deep nested-if wrapping this
                    // `for` (no `else` anywhere: an unmet condition just skips the arg-hoist
                    // below, falling through to the record-FIELD hoist pass after this block
                    // — unchanged, since `break` exits the labeled block and resumes there).
                    // No behavior change — see docs/roadmap/active/code-health-codopsy.md.
                    'call_arg_hoist: {
                        if !is_scalar_ty(&value.ty) {
                            break 'call_arg_hoist;
                        }
                        let IrExprKind::Call {
                            target: CallTarget::Named { .. } | CallTarget::Module { .. },
                            args,
                            ..
                        } = &mut value.kind
                        else {
                            break 'call_arg_hoist;
                        };
                        for a in args.iter_mut() {
                            if !matches!(
                                a.kind,
                                IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
                            ) {
                                continue;
                            }
                            let aty = a.ty.clone();
                            let av = vt.alloc(
                                almide_lang::intern::sym("__rec_arg"),
                                aty.clone(),
                                Mutability::Let,
                                None,
                            );
                            let lit = std::mem::replace(
                                a,
                                IrExpr {
                                    kind: IrExprKind::Var { id: av },
                                    ty: aty.clone(),
                                    span: None,
                                    def_id: None,
                                },
                            );
                            hoists.push(IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: av,
                                    mutability: Mutability::Let,
                                    ty: aty,
                                    value: lit,
                                },
                                span: None,
                            });
                        }
                    }
                    // A record-literal BIND whose FIELD is a scalar CALL (`left:
                    // letlib.GAP` — a call-initialized module top-let read reaches
                    // the IR as its init call): the field-position call emitted a
                    // dst-less bare call (result on the stack — invalid wasm).
                    // Hoist each scalar call field to its own bind, declaration
                    // order preserved (= v0's field evaluation order).
                    if let IrExprKind::Record { fields, .. } = &mut value.kind {
                        for (_, fe) in fields.iter_mut() {
                            if is_scalar_ty(&fe.ty)
                                && matches!(fe.kind, IrExprKind::Call { .. })
                            {
                                let fty = fe.ty.clone();
                                let fv = vt.alloc(
                                    almide_lang::intern::sym("__rec_fld"),
                                    fty.clone(),
                                    Mutability::Let,
                                    None,
                                );
                                let call = std::mem::replace(
                                    fe,
                                    IrExpr {
                                        kind: IrExprKind::Var { id: fv },
                                        ty: fty.clone(),
                                        span: None,
                                        def_id: None,
                                    },
                                );
                                hoists.push(IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: fv,
                                        mutability: Mutability::Let,
                                        ty: fty,
                                        value: call,
                                    },
                                    span: None,
                                });
                            }
                        }
                    }
                }
                IrStmtKind::Expr { expr } => rewrite_expr(expr, vt),
                _ => {}
            }
            let has_hoists = !hoists.is_empty();
            for (k, h) in hoists.into_iter().enumerate() {
                stmts.insert(i + k, h);
            }
            // Re-visit from the first inserted bind: a hoisted record-literal ARG
            // bind may itself carry call FIELDS (`let __rec_arg = { left:
            // default_gap() }` — the substituted #785 shape) that the field pass
            // must hoist in turn. Already-rewritten stmts are no-ops on re-visit
            // (their literals are Vars now), so this terminates.
            if !has_hoists {
                i += 1;
            }
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

    pub(crate) fn rewrite_expr_entry(e: &mut IrExpr, vt: &mut VarTable) {
        rewrite_expr(e, vt)
    }
}

pub(crate) use hoist_impl::rewrite_expr_entry as hoist_rewrite_expr;

/// The small-int scalar classes, shared with the hoist above (calls_p4's
/// int_eq_operand_ty is method-scoped; this free twin serves the desugar).
pub(crate) fn calls_p4_is_small_int(ty: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::Ty;
    matches!(
        ty,
        Ty::Int8
            | Ty::Int16
            | Ty::Int32
            | Ty::Int64
            | Ty::UInt8
            | Ty::UInt16
            | Ty::UInt32
            | Ty::UInt64
            | Ty::Float32
    )
}

/// MEMBER-CHAIN TYPE REPAIR (a pre-lowering per-fn pass): a monomorphized
/// open-record reader (`fn get_port(app: { config: { port: Int, .. }, .. })` →
/// `get_port__App`) leaves an INTERMEDIATE Member node mistyped — `app.config`
/// carries `Named(App)` (the OUTER type) instead of the FIELD's declared type, so
/// the next member (`__.port`) resolves against the wrong record and the scalar
/// tail walls. The DECLARED field type is authoritative: repair every Member
/// node whose object type resolves and whose field type disagrees. Children
/// first (the object repairs before its member); a non-resolvable object type
/// (a genuinely open record at a non-mono site) is left untouched.
pub fn repair_member_field_tys(
    func: &mut almide_ir::IrFunction,
    layouts: &crate::lower::RecordLayouts,
) {
    use almide_ir::{walk_expr_mut, IrExpr, IrExprKind, IrMutVisitor};
    use almide_lang::types::Ty;

    fn field_ty_of(
        layouts: &crate::lower::RecordLayouts,
        ty: &Ty,
        field: almide_lang::intern::Sym,
    ) -> Option<Ty> {
        match ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| *n == field).map(|(_, t)| t.clone())
            }
            Ty::Named(name, args) => {
                let key = crate::lower::canonical_record_key(layouts, name.as_str())?;
                let (generics, decl_fields) = layouts.get(key)?;
                let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                    std::collections::HashMap::new();
                for (g, a) in generics.iter().zip(args.iter()) {
                    subst.insert(*g, a.clone());
                }
                decl_fields
                    .iter()
                    .find(|(n, _)| *n == field)
                    .map(|(_, t)| calls::subst_type_var(t, &subst))
            }
            _ => None,
        }
    }

    struct R<'a> {
        layouts: &'a crate::lower::RecordLayouts,
    }
    impl IrMutVisitor for R<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Member { object, field } = &e.kind else { return };
            let Some(fty) = field_ty_of(self.layouts, &object.ty, *field) else { return };
            if e.ty != fty {
                e.ty = fty;
            }
        }
    }
    let mut r = R { layouts };
    r.visit_expr_mut(&mut func.body);
}

/// RECORD-LITERAL FIELD-TYPE REPAIR (per-fn): a cross-module-linked anon record
/// literal (`{ top: 0.0, left: letlib.GAP }` — #785) reaches lowering with its
/// node type carrying an UNKNOWN field (`Ty::Record { top: Float, left: Unknown }`
/// — the ref-entry inference gap survives the v1 link), so the construct's
/// `scalar_slots` declines and the bind defers to an Opaque (a runtime trap once
/// passed by value). The literal's OWN field expressions are authoritative:
/// replace each Unknown declared-field type with the same-named literal field's
/// concrete type (and synthesize the whole Record type when the node is fully
/// Unknown). Children first, so a repaired inner literal feeds its parent.
pub fn repair_record_literal_field_tys(func: &mut almide_ir::IrFunction) {
    use almide_ir::{walk_expr_mut, IrExpr, IrExprKind, IrMutVisitor};
    use almide_lang::types::Ty;

    // Pure post-order repair for ONE node — `R` carries no fields, so unlike a
    // real state-threading walker (an accumulator flag read back across sibling
    // nodes) this trait method is just "recurse, then run a stateless per-node
    // check" — extracting the check into its own fn changes nothing observable.
    // The `Record { fields: tfs }` arm's fill loop, extracted to its own fn so its
    // 3-deep nesting (for → if → if-let) doesn't stack onto `repair_node`'s own
    // cognitive-complexity count — a plain data transform, no visitor state.
    fn fill_unknown_record_field_tys(
        tfs: &mut [(almide_lang::intern::Sym, Ty)],
        fields: &[(almide_lang::intern::Sym, IrExpr)],
    ) {
        for (tn, tt) in tfs.iter_mut() {
            if matches!(tt, Ty::Unknown) {
                if let Some((_, f)) = fields.iter().find(|(n, _)| n == tn) {
                    *tt = f.ty.clone();
                }
            }
        }
    }

    fn repair_node(e: &mut IrExpr) {
        let IrExprKind::Record { name: None, fields } = &e.kind else { return };
        if fields.iter().any(|(_, f)| matches!(f.ty, Ty::Unknown)) {
            return;
        }
        match &mut e.ty {
            t @ Ty::Unknown => {
                *t = Ty::Record {
                    fields: fields.iter().map(|(n, f)| (*n, f.ty.clone())).collect(),
                };
            }
            Ty::Record { fields: tfs } => fill_unknown_record_field_tys(tfs, fields),
            _ => {}
        }
    }

    struct R;
    impl IrMutVisitor for R {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            repair_node(e);
        }
    }
    R.visit_expr_mut(&mut func.body);
}
