// `infer_expr_inner` group 2 — literals, identifiers, simple containers,
// and the operator / control-flow arms (Int … Match). Disjoint from every
// other group; see `infer_expr_inner` for the dispatch contract. Split out
// of `infer.rs` (via `include!`) to keep each file under the 1000-line
// ceiling; imports come from `infer.rs` (this file is textually inlined).

/// What [`Checker::infer_match_arms`] learned about a match's arms.
///
/// `types` are the JOIN types (an `err(..)` arm reads as `Never`, and in an
/// effect fn a `Result[T, E]` arm reads as `T`); `real_types` are the
/// un-substituted ones, which recover a concrete type when every arm is
/// `Never`; `peers` is the #880 peer set — (arm type, span, body is
/// literal-only) — which match arms join by exactly like list elements and
/// `if` branches.
#[derive(Default)]
struct MatchArmTypes {
    types: Vec<Ty>,
    real_types: Vec<Ty>,
    peers: Vec<(Ty, Option<ast::Span>, bool)>,
    /// Every `err(..)` arm's payload type and body span. The join reads such
    /// an arm as `Never`, so its payload is judged separately against the
    /// error type the match produces (#2722).
    err_payloads: Vec<(Ty, Option<ast::Span>)>,
    /// Per arm: where its value is reported (a block body's tail) and the
    /// un-`!`ed call it wraps in `ok(..)` / `some(..)`, if any (#2927).
    blame_spans: Vec<Option<ast::Span>>,
    bangs: Vec<Option<(ast::Span, String)>>,
}

/// The two operands of a time-typed binop as the S3 matrix reads them: each
/// side's canonical type (`lc`/`rc`), its still-unsolved type (`lt`/`rt`), and
/// its clock name (`None` when that side is not a time type).
struct TimeOperands<'a> {
    lc: &'a Ty,
    rc: &'a Ty,
    lt: &'a Ty,
    rt: &'a Ty,
    l: Option<&'static str>,
    r: Option<&'static str>,
}

impl TimeOperands<'_> {
    /// The canonical type of whichever side IS a time — the result type every
    /// error arm reports, so the caller keeps inferring against something real.
    fn time_side(&self) -> &Ty {
        if self.l.is_some() { self.lc } else { self.rc }
    }
}

/// The clock name of a time type (`Compute`, `Duration`, …), or `None` for
/// anything outside the ADR-0001 time family.
fn time_clock_of(t: &Ty) -> Option<&'static str> {
    let Ty::Named(n, args) = t else { return None };
    let is_time = args.is_empty()
        && almide_lang::time_units::TIME_MODULES
            .iter()
            .any(|(_, ty)| *ty == n.as_str());
    is_time.then(|| n.as_str())
}

/// The constructor module for a clock (`Compute` → `compute`), used by the
/// "wrap it" hints.
fn time_module_of(clock_name: &str) -> &'static str {
    almide_lang::time_units::TIME_MODULES
        .iter()
        .find(|(_, ty)| *ty == clock_name)
        .map(|(m, _)| *m)
        .unwrap_or("compute")
}


impl Checker {
    pub(super) fn infer_expr_inner_g2(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        if let Some(ty) = self.infer_expr_g2_literal(expr) { return Some(ty); }
        if let Some(ty) = self.infer_expr_g2_collection(expr) { return Some(ty); }
        None
    }

    /// Scalar literals, interpolated strings, and bare identifiers — the leaf forms
    /// whose type needs no sub-expression.
    ///
    /// One group of the `infer_expr_inner` arm table, arms verbatim and in
    /// source order. `None` means "not my group" — the dispatcher tries the
    /// groups in that order, so the dispatch an expression sees is unchanged.
    pub(super) fn infer_expr_g2_literal(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Int { .. } => Ty::Int,
            ExprKind::Float { .. } => Ty::Float,
            ExprKind::String { .. } => Ty::String,
            ExprKind::InterpolatedString { parts, .. } => {
                for part in parts.iter_mut() {
                    if let ast::StringPart::Expr { expr } = part {
                        let t = self.infer_expr(expr);
                        // #1051: a segment the lowering will NOT auto-? (a
                        // CALL in an effect-fn body takes the `?`; everything
                        // else keeps its value) prints a Result as its debug
                        // form. Queue it for the post-solve warning so
                        // `"${resp}"` never surprises silently.
                        let auto_unwraps =
                            self.env.auto_unwrap && matches!(expr.kind, ExprKind::Call { .. });
                        // Every segment takes the string-form checks (#2496:
                        // a CALL segment in an effect fn skipped them, so
                        // `"${bytes.from_list(xs)}"` reached rustc); the
                        // Result-debug-form warning is the one that yields
                        // to the implicit-propagation check below.
                        self.deferred_result_interp_checks.push(super::InterpSite {
                            ty: t.clone(),
                            span: expr.span,
                            auto_unwrap_call: auto_unwraps,
                            in_fn: self.current_fn.clone(),
                        });
                        if auto_unwraps {
                            // #1123: the segment's Result is stripped implicitly.
                            self.deferred_implicit_prop_checks.push((t.clone(), expr.span, "of this interpolated call", false, false));
                        }
                        // #1115: a segment whose type keeps an undecidable slot
                        // (`"${none}"`, `"${some(none)}"`, `"${ok(none)}"`)
                        // passed check and died at codegen (rustc E0282 or the
                        // AllTypesConcrete gate), while `"${ok(1)}"` silently
                        // concretized E — against the never-silently-defaulted
                        // doctrine. Queue every segment for the post-solve E025
                        // sweep so all four are the SAME check-time error.
                        self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                            ty: t, name: None, span: expr.span,
                        });
                    }
                }
                Ty::String
            }
            ExprKind::Bool { .. } => Ty::Bool,
            ExprKind::Unit => Ty::Unit,

            ExprKind::None => Ty::option(self.fresh_var()),

            ExprKind::Ident { name: _, .. } => self.infer_expr_g2_ident(expr),
            _ => return None,
        })
    }

    /// Collections, indexing, operators, and the branching forms whose type is the
    /// join of their parts.
    ///
    /// One group of the `infer_expr_inner` arm table, arms verbatim and in
    /// source order. `None` means "not my group" — the dispatcher tries the
    /// groups in that order, so the dispatch an expression sees is unchanged.
    pub(super) fn infer_expr_g2_collection(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::List { elements, .. } => {
                if elements.is_empty() {
                    let ty = Ty::list(self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::ListLiteral);
                    ty
                }
                else {
                    // #880: the list's element type is the SIZED peer's width when the
                    // elements mix one in, not element 0's — `[1, u8v]` and `[u8v, 1]`
                    // are the same list, and only the `[u8v, 1]` spelling used to say
                    // so. The peer set is collected alongside the ORIGINAL infer /
                    // constrain order (each element still unifies with element 0 as it
                    // is inferred, since a later element's inference can read a var an
                    // earlier constraint bound); only the RESULT type is the join.
                    let elem_expect = self.list_elem_expect.take();
                    if let Some(t) = &elem_expect {
                        self.expect_lambda(&elements[0], t);
                    }
                    let first = self.infer_expr(&mut elements[0]);
                    let mut peers: Vec<(Ty, Option<ast::Span>, bool)> = vec![
                        (first.clone(), elements[0].span, super::is_literal_numeric_ast(&elements[0])),
                    ];
                    for elem in elements.iter_mut().skip(1) {
                        if let Some(t) = &elem_expect {
                            self.expect_lambda(elem, t);
                        }
                        let et = self.infer_expr(elem);
                        peers.push((et.clone(), elem.span, super::is_literal_numeric_ast(elem)));
                        self.constrain(first.clone(), et, "list element");
                    }
                    let joined = self.join_sized_peers(&peers, "list element").unwrap_or(first);
                    // The joined width is also the RANGE context for every bare
                    // literal element (`[300, u8v]` is out of range, not a
                    // wrap) — the same pinning an ANNOTATED element type does,
                    // via the same helper. Without it the join would stamp
                    // `300u8` at lowering and leave the diagnostic to rustc,
                    // which is the acceptance gap this issue is about (#880).
                    if super::is_narrow_sized(&joined) {
                        for elem in elements.iter() {
                            self.record_int_literal_context(elem, &joined);
                        }
                    }
                    Ty::list(joined)
                }
            }

            ExprKind::Tuple { elements, .. } => Ty::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),
            ExprKind::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                // #3358: an updated field takes the base record's DECLARED field
                // type, as the same field of a record literal does
                // (`constrain_record_fields`). Without this the value was
                // inferred bare: `{ ...r, rows: [] }` was E018 though `rows` is
                // `List[Int]`, and `{ ...r, rows: ["x"] }` passed check. Only a
                // base whose record shape is already known pins its fields; an
                // unresolved base is left as before.
                let decl = match self.env.resolve_named(&resolve_ty(&base_ty, &self.uf)) {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => fields,
                    _ => Vec::new(),
                };
                for f in fields.iter() {
                    if let Some((_, ety)) = decl.iter().find(|(n, _)| n.as_str() == f.name.as_str()) {
                        self.record_int_literal_context(&f.value, ety);
                        if let Some(vty) = self.type_map.get(&f.value.id).cloned() {
                            self.constrain(ety.clone(), vty, format!("field {}", f.name));
                        }
                    }
                }
                base_ty
            }
            ExprKind::IndexAccess { object: _, index: _, .. } => self.infer_expr_g2_index_access(expr),
            ExprKind::Binary { op: _, left: _, right: _, .. } => self.infer_expr_g2_binary(expr),

            ExprKind::Unary { op: _, operand: _, .. } => self.infer_expr_g2_unary(expr),

            ExprKind::If { cond: _, then: _, else_: _, .. } => self.infer_expr_g2_if(expr),

            ExprKind::IfLet { name: _, scrutinee: _, then: _, else_: _ } => self.infer_expr_g2_if_let(expr),

            ExprKind::Match { subject: _, arms: _, .. } => self.infer_expr_g2_match(expr),
            _ => return None,
        })
    }
}


impl Checker {
    fn infer_expr_g2_match(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Match { subject, arms, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_match called on the wrong ExprKind") };
        let expect = self.expr_expect.clone();
        let subject_ty = self.infer_expr(subject);
        let sc = resolve_ty(&subject_ty, &self.uf);
        self.queue_match_implicit_prop(subject, &subject_ty, arms);
        self.check_match_exhaustiveness(&sc, arms);
        let mut inferred = self.infer_match_arms(&subject_ty, arms, expect.as_ref());
        let err_payloads = std::mem::take(&mut inferred.err_payloads);
        let joined = self.join_match_arms(inferred, expect.as_ref());
        self.check_err_arm_payloads(&joined, err_payloads);
        joined
    }

    /// #2722: an `err(..)` arm joins as `Never`, so nothing checked its payload
    /// against the error type the match produces. `match r { ok(v) => ok(v),
    /// err(e) => err(e) }` with `e: String` in a `-> T!E` fn passed check and
    /// died in rustc (E0308) — while the same two ctors as `if` branches were
    /// already E001. The payload flows into the joined `Result`'s error slot;
    /// when the arms join to a plain value, the `err(..)` arm returns from the
    /// fn, so it flows into the fn's own error channel.
    ///
    /// The constraint is DEFERRED (solved with the rest, not unified here): the
    /// joined slot is often still open (#2599 leaves `ok(v)`'s error slot
    /// fresh), and the payload must not pin it before the fn's declared return
    /// does — the arm then reports at itself, not at the fn.
    fn check_err_arm_payloads(&mut self, joined: &Ty, err_payloads: Vec<(Ty, Option<ast::Span>)>) {
        if err_payloads.is_empty() {
            return;
        }
        let target = match resolve_ty(joined, &self.uf) {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => Some(args),
            Ty::Never | Ty::Unknown | Ty::TypeVar(_) => None,
            // ADR-0021: inside a lambda a value-join `err(..)` arm returns into
            // the lambda's own channel — its error type joins ε.
            _ if self.env.lambda_depth > 0 => {
                for (payload, span) in &err_payloads {
                    self.record_lambda_returned_err(&Ty::result(Ty::Unit, payload.clone()), false, *span);
                }
                None
            }
            value => self.bang_channel_err_ty().map(|e| vec![value, e]),
        };
        let Some(target) = target else { return };
        for (payload, span) in err_payloads {
            let fix_hint = self.erased_callback_behind(&payload);
            self.constraints.push(super::types::Constraint {
                expected: Ty::result(target[0].clone(), target[1].clone()),
                actual: Ty::result(target[0].clone(), payload),
                context: "match arm".into(),
                span: span.or(self.current_span),
                fix_hint,
            });
        }
    }

    /// The `err(..)` arm carries `String`, and a callback in this fn erased a
    /// typed error into its `String` channel with a `!` — the value-consumed
    /// form of #2601 (`let r = xs |> list.map((x) => ... f(x)! ...)` then
    /// `match r { ..., err(e) => err(e) }`). Carried to the report, which names
    /// the callback's `!` when the arm's error slot really is that typed error.
    fn erased_callback_behind(&self, payload: &Ty) -> Option<super::types::FixHint> {
        if resolve_ty(payload, &self.uf) != Ty::String {
            return None;
        }
        let here = self.current_fn.as_ref().map(|f| f.0);
        self.lambda_err_erasures
            .iter()
            .find(|(_, _, owner)| *owner == here)
            .map(|(erased, at, _)| super::types::FixHint::ErrArmErased { erased: erased.clone(), at: *at })
    }

    /// #1123: a match over an effect call whose arms are VALUE patterns takes
    /// the implicit strip (ok/err-pattern arms keep the Result). Queue for the
    /// E041 deprecation.
    fn queue_match_implicit_prop(
        &mut self,
        subject: &ast::Expr,
        subject_ty: &Ty,
        arms: &[ast::MatchArm],
    ) {
        let value_patterns_only = !arms
            .iter()
            .any(|a| matches!(a.pattern, ast::Pattern::Ok { .. } | ast::Pattern::Err { .. }));
        if self.env.auto_unwrap
            && matches!(subject.kind, ExprKind::Call { .. })
            && value_patterns_only
        {
            self.deferred_implicit_prop_checks.push((
                subject_ty.clone(), subject.span, "of this match subject", true, false,
            ));
        }
    }

    /// Infer every arm in its own scope, with the subject's pattern bindings
    /// visible to that arm's guard and body.
    fn infer_match_arms(
        &mut self,
        subject_ty: &Ty,
        arms: &mut [ast::MatchArm],
        expect: Option<&super::types::TailExpect>,
    ) -> MatchArmTypes {
        // If ANY arm is an explicit `ok(..)`/`err(..)` ctor, this match PRODUCES a Result (it
        // re-wraps — base64 decode's `match bs { ok(b) => ok(string.from_bytes(b)), err(e) =>
        // err(e) }`), so NO arm is auto-unwrapped: every arm keeps its Result type and the
        // match types as Result, not its OK type. (Auto-unwrapping only the effect-call arms
        // while a ctor arm stayed Result mismatched — `Result[(String,Int),String]` vs
        // `(String,Int)` in toml parse_key_part; mistyping the whole match as the OK type
        // walled the v1 MIR / mis-rewrapped native — base64 decode.) The pure auto-unwrap case
        // (no ctor arm, just effect-call/value arms unifying to T) is unchanged.
        let arms_have_result_ctor = arms
            .iter()
            .any(|a| matches!(&a.body.kind, ExprKind::Ok { .. } | ExprKind::Err { .. }));
        let mut out = MatchArmTypes::default();
        for arm in arms.iter_mut() {
            self.env.push_scope();
            let sub_c = resolve_ty(subject_ty, &self.uf);
            self.bind_pattern(&arm.pattern, &sub_c);
            if let Some(ref mut guard) = arm.guard {
                // A guard is a CONDITION: constrain it to Bool. Uninferred,
                // `p if p + 1` passed check and native ran the Int as truthy
                // (diagnostic sweep 2026-08-18) — the same discipline as an
                // `if` condition, at the same rank.
                let gty = self.infer_expr(guard);
                self.constrain(crate::types::Ty::Bool, gty, "match guard");
            }
            self.tail_expect = expect.cloned();
            let arm_ty = self.infer_expr(&mut arm.body);
            out.real_types.push(arm_ty.clone());
            out.blame_spans.push(super::arm_blame::value_leaf_span(&arm.body));
            out.bangs.push(self.wrapped_unbanged_call(&arm.body));
            if matches!(&arm.body.kind, ExprKind::Err { .. }) {
                if let Some((_, payload)) = resolve_ty(&arm_ty, &self.uf).inner2() {
                    out.err_payloads.push((payload.clone(), arm.body.span));
                }
            }
            let arm_ty = self.match_arm_join_ty(arm, arm_ty, arms_have_result_ctor);
            out.peers.push((arm_ty.clone(), arm.body.span, super::is_literal_numeric_ast(&arm.body)));
            out.types.push(arm_ty);
            self.env.pop_scope();
        }
        out
    }

    /// The type an arm contributes to the JOIN, which is not always the type it
    /// infers to: `err()` in a match arm is an early return, so it joins as
    /// `Never` and does not constrain its siblings. In effect fn bodies an arm's
    /// `Result[T, E]` auto-unwraps to `T` so arms mixing effect calls with pure
    /// expressions unify — skipped when an arm is an explicit ok/err ctor, since
    /// then the match re-wraps and ALL arms keep the Result.
    fn match_arm_join_ty(&mut self, arm: &ast::MatchArm, arm_ty: Ty, has_result_ctor: bool) -> Ty {
        if matches!(&arm.body.kind, ExprKind::Err { .. }) {
            return Ty::Never;
        }
        if !self.env.auto_unwrap || has_result_ctor {
            return arm_ty;
        }
        match resolve_ty(&arm_ty, &self.uf) {
            Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => {
                // #2182: the strip is RECOVERY for the join; the arm's value
                // is implicit propagation and is reported (E041 here — a
                // statement-position or `-> Unit`-tail match upgrades it to
                // the must-use E042 through the same leaf span).
                self.queue_implicit_prop_leaves(&arm.body, "of this match arm's value", false);
                args[0].clone()
            }
            _ => arm_ty,
        }
    }

    /// Unify the arm types with each other (not with a shared result var that
    /// external constraints could contaminate) and pick the match's own type.
    ///
    /// #2927: the arm the others are compared against is the first one the
    /// tail expectation accepts (see `arm_blame.rs`), so a wrong FIRST arm is
    /// the one reported — at its own span, not wherever inference ended.
    fn join_match_arms(&mut self, inferred: MatchArmTypes, expect: Option<&super::types::TailExpect>) -> Ty {
        let MatchArmTypes { types, real_types, peers, blame_spans, bangs, .. } = inferred;
        if types.is_empty() { return Ty::Unit };
        // #3385: in a lifting tail each arm lifts into `ok(..)` on its own,
        // so a value arm and an explicit `ok(..)` arm join at the lifted
        // level — the same rule the `if` branches follow.
        let (types, peers) = match self.lift_mixed_tail_peers(expect, &types) {
            Some(lifted) => {
                let peers = peers.into_iter().zip(&lifted).map(|((_, s, lit), t)| (t.clone(), s, lit)).collect();
                (lifted, peers)
            }
            None => (types, peers),
        };
        let (anchor, declared) = self.pick_join_anchor(expect, &types);
        let first = types[anchor].clone();
        for (i, aty) in types.iter().enumerate() {
            if i == anchor { continue; }
            let hint = FixHint::ArmBlame { anchor: blame_spans[anchor], declared: declared.clone(), bang: bangs[i].clone(), real: real_types[i].clone() };
            self.constrain_peer((&first, blame_spans[anchor]), (aty, blame_spans[i]), "match arm", Some(hint));
        }
        // #880: a sized arm wins the join over canonical peers, the same rule
        // the `if` arms and list elements follow. Checked before the `Never`
        // recovery below because a numeric-scalar join and a `Never`/Result arm
        // set are disjoint cases.
        if let Some(joined) = self.join_sized_peers(&peers, "match arm") {
            return joined;
        }
        if !matches!(first, Ty::Never) {
            return first;
        }
        // The overall match type is the first non-`Never` arm type. `Never`
        // arms (every `err(..)` arm) carry no useful result type but they DO
        // produce a Result value, so when they are the only arms we recover the
        // concrete type from the real (un-substituted) arm types — preferring an
        // `err` arm's `Result[T, E]` so the match types as Result, never `Never`.
        types
            .iter()
            .find(|t| !matches!(t, Ty::Never))
            .cloned()
            .or_else(|| {
                real_types
                    .iter()
                    .find(|t| !matches!(resolve_ty(t, &self.uf), Ty::Never))
                    .cloned()
            })
            .unwrap_or(first)
    }

    fn infer_expr_g2_if_let(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::IfLet { name, scrutinee, then, else_ } = &mut expr.kind else { unreachable!("infer_expr_g2_if_let called on the wrong ExprKind") };
                // Swift-style implicit unwrap: `name` binds the value INSIDE the
                // scrutinee's Option[T] / Result[T, E] (the T). Lowering desugars this
                // to a `match` on Some/Ok once the scrutinee type is known; the checker
                // only INFERS (no rewrite — desugar belongs in lowering).
                let scrut_ty = self.infer_expr(scrutinee);
                let resolved = resolve_ty(&scrut_ty, &self.uf);
                let bound_ty = match &resolved {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
                        args[0].clone()
                    }
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
                        args[0].clone()
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.emit(super::err(
                            format!("`if let` requires an Option or Result, found `{}`", other.display()),
                            "bind the inner value of an Option/Result: `if let v = some_option { … } else { … }`".to_string(),
                            "if let scrutinee".to_string(),
                        ).with_code("E001"));
                        Ty::Unknown
                    }
                };
                self.env.push_scope();
                self.env.define_var(name, bound_ty);
                let then_ty = self.infer_expr(then);
                self.env.pop_scope();
                let else_ty = self.infer_expr(else_);
                self.constrain_with_hint(then_ty.clone(), else_ty.clone(), "if let branches", None);
                // #2769: a diverging `then` arm types the if as its `else` arm.
                if matches!(resolve_ty(&then_ty, &self.uf), Ty::Never) {
                    return else_ty;
                }
                then_ty
    }

    fn infer_expr_g2_if(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::If { cond, then, else_, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_if called on the wrong ExprKind") };
                let expect = self.expr_expect.clone();
                let cond_ty = self.infer_expr(cond);
                self.constrain_condition(cond, cond_ty, "if");
                self.tail_expect = expect.clone();
                let then_ty = self.infer_expr(then);
                self.tail_expect = expect.clone();
                let else_ty = self.infer_expr(else_);
                // In effect fn bodies, auto-unwrap Result[T, E] → T per
                // branch before unifying them, mirroring the match-arm rule
                // (see ExprKind::Match above). Without this, an `if` whose
                // one branch is a `match` on an effect-fn call (auto-unwrapped
                // to T) and whose other branch is an explicit `ok(...)`
                // (stays Result[T, E]) fails E001 — the asymmetry is a
                // checker artefact, not a real type error: codegen's
                // wrap_tail_in_ok normalizes both to Result form. Scoped to
                // `auto_unwrap`, so pure-fn / test if/else are untouched.
                // Auto-unwrap Result[T, E] → T on BOTH branches for the
                // cross-branch COMPARISON only, then return the THEN branch's
                // real (non-unwrapped) type as the if-expression's type.
                //
                // Two requirements pull in opposite directions and this split
                // satisfies both:
                //   • M1 (E001): an `if` whose one branch is a `match` on an
                //     effect-fn call (auto-unwrapped to `T` inside the match)
                //     and whose other branch is an explicit `ok(...)`
                //     (`Result[T, E]`) must not error. Comparing both at the
                //     unwrapped `T` level removes the spurious asymmetry.
                //   • No-regress (`validate_positive`: `if .. then ok(n) else
                //     err(..)`): the if's TYPE must stay `Result[T, E]` so the
                //     WASM emitter sees the real value shape (the branches are
                //     genuine Result constructors). Returning the un-unwrapped
                //     `then_ty` preserves this; codegen's wrap_tail_in_ok then
                //     normalizes every branch to Result form regardless.
                // Scoped to `auto_unwrap`, so pure-fn / test if/else are
                // untouched (they keep the strict same-type rule).
                let cmp_unwrap = |t: &Ty, uf: &_| -> Ty {
                    match resolve_ty(t, uf) {
                        Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
                        _ => t.clone(),
                    }
                };
                let lifted = self.lift_mixed_tail_peers(expect.as_ref(), &[then_ty.clone(), else_ty.clone()]);
                let (cmp_then, cmp_else) = if let Some(lifted) = lifted {
                    // #3385 / #3395: a lifting tail (`-> T!`, or an effect fn
                    // declaring `-> Result[..]`) lifts each branch on its own —
                    // the same rule the match arms follow.
                    (lifted[0].clone(), lifted[1].clone())
                } else if self.env.auto_unwrap {
                    // #2182: a branch whose Result the comparison strips is
                    // implicit propagation — report it at the branch's tail
                    // leaves (the `else` side never reached any report: the
                    // `if` types as its `then` arm, so no consumer saw the
                    // Result). The strip itself stays, as recovery.
                    for branch in [&**then, &**else_] {
                        if resolve_ty(&self.type_map.get(&branch.id).cloned().unwrap_or(Ty::Unknown), &self.uf).is_result() {
                            self.queue_implicit_prop_leaves(branch, "of this if branch's value", false);
                        }
                    }
                    (cmp_unwrap(&then_ty, &self.uf), cmp_unwrap(&else_ty, &self.uf))
                } else {
                    (then_ty.clone(), else_ty.clone())
                };
                // Specialize the Unit-leak `try:` snippet: if an arm is a
                // bare assignment `x = ...` (returns Unit), we want to cite
                // the actual variable name in the suggested rewrite.
                let hint = if_arm_fix_hint(then, else_);
                // #880: the two arms are PEERS, but the if's type was the THEN
                // arm's — so `if b then 1 else u8v` typed `Int` and emitted an
                // i64 `if` whose else arm is a `u8`. The sized arm wins the join
                // (a canonical arm may only be a literal); everything else keeps
                // the then-arm rule, including the Result shape the auto-unwrap
                // comment above depends on.
                let peers = [
                    (cmp_then.clone(), then.span, super::is_literal_numeric_ast(then)),
                    (cmp_else.clone(), else_.span, super::is_literal_numeric_ast(else_)),
                ];
                let joined = self.join_sized_peers(&peers, "if branches");
                // #2927: the `then` branch fixes the if's type unless the tail
                // expectation rejects it and accepts the `else` — then the
                // `then` branch is the one reported, and the if types as `else`.
                let (anchor, declared) = self.pick_join_anchor(expect.as_ref(), &[then_ty.clone(), else_ty.clone()]);
                let then_at = super::arm_blame::value_leaf_span(then);
                let else_at = super::arm_blame::value_leaf_span(else_);
                let swapped = anchor == 1;
                let (anchor_peer, blamed_peer, blamed_expr, blamed_real) = if swapped {
                    ((&cmp_else, else_at), (&cmp_then, then_at), &**then, &then_ty)
                } else {
                    ((&cmp_then, then_at), (&cmp_else, else_at), &**else_, &else_ty)
                };
                let hint = hint.or_else(|| Some(FixHint::ArmBlame {
                    anchor: anchor_peer.1,
                    declared,
                    bang: self.wrapped_unbanged_call(blamed_expr),
                    real: blamed_real.clone(),
                }));
                self.constrain_peer(anchor_peer, blamed_peer, "if branches", hint);
                if swapped {
                    return else_ty;
                }
                // The join only replaces the then-arm rule when the then arm is
                // ITSELF a bare numeric scalar. `cmp_then` may be an auto-unwrapped
                // `Result[T, E]`, and the paragraph above requires the if's type to
                // keep that wrapper for the wasm emitter — handing back the
                // unwrapped width there would retype the whole expression.
                // #2769: a diverging `then` arm (`panic(..)`, `Never`) produces no
                // value, so the if types as its `else` arm — the rule the match
                // join already follows (the first non-`Never` arm). Typing the
                // if `Never` made the value-producing `else` arm unreachable to
                // every lowering: MIR dropped it and returned nothing.
                if matches!(resolve_ty(&then_ty, &self.uf), Ty::Never) {
                    return else_ty;
                }
                match joined {
                    Some(t) if super::solving::is_numeric_scalar(&resolve_ty(&then_ty, &self.uf)) => t,
                    _ => then_ty,
                }
    }

}

include!("infer_operators.rs");

impl Checker {

    fn infer_expr_g2_index_access(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::IndexAccess { object, index, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_index_access called on the wrong ExprKind") };
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let is_range = matches!(&index.kind, ExprKind::Range { .. });
                let concrete = resolve_ty(&obj_ty, &self.uf);
                if is_range {
                    concrete
                } else {
                    match &concrete {
                        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::option(args[1].clone()),
                        Ty::Bytes => Ty::Int,
                        Ty::String => {
                            self.emit(super::err(
                                "cannot index a String with `[]`",
                                "a String is a UTF-8 codepoint sequence, not an array — use `string.get(s, i)` (returns `Option[String]`) or `string.char_at(s, i)`",
                                "string index",
                            ).with_code("E026"));
                            Ty::Unknown
                        }
                        _ => Ty::Unknown,
                    }
                }
    }

    fn infer_expr_g2_ident(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Ident { name, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_ident called on the wrong ExprKind") };
                self.env.used_vars.insert(sym(name));
                // NOTE: no let-polymorphism here — the old `instantiate_ty`
                // never freshened anything (its mapping was never written), so
                // it was an identity deep copy of the already-cloned type.
                if let Some(ty) = self.env.lookup_var(name).cloned() { ty }
                else if let Some(ty) = self.selective_top_let_ty(name) { ty }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { ty }
                // Const param: `N: Int` in generic params resolves to its underlying type
                else if let Some(Ty::ConstParam { ty, .. }) = self.env.types.get(&sym(name)).cloned() {
                    *ty
                }
                else if let Some(sig) = self.env.functions.get(&sym(name)).cloned() {
                    let callee = self.purity_callee(name);
                    self.record_purity_ref(callee, expr.span);
                    self.fn_value_ty(&sig)
                }
                else {
                    self.report_undefined_variable(name)
                }
    }

    /// The Ty a named fn carries when referenced as a VALUE (#1055, #1148):
    /// the carrier baked into `ret` — an effect fn's value is a
    /// `(A) -> Result[B, String]` closure — and `is_effect: false`, because
    /// the effect BIT belongs to declared slot types only (there `ret` is the
    /// unwrapped B). `FnSig.ret` stores the DECLARED B (registration.rs does
    /// not wrap), so the bake happens here, mirroring
    /// `finalize_call_return_ty`'s call-site wrap.
    pub(crate) fn fn_value_ty(&self, sig: &crate::types::FnSig) -> Ty {
        let ret = if sig.is_effect && !sig.ret.is_result() {
            Ty::result(sig.ret.clone(), Ty::String)
        } else {
            sig.ret.clone()
        };
        Ty::Fn {
            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
            ret: Box::new(ret),
            is_effect: false,
        }
    }


    /// Emit the E003 for an identifier that resolves to nothing, and return the
    /// recovery type.
    ///
    /// The hint is one of three, in order of how likely it is to be the actual
    /// fix: a missing `import` for a module that needs one, a fuzzy match against
    /// every visible name, or nothing better than "check the name". Only the
    /// first two carry a `try_replace`, because only they name a concrete edit.
    fn report_undefined_variable(&mut self, name: &str) -> Ty {
                // Only suggest `import` for modules that require explicit import
                // and whose names won't be confused with common variable names.
                // e.g. `value`, `error`, `string`, `list` are too common as
                // variable names — suggesting `import value` is misleading.
                let (hint, fix): (String, Option<String>) = if crate::stdlib::is_import_suggestable(name) {
                    let desc = crate::stdlib::module_description(name);
                    (format!("Add `import {}` (stdlib: {})\nOr run `almide fmt` to auto-add missing imports", name, desc),
                     Some(format!("import {}", name)))
                } else {
                    let candidates = self.env.all_visible_names();
                    if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                        (format!("Did you mean `{}`?", suggestion), Some(suggestion.to_string()))
                    } else {
                        ("Check the variable name".to_string(), None)
                    }
                };
                let mut diag = super::err(format!("undefined variable '{}'", name), hint, format!("variable {}", name)).with_code("E003");
                if let Some(fix) = fix {
                    if let Some(stripped) = fix.strip_prefix("import ") {
                        // Zero-width insert at the top of file — the
                        // new `import <module>\n` line is prepended.
                        // `apply_try_to` handles `end_col == col` as
                        // an insertion point.
                        //
                        // SUGGESTION, not machine-applicable (#1312):
                        // 1:1 is a PLACEMENT HEURISTIC, not this
                        // diagnostic's own span — the import block's real
                        // position depends on what is already there.
                        // `almide fix` routes import insertion through
                        // `auto_imports`, which owns that block.
                        diag = diag.with_suggested_fix(
                            1, 1, 1,
                            format!("import {}\n", stripped),
                        );
                    } else if let Some(span) = self.current_span {
                        // Typo fuzzy suggestion: replace the
                        // offending identifier with the suggested name.
                        // SUGGESTION: an edit distance picked the name.
                        diag = diag.with_suggested_fix(
                            span.line, span.col, span.end_col,
                            fix,
                        );
                    } else {
                        diag = diag.with_try(format!("// {}  →  {}\n{}", name, fix, fix));
                    }
                }
                self.emit(diag);
                Ty::Unknown
    }

}

/// #1116: true when both sides are the SAME outer type constructor
/// (`Option`/`Result`/`List`/`Map`/`Set`/named) applied to DIFFERENT, fully
/// concrete arguments — `Result[Int, String]` vs `Result[Int, Int]`. Rigid
/// generics and unresolved slots (any `TypeVar`/`Unknown`) exclude the pair:
/// they may still unify, and the undecidable case is the E025 sweep's job.
fn same_head_applied_mismatch(lc: &Ty, rc: &Ty) -> bool {
    fn fully_concrete(t: &Ty) -> bool {
        let hit = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
        !hit(t) && !t.any_child_recursive(&hit)
    }
    match (lc, rc) {
        (Ty::Applied(c1, _), Ty::Applied(c2, _)) =>
            c1 == c2 && lc != rc && fully_concrete(lc) && fully_concrete(rc),
        _ => false,
    }
}
