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
                        if !auto_unwraps {
                            self.deferred_result_interp_checks.push((t.clone(), expr.span));
                        } else {
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

            ExprKind::Ident { name, .. } => self.infer_expr_g2_ident(expr),
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
                    let first = self.infer_expr(&mut elements[0]);
                    let mut peers: Vec<(Ty, Option<ast::Span>, bool)> = vec![
                        (first.clone(), elements[0].span, super::is_literal_numeric_ast(&elements[0])),
                    ];
                    for elem in elements.iter_mut().skip(1) {
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
                base_ty
            }
            ExprKind::IndexAccess { object, index, .. } => self.infer_expr_g2_index_access(expr),
            ExprKind::Binary { op, left, right, .. } => self.infer_expr_g2_binary(expr),

            ExprKind::Unary { op, operand, .. } => self.infer_expr_g2_unary(expr),

            ExprKind::If { cond, then, else_, .. } => self.infer_expr_g2_if(expr),

            ExprKind::IfLet { name, scrutinee, then, else_ } => self.infer_expr_g2_if_let(expr),

            ExprKind::Match { subject, arms, .. } => self.infer_expr_g2_match(expr),
            _ => return None,
        })
    }
}


impl Checker {
    fn infer_expr_g2_match(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Match { subject, arms, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_match called on the wrong ExprKind") };
        let subject_ty = self.infer_expr(subject);
        let sc = resolve_ty(&subject_ty, &self.uf);
        self.queue_match_implicit_prop(subject, &subject_ty, arms);
        self.check_match_exhaustiveness(&sc, arms);
        let inferred = self.infer_match_arms(&subject_ty, arms);
        self.join_match_arms(inferred)
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
    fn infer_match_arms(&mut self, subject_ty: &Ty, arms: &mut [ast::MatchArm]) -> MatchArmTypes {
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
            if let Some(ref mut guard) = arm.guard { self.infer_expr(guard); }
            let arm_ty = self.infer_expr(&mut arm.body);
            out.real_types.push(arm_ty.clone());
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
            Ty::Applied(TypeConstructorId::Result, ref args) if args.len() == 2 => args[0].clone(),
            _ => arm_ty,
        }
    }

    /// Unify the arm types with each other (not with a shared result var that
    /// external constraints could contaminate) and pick the match's own type.
    fn join_match_arms(&mut self, inferred: MatchArmTypes) -> Ty {
        let MatchArmTypes { types, real_types, peers } = inferred;
        let Some(first) = types.first().cloned() else { return Ty::Unit };
        for aty in &types[1..] {
            self.constrain(first.clone(), aty.clone(), "match arm");
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
                self.constrain_with_hint(then_ty.clone(), else_ty, "if let branches", None);
                then_ty
    }

    fn infer_expr_g2_if(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::If { cond, then, else_, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_if called on the wrong ExprKind") };
                let cond_ty = self.infer_expr(cond);
                self.constrain_condition(cond, cond_ty, "if");
                let then_ty = self.infer_expr(then);
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
                let (cmp_then, cmp_else) = if self.env.auto_unwrap {
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
                self.constrain_with_hint(cmp_then, cmp_else, "if branches", hint);
                // The join only replaces the then-arm rule when the then arm is
                // ITSELF a bare numeric scalar. `cmp_then` may be an auto-unwrapped
                // `Result[T, E]`, and the paragraph above requires the if's type to
                // keep that wrapper for the wasm emitter — handing back the
                // unwrapped width there would retype the whole expression.
                match joined {
                    Some(t) if super::solving::is_numeric_scalar(&resolve_ty(&then_ty, &self.uf)) => t,
                    _ => then_ty,
                }
    }

    fn infer_expr_g2_unary(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Unary { op, operand, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_unary called on the wrong ExprKind") };
                // #626: `-<int literal>` lets the negation reach i64::MIN, whose
                // magnitude (2^63) overflows a bare positive literal but is a
                // valid i64. Mark the candidate (registered while inferring the
                // operand) so its post-solve range check uses the signed MIN bound.
                //
                // The sign recorded is the NET sign of this node's WHOLE operand
                // chain, not "this node is a minus": `--300` is +300 and
                // `--9223372036854775808` is +2^63, which no signed type holds.
                // Each `Unary` on the way up writes the parity of its own subtree
                // and the operand is inferred first, so the OUTERMOST minus writes
                // last and its answer — the one the source actually states — wins.
                let chain = (op.as_str() == "-")
                    .then(|| super::int_literal_chain(operand))
                    .flatten()
                    .map(|(lit_id, _, inner_negated)| (lit_id, !inner_negated));
                let t = self.infer_expr(operand);
                if let Some((lit_id, negated)) = chain {
                    if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == lit_id) {
                        site.negated = negated;
                    }
                }
                match op.as_str() { "not" => Ty::Bool, _ => t }
    }

    fn infer_expr_g2_binary(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Binary { op, left, right, .. } = &mut expr.kind else { unreachable!("infer_expr_g2_binary called on the wrong ExprKind") };
        let lt = self.infer_expr(left);
        let rt = self.infer_expr(right);
        // #1050: operand-position implicit unwrap. In an effect-fn body the
        // checker strips Result from a CALL in binding position and the
        // lowering auto-?'s it; `insert_auto_try` wraps every Result-typed
        // call, operand position included — so the checker admitting
        // `helper() + 1` is the same one rule as `let x = helper()`. VARs are
        // untouched: a var's unwrap is decided at its binding, and a
        // Result-typed var operand stays a type error (whose hint names the
        // unwrap operators). This also closes an acceptance-parity hole:
        // `helper() == ok(0)` used to pass check and explode in the generated
        // Rust (auto-? unwrapped the left side under a Result comparand);
        // it is now an honest check-time mismatch.
        let lt = self.operand_effect_unwrap(left, lt);
        let rt = self.operand_effect_unwrap(right, rt);
        self.pin_binop_literal_context(op, left, right, &lt, &rt);
        // ADR-0001 S3: the time-type operator matrix intercepts BEFORE the
        // generic paths — `Named` types pass the generic numeric check (the
        // GPU-vector allowance), which would silently admit `T * T`.
        {
            let lc0 = resolve_ty(&lt, &self.uf);
            let rc0 = resolve_ty(&rt, &self.uf);
            if let Some(t) = self.infer_time_binop(op.as_str(), &lc0, &rc0, &lt, &rt) {
                return t;
            }
        }
        match op.as_str() {
            "+" => {
                let lc = resolve_ty(&lt, &self.uf);
                let rc = resolve_ty(&rt, &self.uf);
                self.infer_plus_op(&lc, &rc, lt, left, right)
            }
            "-" | "*" | "/" | "%" | "^" => self.infer_binop_arith(op, &lt, &rt, left, right),
            "++" => {
                self.emit(super::err(
                    format!("operator '++' has been removed. Use '+' for concatenation"),
                    "Replace ++ with +", "operator ++"));
                lt
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => self.infer_binop_compare(op, left, right, &lt, &rt),
            "and" | "or" => self.infer_binop_logical(op, &lt, &rt),
            _ => lt,
        }
    }

    /// The #1050 operand strip: `expr` is a binary operand; when it is a CALL
    /// whose type resolved to `Result[T, E]` inside an auto-unwrap context
    /// (an effect-fn body outside lambdas), give the operator `T` — the
    /// lowering's `insert_auto_try` wraps exactly this shape in a `?`.
    /// Anything else (vars, ctors, non-effect contexts) passes through.
    fn operand_effect_unwrap(&mut self, operand: &ast::Expr, t: Ty) -> Ty {
        if !self.env.auto_unwrap || !matches!(operand.kind, ExprKind::Call { .. }) {
            return t;
        }
        let resolved = resolve_ty(&t, &self.uf);
        resolved.result_ok_ty().unwrap_or(t)
    }

    /// ADR-0001 S3: the time-type operator matrix. `None` = no time operand
    /// (or an op outside the matrix) — fall through to the generic paths.
    /// Same-clock algebra: `T + T`, `T - T` (0-saturating), `T * Int` /
    /// `Int * T`, and comparisons. Everything else on a time type is a NAMED
    /// error: `T * T` (the Go #64420 silent-10⁹ class), clock mixing, bare
    /// Int join, and the intentionally omitted `/` (S7).
    fn infer_time_binop(&mut self, op: &str, lc: &Ty, rc: &Ty, lt: &Ty, rt: &Ty) -> Option<Ty> {
        let sides = TimeOperands {
            lc,
            rc,
            lt,
            rt,
            l: time_clock_of(lc),
            r: time_clock_of(rc),
        };
        if sides.l.is_none() && sides.r.is_none() {
            return None;
        }
        match op {
            "/" | "%" | "^" => Some(self.time_binop_undefined(op, &sides)),
            "*" => Some(self.time_binop_scale(&sides)),
            "+" | "-" => Some(self.time_binop_additive(op, &sides)),
            "==" | "!=" | "<" | ">" | "<=" | ">=" => Some(self.time_binop_compare(op, &sides)),
            _ => None,
        }
    }

    /// `/`, `%`, `^` on a time type: never defined. `/` is intentionally
    /// omitted (ADR-0001 S7); the others were never in the matrix.
    fn time_binop_undefined(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        let hint = if op == "/" {
            "Division is intentionally omitted (ADR-0001 S7) — divide the Int \
             before constructing, or scale with `*`"
        } else {
            "The time algebra is `T + T`, `T - T` (0-saturating), `T * Int`, \
             and comparisons — nothing else"
        };
        self.emit(super::err(
            format!("operator '{op}' is not defined on time types"),
            hint,
            format!("operator {op}")));
        sides.time_side().clone()
    }

    /// `T * Int` / `Int * T` scales; `T * T` would be time², which has no
    /// meaning.
    fn time_binop_scale(&mut self, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(_), Some(_)) => {
                self.emit(super::err(
                    "cannot multiply two time quantities".to_string(),
                    "time × time has no meaning (the result would be time²) — scale \
                     with an Int: `t * 3`",
                    "operator *".to_string()));
                sides.lc.clone()
            }
            (Some(_), None) => {
                self.constrain(sides.rt.clone(), Ty::Int, "time scale factor");
                sides.lc.clone()
            }
            _ => {
                self.constrain(sides.lt.clone(), Ty::Int, "time scale factor");
                sides.rc.clone()
            }
        }
    }

    /// `T + T` / `T - T` — same clock only. A bare Int operand is a NAMED
    /// error, never an implicit join.
    fn time_binop_additive(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(a), Some(b)) => {
                if a != b {
                    self.emit_clock_mix(op, if op == "+" { "add" } else { "subtract" });
                }
                sides.lc.clone()
            }
            (Some(a), None) | (None, Some(a)) => {
                self.emit(super::err(
                    format!(
                        "operator '{op}' needs two {a} values, found {} and {}",
                        sides.lc.display(),
                        sides.rc.display()
                    ),
                    format!(
                        "A bare number is never a time — wrap it: {}.ms(n)",
                        time_module_of(a)
                    ),
                    format!("operator {op}")));
                sides.time_side().clone()
            }
            (None, None) => sides.lc.clone(),
        }
    }

    /// Comparisons — same clock only, and never against a bare Int.
    fn time_binop_compare(&mut self, op: &str, sides: &TimeOperands) -> Ty {
        match (sides.l, sides.r) {
            (Some(a), Some(b)) => {
                if a != b {
                    self.emit_clock_mix(op, "compare");
                }
            }
            (Some(a), None) | (None, Some(a)) => {
                self.emit(super::err(
                    format!(
                        "cannot compare {} with {} — both sides must be {a}",
                        sides.lc.display(),
                        sides.rc.display()
                    ),
                    format!(
                        "A bare number is never a time — wrap it: {}.ms(n)",
                        time_module_of(a)
                    ),
                    format!("operator {op}")));
            }
            (None, None) => {}
        }
        Ty::Bool
    }

    /// The two clocks have no bridge (ADR-0001): a deterministic budget is
    /// Compute, a wall-clock deadline is Duration — there is no conversion.
    fn emit_clock_mix(&mut self, op: &str, verb: &str) {
        self.emit(super::err(
            format!("cannot {verb} Compute and Duration"),
            "The two clocks have no bridge (ADR-0001): a deterministic budget is \
             Compute, a wall-clock deadline is Duration — there is no conversion",
            format!("operator {op}")));
    }

    /// E024, binop-operand edition (fuzz seed-20260718 index 114): a bare
    /// int literal meeting a SIZED operand adopts its width at lowering —
    /// pin that width as the literal's range context so the post-solve
    /// check fires (`(x - x) - 256` with x: Int8 — native rustc rejected
    /// `256i8` while check passed). Every literal has a site now (the
    /// liberal enqueue), so this only sets context_ty. Verbatim text move
    /// out of [`Self::infer_expr_g2_binary`].
    fn pin_binop_literal_context(&mut self, op: &Sym, left: &ast::Expr, right: &ast::Expr, lt: &Ty, rt: &Ty) {
        if matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "^") {
            let lit_id = |e: &ast::Expr| match &e.kind {
                ExprKind::Int { .. } => Some(e.id),
                ExprKind::Unary { op, operand, .. }
                    if op.as_str() == "-"
                        && matches!(&operand.kind, ExprKind::Int { .. }) =>
                {
                    Some(operand.id)
                }
                ExprKind::Paren { expr }
                    if matches!(&expr.kind, ExprKind::Int { .. }) =>
                {
                    Some(expr.id)
                }
                _ => None,
            };
            let is_sized_int = |t: &Ty| matches!(
                t,
                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            );
            let lc0 = resolve_ty(lt, &self.uf);
            let rc0 = resolve_ty(rt, &self.uf);
            let l_lit = lit_id(left);
            let r_lit = lit_id(right);
            if is_sized_int(&lc0) {
                if let Some(id) = r_lit {
                    self.pin_int_literal_context(id, &lc0);
                }
            }
            if is_sized_int(&rc0) {
                if let Some(id) = l_lit {
                    self.pin_int_literal_context(id, &rc0);
                }
            }
        }
    }

    /// `-`/`*`/`/`/`%`/`^` arm of [`Self::infer_expr_g2_binary`]: Matrix
    /// arithmetic, numeric-operand and mixed-sized-width diagnostics, and
    /// same-width/Float-promotion result-type resolution. Verbatim text move.
    fn infer_binop_arith(&mut self, op: &Sym, lt: &Ty, rt: &Ty, left: &ast::Expr, right: &ast::Expr) -> Ty {
        let lc = resolve_ty(lt, &self.uf);
        let rc = resolve_ty(rt, &self.uf);
        // Matrix operators: *, +, - on Matrix types
        if lc == Ty::Matrix || rc == Ty::Matrix {
            Ty::Matrix
        } else {
            // Sized Numeric Types (Stage 1c): same-width
            // arithmetic accepts every sized numeric variant.
            let is_numeric = |t: &Ty| matches!(
                t,
                Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                    | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                    | Ty::Float32 | Ty::Float64
                    | Ty::Matrix
                    // GPU vector/matrix types (Vec2, Vec3, Vec4, Mat3, Mat4)
                    // support arithmetic ops; emitted as WGSL builtins.
                    | Ty::Named(..)
            );
            let is_sized_scalar = |t: &Ty| matches!(
                t,
                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                    | Ty::Float32 | Ty::Float64
            );
            if !is_numeric(&lc) || !is_numeric(&rc) {
                // Same Result-specific hint as `+` (#1050): the generic text
                // never named the unwrap operators.
                let hint = if lc.is_result() || rc.is_result() {
                    "Unwrap the Result operand first: `!` propagates the error (effect fn body), \
                     `?? fallback` supplies a default, or `match` handles ok/err"
                } else {
                    "Use numeric types (Int or Float)"
                };
                self.emit(super::err(
                    format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                    hint, format!("operator {}", op)));
            }
            // A sized operand meeting a canonical `Int`/`Float` VALUE is the
            // same mistake with the wide side spelled differently (#902).
            if let Some(t) = self.check_mixed_canonical_width(op.as_str(), &lc, &rc, left, right) {
                return t;
            }
            // Stage 1c: reject mixed-sized-width arithmetic.
            // See `infer_plus_op` for rationale.
            if is_sized_scalar(&lc) && is_sized_scalar(&rc) && lc != rc {
                self.emit(super::err(
                    format!(
                        "operator '{}' mixes sized numeric types {} and {} — \
                         explicit conversion required (e.g. `.to_{}()`)",
                        op, lc.display(), rc.display(),
                        lc.display().to_lowercase()),
                    "Convert one side with `.to_intN()` / `.to_floatN()` before the op",
                    format!("operator {}", op)));
                lc
            } else if lc.compatible(&rc) && is_sized_scalar(&lc) {
                lc
            } else if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt.clone() }
        }
    }

    /// `==`/`!=`/`<`/`>`/`<=`/`>=` arm of [`Self::infer_expr_g2_binary`]:
    /// none-comparison validity, TypeVar unification, and the ordering
    /// (`<`/`>`/`<=`/`>=`) scalar-orderable-types restriction (#652).
    /// Verbatim text move.
    fn infer_binop_compare(&mut self, op: &Sym, left: &ast::Expr, right: &ast::Expr, lt: &Ty, rt: &Ty) -> Ty {
        // Check none comparison: only valid with Option types
        let left_is_none = matches!(left.kind, ExprKind::None);
        let right_is_none = matches!(right.kind, ExprKind::None);
        if right_is_none && !left_is_none {
            let lc = resolve_ty(lt, &self.uf);
            if !lc.is_option() && !matches!(lc, Ty::Unknown | Ty::TypeVar(_)) {
                self.emit(super::err(
                    format!("cannot compare {} with none — only Option types support none comparison", lc.display()),
                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
            }
        }
        if left_is_none && !right_is_none {
            let rc = resolve_ty(rt, &self.uf);
            if !rc.is_option() && !matches!(rc, Ty::Unknown | Ty::TypeVar(_)) {
                self.emit(super::err(
                    format!("cannot compare none with {} — only Option types support none comparison", rc.display()),
                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
            }
        }
        // Unify left/right types so TypeVars in none/err/constructors get resolved
        self.unify_infer(lt, rt);
        // #1050: a Result on exactly ONE side of ==/!= can never compare —
        // `unify_infer` stays silent on a concrete mismatch, so this shape
        // used to pass check and explode as an E0308 in the generated Rust
        // (`helper() == ok(0)`: the operand strip / auto-? gives the call
        // side `T` while the ctor side keeps `Result`). Report it here with
        // the unwrap operators named.
        if matches!(op.as_str(), "==" | "!=") {
            let lc = resolve_ty(lt, &self.uf);
            let rc = resolve_ty(rt, &self.uf);
            let opaque = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
            if lc.is_result() != rc.is_result() && !opaque(&lc) && !opaque(&rc) {
                self.emit(super::err(
                    format!("operator '{}' compares {} with {}", op, lc.display(), rc.display()),
                    "Unwrap the Result operand first (`!` in an effect fn body, `?? fallback`, \
                     or `match` on ok/err) — or compare two Results",
                    format!("operator {}", op)).with_code("E037"));
            } else if same_head_applied_mismatch(&lc, &rc) {
                // #1116: `unify_infer` stays silent on a concrete mismatch, so
                // same-head shapes with different params (`Result[Int, String]
                // == Result[Int, Int]`, `Option[Int] == Option[String]`)
                // passed check and died as rustc E0308 behind the
                // codegen-produced-invalid-Rust wall. Both sides fully
                // concrete + structurally different = can never compare.
                self.emit(super::err(
                    format!("operator '{}' compares {} with {}", op, lc.display(), rc.display()),
                    "== requires both operands to have the same type — convert one side \
                     (or compare the payloads after unwrapping)",
                    format!("operator {}", op)).with_code("E037"));
            }
            // #1116: `none == none` (both sides `Option[?0]`, never pinned)
            // passed check and died as rustc E0282. Queue the unified operand
            // type for the post-solve E025 sweep — same rule as bindings and
            // interpolation segments.
            self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                ty: lt.clone(), name: None, span: left.span,
            });
        }
        // Ordering (< <= > >=) is defined ONLY on scalar orderable
        // types. On a compound operand (Tuple/Option/Result/List/
        // Map/Set/Record/custom) the checker used to pass while
        // codegen diverged: native silently relied on Rust's derive
        // (and FAILED on records, E0369) and WASM ICEd
        // (equality.rs no-comparison arm). Reject uniformly so check
        // matches codegen on both targets; equality (== !=) still
        // works (deep structural). #652
        if matches!(op.as_str(), "<" | ">" | "<=" | ">=") {
            let lc = resolve_ty(lt, &self.uf);
            let orderable = matches!(lc,
                Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64 | Ty::String | Ty::Bool
                | Ty::Unknown | Ty::TypeVar(_) | Ty::Never);
            if !orderable {
                self.emit(super::err(
                    format!("operator '{}' is not defined for {} — ordering applies to Int, Float, String, and Bool", op, lc.display()),
                    "Compare scalar fields explicitly, or use list.sort / list.min / list.max for ordered collections",
                    format!("operator {}", op)));
            }
        }
        Ty::Bool
    }

    /// `and`/`or` arm of [`Self::infer_expr_g2_binary`]. Verbatim text move.
    fn infer_binop_logical(&mut self, op: &Sym, lt: &Ty, rt: &Ty) -> Ty {
        let lc = resolve_ty(lt, &self.uf);
        let rc = resolve_ty(rt, &self.uf);
        let is_bool = |t: &Ty| matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_));
        if !is_bool(&lc) {
            self.emit(super::err(
                format!("operator '{}' requires Bool but got {}", op, lc.display()),
                "Use Bool values with logical operators", format!("operator {}", op)));
        }
        if !is_bool(&rc) {
            self.emit(super::err(
                format!("operator '{}' requires Bool but got {}", op, rc.display()),
                "Use Bool values with logical operators", format!("operator {}", op)));
        }
        Ty::Bool
    }

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
                if let Some(ty) = self.env.lookup_var(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { self.instantiate_ty(&ty) }
                // Const param: `N: Int` in generic params resolves to its underlying type
                else if let Some(Ty::ConstParam { ty, .. }) = self.env.types.get(&sym(name)).cloned() {
                    *ty
                }
                else if let Some(sig) = self.env.functions.get(&sym(name)).cloned() {
                    Ty::Fn {
                        params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                        ret: Box::new(sig.ret.clone()),
                        is_effect: false /* named-fn VALUES keep the carrier in `ret` (sig.ret is already Result for effect fns); the effect BIT belongs to declared slot types only, where ret is the unwrapped B (#1055) */,
                    }
                }
                else {
                    self.report_undefined_variable(name)
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
                        diag = diag.with_try_replace(
                            1, 1, 1,
                            format!("import {}\n", stripped),
                        );
                    } else if let Some(span) = self.current_span {
                        // Typo fuzzy suggestion: replace the
                        // offending identifier with the suggested name.
                        diag = diag.with_try_replace(
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
