// ── tail of desugar_guard_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

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

    /// Replace `slot` with a fresh `name`d Var and hand back the bind that gives
    /// that Var whatever `slot` held. The two record hoists below differ only in
    /// WHICH slots they pick, so the swap itself lives here once.
    fn hoist_to_bind(slot: &mut IrExpr, vt: &mut VarTable, name: &str) -> IrStmt {
        let ty = slot.ty.clone();
        let var = vt.alloc(almide_lang::intern::sym(name), ty.clone(), Mutability::Let, None);
        let value = std::mem::replace(
            slot,
            IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: None, def_id: None },
        );
        IrStmt {
            kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value },
            span: None,
        }
    }

    /// Hoist every RECORD-literal ARGUMENT of a SCALAR-result named/module call to
    /// its own `__rec_arg` bind. Anything else (a non-scalar result, a non-call, a
    /// computed/method target) is left alone and falls through to the record-FIELD
    /// hoist below.
    fn hoist_record_literal_call_args(
        value: &mut IrExpr,
        vt: &mut VarTable,
        hoists: &mut Vec<IrStmt>,
    ) {
        if !is_scalar_ty(&value.ty) {
            return;
        }
        let IrExprKind::Call {
            target: CallTarget::Named { .. } | CallTarget::Module { .. },
            args,
            ..
        } = &mut value.kind
        else {
            return;
        };
        for a in args.iter_mut() {
            if matches!(a.kind, IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }) {
                hoists.push(hoist_to_bind(a, vt, "__rec_arg"));
            }
        }
    }

    /// A record-literal BIND whose FIELD is a scalar CALL (`left: letlib.GAP` — a
    /// call-initialized module top-let read reaches the IR as its init call): the
    /// field-position call emitted a dst-less bare call (result on the stack —
    /// invalid wasm). Hoist each such field to its own `__rec_fld` bind, declaration
    /// order preserved (= v0's field evaluation order).
    fn hoist_scalar_call_record_fields(
        value: &mut IrExpr,
        vt: &mut VarTable,
        hoists: &mut Vec<IrStmt>,
    ) {
        let IrExprKind::Record { fields, .. } = &mut value.kind else { return };
        for (_, fe) in fields.iter_mut() {
            if is_scalar_ty(&fe.ty) && matches!(fe.kind, IrExprKind::Call { .. }) {
                hoists.push(hoist_to_bind(fe, vt, "__rec_fld"));
            }
        }
    }

    /// #1581: a `!` PROPAGATION inline in a record-literal FIELD
    /// (`Out { a: label(r)! }` — the fallible-DTO shape) walled whenever the
    /// field is heap-typed and the literal is a Result carrier's Ok payload;
    /// the hoisted `let` spelling always lowered. Mechanize that spelling:
    /// hoist every field up to and including the LAST `!` field that is not a
    /// trivially pure read (Var / literal) to its own `__rec_fld` bind —
    /// evaluation order is preserved among the hoisted fields, later fields
    /// stay inline and still evaluate after them, and the `!`'s early return
    /// fires before the record materializes exactly as it did inline.
    /// Descends through the ADR-0002 lifted tail's ctor (`ok(Out { … })`) to
    /// the literal. The Unwrap/Try node carries the PAYLOAD type (the call
    /// under it carries the carrier), so the hoisted bind is the proven C-222
    /// bind-position unwrap verbatim.
    fn trivially_pure(e: &IrExpr) -> bool {
        matches!(
            e.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::Unit
        )
    }

    /// The Record-arm rule shared by the direct and tuple-slot positions:
    /// hoist every non-trivially-pure field up to `upto` (inclusive).
    fn hoist_record_fields_upto(
        fields: &mut [(almide_lang::intern::Sym, IrExpr)],
        upto: usize,
        vt: &mut VarTable,
        hoists: &mut Vec<IrStmt>,
    ) {
        for (idx, (_, fe)) in fields.iter_mut().enumerate() {
            if idx > upto {
                break;
            }
            if !trivially_pure(fe) {
                hoists.push(hoist_to_bind(fe, vt, "__rec_fld"));
            }
        }
    }

    fn record_last_bang(e: &IrExpr) -> Option<usize> {
        let IrExprKind::Record { fields, .. } = &e.kind else { return None };
        fields
            .iter()
            .rposition(|(_, fe)| matches!(fe.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. }))
    }

    fn hoist_bang_record_fields(
        value: &mut IrExpr,
        vt: &mut VarTable,
        hoists: &mut Vec<IrStmt>,
    ) {
        let rec = match &mut value.kind {
            IrExprKind::ResultOk { expr }
            | IrExprKind::ResultErr { expr }
            | IrExprKind::OptionSome { expr } => &mut **expr,
            _ => value,
        };
        match &mut rec.kind {
            IrExprKind::Record { fields, .. } => {
                let Some(last_bang) = fields.iter().rposition(|(_, fe)| {
                    matches!(fe.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. })
                }) else {
                    return;
                };
                hoist_record_fields_upto(fields, last_bang, vt, hoists);
            }
            // #1581 residual: the record literal sits in a TUPLE SLOT of the
            // carrier's Ok payload (`fn f(r) -> (Row, Out)! = (r, Out { a:
            // label(r)! })` — the functional-port `(state, dto)` pair). Hoist
            // through the tuple layer: every non-trivially-pure slot (or, for
            // a record slot, its non-trivially-pure fields) up to and
            // including the LAST bang-bearing record slot, in evaluation
            // order — earlier effectful slots hoist too, so nothing reorders
            // across the `!`'s early return.
            IrExprKind::Tuple { elements } => {
                let Some(last_slot) =
                    elements.iter().rposition(|e| record_last_bang(e).is_some())
                else {
                    return;
                };
                for (idx, slot) in elements.iter_mut().enumerate() {
                    if idx > last_slot {
                        break;
                    }
                    if let IrExprKind::Record { fields, .. } = &mut slot.kind {
                        let upto = if idx == last_slot {
                            match record_last_bang_fields(fields) {
                                Some(b) => b,
                                None => fields.len().saturating_sub(1),
                            }
                        } else {
                            fields.len().saturating_sub(1)
                        };
                        hoist_record_fields_upto(fields, upto, vt, hoists);
                    } else if !trivially_pure(slot) {
                        hoists.push(hoist_to_bind(slot, vt, "__tup_slot"));
                    }
                }
            }
            _ => {}
        }
    }

    fn record_last_bang_fields(
        fields: &[(almide_lang::intern::Sym, IrExpr)],
    ) -> Option<usize> {
        fields
            .iter()
            .rposition(|(_, fe)| matches!(fe.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. }))
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
                    hoist_record_literal_call_args(value, vt, &mut hoists);
                    hoist_scalar_call_record_fields(value, vt, &mut hoists);
                    hoist_bang_record_fields(value, vt, &mut hoists);
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
                    // A TAIL-position record literal with a `!` field
                    // (`fn f(r) = { …; Out { a: label(r)! } }` and the lifted
                    // `ok(Out { … })` spelling — #1581): hoist the fields into
                    // this block's own statements, right before the tail.
                    let mut hoists: Vec<IrStmt> = Vec::new();
                    hoist_bang_record_fields(t, vt, &mut hoists);
                    stmts.extend(hoists);
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
        rewrite_expr(e, vt);
        // A NON-Block fn body (`fn f(r) = Out { a: label(r)! }` — the 5-line
        // #1581 repro): there is no statement list to hoist into, so wrap the
        // body in a Block carrying the hoisted binds ahead of the literal.
        let mut hoists: Vec<IrStmt> = Vec::new();
        hoist_bang_record_fields(e, vt, &mut hoists);
        if !hoists.is_empty() {
            let ty = e.ty.clone();
            let tail = std::mem::replace(
                e,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            *e = IrExpr {
                kind: IrExprKind::Block { stmts: hoists, expr: Some(Box::new(tail)) },
                ty,
                span: None,
                def_id: None,
            };
        }
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
