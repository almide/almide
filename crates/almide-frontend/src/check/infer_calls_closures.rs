// `infer_expr_inner` group 3 — blocks, calls, pipes, closures, loops, the
// Option/Result constructor & postfix-operator arms, and map literals (Block …
// TypeAscription, minus the `return`-bearing arms kept in the dispatcher).
// Plus the smaller extracted inference / call-resolution helpers. Disjoint
// from groups 1 & 2; see `infer_expr_inner`. `include!`d into `infer.rs`, so
// imports come from there.

impl Checker {
    pub(super) fn infer_expr_inner_g3(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        if let Some(ty) = self.infer_expr_g3_scoped(expr) { return Some(ty); }
        if let Some(ty) = self.infer_expr_g3_operand(expr) { return Some(ty); }
        if let Some(ty) = self.infer_expr_g3_postfix(expr) { return Some(ty); }
        None
    }

    /// Blocks, fan, calls, pipes, composition, lambdas and the loop forms — the
    /// expressions that introduce or thread a scope.
    ///
    /// One group of the `infer_expr_inner` arm table, arms verbatim and in
    /// source order. `None` means "not my group" — the dispatcher tries the
    /// groups in that order, so the dispatch an expression sees is unchanged.
    pub(super) fn infer_expr_g3_scoped(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Block { .. } => self.infer_expr_g3_block(expr),
            ExprKind::Fan { .. } => self.infer_expr_g3_fan(expr),
            ExprKind::FanBounded { .. } => self.infer_expr_g3_fan_bounded(expr),
            ExprKind::FanRace { .. } => self.infer_expr_g3_fan_race(expr),
            ExprKind::FanRaceMap { .. } => self.infer_expr_g3_fan_race_map(expr),
            ExprKind::FanSettle { .. } => self.infer_expr_g3_fan_settle(expr),
            ExprKind::FanTimeout { .. } => self.infer_expr_g3_fan_timeout(expr),
            ExprKind::Call { .. } => self.infer_expr_g3_call(expr),

            ExprKind::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ExprKind::Compose { left, right, .. } => {
                let left_ty = self.infer_expr(left);
                let right_ty = self.infer_expr(right);
                // If left is Fn[A] -> B and right is Fn[B] -> C, result is Fn[A] -> C
                let resolved_left = resolve_ty(&left_ty, &self.uf);
                let resolved_right = resolve_ty(&right_ty, &self.uf);
                match (&resolved_left, &resolved_right) {
                    (Ty::Fn { params: a_params, .. }, Ty::Fn { ret: c_ret, .. }) => {
                        Ty::Fn { params: a_params.clone(), ret: c_ret.clone() }
                    }
                    _ => Ty::Unknown,
                }
            }

            ExprKind::Lambda { .. } => self.infer_expr_g3_lambda(expr),

            ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
                self.infer_for_in(var, var_tuple, iterable, body)
            }

            ExprKind::While { cond, body, .. } => {
                let cond_ty = self.infer_expr(cond);
                self.constrain_condition(cond, cond_ty, "while");
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                Ty::Unit
            }

            _ => return None,
        })
    }

    /// Ranges, the `Option`/`Result` constructors, `?`, parenthesised expressions,
    /// `break`, and the typed hole.
    ///
    /// One group of the `infer_expr_inner` arm table, arms verbatim and in
    /// source order. `None` means "not my group" — the dispatcher tries the
    /// groups in that order, so the dispatch an expression sees is unchanged.
    pub(super) fn infer_expr_g3_operand(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        if let Some(ty) = self.infer_expr_g3_range_ctor(expr) { return Some(ty); }
        if let Some(ty) = self.infer_expr_g3_grouping(expr) { return Some(ty); }
        None
    }

    /// Ranges and the `Option` / `Result` constructors.
    ///
    /// One group of `infer_expr_inner`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so the dispatch is unchanged.
    pub(super) fn infer_expr_g3_range_ctor(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); Ty::list(st) }

            ExprKind::Some { expr, .. } => { let inner = self.infer_expr(expr); Ty::option(inner) }
            ExprKind::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            _ => return None,
        })
    }

    /// `?`, parenthesised expressions, `break`, and the typed hole.
    ///
    /// One group of `infer_expr_inner`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so the dispatch is unchanged.
    pub(super) fn infer_expr_g3_grouping(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty,
                }
            }

            ExprKind::Paren { expr, .. } => self.infer_expr(expr),
            ExprKind::Break | ExprKind::Continue => Ty::Unit,
            ExprKind::Hole | ExprKind::Todo { .. } => self.fresh_var(),
            _ => return None,
        })
    }

    /// `await`, the unwrap family, `err(..)`, the map-literal forms, and type
    /// ascription.
    ///
    /// One group of the `infer_expr_inner` arm table, arms verbatim and in
    /// source order. `None` means "not my group" — the dispatcher tries the
    /// groups in that order, so the dispatch an expression sees is unchanged.
    pub(super) fn infer_expr_g3_postfix(&mut self, expr: &mut ast::Expr) -> Option<Ty> {
        Some(match &mut expr.kind {
            ExprKind::Unwrap { .. } => self.infer_expr_g3_unwrap(expr),
            ExprKind::UnwrapOr { .. } => self.infer_expr_g3_unwrap_or(expr),
            ExprKind::ToOption { .. } => self.infer_expr_g3_to_option(expr),
            ExprKind::Error | ExprKind::Placeholder => Ty::Unknown,

            ExprKind::MapLiteral { entries, .. } => {
                if entries.is_empty() {
                    let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                    self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                    ty
                }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    self.deferred_map_key_checks.push((kt.clone(), self.current_span));
                    Ty::map_of(kt, vt)
                }
            }
            ExprKind::EmptyMap => {
                let ty = Ty::map_of(self.fresh_var(), self.fresh_var());
                self.register_empty_collection(ty.clone(), super::EmptyCollectionKind::MapLiteral);
                ty
            }

            ExprKind::TypeAscription { expr, ty } => {
                let inferred = self.infer_expr(expr);
                let ascribed = self.resolve_type_expr(ty);
                self.constrain(ascribed.clone(), inferred, "type ascription");
                ascribed
            }
            _ => return None,
        })
    }

    /// `ExprKind::Block` arm of [`Self::infer_expr_inner_g3`]. Verbatim text move.
    fn infer_expr_g3_block(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Block { stmts, expr, .. } = &mut expr.kind else { unreachable!() };
        self.env.push_scope();
        // Pre-scan for vars used as match subjects with Ok/Err
        // patterns — those bindings must keep their Result type.
        let saved_skip = std::mem::take(&mut self.env.skip_auto_unwrap_for);
        let result_match_vars = collect_block_result_match_vars(stmts, expr.as_deref());
        for n in &result_match_vars {
            self.env.skip_auto_unwrap_for.insert(*n);
        }
        for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
        let ty = if let Some(e) = expr { self.infer_expr(e) } else { Ty::Unit };
        self.env.pop_scope();
        self.env.skip_auto_unwrap_for = saved_skip;
        ty
    }

    /// `ExprKind::Fan` arm of [`Self::infer_expr_inner_g3`]: effect-fn
    /// gate, mutable-capture diagnostic, and per-expr Result auto-unwrap.
    /// Verbatim text move.
    fn infer_expr_g3_fan(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Fan { exprs, .. } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan block can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan block".to_string()).with_code("E007"));
        }
        // Check for mutable variable capture
        let mutable_captures: Vec<String> = exprs.iter().flat_map(|e| {
            let mut idents = Vec::new();
            collect_idents(e, &mut idents);
            idents.into_iter().filter(|name| self.env.mutable_vars.contains(&sym(name))).collect::<Vec<_>>()
        }).collect();
        for name in &mutable_captures {
            self.emit(super::err(
                format!("cannot capture mutable variable '{}' inside fan block", name),
                "Use a `let` binding instead of `var` for values shared across fan expressions",
                "fan block".to_string()).with_code("E008"));
        }
        let tys: Vec<Ty> = exprs.iter_mut().map(|e| {
            let ty = self.infer_expr(e);
            // Auto-unwrap Result: fan unwraps Result<T, E> to T
            let concrete = resolve_ty(&ty, &self.uf);
            match &concrete {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                _ => ty,
            }
        }).collect();
        match tys.len() {
            1 => tys.into_iter().next().unwrap_or(Ty::Unknown),
            _ => Ty::Tuple(tys.iter().map(|t| resolve_ty(t, &self.uf)).collect()),
        }
    }

    /// Budget clock firewall (ADR-0001 S4 / S6-6): the EXPECTED clock comes
    /// from the declared `TIME_CONSUMING_SURFACES` table, never from the call
    /// site — this lookup is the S6-6 face check's reading side, so a new
    /// time-consuming surface that skipped the declaration fails loudly on its
    /// first type-check anywhere in the test suite.
    fn check_budget_clock(&mut self, surface: &'static str, budget_concrete: &Ty) {
        let clock = almide_lang::time_units::surface_clock(surface).unwrap_or_else(|| {
            panic!(
                "{surface} consumes a time quantity but is not declared in \
                 TIME_CONSUMING_SURFACES (ADR-0001 S6-6)"
            )
        });
        match budget_concrete {
            Ty::Named(n, _) if n.as_str() == clock => {}
            Ty::Named(n, _) if n.as_str() == "Duration" && clock == "Compute" => {
                self.emit(super::err(
                    format!("expected {clock}, found Duration"),
                    format!(
                        "{surface} budgets deterministic computation, not wall-clock time. \
                         Build the budget with compute.ms(...); for a wall-clock limit use \
                         fan.timeout (oracle tier)"
                    ),
                    format!("{surface} budget")));
            }
            Ty::Named(n, _) if n.as_str() == "Compute" && clock == "Duration" => {
                self.emit(super::err(
                    format!("expected {clock}, found Compute"),
                    format!(
                        "{surface} takes a WALL-CLOCK deadline. Build it with \
                         duration.ms(...); for a deterministic compute budget use \
                         fan.bounded"
                    ),
                    format!("{surface} budget")));
            }
            other => {
                self.emit(super::err(
                    format!("expected {clock}, found {}", other.display()),
                    format!(
                        "Budgets carry a unit and a clock: {surface}(compute.ms(100)) {{ ... }}"
                    ),
                    format!("{surface} budget")));
            }
        }
    }

    /// `ExprKind::FanBounded` arm: `fan.bounded(budget) { body }` (Stage 2 v1).
    /// Effect-fn gate; budget must be a `Compute` (the ADR-0001 clock firewall
    /// — bare Int and wall-clock `Duration` are named type errors); the body is
    /// checked in a PURE context (Rung 0) and, in v1, must be a single call
    /// returning a plain (non-Result) value. Result[T, String] like fan.map.
    fn infer_expr_g3_fan_bounded(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::FanBounded { budget, body } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan.bounded can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan.bounded".to_string()).with_code("E007"));
        }
        let budget_ty = self.infer_expr(budget);
        let budget_concrete = resolve_ty(&budget_ty, &self.uf);
        self.check_budget_clock("fan.bounded", &budget_concrete);
        let saved_effect = self.env.can_call_effect;
        let saved_region = self.env.metered_region;
        self.env.can_call_effect = false;
        self.env.metered_region = Some("fan.bounded");
        let body_ty = self.infer_expr(body);
        self.env.can_call_effect = saved_effect;
        self.env.metered_region = saved_region;
        let body_concrete = resolve_ty(&body_ty, &self.uf);
        if body_concrete.is_result() {
            self.emit(super::err(
                "fan.bounded body must return a plain value in v1".to_string(),
                "Return the value directly; the budget adds its own Err channel".to_string(),
                "fan.bounded body".to_string()));
        }
        Ty::result(body_concrete, Ty::String)
    }

    /// `ExprKind::FanRace` arm: `fan.race(budget?) { arms }` (Stage 3 v1).
    /// Effect-fn gate; the optional budget is a `Compute` (same firewall as
    /// bounded); every arm is a single pure call and all arms unify to one T.
    /// Result[T, String] — Err is the ledger-constant no-winner verdict.
    fn infer_expr_g3_fan_race(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::FanRace { budget, arms } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan.race can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan.race".to_string()).with_code("E007"));
        }
        if let Some(b) = budget {
            let budget_ty = self.infer_expr(b);
            let budget_concrete = resolve_ty(&budget_ty, &self.uf);
            self.check_budget_clock("fan.race", &budget_concrete);
        }
        let saved_effect = self.env.can_call_effect;
        let saved_region = self.env.metered_region;
        self.env.can_call_effect = false;
        self.env.metered_region = Some("fan.race");
        let mut arm_ty: Option<Ty> = None;
        for arm in arms.iter_mut() {
            let t = self.infer_expr(arm);
            let concrete = resolve_ty(&t, &self.uf);
            // T2-2: an arm may return Result[T, E] — its Err SELF-DISQUALIFIES
            // the arm (symmetric with fan.any), and its Ok type joins the
            // unification. Plain arms are always candidates.
            let candidate = match &concrete {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
                _ => t,
            };
            match &arm_ty {
                None => arm_ty = Some(candidate),
                Some(t0) => self.constrain(candidate, t0.clone(), "fan.race arm type"),
            }
        }
        self.env.can_call_effect = saved_effect;
        self.env.metered_region = saved_region;
        let t = arm_ty.map(|t| resolve_ty(&t, &self.uf)).unwrap_or(Ty::Unknown);
        Ty::result(t, Ty::String)
    }

    /// `ExprKind::FanRaceMap` arm: `fan.race(xs, f)` / `fan.race(budget, xs, f)`
    /// (T7-1 — the mapper cell). Effect-fn gate + budget clock like the block
    /// form; `xs` unifies to `List[X]`; the mapper is a PURE 1-param lambda
    /// `(X) -> Result[T, E]` (mapper form contract: Result REQUIRED — ok
    /// competes, err self-disqualifies). Result[T, String] like the block form.
    fn infer_expr_g3_fan_race_map(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::FanRaceMap { budget, list, mapper } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan.race can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan.race".to_string()).with_code("E007"));
        }
        if let Some(b) = budget {
            let budget_ty = self.infer_expr(b);
            let budget_concrete = resolve_ty(&budget_ty, &self.uf);
            self.check_budget_clock("fan.race", &budget_concrete);
        }
        let list_ty = self.infer_expr(list);
        let elem = self.fresh_var();
        self.constrain(
            list_ty,
            Ty::Applied(TypeConstructorId::List, vec![elem.clone()]),
            "fan.race mapper list",
        );
        // The mapper body is a metered PURE region, exactly like a block arm.
        let saved_effect = self.env.can_call_effect;
        let saved_region = self.env.metered_region;
        self.env.can_call_effect = false;
        self.env.metered_region = Some("fan.race");
        let mapper_ty = self.infer_expr(mapper);
        self.env.can_call_effect = saved_effect;
        self.env.metered_region = saved_region;
        // The mapper's return is pinned to `Result[T, String]` OUTRIGHT (not a
        // free Err var): the Err payload only self-disqualifies — it is never
        // read — and an unconstrained E left `Result[T, Unknown]` in the
        // lowered fold (a ConcretizeTypes refusal). String is the fan world's
        // uniform error channel. A concretely non-Result mapper gets the
        // contract named before the unification error would garble it.
        let mapper_concrete = resolve_ty(&mapper_ty, &self.uf);
        if let Ty::Fn { ret, .. } = &mapper_concrete {
            let r = resolve_ty(ret, &self.uf);
            if !matches!(r, Ty::Applied(TypeConstructorId::Result, _) | Ty::TypeVar(_) | Ty::Unknown) {
                self.emit(super::err(
                    format!("fan.race mapper must return a Result, got {}", r.display()),
                    "Return ok(value) to compete and err(reason) to disqualify the element — the mapper-form contract (like fan.map)",
                    "fan.race mapper".to_string()));
            }
        }
        let winner = self.fresh_var();
        self.constrain(
            mapper_ty,
            Ty::Fn {
                params: vec![elem],
                ret: Box::new(Ty::result(winner.clone(), Ty::String)),
            },
            "fan.race mapper",
        );
        Ty::result(resolve_ty(&winner, &self.uf), Ty::String)
    }

    /// `ExprKind::FanTimeout` arm: `fan.timeout(deadline) { body }` (T5-1,
    /// the ORACLE tier). The deadline is a `Duration` (the S4 matrix's first
    /// wall-clock row — Compute and bare Int are named errors via the same
    /// S6-6 table lookup); the body is PURE like bounded's (v1) and the
    /// verdict is ω-relative: Err iff the wall deadline fires at a charge
    /// site before the body completes. Result[T, String].
    fn infer_expr_g3_fan_timeout(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::FanTimeout { deadline, body } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan.timeout can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan.timeout".to_string()).with_code("E007"));
        }
        let deadline_ty = self.infer_expr(deadline);
        let deadline_concrete = resolve_ty(&deadline_ty, &self.uf);
        self.check_budget_clock("fan.timeout", &deadline_concrete);
        let saved_effect = self.env.can_call_effect;
        let saved_region = self.env.metered_region;
        self.env.can_call_effect = false;
        self.env.metered_region = Some("fan.timeout");
        let body_ty = self.infer_expr(body);
        self.env.can_call_effect = saved_effect;
        self.env.metered_region = saved_region;
        let body_concrete = resolve_ty(&body_ty, &self.uf);
        if body_concrete.is_result() {
            self.emit(super::err(
                "fan.timeout body must return a plain value in v1".to_string(),
                "Return the value directly; the deadline adds its own Err channel".to_string(),
                "fan.timeout body".to_string()));
        }
        Ty::result(body_concrete, Ty::String)
    }

    /// `ExprKind::FanSettle` arm: `fan.settle { arms }` (T2-4). Every arm
    /// settles into its OWN `Result` slot — heterogeneous arm types are
    /// allowed and the value is the TUPLE `(Result[A, String], …)` in arm
    /// order. Arms may be effectful (unlike the metered regions): an arm's
    /// Err is CAPTURED into its slot, never propagated, so arm inference
    /// runs with auto-unwrap OFF.
    fn infer_expr_g3_fan_settle(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::FanSettle { arms } = &mut expr.kind else { unreachable!() };
        if !self.env.can_call_effect {
            self.emit(super::err(
                "fan.settle can only be used inside an effect fn".to_string(),
                "Mark the enclosing function as `effect fn`",
                "fan.settle".to_string()).with_code("E007"));
        }
        let saved_unwrap = self.env.auto_unwrap;
        self.env.auto_unwrap = false;
        let mut elems = Vec::with_capacity(arms.len());
        for arm in arms.iter_mut() {
            let t = self.infer_expr(arm);
            let c = resolve_ty(&t, &self.uf);
            elems.push(match &c {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => c.clone(),
                _ => Ty::result(c, Ty::String),
            });
        }
        self.env.auto_unwrap = saved_unwrap;
        Ty::Tuple(elems)
    }

    /// `ExprKind::Call` arm of [`Self::infer_expr_inner_g3`]. Verbatim text move.
    fn infer_expr_g3_call(&mut self, expr: &mut ast::Expr) -> Ty {
        let span = expr.span;
        let ExprKind::Call { callee, args, named_args, type_args, .. } = &mut expr.kind else { unreachable!() };
        // Publish the outer Call's span so UFCS / whole-expr
        // rewrites (E002 method-UFCS, E013 no-field) can emit
        // a `try_replace` range covering `callee(args)` in
        // full, not just the callee reference. Nested calls
        // save/restore the previous value.
        let prev_call = self.call_span_hint.take();
        self.call_span_hint = span;
        let ty = self.infer_call(callee, args, named_args, type_args);
        self.call_span_hint = prev_call;
        // A generic collection constructor whose element type NO argument
        // constrains — `set.new()` / `list.with_capacity(n)` — must have
        // its element pinned by context (annotation / later use). Register
        // it for the post-solve undecidable-empty-collection check (E018).
        if let Some(kind) = empty_collection_ctor_kind(callee) {
            self.register_empty_collection(ty.clone(), kind);
        }
        // #880: `assert_eq` / `assert_ne` compare two PEERS. `check_builtin_output`
        // constrains them symmetrically (it only ever sees the arg TYPES), so
        // `assert_eq(n, u8v)` passed check and emitted `almide_eq!(i64, u8)` —
        // a native E0308. The arg AST is only in scope here, which is where the
        // literal exemption can be decided.
        if matches!(&callee.kind, ExprKind::Ident { name, .. } if matches!(name.as_str(), "assert_eq" | "assert_ne"))
            && args.len() >= 2
        {
            let peers: Vec<(Ty, Option<ast::Span>, bool)> = args.iter().take(2).map(|a| (
                self.type_map.get(&a.id).cloned().unwrap_or(Ty::Unknown),
                a.span,
                super::is_literal_numeric_ast(a),
            )).collect();
            self.join_sized_peers(&peers, "assert argument");
        }
        ty
    }

    /// `ExprKind::Lambda` arm of [`Self::infer_expr_inner_g3`]. Verbatim text move.
    fn infer_expr_g3_lambda(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Lambda { params, body, .. } = &mut expr.kind else { unreachable!() };
        self.env.push_scope();
        // Lambda has its own return context — don't leak outer function's current_ret
        let saved_ret = self.env.current_ret.take();
        // A lambda is its own function: the enclosing effect fn's
        // auto-`?` cannot propagate out of a closure body (the closure
        // may escape), so an effect call inside a lambda yields the
        // EXPLICIT Result — auto_unwrap is off, matching the lowering,
        // which never inserts `?` inside Lambda bodies (#489).
        let saved_auto_unwrap = self.env.auto_unwrap;
        self.env.auto_unwrap = false;
        self.env.lambda_depth += 1;
        // Expected-type hint from the enclosing call (#653): when this
        // lambda is an argument whose parameter slot is a `Fn`, the
        // caller pins each UNANNOTATED param to the expected element
        // type (e.g. `T` carrying a protocol bound) so the body resolves
        // method calls on the param via the protocol path instead of
        // collapsing it into a closure type. An explicit annotation on
        // the param always wins; the hint only fills inferred slots.
        let param_hint = self.lambda_arg_hint.take();
        let param_tys: Vec<Ty> = params.iter().enumerate().map(|(i, p)| {
            let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te))
                .or_else(|| param_hint.as_ref().and_then(|h| h.get(i).cloned().flatten()))
                .unwrap_or_else(|| self.fresh_var());
            let concrete = resolve_ty(&ty, &self.uf);
            // A tuple-pattern parameter (`((k, v)) => …`) binds EVERY name, not
            // just the first. The parser has produced `tuple_names` since the
            // form was introduced, but nothing downstream read it — so the
            // second name was simply undefined and the writer got E003 on a
            // variable they had just written (#1060).
            match &p.tuple_names {
                Some(names) if names.len() > 1 => {
                    let elems: Vec<Ty> = match &concrete {
                        Ty::Tuple(es) if es.len() == names.len() => es.clone(),
                        _ => {
                            let fresh: Vec<Ty> = names.iter().map(|_| self.fresh_var()).collect();
                            self.constrain(ty.clone(), Ty::Tuple(fresh.clone()), "tuple lambda parameter");
                            fresh
                        }
                    };
                    for (n, et) in names.iter().zip(elems.iter()) {
                        let e = resolve_ty(et, &self.uf);
                        self.env.define_var(n, e);
                    }
                }
                _ => self.env.define_var(&p.name, concrete),
            }
            ty
        }).collect();
        let ret_ty = self.infer_expr(body);
        self.env.lambda_depth -= 1;
        self.env.auto_unwrap = saved_auto_unwrap;
        self.env.current_ret = saved_ret;
        self.env.pop_scope();
        Ty::Fn { params: param_tys, ret: Box::new(ret_ty) }
    }

    /// `expr!` — unwrap with propagation (Option[T] → T, Result[T,E] → T).
    /// `ExprKind::Unwrap` arm of [`Self::infer_expr_inner_g3`]. Verbatim text move.
    fn infer_expr_g3_unwrap(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Unwrap { expr: inner, .. } = &mut expr.kind else { unreachable!() };
        let t = self.infer_expr(inner);
        let resolved = resolve_ty(&t, &self.uf);
        self.check_unwrap_propagation_context(&resolved);
        if let Some(inner_ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
            inner_ty
        } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
            self.fresh_var()
        } else if self.is_effect_call_expr(inner) {
            // #1049: `!` on a NEVER-ERR effect call is a silent no-op. The
            // never-err/can-err split is the lifted ABI's business (#840/#841);
            // the surface rule stays position-independent — "an effect call
            // takes `!`" must compile for every effect fn, or the writer needs
            // each stdlib fn's internal classification to predict the checker.
            // Silent on purpose: a warning would teach removing the `!`, which
            // breaks the caller the day the fn's classification changes. The
            // pipe spelling (`xs |> f!`, infer_pipe) already unwraps by
            // identity here, so this also closes a direct-vs-pipe asymmetry.
            t
        } else {
            self.emit(super::err(
                format!("operator '!' requires Option or Result type but got {}", resolved.display()),
                "Use '!' only on Option[T] or Result[T, E] values",
                "operator !",
            ).with_code("E034"));
            Ty::Unknown
        }
    }

    /// True when `expr` is a CALL whose callee resolves to an `effect fn` —
    /// the `!`-is-a-no-op carve-out above. Covers the two shapes
    /// [`Self::lookup_call_sig`] resolves (bare `Ident` and `module.fn`);
    /// anything else keeps the strict rule.
    fn is_effect_call_expr(&self, expr: &ast::Expr) -> bool {
        let ExprKind::Call { callee, .. } = &expr.kind else { return false };
        self.lookup_call_sig(callee).is_some_and(|sig| sig.is_effect)
    }

    /// `expr ?? fallback` — unwrap with default (Option[T] → T, Result[T,E]
    /// → T). `ExprKind::UnwrapOr` arm of [`Self::infer_expr_inner_g3`].
    /// Verbatim text move.
    fn infer_expr_g3_unwrap_or(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::UnwrapOr { expr: inner, fallback, .. } = &mut expr.kind else { unreachable!() };
        let t = self.infer_expr(inner);
        let ft = self.infer_expr(fallback);
        let resolved = resolve_ty(&t, &self.uf);
        let inner_ty = if let Some(ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
            ty
        } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
            ft.clone()
        } else {
            self.emit(super::err(
                format!("operator '??' requires Option or Result type but got {}", resolved.display()),
                "Use '??' only on Option[T] or Result[T, E] values",
                "operator ??",
            ).with_code("E034"));
            ft.clone()
        };
        // #1119: unify_infer stays SILENT on a concrete mismatch, so
        // `n ?? "hello"` / `n ?? some(1)` (fallback type ≠ unwrapped T)
        // passed check and died as rustc E0308 behind the codegen wall.
        // constrain routes the same unification through the reporting solver.
        self.constrain(inner_ty.clone(), ft, "?? fallback");
        inner_ty
    }

    /// `expr?` — to Option (Result[T,E] → Option[T], Option[T] →
    /// Option[T]). `ExprKind::ToOption` arm of [`Self::infer_expr_inner_g3`].
    /// Verbatim text move.
    fn infer_expr_g3_to_option(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::ToOption { expr: inner, .. } = &mut expr.kind else { unreachable!() };
        let t = self.infer_expr(inner);
        let resolved = resolve_ty(&t, &self.uf);
        if let Some(ok_ty) = resolved.result_ok_ty() {
            Ty::option(ok_ty)
        } else if resolved.is_option() {
            resolved.clone()
        } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
            Ty::option(self.fresh_var())
        } else {
            self.emit(super::err(
                format!("operator '?' requires Option or Result type but got {}", resolved.display()),
                "Use '?' only on Option[T] or Result[T, E] values",
                "operator ?",
            ).with_code("E034"));
            Ty::Unknown
        }
    }

    // ── Extracted inference helpers ──

    fn infer_call(
        &mut self,
        callee: &mut Box<ast::Expr>,
        args: &mut Vec<ast::Expr>,
        named_args: &mut Vec<(almide_base::intern::Sym, ast::Expr)>,
        type_args: &Option<Vec<ast::TypeExpr>>,
    ) -> Ty {
        // Save named arg names, then flatten into positional args temporarily.
        let named_names: Vec<almide_base::intern::Sym> = named_args.iter().map(|(n, _)| *n).collect();
        let named_start = args.len();
        args.extend(std::mem::take(named_args).into_iter().map(|(_, e)| e));
        let resolved_type_args: Option<Vec<crate::types::Ty>> = type_args.as_ref().map(|tas|
            tas.iter().map(|te| self.resolve_type_expr(te)).collect());
        // #558: hand the named-arg shape to check_named_call so it validates
        // by NAME (matching lowering), not by the appended positional slot.
        self.named_arg_meta = if named_names.is_empty() { None }
            else { Some((named_start, named_names.clone())) };
        let ret = self.check_call_with_type_args(callee, args, resolved_type_args.as_deref());
        self.named_arg_meta = None;
        // Restore named args
        let named_exprs: Vec<ast::Expr> = args.drain(named_start..).collect();
        *named_args = named_names.into_iter().zip(named_exprs).collect();
        ret
    }

    /// `expr!` propagates the unwrapped error: lowering renders it as `?`
    /// (effect fn body) or `.unwrap()` (test block). In any other context the
    /// generated `?` lands in a function/closure that does not return Result,
    /// which is invalid Rust and a wasm build failure — yet the type checker
    /// previously accepted it (the `Result/Option → T` rule alone). Error
    /// propagation is possible exactly where `auto_unwrap` is live (an effect
    /// fn body, outside any lambda) or inside a `test` block; reject everywhere
    /// else at type-check time so the failure is a clear diagnostic, not a
    /// codegen ICE (#608).
    fn check_unwrap_propagation_context(&mut self, operand: &Ty) {
        if self.env.auto_unwrap || self.env.in_test_block {
            return;
        }
        // #1067: a PURE fn that DECLARES a `Result`/`Option` return propagates
        // `!` exactly like an effect fn body — Result propagation is pure
        // control flow (the derived Codec decoders have always lowered this
        // way; every peer with hand-written codecs has the same operator: Rust
        // `?`, Zig `try`). The closure-boundary rule is unchanged (#489):
        // inside a lambda `!` cannot propagate out, whatever the fn returns.
        if self.env.lambda_depth == 0 {
            if let Some(ret) = self.env.current_ret.clone() {
                let ret = resolve_ty(&ret, &self.uf);
                let op = resolve_ty(operand, &self.uf);
                match (&ret, &op) {
                    // A Result operand's error type must BE the fn's error type
                    // (the lowered `?` converts nothing) — unify, so a mismatch
                    // is a check-time error, never generated-Rust E0308.
                    (
                        Ty::Applied(TypeConstructorId::Result, ra),
                        Ty::Applied(TypeConstructorId::Result, oa),
                    ) if ra.len() == 2 && oa.len() == 2 => {
                        self.unify_infer(&ra[1], &oa[1]);
                        return;
                    }
                    // Option operand in a Result fn: none becomes the same
                    // manufactured error the effect-fn lowering already emits.
                    (
                        Ty::Applied(TypeConstructorId::Result, _),
                        Ty::Applied(TypeConstructorId::Option, _),
                    ) => return,
                    // Option operand in an Option fn: none propagates as none.
                    (
                        Ty::Applied(TypeConstructorId::Option, _),
                        Ty::Applied(TypeConstructorId::Option, _),
                    ) => return,
                    // Error-recovery parity with the unwrap typing rule.
                    (
                        Ty::Applied(TypeConstructorId::Result, _)
                        | Ty::Applied(TypeConstructorId::Option, _),
                        Ty::Unknown | Ty::TypeVar(_),
                    ) => return,
                    _ => {}
                }
            }
        }
        // Inside a lambda within an effect fn the call site *looks* effectful,
        // but `?` cannot propagate out of the closure (#489) — point there
        // specifically; otherwise the fn needs a propagation-capable signature.
        let hint = if self.env.lambda_depth > 0 {
            "`!` cannot propagate an error out of a lambda; use `??` for a fallback value or move the call out of the closure"
        } else {
            "Declare the fn's return type as Result (a Result operand propagates its err) or Option, mark it `effect fn`, or use `??` to provide a fallback value"
        };
        self.emit(super::err(
            "operator '!' propagates errors and is only valid inside an `effect fn` body, a `test` block, or a fn returning Result/Option".to_string(),
            hint,
            "operator !",
        ).with_code("E022"));
    }

    fn infer_pipe(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        // Unwrap postfix operators (??, !, ?) on the RHS so the pipe targets the inner Call.
        // e.g. `xs |> list.find(pred) ?? fallback` → pipe into list.find, then apply ??
        match &mut right.kind {
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => self.infer_pipe_unwrap_or(left, inner, fallback),
            ExprKind::Unwrap { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                self.check_unwrap_propagation_context(&inner_ty);
                // Annotate the inner expression with its resolved type so the lowering
                // can construct the correct IR type (e.g., Result[List[T], List[E]] for
                // result.collect rather than hardcoding Result[T, String]).
                self.type_map.insert(inner.id, inner_ty.clone());
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Try { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 =>
                        Ty::Applied(TypeConstructorId::Option, vec![args[0].clone()]),
                    _ => Ty::Applied(TypeConstructorId::Option, vec![inner_ty]),
                }
            }
            _ => self.infer_pipe_direct(left, right),
        }
    }

    /// `ExprKind::UnwrapOr` arm of [`Self::infer_pipe`]. Verbatim text move.
    fn infer_pipe_unwrap_or(&mut self, left: &mut Box<ast::Expr>, inner: &mut Box<ast::Expr>, fallback: &mut Box<ast::Expr>) -> Ty {
        let inner_ty = self.infer_pipe(left, inner);
        let fb_ty = self.infer_expr(fallback);
        self.unify_infer(&inner_ty, &fb_ty);
        // UnwrapOr unwraps Option[T]/Result[T,E] → T
        match &inner_ty {
            Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            _ => inner_ty,
        }
    }

    fn infer_pipe_direct(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        let left_ty = self.infer_expr(left);
        // Resolve TypeVars eagerly via UnionFind — earlier pipes in the chain
        // have already been unified (constrain() calls unify_infer immediately),
        // so the concrete type is available now. Without this, chained UFCS like
        // `xs |> list.map(f) |> list.join(",")` sees a raw TypeVar for the
        // intermediate result, causing module resolution to fail.
        let left_ty = super::types::resolve_ty(&left_ty, &self.uf);
        match &mut right.kind {
            ExprKind::Call { callee, args, .. } => {
                // Pipe inserts left as the first argument
                let mut all_arg_tys: Vec<Ty> = vec![left_ty];
                all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                match &mut callee.kind {
                    ExprKind::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                    ExprKind::Member { object, field, .. } => {
                        let module_key = self.resolve_module_call(object, field);
                        if let Some(key) = module_key {
                            return self.check_named_call(&key, &all_arg_tys);
                        }
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                    _ => {
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                }
            }
            // Pipe RHS is a bare function name (e.g. `5 |> double`)
            ExprKind::Ident { name, .. } => {
                let all_arg_tys = vec![left_ty];
                self.check_named_call(name, &all_arg_tys)
            }
            // Pipe RHS is a module-qualified function (e.g. `5 |> int.abs`)
            ExprKind::Member { object, field, .. } => {
                let all_arg_tys = vec![left_ty];
                if let Some(key) = self.resolve_module_call(object, field) {
                    return self.check_named_call(&key, &all_arg_tys);
                }
                let ct = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
            _ => {
                let rt = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(rt, Ty::Fn { params: vec![left_ty], ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
        }
    }

}
