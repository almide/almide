
thread_local! {
    /// The names of NEVER-ERR LIFTED user effect fns (an `effect fn` whose declared return is
    /// non-Result, so the frontend lifts its call type to `Result[T, String]`, but whose body builds
    /// no `err` and returns raw `T`). Populated by `inline_mutual_tail_recursion` (which knows the
    /// `can_err` × `lifted_effect_fns` sets) and read by the match-subject lowering so an UN-REWRITTEN
    /// `match <such call> {…}` (an `ok(_)`/structured/guarded Ok arm the `rewrite_never_err_effect_match`
    /// pass left in place) WALLs cleanly instead of reading the raw handle as a Result block (a trap).
    /// A common `ok(x)` match is already rewritten away to a `let`-block, so this only catches the rare
    /// residue. Thread-local because lowering runs single-threaded per program right after the pre-pass.
    pub(crate) static NEVER_ERR_LIFTED_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// The names of AUTO-WRAP ABI functions — an `effect fn` declared with a bare scalar return
    /// (`-> Int`, not `-> Result[Int, String]`) whose body contains a STATEMENT-position
    /// propagating `!`/auto-`?` (`body_has_stmt_position_propagating_unwrap`, mod.rs), so its
    /// TRUE compiled ABI is `Result[<declared>, String]` even though `func.ret_ty` stays the bare
    /// sugar type. Populated by `inline_mutual_tail_recursion` (the SAME program-wide pre-pass
    /// `NEVER_ERR_LIFTED_FNS` uses, run once before any per-function lowering, so it is fully
    /// populated before ANY caller's own lowering — including a caller that is itself never-err
    /// lifted and processed before this callee). EXCLUDES `main` — see the population site.
    pub(crate) static AUTO_WRAP_ABI_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// A function CAN-ERR (returns `Err` on some input) iff its body has a direct `err(…)` (`ResultErr`) OR
/// it `!`-PROPAGATES (an `Unwrap` over a `Named` call to) a can-err function. A function whose entire
/// `!`-call closure is err-free NEVER returns `Err`, so `let pat = f()!` over it is faithfully
/// `let pat = f()` (the same pass-through the tail `!` already uses). KEY: an error reached only through
/// a `match`/`??` (e.g. the yaml cluster calling the PURE `oct_rec`/`bin_rec` int parsers, which DO have
/// `err(…)`, but via `match` not `!`) is HANDLED, not propagated, so it does NOT make the caller can-err —
/// the yaml parser cluster is therefore entirely never-err.
fn has_result_err(body: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            // A direct `err(…)` (`ResultErr`) OR a RESULT-CONSTRUCTING PRIM whose Err arm is
            // runtime-reachable. `prim.read_text_file` builds an `Err(message)` inside the WASI
            // render (path_open failure) — invisible to the IR as a `ResultErr` node, but the fn
            // genuinely CAN return `Err`. So a `fn f() = prim.read_text_file(p)` (the self-host
            // `fs_read_text`) is can-err: its `!` must NOT be stripped as never-err (which would bind
            // the whole Result block where the Ok String was expected). `fs.read_text` reaches main
            // as a `Module` call so the strip already leaves it; this keeps the analysis honest if a
            // `Named`-call form ever appears (e.g. after inlining).
            if matches!(&e.kind, IrExprKind::ResultErr { .. }) {
                self.0 = true;
            }
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &e.kind
            {
                if module.as_str() == "prim"
                    && (func.as_str() == "read_text_file" || func.as_str() == "read_bytes_file")
                {
                    self.0 = true;
                }
            }
            // A `!` over a MODULE / runtime call propagates an error channel this
            // Named-call-only analysis cannot see (`json.parse(body)!` inside
            // porta's parse_and_wrap): the fn IS can-err. Without this it was
            // classified never-err, its callers' `!` got stripped, and the caller
            // read the REAL Result block as the raw payload (record fields off a
            // Result handle — the read_message `method=` garbage, 2026-07-03).
            if let IrExprKind::Unwrap { expr: inner } = &e.kind {
                if matches!(&inner.kind,
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        | IrExprKind::RuntimeCall { .. })
                {
                    self.0 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_expr(body);
    v.0
}

fn unwrap_named_callees(body: &IrExpr) -> std::collections::HashSet<String> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(std::collections::HashSet<String>);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Unwrap { expr } = &e.kind {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind {
                    self.0.insert(name.as_str().to_string());
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(std::collections::HashSet::new());
    v.visit_expr(body);
    v.0
}

/// The set of function names that CAN return `Err` — `has_result_err` seeds + `!`-propagation fixpoint.
pub fn compute_can_err(fns: &[IrFunction]) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut can_err: HashSet<String> = fns
        .iter()
        .filter(|f| has_result_err(&f.body))
        .map(|f| f.name.as_str().to_string())
        .collect();
    let callees: Vec<(String, HashSet<String>)> = fns
        .iter()
        .map(|f| (f.name.as_str().to_string(), unwrap_named_callees(&f.body)))
        .collect();
    loop {
        let mut changed = false;
        for (name, cs) in &callees {
            if !can_err.contains(name) && cs.iter().any(|g| can_err.contains(g)) {
                can_err.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    can_err
}

/// Strip `Unwrap` (`!`) over a NEVER-ERR `Named` call: `let pat = f()!` → `let pat = f()` and a
/// `f()!` self-call → bare `f()` (so `tco_collect` sees the recursion). SOUND — a never-err callee always
/// returns `Ok`, so the `!` is a no-op; a CAN-ERR callee's `!` is LEFT untouched (it still walls in
/// `lower_destructure`/`lower_bind`), so its error is never silently dropped (the blanket strip that did
/// drop it byte-mismatched safe_div_chain & co. — see the roadmap note).
pub fn strip_never_err_unwraps(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
    self_name: &str,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct S<'a> {
        can_err: &'a std::collections::HashSet<String>,
        lifted: &'a std::collections::HashSet<String>,
        self_name: &'a str,
    }
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            // The strip is REPRESENTATION-sound only when the callee's v1 body
            // returns the raw `T`: a LIFTED effect fn (the frontend added the
            // Result ABI; the MIR body never built a Result block). A pure /
            // effect fn DECLARED `-> Result[..]` builds a REAL Result block even
            // when it never errs (`ok(rec)`), so stripping its bind-position `!`
            // made the consumer read record fields off the Result handle (the
            // r8 / read_message miscompile, 2026-07-03). SELF-calls keep the
            // strip unconditionally — that is the tail-TCO shape (`f(..)!` →
            // `f(..)`), a same-Result pass-through the tail lowering already
            // treats as such (the yaml parser cluster).
            // `Try` (the frontend auto-`?`) over the same never-err lifted callee is
            // the identical no-op — `x = step(x)` carries `Try{step(x)}`, which an
            // unstripped path lowered to a deferred Const-0 (effect_assign_unwrap).
            let strip = matches!(&expr.kind,
                IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }
                if matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if !self.can_err.contains(name.as_str())
                        && (self.lifted.contains(name.as_str()) || name.as_str() == self.self_name)
                        && !crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(name.as_str()))));
            if strip {
                if let IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } =
                    &expr.kind
                {
                    let inner = (**inner).clone();
                    *expr = inner;
                }
            }
        }
    }
    S { can_err, lifted: lifted_effect_fns, self_name }.visit_expr_mut(body);
}

/// Rewrite `Try/Unwrap { fan.map(xs, (x) => ok(E)) }` → `list.map(xs, (x) => E)` — a PURE
/// IR transformation. `fan.map` maps in list order and collects the first Err; a lambda
/// whose every exit is `ok(…)` can never Err, and fan lambdas cannot capture a `var`, so
/// parallelism is unobservable — the call IS list.map. This lets the C1 defunctionalization
/// / self-host list machinery execute it on the v1 path (it previously fell to the elided
/// deferred-Const and printed all-zero results — fan_map_inline_lambda, 2026-07-03).
/// Call-count-INVARIANT: one Module call becomes one Module call (`ok` is a constructor).
/// A lambda with a non-`ok` exit (a real Err path) is left untouched and keeps walling.
pub fn rewrite_fan_map_pure(body: &mut IrExpr) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    // The lambda's tail must be `ok(E)` (directly, or as a block tail). Returns the
    // rewritten body with the `ok` stripped, or None if any exit is not ok-wrapped.
    fn strip_ok(e: &IrExpr) -> Option<IrExpr> {
        match &e.kind {
            IrExprKind::ResultOk { expr } => Some((**expr).clone()),
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let new_tail = strip_ok(tail)?;
                Some(IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(new_tail)) },
                    ty: new_tail_ty(&e.ty),
                    span: e.span.clone(),
                    def_id: e.def_id,
                })
            }
            _ => None,
        }
    }
    fn new_tail_ty(t: &Ty) -> Ty {
        match t {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
            other => other.clone(),
        }
    }
    struct S;
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let (IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }) = &expr.kind
            else {
                return;
            };
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, type_args } =
                &inner.kind
            else {
                return;
            };
            if module.as_str() != "fan" || func.as_str() != "map" || args.len() != 2 {
                return;
            }
            let IrExprKind::Lambda { params, body, lambda_id } = &args[1].kind else { return };
            let Some(new_body) = strip_ok(body) else { return };
            let new_lambda_ty = match &args[1].ty {
                Ty::Fn { params: ps, ret } => Ty::Fn { params: ps.clone(), ret: Box::new(new_tail_ty(ret)) },
                other => other.clone(),
            };
            let new_lambda = IrExpr {
                kind: IrExprKind::Lambda {
                    params: params.clone(),
                    body: Box::new(new_body),
                    lambda_id: *lambda_id,
                },
                ty: new_lambda_ty,
                span: args[1].span.clone(),
                def_id: args[1].def_id,
            };
            let result_ty = expr.ty.clone(); // the Try-unwrapped List[B]
            *expr = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: almide_lang::intern::sym("list"),
                        func: almide_lang::intern::sym("map"),
                        def_id: None,
                    },
                    args: vec![args[0].clone(), new_lambda],
                    type_args: type_args.clone(),
                },
                ty: result_ty,
                span: expr.span.clone(),
                def_id: expr.def_id,
            };
        }
    }
    S.visit_expr_mut(body);
}

/// Rewrite a NEVER-ERR user `effect fn` `Named` CALL's result type from the lifted-ABI
/// `Result[T, String]` (what the frontend reports so consumers `auto_unwrap`) back to the RAW `T`
/// the v1 function body actually returns. A never-err effect fn's body returns the bare value (no
/// `ok`/`err` wrap — `$f` is `(result i64)`/raw String/List handle, NOT a Result block), so EVERY
/// value-position consumer must see raw `T`: an arg `g(f())`, `list.len(lst())`, a value tail, a
/// `let` — all read the call as a scalar/heap `T`, never a Result handle. Without this the consumer
/// keyed off the `Result[T, _]` `.ty` emitted Result-handle reads (`i32.load` + DropListStr / cap-tag)
/// over the raw i64/handle the never-err callee returns → INVALID WAT (scalar i32/i64 mismatch) or a
/// runtime TRAP (heap: the DropListStr walks the raw String's bytes as element pointers, hitting the
/// `$rc_dec` double-free sentinel). The bind position already works because `lower_bind` uses the
/// LET's unwrapped type; this extends that consistency to arg/value positions, for BOTH scalar and
/// heap `T` (the heap re-type is sound — corpus-wall ACCEPTs — because it is gated to LIFTED effect
/// fns only, whose body genuinely returns a raw handle, so the plain-heap drop is the CORRECT drop,
/// not the trapping Result-as-tag one). A CAN-ERR callee is LEFT untouched (real `Result[T, String]`
/// handle); a PURE `fn` returning a real Result is NOT in `lifted_effect_fns`; a bundled stdlib
/// effect fn is a `Module` call (never matched here). The `match` shape is handled by
/// `rewrite_never_err_effect_match` FIRST (it needs the Result tag to pick the Ok arm); a `Match`
/// SUBJECT call is SKIPPED here so an un-rewritten `match` keeps its lifted type and WALLs cleanly.
pub fn unwrap_never_err_call_types(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S<'a>(&'a std::collections::HashSet<String>, &'a std::collections::HashSet<String>);
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            // A `match SUBJ {…}` whose SUBJ is a never-err lifted-effect call: do NOT unwrap the
            // subject's lifted Result type here. `rewrite_never_err_effect_match` (run before this)
            // already turned every REWRITABLE such `match` into `{ let ok-pat = call; ok-arm }`; any
            // `match` that REMAINS (an un-rewritable Ok pattern) must keep its lifted Result type so
            // the match-subject lowering WALLs it (a clean Unsupported) rather than reading a raw
            // handle as a Result block (a trap). Still recurse into the arm bodies.
            if matches!(&expr.kind, IrExprKind::Match { .. }) {
                if let IrExprKind::Match { subject, arms } = &mut expr.kind {
                    let skip_subject = matches!(&subject.kind,
                        IrExprKind::Call { target: CallTarget::Named { name }, .. }
                            if self.1.contains(name.as_str()) && !self.0.contains(name.as_str()));
                    if skip_subject {
                        for arm in arms.iter_mut() {
                            self.visit_expr_mut(&mut arm.body);
                        }
                        return;
                    }
                }
            }
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind {
                // Unwrap ONLY a call to a LIFTED user effect fn that is also NEVER-err. (Pure Result
                // fns are excluded — not in `lifted_effect_fns` — the list_iter_tco regression fix.)
                if self.1.contains(name.as_str())
                    && !self.0.contains(name.as_str())
                    && !crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(name.as_str()))
                {
                    if let Ty::Applied(TypeConstructorId::Result, a) = &expr.ty {
                        if a.len() == 2 && matches!(a[1], Ty::String) {
                            expr.ty = a[0].clone();
                        }
                    }
                }
            }
        }
    }
    S(can_err, lifted_effect_fns).visit_expr_mut(body);
}

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
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    // Pass 1: vars DECLARED with a Result type (Bind.ty).
    fn collect_result_vars(e: &IrExpr, out: &mut std::collections::HashSet<u32>) {
        use almide_ir::visit::{walk_expr, IrVisitor};
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
            match &mut expr.kind {
                // `[step(), step()]: List[Result[..]]` — the element slot type is the LIST's
                // own type's sole type arg (mirrors auto_try.rs's `elem_is_result`).
                IrExprKind::List { elements } => {
                    if let Ty::Applied(TypeConstructorId::List, a) = &expr.ty {
                        if a.len() == 1 {
                            if let Ty::Applied(TypeConstructorId::Result, _) = &a[0] {
                                let elem_ty = a[0].clone();
                                for el in elements.iter_mut() {
                                    if self.is_raw_never_err_call(el) {
                                        self.wrap(el, elem_ty.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                // `(step(), 9): (Result[..], Int)` — each slot's type comes directly from the
                // TUPLE expr's own `Ty::Tuple` positionally (no registry lookup needed).
                IrExprKind::Tuple { elements } => {
                    if let Ty::Tuple(tys) = &expr.ty {
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
                // `Holder { r: step() }` — field types come from the record expr's own
                // structural type (`Ty::Record`/`Ty::OpenRecord`) or, for a NAMED record, the
                // declared layout registry — mirrors auto_try.rs's `field_tys` construction.
                IrExprKind::Record { name, fields } => {
                    let field_tys: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                        match &expr.ty {
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
                _ => {}
            }
        }
    }
    S { can_err, lifted: lifted_effect_fns, result_vars, record_layouts }.visit_expr_mut(body);
}

/// Rewrite `match <never-err lifted-effect call> { ok(pat) => A, err(_) => B }` to `{ let pat =
/// <call>; A }`. A never-err lifted effect fn always returns Ok (its body builds no `err`), and its
/// v1 result is the RAW `T` — so the `match` has no real Result tag to dispatch on (reading the raw
/// handle as a Result block TRAPs / linearizes both arms). The sound, byte-matching lowering is: bind
/// the Ok arm's pattern to the raw call result and run the Ok arm; the `err` arm is dead. Handles the
/// common `ok(x)` (Bind) and `ok(_)` (Wildcard) Ok patterns; an Ok arm with a NESTED/structured
/// pattern, a guard, or no Ok arm is LEFT untouched so it stays a `match` that the call-type-unwrap
/// SKIPS and the match-subject lowering WALLs cleanly (never a trap). Runs BEFORE
/// `unwrap_never_err_call_types`.
pub fn rewrite_never_err_effect_match(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S<'a>(&'a std::collections::HashSet<String>, &'a std::collections::HashSet<String>);
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let is_target = matches!(&expr.kind, IrExprKind::Match { subject, .. }
                if matches!(&subject.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.1.contains(name.as_str()) && !self.0.contains(name.as_str())));
            if !is_target {
                return;
            }
            let IrExprKind::Match { subject, arms } = &expr.kind else { return };
            let Some(ok_arm) = arms.iter().find(|a| matches!(&a.pattern, IrPattern::Ok { .. })) else {
                return;
            };
            if ok_arm.guard.is_some() {
                return;
            }
            let IrPattern::Ok { inner } = &ok_arm.pattern else { return };
            // The raw Ok payload type = the subject call's Ok type (Result[T, String] → T).
            let ok_ty = match &subject.ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
                _ => return,
            };
            let raw_call = IrExpr { ty: ok_ty.clone(), ..(**subject).clone() };
            // Only the `ok(x)` BIND pattern is rewritten — the bound `var` gives the raw call result a
            // named owner with a sound scope-end drop. An `ok(_)` WILDCARD is LEFT as a `match` (it then
            // WALLs cleanly via the un-rewritten path): binding the result to a fresh throwaway var would
            // need a unique VarId the pre-pass cannot mint, and a bare `Expr`-statement call leaves the
            // heap result un-owned on the stack (invalid wat). A clean wall beats that.
            let bind_stmt = match &**inner {
                IrPattern::Bind { var, .. } => IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *var,
                        mutability: almide_ir::Mutability::Let,
                        ty: ok_ty,
                        value: raw_call,
                    },
                    span: None,
                },
                _ => return,
            };
            let body_expr = ok_arm.body.clone();
            let result_ty = expr.ty.clone();
            expr.kind = IrExprKind::Block { stmts: vec![bind_stmt], expr: Some(Box::new(body_expr)) };
            expr.ty = result_ty;
        }
    }
    S(can_err, lifted_effect_fns).visit_expr_mut(body);
}

/// The set of user functions whose CALL type the frontend LIFTS to `Result[T, String]`: an
/// `effect fn` whose DECLARED return is non-Result (so the call site sees the lifted Result while the
/// body returns raw `T`). EXACTLY the predicate in `check/calls.rs` (`sig.is_effect &&
/// !ret.is_result()`). A pure fn, or an effect fn already declaring `Result`/`Option`, is excluded —
/// its return type is real and must not be unwrapped.
pub fn lifted_effect_fn_names(fns: &[IrFunction]) -> std::collections::HashSet<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    fns.iter()
        .filter(|f| {
            f.is_effect
                && !matches!(&f.ret_ty,
                    Ty::Applied(TypeConstructorId::Result | TypeConstructorId::Option, _))
        })
        .map(|f| f.name.as_str().to_string())
        .collect()
}

/// PROGRAM-level pre-pass: inline a MUTUAL-recursive tail SIBLING so the caller becomes DIRECT
/// self-recursive — exposing the parser loops (`flow_rec ⇄ flow_step`, `collect_seq ⇄ seq_item`, …)
/// to the append-accumulator TCO, which only fires on a SELF-call.
///
/// For a function F that calls a sibling G where G calls F back (a mutual pair) and G is called by
/// ONLY F (so dead after inlining), every `G(args)` in F is replaced by G's body with G's parameters
/// substituted by the call's `args`, and G is dropped. Semantics-preserving (a plain inline).
///
/// TRY-LOWER GUARD (no regression by construction): the inline is applied ONLY when F currently WALLS
/// *and* the inlined F then LOWERS — so a function that already lowers (e.g. `esc_rec`, `collect_block`)
/// is NEVER touched (inlining could make it self-recursive and push it into a TCO path that walls). The
/// guard lowers F and inlined-F with the program's `globals`/`record_layouts`, exactly as the real
/// lowering will, so its verdict matches.
pub fn inline_mutual_tail_recursion(
    fns: &[IrFunction],
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Vec<IrFunction> {
    use std::collections::{HashMap as Map, HashSet};
    fn named_calls(body: &IrExpr) -> HashSet<String> {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C {
            names: HashSet<String>,
        }
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                    self.names.insert(name.as_str().to_string());
                }
                walk_expr(self, e);
            }
        }
        let mut c = C { names: HashSet::new() };
        c.visit_expr(body);
        c.names
    }
    // NEVER-ERR `!` STRIP (sound, the scoped form of the reverted blanket strip): an effect call whose
    // callee provably never returns `Err` has a no-op `!`, so `let pat = f()!` → `let pat = f()` and a
    // `f()!` self-call → bare `f()` (which `tco_collect` then recognizes). This is what lets the yaml
    // parser cluster (entirely never-err) TCO; `safe_div` & co. (can-err) keep their `!` and stay walled.
    // Done HERE, before the inline guard's try-lower, so inlined-F sees the stripped body and lowers.
    let can_err = compute_can_err(fns);
    let lifted_effect_fns = lifted_effect_fn_names(fns);
    // Publish the never-err lifted set (lifted ∖ can-err) for the match-subject wall (the rare residue
    // `rewrite_never_err_effect_match` cannot turn into a `let`-block — `ok(_)`/structured/guarded Ok).
    NEVER_ERR_LIFTED_FNS.with(|s| {
        *s.borrow_mut() =
            lifted_effect_fns.iter().filter(|n| !can_err.contains(*n)).cloned().collect();
    });
    AUTO_WRAP_ABI_FNS.with(|s| {
        *s.borrow_mut() = fns
            .iter()
            .filter(|f| f.name.as_str() != "main")
            .filter(|f| {
                !matches!(
                    &f.ret_ty,
                    Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Result
                            | almide_lang::types::constructor::TypeConstructorId::Option,
                        _
                    )
                ) && (body_has_stmt_position_propagating_unwrap(&f.body)
                    || body_has_tail_position_option_unwrap(&f.body))
            })
            .map(|f| f.name.as_str().to_string())
            .collect();
    });
    let stripped: Vec<IrFunction> = fns
        .iter()
        .map(|f| {
            let mut nf = f.clone();
            strip_never_err_unwraps(&mut nf.body, &can_err, &lifted_effect_fns, f.name.as_str());
            rewrite_fan_map_pure(&mut nf.body);
            crate::lower::desugar_option_str_literal_match(&mut nf.body);
            rewrite_never_err_effect_match(&mut nf.body, &can_err, &lifted_effect_fns);
            unwrap_never_err_call_types(&mut nf.body, &can_err, &lifted_effect_fns);
            rewrap_never_err_into_result_targets(
                &mut nf.body,
                &can_err,
                &lifted_effect_fns,
                record_layouts,
            );
            nf
        })
        .collect();
    let fns: &[IrFunction] = &stripped;
    let lowers =
        |f: &IrFunction| lower_function_all_with_types(f, globals, record_layouts).is_ok();
    let calls: Map<String, HashSet<String>> =
        fns.iter().map(|f| (f.name.as_str().to_string(), named_calls(&f.body))).collect();
    let mut callers: Map<String, HashSet<String>> = Map::new();
    for (f, cs) in &calls {
        for c in cs {
            callers.entry(c.clone()).or_default().insert(f.clone());
        }
    }
    let by_name: Map<&str, &IrFunction> = fns.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut rewritten: Map<String, IrFunction> = Map::new();
    let mut dropped: HashSet<String> = HashSet::new();
    for f in fns {
        let fname = f.name.as_str();
        if dropped.contains(fname) {
            continue;
        }
        // G: F calls G, G calls F back, G ≠ F, G local, ONLY F calls G (droppable).
        let g = calls[fname].iter().find(|g| {
            g.as_str() != fname
                && !dropped.contains(g.as_str())
                && by_name.contains_key(g.as_str())
                && calls.get(*g).is_some_and(|gc| gc.contains(fname))
                && callers.get(*g).is_some_and(|cs| cs.len() == 1 && cs.contains(fname))
        });
        if let Some(g) = g {
            // Guard: only inline if F WALLS now and the inlined F LOWERS (else leave both untouched —
            // no regression of an already-lowering function).
            if !lowers(f) {
                let mut nf = f.clone();
                inline_sibling_calls(&mut nf.body, g, by_name[g.as_str()]);
                if lowers(&nf) {
                    rewritten.insert(fname.to_string(), nf);
                    dropped.insert(g.clone());
                }
            }
        }
    }
    fns.iter()
        .filter(|f| !dropped.contains(f.name.as_str()))
        .map(|f| rewritten.remove(f.name.as_str()).unwrap_or_else(|| f.clone()))
        .collect()
}

/// Replace every `Call(callee_name, args)` in `body` with `callee`'s body, its parameters substituted
/// by `args` (a single-level inline; the inlined body's calls — back to the OUTER fn — are left as-is,
/// turning the caller into a direct self-recursion).
fn inline_sibling_calls(body: &mut IrExpr, callee_name: &str, callee: &IrFunction) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct V<'a> {
        name: &'a str,
        callee: &'a IrFunction,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind {
                if name.as_str() == self.name && args.len() == self.callee.params.len() {
                    let mut b = self.callee.body.clone();
                    for (p, a) in self.callee.params.iter().zip(args.iter()) {
                        b = almide_ir::substitute_var_in_expr(&b, p.var, a);
                    }
                    *expr = b;
                }
            }
        }
    }
    V { name: callee_name, callee }.visit_expr_mut(body);
}

/// Lower a function body expression to MIR (the param-free testable core;
/// `lower_function` is the wrapper that seeds parameters first).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

/// Like [`lower_body`] but returns the main function PLUS any lambda-lifted auxiliaries
/// the body produced (index 0 is the main). The plain [`lower_body`] discards the lifted
/// set, so a test that lifts a closure must use this to see (and verify) the lifted
/// function where the closure's body — and its captured calls — now live.
#[cfg(test)]
pub(crate) fn lower_body_all(body: &IrExpr, name: &str) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx { fn_name: name.to_string(), ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    let lifted = std::mem::take(&mut ctx.lifted);
    let mut all =
        vec![MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() }];
    all.extend(lifted);
    Ok(all)
}

/// Like [`lower_body`] but seeds the declared GLOBAL set (top-level `let`s) so a
/// reference to one is admitted by `value_or_global` instead of walled. Test/diagnostic
/// entry — `lower_function` builds the same context for real programs.
#[cfg(test)]
pub(crate) fn lower_body_with_globals(
    body: &IrExpr,
    name: &str,
    globals: HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx { globals, ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

#[derive(Default)]
pub(crate) struct LowerCtx {
    ops: Vec<Op>,
    /// VarId → the MIR value it denotes. Aliases map to the SAME ValueId.
    value_of: HashMap<VarId, ValueId>,
    /// Heap handles in binding order, for scope-end drops (one Drop per handle).
    live_heap_handles: Vec<ValueId>,
    /// The MIR values that are BORROWED heap parameters (the v1 calling
    /// convention): the caller owns the reference. A direct move-out/return or
    /// in-place mutation of one needs an explicit acquire (`Dup`) the body does
    /// not perform, so it is walled — never lowered to an unbacked cert event.
    param_values: HashSet<ValueId>,
    next_value: u32,
    /// Depth of enclosing control-flow FRAMES (branch arms / loop bodies). A heap
    /// reassignment at depth > 0 must NOT rebind `value_of` — the new handle would
    /// be frame-local (dropped at the frame's end), yet the var is read on the next
    /// iteration or after the branch merges, dereferencing a freed handle (a UAF the
    /// flat fold cannot see). Inside a frame such a reassignment is DEFERRED: the var
    /// keeps its still-live handle and the new value is carried like every `Opaque`.
    in_frame: u32,
    /// Depth of enclosing DEFUNCTIONALIZED HOF bodies (`list.map((x) => …)`) being lowered inline.
    /// When > 0, a SELF-RECURSIVE call in a heap-result body is BOUNDED (the map iterates a finite
    /// list; a `render_el(child, …)` recurses to the tree's depth, not unbounded), so it is ADMITTED —
    /// unlike a function-tail self-call (the unbounded TCO shape that overflows the stack), which the
    /// `lower_heap_result_arm` self-call gate still WALLS when this is 0.
    in_defunc_body: u32,
    /// Depth of enclosing SCALAR-STATE loops being lowered with real markers
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd`). When > 0, a scalar `Assign` reassigns
    /// the var's STABLE local via [`Op::SetLocal`] (the loop-carried state) instead of
    /// rebinding `value_of` to a fresh value (which a loop back-edge could not see), and a
    /// HEAP reassignment ERRORS — that aborts the scalar-loop attempt so `lower_while`
    /// falls back to its sound model-one-iteration form (a heap accumulator is deferred,
    /// not run, exactly as before).
    scalar_loop_depth: u32,
    /// Depth of enclosing EXECUTABLE Unit (statement) `if`/`match` arms — lowered with
    /// real markers (`IfThen`/`Else`/`EndIf`) so exactly ONE arm runs at runtime. When
    /// > 0, a scalar `Assign` to a var that ALREADY has a stable local (declared outside
    /// the arm — `var r = 0`) mutates that local via [`Op::SetLocal`] instead of rebinding
    /// `value_of` to a fresh value. A fresh rebind is frame-local: `value_of[var]` ends up
    /// pointing at whichever arm was lowered LAST (last-writer-wins), so a read after the
    /// branch sees a local only that arm's `local.set` wrote — but at runtime the OTHER
    /// arm ran, leaving it unset (the `match n { 0 => {r=100}, x => {r=999} }` 0-vs-999
    /// silent miscompile). SetLocal-to-the-stable-local is the faithful in-place mutation
    /// v0 performs. Distinct from `scalar_loop_depth` (loops also block heap rebinds and
    /// roll back the whole attempt); here a heap reassignment keeps the existing branch-arm
    /// DEFER behavior. Cert-neutral: a scalar `SetLocal` carries no heap ownership (the
    /// same no-op `verify_ownership` already proves for the loop-carried SetLocal).
    unit_arm_depth: u32,
    /// The module's top-level `let` bindings (VarId → declared Ty). A reference to one
    /// of these resolves to no FUNCTION-local `value_of` entry; this DECLARED set lets
    /// `value_or_global` distinguish a legitimate global reference (materialize a fresh
    /// external value) from a genuine lowering gap (a local that should have been bound
    /// — still WALLED). Confirming against the declared set, not merely a `value_of`
    /// miss, is what keeps the boundary a wall instead of a silent hole.
    globals: HashMap<VarId, Ty>,
    /// The module-level globals' INITIALIZER expressions (VarId → its `let` value). A HEAP
    /// global reference materializes a FRESH OWNED copy by lowering this initializer in place
    /// (`value_or_global`) — sound value semantics (each reference is an independent copy,
    /// dropped at scope end). Only initializers inside the lowering subset (a string literal,
    /// `bytes.from_list([…])`, a scalar-list literal) succeed; anything else keeps the wall.
    /// Empty for the simple entries; populated by the real pipeline from the program's top_lets.
    global_inits: HashMap<VarId, IrExpr>,
    /// MIR values KNOWN to be MATERIALIZED Options (the 0-or-1-element-list layout:
    /// `Some(x)` = `Init::OptSome` len=1, `None` = `Init::Opaque` len=0). A variant
    /// `match` may EXECUTE (read `len` as the tag, extract `data[0]`) ONLY over a
    /// subject in this set — every other Option (a closure/range/deferred `Opaque`, a
    /// non-self-host Option-returning call) is `Opaque` with len=0 and would MISREAD as
    /// `None`, so it keeps the sound LINEARIZED match. This is the gate that makes the
    /// len-as-tag execution safe without any global materialization invariant.
    materialized_options: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED Results (the DynListStr len-as-tag layout: `Ok(int)` =
    /// len 0 with the value in slot 0, `Err(string)` = len 1 owning the message). An `Ok`/`Err`
    /// `match` may EXECUTE (read `len` as the tag — len 0 → Ok, len != 0 → Err — and extract slot
    /// 0) ONLY over a subject in this set; any other Result is a deferred `Opaque` (len 0 → MISREADS
    /// as Ok) and keeps the sound LINEARIZED match. The Result analogue of `materialized_options`.
    materialized_results: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED HEAP-Ok Results (`Result[String, String]` etc.): a 1-slot
    /// DynListStr (cap 1, len 1 — IDENTICAL block size to every String, so the free-list reuses it)
    /// that ALWAYS owns one String in slot 0's LOW 32 bits (@12 — Ok's value OR Err's message), with
    /// the Ok/Err TAG in slot 0's HIGH 32 bits (@16: 0=Ok, 1=Err). `DropListStr` `i32.wrap`s the slot
    /// to the low-32 handle, so the high-32 tag is inert. An `Ok`/`Err` `match` reads @16 and binds
    /// the @12 handle as a borrowed String. The heap-Ok-payload analogue of `materialized_results`.
    materialized_results_str: HashSet<ValueId>,
    /// Lambda-lifted auxiliary functions produced while lowering this function's body
    /// (a non-capturing `let f = (x) => …` or a lambda call-argument lifts its body to a
    /// fresh MirFunction here, bound via `Op::FuncRef`). `lower_function_all` returns these
    /// alongside the main function so the program assembler tables + verifies them.
    lifted: Vec<crate::MirFunction>,
    /// The enclosing source function's name — the file-unique prefix for lifted lambda
    /// names (`__lambda_<fn_name>_<n>`). The corpus harness keys the in-profile map by name
    /// within a file, so two source functions each lifting `__lambda_0` would COLLIDE
    /// without this prefix (one lambda's certificate silently lost). Set by
    /// `lower_function_all`; empty for the param-free testable `lower_body` entry.
    fn_name: String,
    /// MIR values that denote a CLOSURE BLOCK — the uniform first-class function
    /// representation: a heap `[rc][len][cap][fnidx][captured…]` block (`lift_lambda`).
    /// A later call whose callee is one of these (`f(args)` where `f` bound a lambda /
    /// a function-typed param or call result) lowers to `Op::CallIndirect` through the
    /// block (fnidx loaded from slot 0, the block passed as the borrowed env arg —
    /// `emit_closure_call`) instead of deferring — the closure EXECUTES.
    closure_values: HashSet<ValueId>,
    /// C1 DIRECT-CALL INLINE: source-`VarId` → the INLINE lambda (`params`, `body`) a `let f =
    /// (x) => body` statically bound. A later DIRECT call `f(args)` whose callee is this `f`
    /// is DEFUNCTIONALIZED — the body is lowered INLINE with each param bound to its arg, and
    /// the captures resolve through `value_of` (they are in scope at the call site). This is
    /// what makes `let s = "ab"; let f = (x) => string.len(s) + x; f(1)` EXECUTE (return 3)
    /// instead of deferring the capturing lambda to an Opaque + `Const 0`. A lambda that ALSO
    /// lifts (non-capturing) keeps its `funcref_values` CallIndirect path; this map is the
    /// inline route for the CAPTURING / non-lifted case (recorded for BOTH, the call site
    /// prefers inline). Cleared per function (Default).
    lambda_bindings: HashMap<VarId, (Vec<(VarId, Ty)>, IrExpr)>,
    /// MIR values that are `List[String]` (NESTED-OWNERSHIP lists — their i64 slots hold OWNED
    /// String handles). A scope-end drop of one emits [`Op::DropListStr`] (recursive free),
    /// not a flat [`Op::Drop`] — so the element Strings are reclaimed. Populated when an
    /// `alloc_list_str` result or a `List[String]`-typed bind is created (Machinery 2).
    heap_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[List[String]]` (the csv `rows` shape: a list whose element slots
    /// hold owned `List[String]` blocks). A scope-end drop emits [`Op::DropListListStr`] (a NESTED
    /// free: each row's cell Strings, then each row block, then the outer block) — a flat
    /// `DropListStr` would only `rc_dec` each inner-list handle, LEAKING the cells. Populated by the
    /// list-of-lists concat (`rows + [cur]`).
    list_list_str_lists: HashSet<ValueId>,
    /// MIR values that are a `Result[Value, String]` (the `ok(value.array(...))` shape). A scope-end
    /// drop emits [`Op::DropResultValue`] (tag-dispatch: Ok → `$__drop_value`, Err → `rc_dec`) — a
    /// flat `DropListStr` would leak the Ok Value's nested payload.
    value_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(String, Int), String]` wrapper (toml `parse_key_part`'s
    /// `ok((slice, pos))`). A scope-end drop emits [`Op::DropResultStrInt`] (tag-dispatch: Ok → free
    /// the `(String, Int)` tuple @12 recursively (its String slot only), Err → `rc_dec` the String) —
    /// a flat `DropListStr` would leak the Ok tuple's String + free the tuple block as if it were one.
    str_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(Value, Int), String]` wrapper (toml `parse_val`'s
    /// `ok((value.…, pos))`). A scope-end drop emits [`Op::DropResultValueInt`] (Ok → free the
    /// (Value, Int) tuple recursively via `$__drop_value_tuple`, Err → `rc_dec` the String) — a flat
    /// `DropListStr` would leak the Value's nested payload.
    value_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(List[Value], Int), String]` wrapper (toml `collect_array_items`'s
    /// `ok((items, np))`). A scope-end drop emits [`Op::DropResultListValueInt`] (Ok → free the
    /// (List[Value], Int) tuple recursively via `$__drop_list_value_tuple`, Err → `rc_dec` the String).
    list_value_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(List[String], Int), String]` wrapper (toml `parse_key` /
    /// `parse_table_key`'s `ok((keys, pos))`). A scope-end drop emits [`Op::DropResultListStrInt`]
    /// (Ok → free the (List[String], Int) tuple recursively: each element String, the List block, the
    /// tuple block; Err → `rc_dec` the String) — a flat `DropListStr` would leak the List's Strings.
    list_str_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[List[String], String]` wrapper (the `fs.list_dir` `ok([name,…])`
    /// shape — NO tuple). A scope-end drop emits [`Op::DropResultListStr`] (Ok → free the List[String]
    /// payload recursively: each element String, then the List block; Err → `rc_dec` the String) — a
    /// flat `DropListStr` would `rc_dec` only the @12 List HANDLE, leaking the element Strings + block.
    list_str_result_results: HashSet<ValueId>,
    /// MIR values KNOWN to be a REAL, POPULATED list block (a list LITERAL, a heap-list PARAM —
    /// the v1 convention passes a genuine block —, or a self-host list-returning CALL whose closure
    /// args ALL lifted, so the callee actually fills it). A direct `xs[i]` (`lower_scalar_index_access`)
    /// computes a bounds-checked `$elem_addr` load that TRAPS on `i >= cap`, so it may fire ONLY over
    /// a value in this set: an Opaque/deferred list (a `list.map` whose param-invoking lambda could
    /// NOT lift → an empty/garbage block) has cap 0 and would TRAP at `xs[0]`, a NEW crash where the
    /// deferred `Const 0` merely mis-valued. Gating on real materialization keeps `xs[i]` from
    /// regressing an unmaterialized-list program to a runtime trap.
    materialized_lists: HashSet<ValueId>,
    /// Heap binds that fell to the DEFERRED `Alloc{Opaque}` model (an EMPTY block whose
    /// content is never populated — sound only while nothing READS through it). A
    /// custom-variant `match` over such a subject would read a garbage tag and execute
    /// the wrong arm (the record-ctor mt2 miscompile), so the match paths WALL on it.
    pub(crate) deferred_opaque_binds: HashSet<ValueId>,
    /// Set true by `lower_pure_module_call_args` when a closure ARGUMENT to a pure combinator could
    /// NOT be lifted to a FuncRef (a capturing / param-invoking lambda — `list.map(fns, (f) => f(10))`)
    /// and so fell back to `record_elided_calls`. The auto-linked self-host combinator then runs with
    /// a MISSING closure slot → an empty / garbage result list, NOT a faithfully-filled one. The
    /// `list.map` bind reads this to decide whether the result is a `materialized_lists` member (safe
    /// to index directly) — a genuinely-lifted map fills the list (admit `xs[i]`), an unlifted one
    /// does not (defer `xs[i]` to `Const 0`, no trap). Reset before each module-call arg lowering.
    last_call_had_unlifted_closure: bool,
    /// MIR values of the dynamic `Value` type (the Codec data model). A scope-end drop emits
    /// [`Op::DropValue`] (runtime-tag-dispatched: a Str/Array/Object Value frees its one heap
    /// payload, a scalar Value just frees the block) instead of a flat [`Op::Drop`]. Populated
    /// when a `Value`-typed bind is created.
    value_handles: HashSet<ValueId>,
    /// MIR values that are `List[Value]` (a list whose i64 slots hold OWNED dynamic `Value` handles,
    /// each itself possibly a heap-payload Str/Array). A scope-end drop emits [`Op::DropListValue`]
    /// (recursive `$__drop_value` per element) instead of the flat [`Op::DropListStr`], which would
    /// leak each element Value's nested payload. Populated when a `List[Value]` literal/arg is
    /// materialized. Distinct from `heap_elem_lists` (String elements, whose `rc_dec` is the full free).
    value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, Value)]` whose element slots hold owned (String, Value)
    /// TUPLE blocks (the yaml `pairs` shape). A scope-end drop emits [`Op::DropListStrValue`]
    /// (`$__drop_list_str_value`: per tuple, rc_dec the String slot + recursive `$__drop_value` the Value
    /// slot, then the tuple, then the list) — a flat [`Op::DropListStr`] would leak each tuple's payloads.
    /// Populated when a `List[(String,Value)]` concat is materialized via `__list_concat_rc`.
    str_value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, String)]` (the `map.entries` / svg render_attrs shape) —
    /// element slots hold owned (String, String) TUPLE blocks. A scope-end drop emits
    /// [`Op::DropListStrStr`] (`$__drop_list_str_str`: per tuple, rc_dec BOTH String slots, then the
    /// tuple, then the list). The (String,String) counterpart of `str_value_elem_lists`.
    str_str_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `value.as_array` Result `Result[List[Value], String]` (the cap-as-tag
    /// 1-slot block whose Ok payload @12 is a `List[Value]`). A scope-end drop emits
    /// [`Op::DropResultListValue`] (`$__drop_result_lv`: Ok → recursive list free, Err → String free)
    /// instead of the flat [`Op::DropListStr`] (which leaks the list's element Values). Read by the
    /// SAME cap@16 match machinery as a str-result (`materialized_results_str`); only the DROP differs.
    value_result_lists: HashSet<ValueId>,
    /// MIR values KNOWN to be a record/tuple block this brick MATERIALIZED with the uniform
    /// slot layout (`try_lower_scalar_record_construct` / `try_lower_record_construct` /
    /// `try_lower_scalar_tuple_construct` / scalar-tuple/list-slot), plus aggregate-typed
    /// params (the v1 convention passes the same-layout block pointer). A PRECISE field read
    /// that DEREFERENCES a loaded slot — a heap-field BORROW (`b.label`), which passes the
    /// loaded handle to a String/List consumer — is admitted ONLY over a value in this set:
    /// a DEFERRED `Alloc{Opaque}` aggregate (a spread record / a call result) has ZERO
    /// (garbage) slot handles, so loading + dereferencing one would TRAP at `rc_dec`. (A
    /// scalar field read does not dereference, so it tolerates a 0 slot as a benign mis-read;
    /// but a heap-field deref must be gated on REAL materialization.)
    materialized_aggregates: HashSet<ValueId>,
    /// MIR values that are MIXED scalar+heap record/tuple blocks → the i64-SLOT INDICES that
    /// hold an OWNED heap handle (a `String`/`List`/nested-aggregate field). Such a value's
    /// scope-end / per-iteration drop emits a [`Op::DropListStr`] (cert = the SAME single `d`
    /// as any drop — each heap field was accounted `m` when stored), and the render frees
    /// exactly these slots (then the block) via the per-value mask carried on the
    /// [`MirFunction::heap_slot_masks`] side table. A value here is treated like a
    /// `heap_elem_lists` member for drop-op SELECTION, but the mask makes the recursive free
    /// touch only the heap slots (NOT every slot — the scalar fields must not be `rc_dec`'d).
    record_masks: HashMap<ValueId, Vec<usize>>,
    /// The CURRENT binding (`lower_bind`) is a MUTABLE `var` (set by `lower_stmt` from the
    /// `Bind` mutability). A `var b = r.items` heap-field extraction may be COW-mutated later,
    /// so it must take an OWNED container-grain `Dup` (mutable in place), NOT a precise borrow
    /// (a shared field handle the value-model refuses to mutate). Read by `lower_heap_extraction`.
    binding_is_mutable: bool,
    /// Count of SYNTHETIC temp VarIds allocated while lowering this function (for ANF-lifting a
    /// Call-result whose heap field/element/tuple is extracted directly — `f(x).field`). Each
    /// synthetic id is `u32::MAX - n`, descending from the top of the VarId space so it can never
    /// collide with a frontend-assigned source VarId. See [`LowerCtx::fresh_synth_var`].
    synth_var_count: u32,
    /// The VarId of the CURRENT `Bind` being lowered (set by `lower_stmt`), so the value-lowering
    /// can ask whether THIS var is loop-reassigned (`loop_reassigned_vars`). `None` outside a
    /// statement bind (a sub-expression / argument materialization).
    binding_var: Option<VarId>,
    /// VarIds that are the TARGET of an `Assign` lexically INSIDE a loop (`while`/`for`) in the
    /// current function body — the loop-carried-reassignment (option-C) slots. A MUTABLE `var x =
    /// r.field` whose `x` is in this set must NOT take the owned field-`Dup` (the initial owned copy
    /// + the option-C per-iteration drop are an UNPROVEN ownership coordination that the kernel cert
    /// REJECTS as a leak); such a bind WALLS (`lower_heap_extraction`). A var reassigned only at
    /// straight-line top level is NOT here (that owned-Dup + scope-end drop is balanced). Computed
    /// once per function in `lower_body_into`.
    loop_reassigned_vars: std::collections::HashSet<VarId>,
    /// Named-record layout registry (the VALUE-MODEL field structure): type NAME →
    /// (declared generic param names, declared fields in declaration order). A record
    /// literal / field access typed `Ty::Named(name, args)` resolves its fields here
    /// (substituting the generic params with `args`), so `r.x` loads from the same slot
    /// construction stored to. Empty when lowering without a type registry (the
    /// param-free testable entry) — a `Ty::Named` aggregate then stays walled, a
    /// `Ty::Record`/`Ty::Tuple` (structurally typed) still resolves directly.
    record_layouts: RecordLayouts,
    /// Custom-variant (ADT) layout registry (the tag + per-constructor field structure):
    /// type NAME → its [`VariantLayout`], with a ctor-name → type reverse index. A variant
    /// CONSTRUCT / `match` resolves its tag and field slots here, the value-model sibling of
    /// `record_layouts`. Empty when lowering without a type registry — a variant value then
    /// stays walled (the pre-ADT-brick status quo). Populated by [`build_variant_layouts`]
    /// and threaded via [`lower_function_all_with_layouts`].
    variant_layouts: VariantLayouts,
    /// Constructed CUSTOM-VARIANT values whose scope-end drop must be the RECURSIVE
    /// [`Op::DropVariant`] (a nested-variant type — `Add(Expr, Expr)` — whose flat free would leak
    /// child blocks), mapped to their TYPE NAME (so the render calls the generated `$__drop_<ty>`).
    /// `drop_op_for` consults this before the flat/masked drops. Populated by
    /// `try_lower_variant_ctor` for a type that [`VariantLayouts::needs_recursive_drop`] (ADT brick 5b).
    variant_drop_handles: HashMap<ValueId, String>,
    /// True when THIS function's DECLARED return type is an explicit `Result`/`Option` (e.g.
    /// `effect fn fs.write(...) -> Result[Unit, String]`). Such a return is a REAL inspectable
    /// heap value the caller `match`es on — so a `Result[Unit, _]` TAIL must produce the heap
    /// Result (the heap path), NOT be voided. It is ONLY the SYNTHETIC `Result[Unit, _]` of an
    /// `effect fn … -> Unit` (declared return `Unit`, this flag FALSE) that the
    /// `is_unit_result_ty` voiding in `lower_tail` should turn into a void wasm function. Without
    /// this discriminator a `-> Result[Unit, String]` fn (fs.write) would be emitted void while
    /// its call site treats it as a heap result — a `(local.set $r (call $void))` type mismatch
    /// (invalid wasm). EXACTLY the `lifted_effect_fn_names` predicate's complement (an effect fn
    /// already declaring Result/Option is NOT lifted — its return is real). Set in
    /// `lower_function_all_impl`; defaults false (the void convention) for the bare entries.
    decl_ret_is_result: bool,
    /// The fn's REAL compiled ABI returns `Result[T, String]` — either GENUINELY declared
    /// `-> Result[..]` (STRICTLY Result: an `-> Option[..]` fn is excluded, since a bare tail
    /// Option-`!` there is a same-repr pass-through, which is already correct), or auto-wrapped
    /// via [`AUTO_WRAP_ABI_FNS`]. Gates the bare-tail-Option-`!` desugar
    /// (`desugar_tail_effect_unwrap`): under a Result ABI that pass-through returns the RAW
    /// Option handle AS the Result — a confirmed silent wrong-value. Threaded as an explicit
    /// per-fn FACT (the `unit_main` pattern) so the desugar never trusts a tree-local `.ty`.
    ret_is_result_abi: bool,
}

/// Type NAME → (generic param names, declaration-ordered fields) — the VALUE-MODEL
/// field registry threaded into lowering (see [`LowerCtx::record_layouts`]).
pub type RecordLayouts =
    HashMap<String, (Vec<almide_lang::intern::Sym>, Vec<(almide_lang::intern::Sym, Ty)>)>;

/// One constructor of a variant type, as the value model sees it: its name, its `tag`
/// (the declaration index — `type E = Lit(Int) | Add(E,E) | Neg(E)` gives Lit=0, Add=1,
/// Neg=2), and its declaration-ordered fields. A TUPLE constructor's positional fields
/// are named `_0`, `_1`, … and a RECORD constructor keeps its declared names — the same
/// synthesis v0 (`emit_wasm` variant registration) uses, so the two backends agree on
/// field identity. A UNIT constructor has no fields.
#[derive(Clone, Debug)]
pub struct VariantCaseLayout {
    pub ctor: almide_lang::intern::Sym,
    pub tag: u32,
    pub fields: Vec<(almide_lang::intern::Sym, Ty)>,
}

/// One variant type's VALUE-MODEL layout. A v1 variant value is a record-like heap block
/// in the SAME uniform-i64-slot model records use (NOT v0's byte-packed layout — only the
/// OBSERVABLE output must match v0, never the internal bytes): `slot 0` holds the tag and
/// `slots 1..` hold the ACTIVE constructor's fields. `slot_count` is `1 + max arity over
/// all cases`, so EVERY constructor of the type occupies an identically sized block — a
/// uniform alloc and a sound `==` over the whole block, the v1 analogue of v0's
/// max-payload padding (`variant_alloc_size`).
#[derive(Clone, Debug)]
pub struct VariantLayout {
    pub generics: Vec<almide_lang::intern::Sym>,
    /// Indexed by tag (`cases[t].tag == t`).
    pub cases: Vec<VariantCaseLayout>,
    pub slot_count: usize,
}

impl VariantLayout {
    /// The case whose constructor is `ctor`, if any.
    pub fn case_by_ctor(&self, ctor: &str) -> Option<&VariantCaseLayout> {
        self.cases.iter().find(|c| c.ctor.as_str() == ctor)
    }
}

/// Recursively replace a bare generic-parameter reference (`Ty::Named(p, [])` where `p` is a
/// key of `subst`) with its concrete binding — the DECLARATION-time field type of a generic
/// variant (`type Either[L,R] = Left(L) | Right(R)`) stores `L`/`R` verbatim as `Named("L",[])`/
/// `Named("R",[])` (confirmed via debug trace, NOT `Ty::TypeVar`), so heap/flat classification
/// over the RAW registry entry is blind to any concrete instantiation. Recurses into `Named`'s
/// own args and `Applied`'s args (a generic parameter could itself appear nested, e.g.
/// `List[L]`) so a partially-generic composite field also resolves correctly.
fn substitute_generic_ty(ty: &Ty, subst: &HashMap<almide_lang::intern::Sym, Ty>) -> Ty {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, args) if args.is_empty() => {
            subst.get(n).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(n, args) => {
            Ty::Named(*n, args.iter().map(|a| substitute_generic_ty(a, subst)).collect())
        }
        Ty::Applied(TypeConstructorId::UserDefined(n), args) => Ty::Applied(
            TypeConstructorId::UserDefined(n.clone()),
            args.iter().map(|a| substitute_generic_ty(a, subst)).collect(),
        ),
        Ty::Applied(c, args) => {
            Ty::Applied(c.clone(), args.iter().map(|a| substitute_generic_ty(a, subst)).collect())
        }
        _ => ty.clone(),
    }
}

/// A WASM-identifier-safe, unique suffix for a generic variant instantiation (`Either` +
/// `[Int, String]` → `"Either_Int_String"`) — the name of the PER-INSTANTIATION drop function
/// (`$__drop_<this>`/`$__drop_list_<this>`) generated for it, distinct from the bare generic
/// name so two different instantiations of the same type (were the corpus ever to use both)
/// never collide on one ambiguous function. `None` for an arg shape not confidently nameable
/// here (a nested generic instantiation, a tuple, …) — the caller declines (stays walled)
/// rather than guess a name that could collide or misrender.
/// The bare Almide SOURCE spelling of a scalar `Ty` — the set both the instantiation-name
/// mangler and the shadow-type-declaration renderer (`generate_generic_variant_instantiation_
/// sources`, drop_sources.rs) treat as safely nameable/renderable. Kept as ONE shared list so
/// the two never drift apart (a type nameable-but-not-renderable, or vice versa, would break the
/// admission⟹generation invariant `is_rich_variant_ty` depends on).
pub fn generic_variant_instantiation_scalar_name(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Int => Some("Int"),
        Ty::Float => Some("Float"),
        Ty::Bool => Some("Bool"),
        Ty::String => Some("String"),
        Ty::Int8 => Some("Int8"),
        Ty::Int16 => Some("Int16"),
        Ty::Int32 => Some("Int32"),
        Ty::Int64 => Some("Int64"),
        Ty::UInt8 => Some("UInt8"),
        Ty::UInt16 => Some("UInt16"),
        Ty::UInt32 => Some("UInt32"),
        Ty::UInt64 => Some("UInt64"),
        Ty::Float32 => Some("Float32"),
        Ty::Float64 => Some("Float64"),
        _ => None,
    }
}

pub fn generic_variant_instantiation_name(base: &str, args: &[Ty]) -> Option<String> {
    let mut out = base.to_string();
    for a in args {
        let piece = generic_variant_instantiation_scalar_name(a)?;
        out.push('_');
        out.push_str(piece);
    }
    Some(out)
}

/// The variant-type sibling of [`RecordLayouts`]: type NAME → its [`VariantLayout`], plus a
/// constructor-name → owning-type reverse index (a `Lit(7)` constructor expression carries
/// its ctor name; this resolves the variant type the way v0's `find_variant_tag_by_ctor`
/// fallback does). Threaded into lowering alongside `record_layouts` so a variant
/// construct / `match` can find its tag + field layout. Empty when lowering without a type
/// registry — a variant value then stays walled (the pre-ADT-brick status quo).
#[derive(Clone, Debug, Default)]
pub struct VariantLayouts {
    pub by_type: HashMap<String, VariantLayout>,
    pub ctor_to_type: HashMap<String, String>,
    /// Record-variant field DEFAULT exprs (`Rect { color: String = "" }`), keyed
    /// `ctor → field → expr` — consulted by the ctor builder when a literal OMITS a
    /// defaulted field (v0 fills the default at construction; leaving the slot would be
    /// garbage, and declining walled the whole default_fields family).
    pub ctor_field_defaults: HashMap<String, HashMap<String, almide_ir::IrExpr>>,
}

impl VariantLayouts {
    /// Resolve a constructor name to its owning type's name + layout + the specific case.
    pub fn lookup_ctor(&self, ctor: &str) -> Option<(&str, &VariantLayout, &VariantCaseLayout)> {
        let ty = self.ctor_to_type.get(ctor)?;
        let layout = self.by_type.get(ty)?;
        let case = layout.case_by_ctor(ctor)?;
        Some((ty.as_str(), layout, case))
    }

    /// The CORE of [`Self::needs_recursive_drop`], factored out so an INSTANTIATED (generic-
    /// substituted) case list can share the exact same classification as the raw registry entry
    /// — the two must never disagree (a false "doesn't need recursion" verdict on a heap field
    /// is a silent leak, not a wall).
    fn cases_need_recursive_drop(
        &self,
        cases: &[VariantCaseLayout],
        is_record: &dyn Fn(&str) -> bool,
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        // Mirrors the generator's `variant_needs_recursive_drop`: a nested-variant field (the
        // original rule) OR heap fields the generated drop can ALL free (String / List[scalar] /
        // List[variant] / List[String] (per-element via `__drop_list_str`) / a RECORD — via
        // `$__drop_<R>` or a scalar-only record's flat rc_dec). The `is_record` predicate is
        // supplied by the caller (LowerCtx checks its record registry).
        let supported_heap = |t: &Ty| -> bool {
            self.field_is_variant(t)
                || matches!(t, Ty::Named(n, _) if is_record(n.as_str()))
                || matches!(t, Ty::String)
                || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1
                        && (!is_heap_ty(&a[0])
                            || matches!(a[0], Ty::String)
                            || self.field_is_variant(&a[0])))
                || matches!(t, Ty::Applied(TypeConstructorId::Option, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
        };
        let mut any_heap = false;
        let mut all_supported = true;
        let mut has_variant_field = false;
        for c in cases {
            for (_, ty) in &c.fields {
                if self.field_is_variant(ty) {
                    has_variant_field = true;
                }
                if is_heap_ty(ty) {
                    any_heap = true;
                    if !supported_heap(ty) {
                        all_supported = false;
                    }
                }
            }
        }
        has_variant_field || (any_heap && all_supported)
    }

    /// Does the variant type `type_name` need the RECURSIVE [`Op::DropVariant`] (the generated
    /// `$__drop_<ty>`) — i.e. does some ctor field hold another user variant whose flat free would
    /// leak its children? A String-only-field variant uses the masked `DropListStr` instead (ADT
    /// brick 5a/5c). This is the lowering-side mirror of
    /// [`crate::lower::variant_needs_recursive_drop`], computed from the registry's field Tys.
    /// UNSUBSTITUTED: for a GENERIC variant this reads the raw declaration (type-parameter
    /// placeholders, never a concrete instantiation) — see [`Self::instantiated_needs_recursive_
    /// drop`] for the instantiation-aware sibling a `List[<generic variant>]` element check needs.
    pub fn needs_recursive_drop(&self, type_name: &str, is_record: &dyn Fn(&str) -> bool) -> bool {
        let Some(layout) = self.by_type.get(type_name) else { return false };
        self.cases_need_recursive_drop(&layout.cases, is_record)
    }

    /// Substitute a generic variant's DECLARED field types (`Left(L) | Right(R)` → `L`/`R` as
    /// bare `Ty::Named(sym,[])` placeholders, confirmed via debug trace — never `Ty::TypeVar`)
    /// with the CONCRETE type args at one instantiation site (`Either[Int,String]`'s `[Int,
    /// String]`, zipped positionally against `layout.generics`). A NON-generic variant (`layout.
    /// generics.is_empty()`) returns its cases UNCHANGED (zero-cost passthrough, no behavior
    /// change for the entire existing non-generic corpus). `None` on an arity mismatch (the
    /// checker guarantees this never happens for a well-typed program, but a mismatched
    /// registry/call-site pairing declines rather than substituting garbage).
    fn instantiated_cases(&self, type_name: &str, args: &[Ty]) -> Option<Vec<VariantCaseLayout>> {
        let layout = self.by_type.get(type_name)?;
        if layout.generics.is_empty() {
            return Some(layout.cases.clone());
        }
        if layout.generics.len() != args.len() {
            return None;
        }
        let subst: HashMap<almide_lang::intern::Sym, Ty> =
            layout.generics.iter().copied().zip(args.iter().cloned()).collect();
        Some(
            layout
                .cases
                .iter()
                .map(|c| VariantCaseLayout {
                    ctor: c.ctor,
                    tag: c.tag,
                    fields: c
                        .fields
                        .iter()
                        .map(|(n, t)| (*n, substitute_generic_ty(t, &subst)))
                        .collect(),
                })
                .collect(),
        )
    }

    /// The instantiation-aware sibling of [`Self::needs_recursive_drop`] — substitutes generic
    /// field types with `args` BEFORE classifying, so `Either[Int,String]`'s `Right(String)` case
    /// is correctly seen as heap (unlike the raw registry's unresolved `Right(R)`). Identical to
    /// `needs_recursive_drop` for a non-generic type (args ignored via `instantiated_cases`'s
    /// passthrough).
    pub fn instantiated_needs_recursive_drop(
        &self,
        type_name: &str,
        args: &[Ty],
        is_record: &dyn Fn(&str) -> bool,
    ) -> bool {
        match self.instantiated_cases(type_name, args) {
            Some(cases) => self.cases_need_recursive_drop(&cases, is_record),
            None => false,
        }
    }

    /// Extract `(bare type name, concrete type args)` from a variant-type reference — the
    /// SHARED match arms `is_flat_variant_ty`/`is_rich_variant_ty`/`field_variant_name` each
    /// duplicated (discarding the args); factored out so the instantiation-aware paths can see
    /// both halves. `Ty::Named(n, args)` carries a GENERIC variant's concrete instantiation args
    /// at a USE site (`Either[Int,String]` → `Named("Either", [Int, String])`, confirmed via
    /// debug trace) — `args` is empty for a non-generic reference or an unresolved bare mention.
    fn variant_name_and_args(ty: &Ty) -> Option<(&str, &[Ty])> {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Named(n, args) => Some((n.as_str(), args.as_slice())),
            Ty::Variant { name, .. } => Some((name.as_str(), &[])),
            Ty::Applied(TypeConstructorId::UserDefined(n), args) => Some((n.as_str(), args.as_slice())),
            _ => None,
        }
    }

    /// Is `ty` a registry variant ALL of whose constructors have ONLY scalar fields — i.e. a FLAT
    /// tag-block with NO heap slot (a nullary enum like `Capability`, or a scalar-payload variant)?
    /// Such a block is a single allocation freed by one `prim.rc_dec`, so a `List[flat-variant]`
    /// drops correctly via the per-element-`rc_dec` `__drop_list_str` (each element + the list block),
    /// the SAME flat shape as a `List[String]`. A variant carrying a `String`/nested/`List` field is
    /// NOT flat (its block owns an inner handle a flat `rc_dec` would leak) → `false` (stays walled).
    /// Substitutes generic field types against `ty`'s own instantiation args first (a no-op for a
    /// non-generic variant), so a generic instantiated with an all-scalar arg set (`Pair[Int,Int]`)
    /// is correctly flat while one with a heap arg (`Either[Int,String]`) correctly is not.
    pub fn is_flat_variant_ty(&self, ty: &Ty) -> bool {
        let Some((n, args)) = Self::variant_name_and_args(ty) else { return false };
        match self.instantiated_cases(n, args) {
            Some(cases) => cases.iter().all(|c| c.fields.iter().all(|(_, fty)| !is_heap_ty(fty))),
            None => false,
        }
    }

    /// Is `ty` a RICH (recursive-drop) registry variant — a user variant for which `$__drop_<V>` and
    /// `$__drop_list_<V>` are generated (some ctor holds a nested user variant whose flat free would
    /// leak its children)? This is the lowering-side gate for admitting a `List[<rich variant>]`
    /// element (the wasm `Instr` accumulator) — its drop routes to `$__drop_list_<V>` via
    /// `variant_drop_handles`. Mirrors [`crate::lower::variant_needs_recursive_drop`] (the generator's
    /// gate) so the two never disagree: a variant admitted here ALWAYS has a generated `$__drop_list_<V>`.
    ///
    /// For a GENERIC variant instantiated with concrete args (`Either[Int,String]`), the returned
    /// name is the INSTANTIATION-SPECIFIC one (`generic_variant_instantiation_name`, e.g.
    /// `"Either_Int_String"`) rather than the bare generic name — a distinct `$__drop_list_<this>`
    /// is generated per instantiation actually used (see `discover_generic_variant_list_
    /// instantiations` in drop_sources.rs), since a single shared function could not correctly
    /// serve two instantiations with DIFFERENT per-slot heap-ness. `None` if the args aren't a
    /// confidently nameable shape (declines / stays walled rather than risk a colliding name) or
    /// this specific instantiation doesn't actually need recursive drop.
    pub fn is_rich_variant_ty(&self, ty: &Ty) -> Option<String> {
        let (n, args) = Self::variant_name_and_args(ty)?;
        if !self.by_type.contains_key(n) {
            return None;
        }
        // A `List[<variant>]` ELEMENT check keys on nested-variant fields only (a variant with a
        // record field is neither flat nor list-rich here → the list materializer walls it cleanly,
        // never a leak). The record-widening applies to the direct-ctor `needs_rec` (LowerCtx), not
        // to the list-element admission.
        if args.is_empty() {
            return self
                .needs_recursive_drop(n, &|_| false)
                .then(|| n.to_string());
        }
        let inst_name = generic_variant_instantiation_name(n, args)?;
        // ADMISSION must never outrun GENERATION: the shadow `type <inst_name> = ...` +
        // `$__drop_<inst_name>` source text (`generate_generic_variant_instantiation_sources`,
        // drop_sources.rs) can only render a field whose SUBSTITUTED type is one of the scalars
        // `generic_variant_instantiation_name` itself already supports, or another ALREADY-
        // DECLARED (non-generic) user variant referenced by its real bare name. A field type
        // outside that set (e.g. a generic field like `Left(List[L])` — Either's OWN fields
        // happen to be bare type params, so this never fires for it, but a future generic
        // variant might declare a composite field) declines the WHOLE instantiation here, so a
        // "yes" from this method is ALWAYS backed by real generated source — never a dangling
        // `$__drop_list_<inst_name>` call (the exact class of bug this campaign nearly shipped
        // once already, this session, on a different wall).
        let cases = self.instantiated_cases(n, args)?;
        if !cases.iter().all(|c| {
            c.fields.iter().all(|(_, fty)| {
                generic_variant_instantiation_scalar_name(fty).is_some() || self.field_is_variant(fty)
            })
        }) {
            return None;
        }
        self.instantiated_needs_recursive_drop(n, args, &|_| false)
            .then_some(inst_name)
    }

    /// Is `ty` one of the variant types in this registry (a nested-variant ctor field)?
    pub fn field_is_variant(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str(),
            Ty::Variant { name, .. } => name.as_str(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.as_str(),
            _ => return false,
        };
        self.by_type.contains_key(n)
    }

    /// The variant type NAME of `ty` if it is a registry variant (the recursion / construct target).
    pub fn field_variant_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Variant { name, .. } => name.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        self.by_type.contains_key(&n).then_some(n)
    }
}

/// Is `ty` the dynamic `Value` type (the Codec data model)? Its scope-end drop is the
/// runtime-tag-dispatched [`Op::DropValue`], since a heap-payload Value (Str/Array/Object) owns a
/// handle the flat `Drop` would leak.
pub fn is_value_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Named(name, _) => name.as_str() == "Value",
        Ty::Variant { name, .. } => name.as_str() == "Value",
        _ => false,
    }
}

/// Does `ty` CONTAIN a function type anywhere (a `Ty::Fn`, or a List/Option/etc. OF functions —
/// `List[(Int) -> Int]`)? A self-host list combinator over such an argument (`list.map(fns, …)`
/// where `fns: List[(Int)->Int]`) cannot faithfully fill its result (the v1 model has no
/// representation for a list of closures), so the result is empty/garbage and must NOT be treated
/// as a real `materialized_lists` block (a direct `xs[i]` over it would trap on cap 0).
pub(crate) fn ty_contains_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Applied(_, args) => args.iter().any(ty_contains_fn),
        Ty::Tuple(tys) => tys.iter().any(ty_contains_fn),
        _ => false,
    }
}

/// Is `ty` a `List[T]` whose element `T` is a SCALAR (non-heap) type (`List[Int/Float/Bool]`)?
/// Such a list's slots are plain i64 values — a direct `xs[i]` reads one with `Load { width: 8 }`,
/// and `__list_concat` byte-copies them with no ownership. The complement of `is_heap_elem_list_ty`
/// for the List constructor.
pub(crate) fn is_scalar_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]))
}

/// Is `ty` a `List[T]` / `Option[T]` whose element `T` is itself a HEAP type (e.g. `List[String]`,
/// `Option[String]`)? Such a container OWNS its element(s) — it needs the recursive
/// [`Op::DropListStr`], not a flat drop. An `Option[String]` is physically a 0-or-1-element
/// `List[String]` (Machinery 2), so the SAME recursive free applies (len 0 frees nothing, len 1
/// frees the one element + the block).
/// A `List[List[String]]` — its element slots hold owned `List[String]` blocks (the csv `rows`
/// shape). Its scope-end drop must be [`Op::DropListListStr`] (the nested cell + row free); a flat
/// `DropListStr` (what `is_heap_elem_list_ty` would route it to, since List[List[String]] is also a
/// `List[heap]`) would only `rc_dec` each row HANDLE, leaking the cell Strings. So EVERY tracking
/// site checks this FIRST.
pub(crate) fn is_list_list_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // A `List[Matrix]` (matrix.split_cols_even's result) is the SAME two-level shape: a
    // v1 Matrix IS a List[List[Float]] block whose slots hold owned row handles, so
    // `DropListListStr`'s per-element inner sweep (rc_dec each row + the matrix block,
    // then the outer block) is its exact recursive free — each row is a FLAT f64 block,
    // like a String. The flat `DropListStr` would leak every row.
    if matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1
            && matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)))
    {
        return true;
    }
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)))
}

/// An `Option[List[String]]` — the heap-accumulator fold's value (is_balanced's paren
/// stack). PHYSICALLY a 0/1-element `List[List[String]]`, so `DropListListStr`'s nested
/// sweep (per outer slot: rc_dec each inner cell String + the inner block, then the outer
/// block) is its exact recursive free — the flat `DropListStr` (`heap_elem_lists`) would
/// rc_dec only the inner-list HANDLE, leaking every stack String (a fold loop OOMs).
pub(crate) fn is_opt_list_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(&a[0],
            Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)))
}

/// A `List[(String, String)]` — the `map.entries` / render_attrs shape. Each element is an owned
/// (String, String) TUPLE; its scope-end drop must be [`Op::DropListStrStr`] (per tuple: rc_dec BOTH
/// String slots, then the tuple, then the list). The flat `DropListStr` (`heap_elem_lists`) would
/// rc_dec only the tuple HANDLE — freeing the tuple block but LEAKING its two Strings (a render loop
/// OOMs). Checked BEFORE `is_heap_elem_list_ty` (which also matches this List type).
pub(crate) fn is_list_str_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // BOTH pair sides must be single FLAT blocks — a String or a List[scalar] row
    // (list.zip_rc over matrix rows) — so DropListStrStr's two per-slot rc_decs are each
    // a FULL free. A rich payload (List[heap], record, Value) stays out (would leak).
    let flat = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, b)
                if b.len() == 1 && !is_heap_ty(&b[0]))
    };
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Tuple(tys) if tys.len() == 2 && flat(&tys[0]) && flat(&tys[1])))
}

/// A `List[(Int, String)]` — the `list.enumerate` result. Each element is an (Int @12 scalar, String
/// @20 heap) tuple; its scope-end drop must be the recursive `$__drop_list_int_str` (rc_dec each
/// tuple's String + block), routed via `variant_drop_handles="list_int_str"`. A flat `DropListStr`
/// would leak each tuple's String (a 10⁴ loop OOMs).
/// `Map[Int, String]` — the scalar-key / owned-heap-value map (self-host map_ivh).
pub fn is_map_ivh_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::Int) && matches!(a[1], Ty::String))
}

/// `Map[String, List[scalar]]` — the String-key / FLAT-heap-value map (self-host
/// map_hval; a flat value block's rc_dec is its full free).
pub fn is_map_hval_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String) && matches!(&a[1],
                Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && !is_heap_ty(&e[0])))
}

pub(crate) fn is_list_int_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)))
}

/// `Result[Unit, _]` — the static type of an `effect fn … -> Unit` CALL (the auto-`?`
/// effect Result carrying no value). The v1 MIR pipeline lowers such an effect fn to a
/// VOID wasm function (no `func.ret`), so a call to it is an EFFECT statement, never a
/// scalar/heap value. Used to route a `Result[Unit, _]`-typed tail/value call to the
/// effect-call path instead of the scalar-call path (which would expect an i32 result
/// the void callee never produces — an invalid-wasm type mismatch).
pub(crate) fn is_unit_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(a[0], Ty::Unit))
}

/// A `Result[Value, String]` — the `ok(value.array(...))` shape. Its Ok payload is a dynamic Value
/// (freed RECURSIVELY via `$__drop_value`), its Err a String. Its scope-end drop must be
/// [`Op::DropResultValue`] (the tag-dispatched recursive free); a flat `DropListStr` would leak the
/// Ok Value's nested payload.
pub fn is_value_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && is_value_ty(&a[0]) && matches!(a[1], Ty::String))
}

/// `Result[(String, Int), String]` — the toml `parse_key_part` `ok((slice, pos))` shape. Its Ok
/// payload is a `(String, Int)` tuple (a heap String slot + a scalar Int slot), so both the producer
/// (`try_lower_result_str_int_ctor`) and the match-subject drop route it to `str_int_result_results`
/// (the recursive `Op::DropResultStrInt`), NOT the flat `heap_elem_lists`/`DropListStr` which would
/// leak the tuple's String.
pub fn is_str_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[(Value, Int), String]` — the toml `parse_val` `ok((value.…, pos))` shape. The Ok payload
/// is a `(Value, Int)` tuple (a dynamic-Value heap slot + a scalar Int); routed to
/// `value_int_result_results` (recursive `Op::DropResultValueInt` via `$__drop_value_tuple`).
pub fn is_value_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && is_value_ty(&ts[0]) && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[(List[String], Int), String]` — the toml `parse_key` / `parse_table_key` `ok((keys, pos))`
/// shape. The Ok-tuple's slot0 is a `List[String]`; routed to `list_str_int_result_results` (recursive
/// `Op::DropResultListStrInt`, which frees the inner List's element Strings).
pub fn is_list_str_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(&ts[0], Ty::Applied(TypeConstructorId::List, le)
                    if le.len() == 1 && matches!(le[0], Ty::String))
                && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[List[String], String]` — the `fs.list_dir` `ok([name,…])` shape (NO tuple, the DIRECT
/// list). The Ok payload @12 is a `List[String]`; routed to `list_str_result_results` (recursive
/// `Op::DropResultListStr`, which frees the inner List's element Strings + block). Distinct from
/// `is_list_str_int_result_ty` (that one's Ok is a `(List[String], Int)` tuple).
pub fn is_list_str_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Applied(TypeConstructorId::List, le)
                if le.len() == 1 && matches!(le[0], Ty::String))
            && matches!(a[1], Ty::String))
}

/// `Result[(List[Value], Int), String]` — toml `collect_array_items`. The Ok-tuple's slot0 is a
/// `List[Value]`; routed to `list_value_int_result_results` (recursive `Op::DropResultListValueInt`,
/// freeing each element Value via `$__drop_list_value`).
pub fn is_list_value_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(&ts[0], Ty::Applied(TypeConstructorId::List, le)
                    if le.len() == 1 && is_value_ty(&le[0]))
                && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

pub(crate) fn is_heap_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        // A `Matrix` VALUE (the v1 value model): a List[List[Float]] block whose slots
        // hold owned row handles — each row a FLAT f64 block, so the per-slot-rc_dec
        // `DropListStr` is its exact recursive free (a Matrix drops like a List[String]).
        Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _) => true,
        // `List[heap]` / `Option[heap]` / `Set[heap]` — heap element slots (DynListStr nested
        // ownership). A `Set[heap]` is physically a `List[heap]` of unique elements, so the SAME
        // recursive free applies (each owned element + the block).
        Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option | TypeConstructorId::Set, args)
            if args.len() == 1 && is_heap_ty(&args[0]) =>
        {
            true
        }
        // `Result[_, heap-Err]` is physically the SAME DynListStr (the Ok/Err materialization reuses
        // it): `Err` owns the heap Err payload in slot 0 (len 1 → DropListStr frees it), `Ok` is
        // len 0 (frees nothing). So a Result value is dropped recursively, exactly like Option[heap].
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 && is_heap_ty(&args[1]) => {
            true
        }
        // `Map[heap, heap]` (e.g. `Map[String, String]`) — a DynListStr of INTERLEAVED key+value
        // String handles [k0,v0,k1,v1,...]; EVERY slot is a heap handle, so the uniform recursive
        // DropListStr frees all keys and values. (`len` = the slot count; map.len reads len/2.)
        Ty::Applied(TypeConstructorId::Map, args)
            if args.len() == 2 && is_heap_ty(&args[0]) && is_heap_ty(&args[1]) =>
        {
            true
        }
        _ => false,
    }
}

