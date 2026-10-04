// Statement checking, pattern binding, and the `+`-operator inference rule —
// extracted from `infer.rs` to keep each file under the 1000-line ceiling.
// `include!`d into `infer.rs`, so imports come from there.

impl Checker {
    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Let { .. } => self.check_stmt_let(stmt),
            ast::Stmt::Var { .. } => self.check_stmt_var(stmt),
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let val_ty = self.infer_expr(value);
                let val_resolved = resolve_ty(&val_ty, &self.uf);
                self.bind_pattern(pattern, &val_resolved);
            }
            ast::Stmt::Assign { .. } => self.check_stmt_assign(stmt),
            ast::Stmt::IndexAssign { .. } => self.check_stmt_index_assign(stmt),
            ast::Stmt::FieldAssign { .. } => self.check_stmt_field_assign(stmt),
            ast::Stmt::Guard { cond, else_, .. } => self.check_stmt_guard(cond, else_),
            ast::Stmt::GuardLet { .. } => self.check_stmt_guard_let(stmt),
            ast::Stmt::Expr { expr, .. } => {
                let t = self.infer_expr(expr);
                // ADR-0008 D2 (#1123 N+1): a discarded Result in statement
                // position is the must-use error E042 — in EVERY fn kind, not
                // only the old auto-? contexts. Queue unconditionally;
                // post-solve keeps only Result-typed sites. The `!` insertion
                // hint stays mechanical for a plain call, but only where `!`
                // is legal (effect fn body / test block, the E022 predicate)
                // — in a pure fn the applied fix could never compile, and a
                // span fix that cannot compile is worse than no span fix
                // (#1528: the e042-in-pure-fn fixture pins this).
                // Queued at the statement's TAIL LEAVES (#2182): a `match`
                // whose arms are effect calls typed as their stripped join,
                // and an `if` whose `else` carries the Result, discarded the
                // value with no report — the leaf is where the `!` goes.
                self.queue_implicit_prop_leaves(expr, "of this statement's result", true);
                // #662: a discarded expression statement whose type carries an
                // unconstrained phantom slot (e.g. a bare `result.or_else(r0,
                // (e) => ok(0))`) is undecidable — re-check post-solve.
                self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                    ty: resolve_ty(&t, &self.uf), name: None, span: expr.span,
                });
            }
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
        }
    }

    /// `guard cond else E` — the condition is a Bool and the else IS the early
    /// return, so its value must fit the fn's return channel.
    fn check_stmt_guard(&mut self, cond: &mut ast::Expr, else_: &mut ast::Expr) {
        let cty = self.infer_expr(cond);
        self.constrain(Ty::Bool, cty, "guard condition");
        let ety = self.infer_expr(else_);
        // ADR-0021: inside a lambda the else returns into the lambda's channel.
        self.record_lambda_returned_err(&ety, matches!(else_.kind, ast::ExprKind::Ok { .. }), else_.span);
        // #1118: the else type used to be unconstrained, and
        // `guard x > 0 else "nope"` in a `-> Int` fn passed check then died as
        // rustc E0308 behind the codegen wall. Exempt: loop control
        // (continue/break), a diverging Never else (process.exit), and lambda
        // bodies (the lambda's own return type is not tracked in current_ret).
        // #3115: a block else that ENDS in continue/break leaves the loop the
        // same way the bare jump does; typing it against the fn's return
        // refused `guard c else { log(); continue }` in any non-Unit fn.
        if ends_in_loop_ctl(else_) || self.env.lambda_depth != 0 {
            return;
        }
        let Some(ret) = self.env.current_ret.clone() else { return };
        let er = resolve_ty(&ety, &self.uf);
        if er == Ty::Never {
            return;
        }
        // #2182: the else value flows out through the fn's lifted channel
        // exactly like a tail value — the same explicit-`!` rule applies.
        if self.env.auto_unwrap {
            if ret.is_result() {
                self.unqueue_implicit_prop_leaves(else_);
            } else {
                let must_use = resolve_ty(&ret, &self.uf) == Ty::Unit;
                self.queue_implicit_prop_leaves(else_, "of this guard's else value", must_use);
            }
        }
        self.constrain_guard_else(ret, ety, er);
    }

    /// Constrain a guard's `else` value against the fn's return channel.
    ///
    /// An effect fn with an UNLIFTED return type may return through the lifted
    /// Result channel (`else err(..)`) or with a plain value — the same rule
    /// `constrain_effect_body` applies. Every other fn constrains directly.
    fn constrain_guard_else(&mut self, ret: Ty, ety: Ty, er: Ty) {
        let lifted_effect = self.env.can_call_effect
            && !matches!(ret, Ty::Applied(crate::types::TypeConstructorId::Result, _));
        if !lifted_effect {
            self.constrain(ret, ety, "guard else".to_string());
            return;
        }
        match er {
            Ty::Applied(crate::types::TypeConstructorId::Result, ref args) if !args.is_empty() => {
                self.constrain(ret, args[0].clone(), "guard else".to_string())
            }
            // An empty-arg Result carries nothing to constrain.
            Ty::Applied(crate::types::TypeConstructorId::Result, _) => {}
            // A Unit else is a plain value like any other: it returns `()`
            // from the fn, so it fits only a Unit return (#3042 — `guard c
            // else ()` in a `-> Int` effect fn passed check, then died as
            // rustc E0308; a pure fn already rejected it here).
            _ => self.constrain(ret, ety, "guard else".to_string()),
        }
    }

    /// `ast::Stmt::Let` arm of [`Self::check_stmt`]. Verbatim text move.
    /// #2653: an annotated binding whose value is a CALL carries the call's
    /// span, so a `Result[T, _]` vs `T` mismatch can name the missing `!` and
    /// place it — the guidance E005 gives `f(g())` and E041 gives `let x = g()`.
    fn let_call_fix_hint(&self, value: &ast::Expr) -> Option<super::types::FixHint> {
        let ast::ExprKind::Call { .. } = &value.kind else { return None };
        Some(super::types::FixHint::LetCallValue {
            span: value.span?,
            can_propagate: self.env.auto_unwrap && self.env.lambda_depth == 0,
        })
    }

    fn check_stmt_let(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Let { name, ty, value, span } = stmt else { unreachable!() };
        // #2704: `let f: effect (A) -> B = (x) => …` is an effect slot like a
        // call argument or a declared return (#1055 / #2588): the lambda gets
        // effect-fn body ergonomics. Only the effect form is armed, so a
        // pure annotated lambda infers exactly as before.
        if let Some(te) = ty.as_ref() {
            let declared = self.resolve_type_expr(te);
            if matches!(resolve_ty(&declared, &self.uf), Ty::Fn { is_effect: true, .. }) {
                self.expect_lambda(value, &declared);
            }
        }
        let val_ty = self.infer_expr(value);
        // #3274: resolved BEFORE `define_var`, so `let g = g` reads the outer `g`.
        let effect_target = self.effect_alias_target(value, ty.as_ref());
        let final_ty = if let Some(te) = ty {
            let declared = self.resolve_type_expr(te);
            // E029: an undeclared Named in the annotation compiles to a
            // nonexistent Rust type after `check` accepted (fuzz index 940).
            self.deferred_unknown_type_checks.push((
                declared.clone(), *span, format!("let '{}'", name),
            ));
            self.record_int_literal_context(value, &declared);
            // #867: the numeric-width direction of this annotation is
            // re-checked post-solve (the solver joins widths symmetrically).
            self.deferred_numeric_narrowing_checks.push(super::NumericNarrowingSite {
                expected: declared.clone(), actual: val_ty.clone(),
                context: format!("let '{}'", name), span: value.span,
            });
            let call_hint = self.let_call_fix_hint(value);
            self.constrain_with_hint(declared.clone(), val_ty, format!("let {}", name), call_hint);
            declared
        } else {
            let t = resolve_ty(&val_ty, &self.uf);
            // ADR-0008 (#1123 N+1): a Result on an un-annotated binding is an
            // E041 error unless the binding legitimately KEEPS the Result —
            // matched on ok/err later, a ctor-shaped RHS, or the sanctioned
            // discard `let _ = f()` (D2's second spelling: the Result binds
            // dead, nothing propagates, no error).
            let unwrapped = self.effect_unwrap_rhs_warned(t, value, "of this binding's value", matches!(value.kind, ast::ExprKind::Call { .. }), name == "_"
                || self.env.skip_auto_unwrap_for.contains(&sym(name))
                || Self::rhs_keeps_result_shape(value));
            // #662: an un-annotated binding whose value type carries an
            // unconstrained phantom slot (only an un-exercised branch
            // could pin it) is undecidable — re-check post-solve.
            self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                ty: unwrapped.clone(), name: Some(name.to_string()), span: value.span,
            });
            unwrapped
        };
        if let Some(s) = span {
            self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
        }
        self.check_collection_element_types(&final_ty);
        self.env.define_var(name, final_ty);
        // #2097: after `define_var` — it clears the origin of any binding it
        // shadows, and the `let`'s own value span goes in on top of that.
        if let Some(vs) = value.span {
            self.env.record_let_origin(name, vs);
        }
        if let Some(target) = effect_target {
            self.env.record_effect_alias(name, target);
        }
    }

    /// #3274: the effect fn a `let`/`var` binds when its value is a bare
    /// effect fn reference — unless the annotation is itself an
    /// `effect (A) -> B` type, whose bit the binding's type already carries
    /// (calls of it are E006-checked by `call_fn_typed_local`).
    pub(crate) fn effect_alias_target(&mut self, value: &ast::Expr, ty: Option<&ast::TypeExpr>) -> Option<Sym> {
        let target = self.effect_fn_value_target(value)?;
        let declared_effect = ty.is_some_and(|te| {
            let declared = self.resolve_type_expr(te);
            matches!(resolve_ty(&declared, &self.uf), Ty::Fn { is_effect: true, .. })
        });
        if declared_effect { None } else { Some(target) }
    }

    /// #3274: fill `top_effect_aliases` for this program's top-level `let`s
    /// before any body is checked (a fn may sit above the `let` it calls
    /// through), returning the map it replaces. Iterated to a fixed point so
    /// `let b = a` resolves whichever of `a`/`b` is declared first.
    pub(crate) fn collect_top_effect_aliases(&mut self, decls: &[ast::Decl]) -> std::collections::HashMap<Sym, Sym> {
        let saved = std::mem::take(&mut self.env.top_effect_aliases);
        loop {
            let before = self.env.top_effect_aliases.len();
            for decl in decls {
                let ast::Decl::TopLet { name, ty, value, .. } = decl else { continue };
                if self.env.top_effect_aliases.contains_key(name) {
                    continue;
                }
                if let Some(target) = self.effect_alias_target(value, ty.as_ref()) {
                    self.env.top_effect_aliases.insert(*name, target);
                }
            }
            if self.env.top_effect_aliases.len() == before {
                return saved;
            }
        }
    }

    /// `ast::Stmt::Var` arm of [`Self::check_stmt`]. Verbatim text move.
    fn check_stmt_var(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Var { name, ty, value, span } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
        let effect_target = self.effect_alias_target(value, ty.as_ref());
        let final_ty = if let Some(te) = ty {
            let declared = self.resolve_type_expr(te);
            // E029: same undeclared-Named annotation check as Let.
            self.deferred_unknown_type_checks.push((
                declared.clone(), *span, format!("var '{}'", name),
            ));
            self.record_int_literal_context(value, &declared);
            // #867: same directional annotation re-check as Let.
            self.deferred_numeric_narrowing_checks.push(super::NumericNarrowingSite {
                expected: declared.clone(), actual: val_ty.clone(),
                context: format!("var '{}'", name), span: value.span,
            });
            let call_hint = self.let_call_fix_hint(value);
            self.constrain_with_hint(declared.clone(), val_ty, format!("let {}", name), call_hint);
            declared
        } else {
            let t = resolve_ty(&val_ty, &self.uf);
            // Same rule as Let, including the usage-skip and the `_` discard.
            let unwrapped = self.effect_unwrap_rhs_warned(t, value, "of this binding's value", matches!(value.kind, ast::ExprKind::Call { .. }), name == "_"
                || self.env.skip_auto_unwrap_for.contains(&sym(name))
                || Self::rhs_keeps_result_shape(value));
            // #662: same undecidable-phantom-slot re-check as Let.
            self.deferred_unresolved_binding_checks.push(super::UnresolvedBindingSite {
                ty: unwrapped.clone(), name: Some(name.to_string()), span: value.span,
            });
            unwrapped
        };
        if let Some(s) = span {
            self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
        }
        self.check_collection_element_types(&final_ty);
        self.env.define_var(name, final_ty);
        self.env.mutable_vars.insert(sym(name));
        self.env.var_lambda_depth.insert(sym(name), self.env.lambda_depth);
        if let Some(target) = effect_target {
            self.env.record_effect_alias(name, target);
        }
    }

    /// `ast::Stmt::Assign` arm of [`Self::check_stmt`]: the Unit-mutator
    /// misuse diagnostic (E001), value/target unification, the
    /// immutable-binding reassignment diagnostic (E009), and the
    /// pure-fn-closure escape-analysis diagnostic (E011). Verbatim text move.
    fn check_stmt_assign(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Assign { name, value, .. } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
        // #3274: `var g = pure_fn; g = rd` — the var now may hold `rd`.
        if let Some(target) = self.effect_fn_value_target(value)
            && self.env.lookup_var(name).is_some_and(|t| !matches!(resolve_ty(t, &self.uf), Ty::Fn { is_effect: true, .. }))
        {
            self.env.record_effect_alias(name, target);
        }
        self.check_stmt_assign_unify(name, &val_ty, value);
        self.check_stmt_assign_immutable(name);
        self.check_closure_escape(name.as_str(), format!("{} = ...", name));
    }

    /// A mut-receiver stdlib mutator (`list.push`, `map.insert`,
    /// `string.push`, …) returns Unit and mutates in place. Writing
    /// `b = list.push(b, x)` therefore assigns Unit to a non-Unit
    /// binding. Native catches this at rustc (E0308 "expected Vec,
    /// found ()"); WASM erases Unit and silently RUNS the program —
    /// a cross-target asymmetry (compiles on one target, not the
    /// other). Reject it in the checker so BOTH targets agree (E001), with
    /// the fix spelled out: drop the assignment (the call already
    /// mutates) or rebuild a fresh value. Otherwise, unify the assigned
    /// value's type with the variable's declared type. Verbatim text move
    /// out of [`Self::check_stmt_assign`].
    fn check_stmt_assign_unify(&mut self, name: &Sym, val_ty: &Ty, value: &ast::Expr) {
        if let Some(var_ty) = self.assign_target_ty(name) {
            self.unify_assigned_value(name.as_str(), var_ty, val_ty, value);
        }
    }

    /// The declared type of an assignment target: a local binding
    /// (`lookup_var`) OR a module-level `var` (`top_lets`) — both are valid
    /// assignment targets and both carry a concrete declared type to flow
    /// into the value.
    fn assign_target_ty(&self, name: &Sym) -> Option<Ty> {
        self.env.lookup_var(name).cloned()
            .or_else(|| self.env.top_lets.get(&sym(name)).cloned())
    }

    /// The type of the record or container a nested target writes into:
    /// the root binding's type walked through `path`'s fields (#3064 —
    /// `o.inner.xs = v` writes into `o.inner`). `None` when the root is not
    /// a binding or a step names no field of its record.
    fn place_ty(&mut self, target: &Sym, path: &[Sym]) -> Option<Ty> {
        let (mut ty, path) = match self.module_place(target, path.first()) {
            Some(key) => (self.env.top_lets.get(&key)?.clone(), &path[1..]),
            None => (self.assign_target_ty(target)?, path),
        };
        for step in path {
            let next = self.resolve_field_type(&ty, step.as_str());
            if matches!(resolve_ty(&next, &self.uf), Ty::Unknown) {
                self.report_assign_missing_field(&ty, step);
                return None;
            }
            ty = next;
        }
        Some(ty)
    }

    /// E013 for an assignment through a field a closed record does not have
    /// (`s.nope = v`, `o.nope.xs = v`) — the read `s.nope` was already E013;
    /// the write passed check and failed in codegen.
    fn report_assign_missing_field(&mut self, obj_ty: &Ty, field: &Sym) {
        let concrete = resolve_ty(obj_ty, &self.uf);
        let Ty::Record { fields } = self.env.resolve_named(&concrete) else { return };
        let available = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        let hint = match almide_base::diagnostic::suggest(field, fields.iter().map(|(n, _)| n.as_str())) {
            Some(close) => format!("Did you mean `{}`? Available fields: {}", close, available),
            None => format!("Available fields: {}", available),
        };
        self.emit(super::err(
            format!("no field '{}' on {}", field, concrete.display()),
            hint,
            format!("assignment to .{}", field),
        ).with_code("E013"));
    }

    /// `(.p)*` spelled for a diagnostic label.
    fn place_label(target: &Sym, path: &[Sym]) -> String {
        std::iter::once(target.as_str().to_string())
            .chain(path.iter().map(|p| p.as_str().to_string()))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// `s.f = v` (#3051): the field's declared type is the expected type of
    /// the value, exactly as a variable's is for `x = v` and an annotation's
    /// for `let x: T = v`. Before, the value was inferred with no context, so
    /// `s.xs = []` was an undecidable empty literal (E018) and a mismatched
    /// value (`s.n = "x"`) passed check and failed as rustc E0308 natively.
    fn check_stmt_field_assign(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::FieldAssign { target, path, field, value, .. } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
        let shape = format!("{}.{} = ...", Self::place_label(target, path), field);
        // `m.x = v`: the whole of another module's top-level binding (#3312).
        if path.is_empty() && let Some(key) = self.module_place(target, Some(field)) {
            self.check_module_place_mutable(target, field, key, shape);
            if let Some(var_ty) = self.env.top_lets.get(&key).cloned() {
                self.unify_assigned_value(&format!("{}.{}", target, field), var_ty, &val_ty, value);
            }
            return;
        }
        self.check_place_root_mutable(target, path, shape);
        let Some(obj_ty) = self.place_ty(target, path) else { return };
        let field_ty = self.resolve_field_type(&obj_ty, field.as_str());
        if matches!(resolve_ty(&field_ty, &self.uf), Ty::Unknown) {
            self.report_assign_missing_field(&obj_ty, field);
            return;
        }
        let label = format!("{}.{}", Self::place_label(target, path), field);
        self.unify_assigned_value(&label, field_ty, &val_ty, value);
    }

    /// `xs[i] = v` / `m[k] = v` (#3051): the container's element type is the
    /// expected type of the value, and a Map's key type that of the index.
    /// `key` and `val` are each an expression with its inferred type.
    fn unify_index_assign(&mut self, target: &Sym, path: &[Sym], key: (&ast::Expr, Ty), val: (&ast::Expr, &Ty)) {
        let ((index, idx_ty), (value, val_ty)) = (key, val);
        let Some(container) = self.place_ty(target, path) else { return };
        let resolved = self.env.resolve_named(&resolve_ty(&container, &self.uf));
        let label = format!("{}[...]", Self::place_label(target, path));
        match &resolved {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
                self.unify_assigned_value(&label, args[0].clone(), val_ty, value);
            }
            Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
                // #3185: the key is a value position of the key type too.
                self.record_int_literal_context(index, &args[0]);
                self.constrain(args[0].clone(), idx_ty, format!("key of {}", label));
                self.unify_assigned_value(&label, args[1].clone(), val_ty, value);
            }
            _ => {}
        }
    }

    /// Flow an assignment target's type (`var_ty`) into the assigned value.
    /// `name` spells the target in diagnostics (`x`, `s.f`, `xs[...]`).
    /// One rule for every target (#3063): an un-banged fallible call on the
    /// right is E041 whether the target is `x`, `s.f` or `xs[i]` — ADR-0008's
    /// explicit propagation, which the untyped field and index positions had
    /// silently escaped.
    fn unify_assigned_value(&mut self, name: &str, var_ty: Ty, val_ty: &Ty, value: &ast::Expr) {
        let var_ty = &var_ty;
        // #3185: the target's type narrows the value's literals in lowering,
        // so they face its range check — `v = 1000` into a `UInt8` (local,
        // module `var`, field or element) was accepted and printed 1000.
        self.record_int_literal_context(value, var_ty);
        let val_resolved = resolve_ty(val_ty, &self.uf);
        let var_resolved = self.env.resolve_named(var_ty);
        if matches!(val_resolved, Ty::Unit) && !matches!(var_resolved, Ty::Unit | Ty::Unknown) {
            // Rebuild form is type-directed: a List concatenates a
            // singleton, a String appends a suffix string. Other
            // collections (Map/Set/Bytes) have no `+` rebuild, so we
            // steer toward the statement form only.
            let rebuild = match &var_resolved {
                Ty::Applied(TypeConstructorId::List, _) => Some(format!("{0} = {0} + [<item>]", name)),
                Ty::String => Some(format!("{0} = {0} + \"<suffix>\"", name)),
                _ => None,
            };
            let snippet = match &rebuild {
                Some(rb) => format!(
                    "// the mutator already updates '{n}' in place — drop the `{n} =` and call it as a statement:\n<mutator>({n}, ...)\n// or rebuild a fresh value:\n{rb}",
                    n = name,
                ),
                None => format!(
                    "// the mutator already updates '{n}' in place — drop the `{n} =` and call it as a statement:\n<mutator>({n}, ...)",
                    n = name,
                ),
            };
            let hint = match &rebuild {
                Some(rb) => format!(
                    "the right-hand side returns Unit (an in-place mutator). Call it as a \
                     statement instead of assigning its result, or rebuild '{}' with a \
                     value-returning expression like `{}`",
                    name, rb
                ),
                None => format!(
                    "the right-hand side returns Unit (an in-place mutator). Call it as a \
                     statement instead of assigning its result — '{}' is already mutated in place",
                    name
                ),
            };
            self.emit(super::err(
                format!("cannot assign a Unit value to '{}'", name),
                hint,
                format!("{} = ...", name),
            ).with_code("E001").with_try(snippet));
        } else {
            // Unify the assigned value's type with the variable's
            // declared type. The variable already carries a concrete
            // type from its `var`/`let` declaration; flowing it into
            // the value pins an otherwise-unconstrained element — e.g.
            // `items = []` for `var items: List[Int]` resolves `[]`'s
            // element to `Int` (it is the source of truth, exactly as
            // a typed `let` binding is). Without this, an empty literal
            // assigned to a typed var stays undecidable (E018).
            //
            // #485: apply the same effect-fn auto-unwrap rule as
            // let/var first — `x = step(x)` with x: Int unwraps the
            // lifted Result[Int, E]; a Result-typed target keeps it.
            // Only substitute when the unwrap actually fires, so an
            // unresolved TypeVar RHS keeps flowing through inference.
            // The `!` insertion is machine-applicable for a plain call — the
            // same rule `let x = f()` follows — so `almide fix` migrates
            // `x = f()`, `s.f = f()` and `xs[i] = f()` alike (#3063).
            let mechanical = matches!(value.kind, ast::ExprKind::Call { .. });
            let unwrapped = self.effect_unwrap_rhs_warned(val_resolved.clone(), value, "of this assignment's value", mechanical, var_resolved.is_result());
            let constrain_val = if unwrapped != val_resolved { unwrapped } else { val_ty.clone() };
            self.constrain(var_ty.clone(), constrain_val, format!("assign {}", name));
        }
    }

    /// E009: reassigning an immutable `let` binding (or a function
    /// parameter). Verbatim text move out of [`Self::check_stmt_assign`].
    fn check_stmt_assign_immutable(&mut self, name: &Sym) {
        let known_top_let = self.env.top_lets.contains_key(&sym(name))
            || self
                .current_module_prefix
                .as_deref()
                .is_some_and(|pfx| self.env.top_lets.contains_key(&sym(&format!("{}.{}", pfx, name))));
        // A MODULE-LEVEL `let` lives in env.top_lets, not the var scopes, so
        // the local lookup below never saw it and reassignment from a fn body
        // passed silently (diagnostic sweep 2026-08-18; a `var` top-let is in
        // mutable_vars via check_decl_top_let and stays assignable).
        if self.env.lookup_var(name).is_none()
            && known_top_let
            && !self.env.mutable_vars.contains(&sym(name))
        {
            let mut diag = crate::check::err(
                format!("cannot reassign immutable binding '{}'", name),
                format!("'{0}' is a module-level `let`. Declare it `var {0} = ...` to make it assignable", name),
                format!("{} = ...", name),
            ).with_code("E009");
            if let Some(&(line, col)) = self.env.var_decl_locs.get(&sym(name)) {
                diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", name));
            }
            self.emit(diag);
            return;
        }
        // No binding at all: the target is neither a local nor a top-let.
        // Silent before (and the lowering's error-recovery VarId(0) would
        // alias the first allocated local), reachable for any spelling now
        // that uppercase targets parse as assignments.
        if self.env.lookup_var(name).is_none() && !known_top_let {
            let hint = if self.env.types.contains_key(&sym(name)) {
                format!("'{}' names a TYPE — a type is not an assignable binding. Declare a variable: `var {0}_v = ...`", name)
            } else {
                format!("No `let`/`var` named '{}' is in scope to assign to. Declare it first: `var {0} = ...`", name)
            };
            self.emit(crate::check::err(
                format!("cannot assign to undefined binding '{}'", name),
                hint,
                format!("{} = ...", name),
            ).with_code("E003"));
            return;
        }
        if self.env.lookup_var(name).is_some() && !self.env.mutable_vars.contains(&sym(name)) {
            let is_param = self.env.param_vars.contains(&sym(name));
            let hint = if is_param {
                format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
            } else {
                format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
            };
            let snippet = if is_param {
                format!("// '{n}' is a parameter — make a mutable copy:\nvar {n}_ = {n}\n// ...then reassign {n}_ instead of {n}", n = name)
            } else {
                format!("// let {n} = ...  →  var {n} = ...\nvar {n} = <initial value>", n = name)
            };
            let mut diag = super::err(
                format!("cannot reassign immutable binding '{}'", name),
                hint, format!("{} = ...", name)).with_code("E009").with_try(snippet);
            if let Some(&(line, col)) = self.env.var_decl_locs.get(&sym(name)) {
                diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", name));
            }
            self.emit(diag);
        }
    }

    /// `ast::Stmt::IndexAssign` arm of [`Self::check_stmt`]. Verbatim text move.
    /// E009: a place write (`xs[i] = v`, `s.f = v`, `o.inner.xs = v`)
    /// mutates its ROOT binding, which must be a `var` (or a `mut` param).
    /// A module-level `let g` is immutable just like a local `let` —
    /// `lookup_var` only sees locals, so without the `top_lets` arm a global
    /// `let g; g[2]=…` slipped past this check and only failed later as
    /// opaque rustc `E0425`. A field write on a `let` passed check the same
    /// way and failed natively as rustc E0594 (#3064).
    fn check_place_root_mutable(&mut self, target: &Sym, path: &[Sym], shape: String) {
        if let Some(first) = path.first()
            && let Some(key) = self.module_place(target, Some(first))
        {
            return self.check_module_place_mutable(target, first, key, shape);
        }
        self.check_closure_escape(target.as_str(), shape.clone());
        let is_known_binding = self.env.lookup_var(target.as_str()).is_some()
            || self.env.top_lets.contains_key(&sym(target.as_str()));
        if is_known_binding && !self.env.mutable_vars.contains(target) {
            let mut diag = super::err(
                format!("cannot mutate immutable binding '{}'", target),
                format!("Use 'var {} = ...' to declare a mutable variable", target),
                shape).with_code("E009");
            if let Some(&(line, col)) = self.env.var_decl_locs.get(target) {
                diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", target));
            }
            self.emit(diag);
        }
    }

    fn check_stmt_index_assign(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::IndexAssign { target, path, index, value, .. } = stmt else { unreachable!() };
        let idx_ty = self.infer_expr(index);
        let val_ty = self.infer_expr(value);
        self.unify_index_assign(target, path, (index, idx_ty), (value, &val_ty));
        let shape = format!("{}[...] = ...", Self::place_label(target, path));
        self.check_place_root_mutable(target, path, shape);
    }

    /// `ast::Stmt::GuardLet` arm of [`Self::check_stmt`]: Swift-style
    /// `guard let` binding of the Option/Result payload for the rest of
    /// the block. Verbatim text move.
    fn check_stmt_guard_let(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::GuardLet { name, scrutinee, else_, .. } = stmt else { unreachable!() };
        // Swift-style: bind `name` to the value inside the scrutinee's
        // Option/Result for the REST of the block (define_var in the current
        // block scope persists across the following stmts). The else branch
        // diverges; lowering desugars the block tail into a Some/Ok match.
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
                    format!("`guard let` requires an Option or Result, found `{}`", other.display()),
                    "bind the inner value of an Option/Result: `guard let v = some_option else { return }`".to_string(),
                    "guard let scrutinee".to_string(),
                ).with_code("E001"));
                Ty::Unknown
            }
        };
        let ety = self.infer_expr(else_);
        // ADR-0021: inside a lambda the else returns into the lambda's channel.
        self.record_lambda_returned_err(&ety, matches!(else_.kind, ast::ExprKind::Ok { .. }), else_.span);
        self.env.define_var(name, bound_ty);
    }

}

include!("infer_patterns.rs");
include!("infer_module_place.rs");
include!("infer_closure_escape.rs");

impl Checker {

    /// Infer the result type of the + operator (numeric add or string/list concat).
    fn infer_plus_op(&mut self, lc: &Ty, rc: &Ty, lt: Ty, left: &ast::Expr, right: &ast::Expr) -> Ty {
        if let Some(t) = self.infer_plus_op_concat(lc, rc, &lt) {
            return t;
        }
        // Matrix addition
        if *lc == Ty::Matrix || *rc == Ty::Matrix {
            return Ty::Matrix;
        }
        self.infer_plus_op_numeric_check(lc, rc);
        if let Some(t) = self.infer_plus_op_sized(lc, rc, left, right) {
            return t;
        }
        if *lc == Ty::Float || *rc == Ty::Float { Ty::Float } else { lt }
    }

    /// String/List concatenation guard of [`Self::infer_plus_op`]: unify a
    /// TypeVar/Unknown side with the other side's String/List type, and pin
    /// List element types when concatenating two Lists. `Some` means this
    /// rule applied and the caller should return that type immediately.
    /// Verbatim text move.
    fn infer_plus_op_concat(&mut self, lc: &Ty, rc: &Ty, lt: &Ty) -> Option<Ty> {
        let is_concat_ty = |t: &Ty| matches!(t, Ty::String | Ty::Applied(TypeConstructorId::List, _));
        let is_unknown_ty = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
        // When one side is List and the other is TypeVar, unify the TypeVar with the List type
        if is_unknown_ty(lc) && is_concat_ty(rc) {
            self.unify_infer(lt, rc);
            let resolved_lt = resolve_ty(lt, &self.uf);
            // Now unify element types if both resolved to List
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (&resolved_lt, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return Some(resolve_ty(lt, &self.uf));
        }
        if (is_concat_ty(lc) && (is_concat_ty(rc) || is_unknown_ty(rc)))
            || (is_concat_ty(rc) && is_unknown_ty(lc)) {
            // Unify element types for list concatenation: List[?0] + List[Int] → ?0 = Int.
            //
            // `constrain`, not `unify_infer`: the latter binds inference variables and stays
            // SILENT when both sides are concrete and different, so `[1] + ["a"]` type-checked
            // and then emitted invalid Rust natively / printed the String block's ADDRESS as an
            // Int element on wasm (#1030). The list LITERAL path already reports this exact
            // mismatch as E001 (`[1, "a"]`), so the same property of the same language was
            // checked on one path and not the other. `constrain` still unifies, so the
            // inference-variable cases this rule exists for are unaffected — it additionally
            // REPORTS when unification is impossible.
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (lc, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.constrain(le.clone(), re.clone(), "list concatenation element");
                }
            }
            return Some(resolve_ty(lt, &self.uf));
        }
        None
    }

    /// Numeric-operand diagnostic guard of [`Self::infer_plus_op`] — emits
    /// E-diagnostic only; the caller falls through regardless. Verbatim
    /// text move.
    fn infer_plus_op_numeric_check(&mut self, lc: &Ty, rc: &Ty) {
        // Sized Numeric Types (Stage 1c): arithmetic accepts canonical
        // `Int` / `Float` plus every sized variant. Same-type pairing is
        // enforced below; mixing widths is an explicit conversion.
        let is_numeric = |t: &Ty| matches!(
            t,
            Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_)
                | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
                | Ty::Matrix | Ty::Named(..)
        );
        if !is_numeric(lc) || !is_numeric(rc) {
            // A Result operand has a specific way out — the generic "use
            // numeric types" hint never mentioned the unwrap operators, so
            // the one real fix was undiscoverable (#1050).
            let hint = if lc.is_result() || rc.is_result() {
                "Unwrap the Result operand first: `!` propagates the error (effect fn body), \
                 `?? fallback` supplies a default, or `match` handles ok/err"
            } else {
                "Use + with numeric types, String, or List"
            };
            self.emit(super::err(
                format!("operator '+' requires numeric, String, or List types but got {} and {}", lc.display(), rc.display()),
                hint, format!("operator +")));
        }
    }

    /// Sized-numeric-type guard of [`Self::infer_plus_op`]: reject mixed
    /// widths (E-diagnostic + return the left type), or return the common
    /// sized type when both sides are sized and compatible. Verbatim text
    /// move.
    fn infer_plus_op_sized(&mut self, lc: &Ty, rc: &Ty, left: &ast::Expr, right: &ast::Expr) -> Option<Ty> {
        // Result type resolution:
        //   - Same sized type on both sides → that sized type.
        //   - Canonical Float promotes Int mixes to Float (legacy rule).
        //   - Mixed sized widths are rejected; the diagnostic is
        //     emitted by `compatible` / `unify_infer` callers, so here
        //     we just fall through with `lt` to avoid an extra error.
        let is_sized_scalar = |t: &Ty| matches!(
            t,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
        );
        // Sized Numeric Types (Stage 1c): both sides sized AND widths
        // differ is a type error. The permissive `Ty::Int` / `Ty::Float`
        // canonical pair stays (it carries the literal-coercion slot for
        // `let x: Int32 = 42` style bindings). Mixing `Int32` and `Int16`
        // has no such cover — it's always wrong, always needs explicit
        // `.to_intN()`.
        // A sized operand meeting a canonical `Int`/`Float` VALUE is the same
        // mistake with the wide side spelled differently (#902).
        if let Some(t) = self.check_mixed_canonical_width("+", lc, rc, left, right) {
            return Some(t);
        }
        if is_sized_scalar(lc) && is_sized_scalar(rc) && lc != rc {
            self.emit(super::err(
                format!(
                    "operator '+' mixes sized numeric types {} and {} — \
                     explicit conversion required (e.g. `.to_{}()`)",
                    lc.display(), rc.display(),
                    lc.display().to_lowercase()),
                "Convert one side with `.to_intN()` / `.to_floatN()` before the op",
                format!("operator +")));
            return Some(lc.clone());
        }
        if lc.compatible(rc) && is_sized_scalar(lc) {
            return Some(lc.clone());
        }
        None
    }
}

/// Does this guard else leave the enclosing LOOP — a bare `continue` /
/// `break`, or a block whose last step is one (#3115)? Such an else is not
/// the fn's return value, so it is not typed against the return type.
fn ends_in_loop_ctl(e: &ast::Expr) -> bool {
    match &e.kind {
        ast::ExprKind::Break | ast::ExprKind::Continue => true,
        ast::ExprKind::Block { expr: Some(tail), .. } => ends_in_loop_ctl(tail),
        ast::ExprKind::Block { stmts, expr: None } => matches!(
            stmts.last(),
            Some(ast::Stmt::Expr { expr, .. }) if ends_in_loop_ctl(expr)
        ),
        _ => false,
    }
}
