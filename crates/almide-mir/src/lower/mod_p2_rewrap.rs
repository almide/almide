// ── `rewrap_never_err_into_result_targets` and its target-position deciders ──
//
// include!-spliced into mod_p2.rs, whose other half is the never-err type STRIP
// (`unwrap_never_err_call_types`); this file is the RE-WRAP that puts the carrier
// back wherever a position's own declared type says Result. Split out so mod_p2.rs
// stays under the 800-line ceiling the codopsy gate holds this crate to — the two
// halves are one pass and are meant to be read together.

/// Re-wrap a NEVER-ERR lifted call assigned/bound to an EXPLICITLY `Result`-typed target
/// (`var r: Result[Int, String] = ok(0); r = step(5)` / `let r2: Result[Int, String] =
/// step(7)` — the #485 "annotated Result keeps the Result" rule) OR sitting in a
/// CONSTRUCTION position whose declared slot type is Result (`[step(), step()]: List[Result[..]]`,
/// `Holder { r: step() }`, `(step(), 9): (Result[..], Int)` — the SAME C-068 "construction
/// positions are target-directed" rule `auto_try.rs` already applies at the frontend). The
/// never-err type rewrite (`unwrap_never_err_call_types`, run unconditionally over EVERY
/// function by this pre-pass, not just the mutually-recursive ones it exists for) makes the
/// CALL yield raw `T` on v1 — but a List/Record/Tuple slot whose OWN type says Result must
/// still hold a Result block (autotry_construction: v0 already keeps the Result via C-068;
/// this pre-pass silently undid it for v1, since the original bind/assign-only re-wrap never
/// covered construction positions). Since the callee never errs, `ok(call)` is exact.
pub fn rewrap_never_err_into_result_targets(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
    record_layouts: &RecordLayouts,
    param_sigs: &std::collections::HashMap<String, Vec<Ty>>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    // Pass 1: vars DECLARED with a Result type (Bind.ty).
    fn collect_result_vars(e: &IrExpr, out: &mut std::collections::HashSet<u32>) {
        use almide_ir::visit::IrVisitor;
        struct C<'a>(&'a mut std::collections::HashSet<u32>);
        impl IrVisitor for C<'_> {
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let IrStmtKind::Bind { var, ty, .. } = &s.kind {
                    if matches!(ty, Ty::Applied(TypeConstructorId::Result, _)) {
                        self.0.insert(var.0);
                    }
                }
                almide_ir::visit::walk_stmt(self, s);
            }
        }
        C(out).visit_expr(e);
    }
    let mut result_vars = std::collections::HashSet::new();
    collect_result_vars(body, &mut result_vars);

    struct S<'a> {
        can_err: &'a std::collections::HashSet<String>,
        lifted: &'a std::collections::HashSet<String>,
        result_vars: std::collections::HashSet<u32>,
        record_layouts: &'a RecordLayouts,
        param_sigs: &'a std::collections::HashMap<String, Vec<Ty>>,
    }
    impl S<'_> {
        fn is_raw_never_err_call(&self, e: &IrExpr) -> bool {
            !matches!(&e.ty, Ty::Applied(TypeConstructorId::Result, _))
                && matches!(&e.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if self.lifted.contains(name.as_str()) && !self.can_err.contains(name.as_str()))
        }
        fn wrap(&self, e: &mut IrExpr, result_ty: Ty) {
            let inner = std::mem::replace(
                e,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            *e = IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(inner) },
                ty: result_ty,
                span: e.span.clone(),
                def_id: None,
            };
        }

        /// A LAMBDA is a target-directed position too, and the one this pass never
        /// enumerated. Its own `Ty::Fn` ret is the authority — monomorphization already
        /// trusts it (`list.map(xs, (i) => h(i)!)` specializes
        /// `list.__fallible_map__Int_Int_String` off exactly that type) — but the lifter
        /// reads only the BODY, and `unwrap_never_err_call_types` recurses INTO lambda
        /// bodies, so the tail call came out raw `Int` while the node still said
        /// `Result[Int, String]`. The lifted fn was then rendered `(result i64)` where the
        /// `call_indirect` inside the monomorphized twin declares `(result i32)` — a Result
        /// block pointer: `wasm trap: indirect call type mismatch` at run time for the
        /// documented `xs |> list.map((x) => f(x)!)!` idiom, and on `flat_map` the same
        /// mismatch landed somewhere that does not trap and printed empty strings with
        /// exit 0 (almide#1406). Native is unaffected — the strip is a v1-only pre-pass.
        /// Since the callee never errs, `ok(call)` is exact: the argument the four
        /// construction arms already make.
        fn rewrap_result_typed_lambda_tail(&self, lam_ty: &Ty, body: &mut IrExpr) {
            let Ty::Fn { ret, .. } = lam_ty else { return };
            if !matches!(ret.as_ref(), Ty::Applied(TypeConstructorId::Result, _)) {
                return;
            }
            self.rewrap_lambda_return_positions(body, ret.as_ref());
        }

        /// Wrap every RETURN position of a lambda body holding a raw never-err call,
        /// retyping the spine ONLY along paths where something was wrapped — a body with
        /// nothing to wrap must come out byte-identical, since `lift_lambda`'s callee
        /// reads shape decisions off `body.ty`.
        fn rewrap_lambda_return_positions(&self, e: &mut IrExpr, ret_ty: &Ty) -> bool {
            let wrapped = match &mut e.kind {
                IrExprKind::Block { expr: tail, .. } => tail
                    .as_deref_mut()
                    .map(|t| self.rewrap_lambda_return_positions(t, ret_ty))
                    .unwrap_or(false),
                IrExprKind::If { then, else_, .. } => {
                    let a = self.rewrap_lambda_return_positions(then, ret_ty);
                    let b = self.rewrap_lambda_return_positions(else_, ret_ty);
                    a || b
                }
                IrExprKind::Match { arms, .. } => {
                    let mut any = false;
                    for arm in arms.iter_mut() {
                        any |= self.rewrap_lambda_return_positions(&mut arm.body, ret_ty);
                    }
                    any
                }
                _ => {
                    if self.is_raw_never_err_call(e) {
                        self.wrap(e, ret_ty.clone());
                        return true;
                    }
                    false
                }
            };
            if wrapped {
                e.ty = ret_ty.clone();
            }
            wrapped
        }

        /// Extracted verbatim from [`visit_expr_mut`] (codopsy round-3 sweep, #852): decides
        /// whether a LIST literal's element slot is declared Result, and re-wraps every raw
        /// never-err call sitting in one.
        // `[step(), step()]: List[Result[..]]` — the element slot type is the LIST's
        // own type's sole type arg (mirrors auto_try.rs's `elem_is_result`).
        // Guard-clause flattening: this arm is the tail of `visit_expr_mut` (the
        // last statement in the function, and match arms are mutually exclusive),
        // so an early `return` on any unmet condition is identical to falling
        // through to the end of the arm's block. No behavior change.
        fn rewrap_result_typed_list_elements(&self, list_ty: &Ty, elements: &mut [IrExpr]) {
            let Ty::Applied(TypeConstructorId::List, a) = list_ty else {
                return;
            };
            if a.len() != 1 {
                return;
            }
            let Ty::Applied(TypeConstructorId::Result, _) = &a[0] else {
                return;
            };
            let elem_ty = a[0].clone();
            for el in elements.iter_mut() {
                if self.is_raw_never_err_call(el) {
                    self.wrap(el, elem_ty.clone());
                }
            }
        }

        /// Extracted verbatim from [`visit_expr_mut`] (codopsy round-3 sweep, #852): decides
        /// which ARGUMENT slots of a `Named` call are declared Result by the callee's own
        /// param signature, and re-wraps the raw never-err calls sitting in them.
        // `unwrap(step())` — a CALL-ARGUMENT position whose CALLEE PARAM's declared
        // type is Result (#840 follow-up class, the yaml `unwrap(parse(s))` shape):
        // the callee reads a real Result block off its param, so a raw never-err
        // call in that slot must re-wrap exactly like a bind/construction target.
        // Positional zip against the callee's declared params; an arity mismatch
        // (a shape this pre-pass doesn't understand) is left untouched — the
        // lowering's own walls stay the safety net.
        fn rewrap_result_typed_call_args(
            &self,
            name: &almide_lang::intern::Sym,
            args: &mut [IrExpr],
        ) {
            let Some(ptys) = self.param_sigs.get(name.as_str()) else {
                return;
            };
            if ptys.len() != args.len() {
                return;
            }
            for (arg, pty) in args.iter_mut().zip(ptys.iter()) {
                if matches!(pty, Ty::Applied(TypeConstructorId::Result, _))
                    && self.is_raw_never_err_call(arg)
                {
                    self.wrap(arg, pty.clone());
                }
            }
        }

        /// Extracted verbatim from [`visit_expr_mut`] (codopsy round-3 sweep, #852): decides
        /// which TUPLE slots are declared Result by the tuple expr's own type, and re-wraps
        /// the raw never-err calls sitting in them.
        // `(step(), 9): (Result[..], Int)` — each slot's type comes directly from the
        // TUPLE expr's own `Ty::Tuple` positionally (no registry lookup needed).
        fn rewrap_result_typed_tuple_slots(&self, tuple_ty: &Ty, elements: &mut [IrExpr]) {
            if let Ty::Tuple(tys) = tuple_ty {
                if tys.len() == elements.len() {
                    for (el, t) in elements.iter_mut().zip(tys.iter()) {
                        if matches!(t, Ty::Applied(TypeConstructorId::Result, _))
                            && self.is_raw_never_err_call(el)
                        {
                            self.wrap(el, t.clone());
                        }
                    }
                }
            }
        }

        /// Extracted verbatim from [`visit_expr_mut`] (codopsy round-3 sweep, #852): resolves a
        /// record literal's declared field types (structural type first, then the layout
        /// registry) and re-wraps the raw never-err calls sitting in Result-typed fields.
        // `Holder { r: step() }` — field types come from the record expr's own
        // structural type (`Ty::Record`/`Ty::OpenRecord`) or, for a NAMED record, the
        // declared layout registry — mirrors auto_try.rs's `field_tys` construction.
        fn rewrap_result_typed_record_fields(
            &self,
            record_ty: &Ty,
            name: &Option<almide_lang::intern::Sym>,
            fields: &mut [(almide_lang::intern::Sym, IrExpr)],
        ) {
            let field_tys: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                match record_ty {
                    Ty::Record { fields: fs } | Ty::OpenRecord { fields: fs } => {
                        fs.iter().cloned().collect()
                    }
                    Ty::Named(tn, _) => self
                        .record_layouts
                        .get(tn.as_str())
                        .map(|(_, fs)| fs.iter().cloned().collect())
                        .unwrap_or_default(),
                    _ => name
                        .as_ref()
                        .and_then(|n| self.record_layouts.get(n.as_str()))
                        .map(|(_, fs)| fs.iter().cloned().collect())
                        .unwrap_or_default(),
                };
            for (k, v) in fields.iter_mut() {
                if let Some(ft) = field_tys.get(k) {
                    if matches!(ft, Ty::Applied(TypeConstructorId::Result, _))
                        && self.is_raw_never_err_call(v)
                    {
                        self.wrap(v, ft.clone());
                    }
                }
            }
        }
    }
    impl IrMutVisitor for S<'_> {
        fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
            almide_ir::walk_stmt_mut(self, s);
            match &mut s.kind {
                IrStmtKind::Bind { ty, value, .. }
                    if matches!(ty, Ty::Applied(TypeConstructorId::Result, _))
                        && self.is_raw_never_err_call(value) =>
                {
                    let rt = ty.clone();
                    self.wrap(value, rt);
                }
                IrStmtKind::Assign { var, value }
                    if self.result_vars.contains(&var.0) && self.is_raw_never_err_call(value) =>
                {
                    let ok_ty = value.ty.clone();
                    self.wrap(
                        value,
                        Ty::Applied(TypeConstructorId::Result, vec![ok_ty, Ty::String]),
                    );
                }
                _ => {}
            }
        }
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            // The four target-directed positions, each delegating to the decider that owns
            // its slot-type rule (codopsy round-3 sweep, #852). Arms stay mutually exclusive
            // and in their original order; every arm body moved verbatim, comments included.
            match &mut expr.kind {
                IrExprKind::List { elements } => {
                    self.rewrap_result_typed_list_elements(&expr.ty, elements)
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                    self.rewrap_result_typed_call_args(name, args)
                }
                IrExprKind::Tuple { elements } => {
                    self.rewrap_result_typed_tuple_slots(&expr.ty, elements)
                }
                IrExprKind::Record { name, fields } => {
                    self.rewrap_result_typed_record_fields(&expr.ty, name, fields)
                }
                IrExprKind::Lambda { body, .. } => {
                    self.rewrap_result_typed_lambda_tail(&expr.ty, body)
                }
                _ => {}
            }
        }
    }
    S { can_err, lifted: lifted_effect_fns, result_vars, record_layouts, param_sigs }
        .visit_expr_mut(body);
}
