// ── tail of desugar_guard_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

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
    /// hoist every field up to and including the LAST `!`-bearing field (the
    /// `!` is the field or nested in it, [`bang_bearing`]) that is not a
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

    /// A field that propagates: a `!` / `?` as the field itself or nested in it
    /// (`wrap(label(r)!)`, `"p" + label(r)! + "q"`, `"x${label(r)!}y"`), but not
    /// one inside a lambda body, which the field only builds. The hoisted bind
    /// then carries the nested `!`, which the call-argument unwrap lift places.
    fn bang_bearing(e: &IrExpr) -> bool {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct B(bool);
        impl IrVisitor for B {
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    IrExprKind::Unwrap { .. } | IrExprKind::Try { .. } => self.0 = true,
                    IrExprKind::Lambda { .. } => {}
                    _ => walk_expr(self, e),
                }
            }
        }
        let mut b = B(false);
        b.visit_expr(e);
        b.0
    }

    fn record_last_bang(e: &IrExpr) -> Option<usize> {
        let IrExprKind::Record { fields, .. } = &e.kind else { return None };
        record_last_bang_fields(fields)
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
                let Some(last_bang) = record_last_bang_fields(fields) else {
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
        fields.iter().rposition(|(_, fe)| bang_bearing(fe))
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
