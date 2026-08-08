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
            ast::Stmt::FieldAssign { value, .. } => { self.infer_expr(value); }
            ast::Stmt::Guard { cond, else_, .. } => self.check_stmt_guard(cond, else_),
            ast::Stmt::GuardLet { .. } => self.check_stmt_guard_let(stmt),
            ast::Stmt::Expr { expr, .. } => {
                let t = self.infer_expr(expr);
                // ADR-0008 D2 (#1123 N+1): a discarded Result in statement
                // position is the must-use error E042 — in EVERY fn kind, not
                // only the old auto-? contexts. Queue unconditionally;
                // post-solve keeps only Result-typed sites. The `!` insertion
                // hint stays mechanical for a plain call.
                self.deferred_implicit_prop_checks.push((
                    t.clone(), expr.span, "of this statement's result",
                    matches!(expr.kind, ast::ExprKind::Call { .. }), true,
                ));
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
        // #1118: the else type used to be unconstrained, and
        // `guard x > 0 else "nope"` in a `-> Int` fn passed check then died as
        // rustc E0308 behind the codegen wall. Exempt: loop control
        // (continue/break), a diverging Never else (process.exit), and lambda
        // bodies (the lambda's own return type is not tracked in current_ret).
        let is_loop_ctl = matches!(else_.kind, ast::ExprKind::Break | ast::ExprKind::Continue);
        if is_loop_ctl || self.env.lambda_depth != 0 {
            return;
        }
        let Some(ret) = self.env.current_ret.clone() else { return };
        let er = resolve_ty(&ety, &self.uf);
        if er == Ty::Never {
            return;
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
            // An empty-arg Result and a Unit else both carry nothing to constrain.
            Ty::Applied(crate::types::TypeConstructorId::Result, _) | Ty::Unit => {}
            _ => self.constrain(ret, ety, "guard else".to_string()),
        }
    }

    /// `ast::Stmt::Let` arm of [`Self::check_stmt`]. Verbatim text move.
    fn check_stmt_let(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Let { name, ty, value, span } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
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
            self.constrain(declared.clone(), val_ty, format!("let {}", name));
            declared
        } else {
            let t = resolve_ty(&val_ty, &self.uf);
            // ADR-0008 (#1123 N+1): a Result on an un-annotated binding is an
            // E041 error unless the binding legitimately KEEPS the Result —
            // matched on ok/err later, a ctor-shaped RHS, or the sanctioned
            // discard `let _ = f()` (D2's second spelling: the Result binds
            // dead, nothing propagates, no error).
            let unwrapped = self.effect_unwrap_rhs_warned(t, value.span, "of this binding's value", matches!(value.kind, ast::ExprKind::Call { .. }), name == "_"
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
    }

    /// `ast::Stmt::Var` arm of [`Self::check_stmt`]. Verbatim text move.
    fn check_stmt_var(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Var { name, ty, value, span } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
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
            self.constrain(declared.clone(), val_ty, format!("let {}", name));
            declared
        } else {
            let t = resolve_ty(&val_ty, &self.uf);
            // Same rule as Let, including the usage-skip and the `_` discard.
            let unwrapped = self.effect_unwrap_rhs_warned(t, value.span, "of this binding's value", matches!(value.kind, ast::ExprKind::Call { .. }), name == "_"
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
    }

    /// `ast::Stmt::Assign` arm of [`Self::check_stmt`]: the Unit-mutator
    /// misuse diagnostic (E001), value/target unification, the
    /// immutable-binding reassignment diagnostic (E009), and the
    /// pure-fn-closure escape-analysis diagnostic (E011). Verbatim text move.
    fn check_stmt_assign(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::Assign { name, value, .. } = stmt else { unreachable!() };
        let val_ty = self.infer_expr(value);
        self.check_stmt_assign_unify(name, &val_ty, value.span);
        self.check_stmt_assign_immutable(name);
        self.check_stmt_assign_escape(name);
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
    fn check_stmt_assign_unify(&mut self, name: &Sym, val_ty: &Ty, value_span: Option<ast::Span>) {
        // A local binding (`lookup_var`) OR a module-level `var`
        // (`top_lets`) — both are valid assignment targets and both carry
        // a concrete declared type to flow into the value.
        let var_ty = self.env.lookup_var(name).cloned()
            .or_else(|| self.env.top_lets.get(&sym(name)).cloned());
        if let Some(var_ty) = &var_ty {
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
                let unwrapped = self.effect_unwrap_rhs_warned(val_resolved.clone(), value_span, "of this assignment's value", false, var_resolved.is_result());
                let constrain_val = if unwrapped != val_resolved { unwrapped } else { val_ty.clone() };
                self.constrain(var_ty.clone(), constrain_val, format!("assign {}", name));
            }
        }
    }

    /// E009: reassigning an immutable `let` binding (or a function
    /// parameter). Verbatim text move out of [`Self::check_stmt_assign`].
    fn check_stmt_assign_immutable(&mut self, name: &Sym) {
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

    /// E011: escape analysis — block `var` mutation inside a closure in a
    /// pure fn. Verbatim text move out of [`Self::check_stmt_assign`].
    fn check_stmt_assign_escape(&mut self, name: &Sym) {
        if self.env.mutable_vars.contains(&sym(name)) && !self.env.can_call_effect {
            if let Some(&decl_depth) = self.env.var_lambda_depth.get(&sym(name)) {
                if self.env.lambda_depth > decl_depth {
                    self.emit(super::err(
                        format!("mutable variable '{}' is mutated inside a closure in a pure function — use effect fn instead", name),
                        "Move the mutation out of the closure, or mark the enclosing function as `effect fn`",
                        format!("{} = ...", name)).with_code("E011"));
                }
            }
        }
    }

    /// `ast::Stmt::IndexAssign` arm of [`Self::check_stmt`]. Verbatim text move.
    fn check_stmt_index_assign(&mut self, stmt: &mut ast::Stmt) {
        let ast::Stmt::IndexAssign { target, index, value, .. } = stmt else { unreachable!() };
        self.infer_expr(index);
        self.infer_expr(value);
        // A module-level `let g` is immutable just like a local `let` — its
        // contents may not be index-assigned. `lookup_var` only sees locals,
        // so without the `top_lets` arm a global `let g; g[2]=…` slipped past
        // this check and only failed later as opaque rustc `E0425` (the
        // ModuleRc lowering never kicks in for a non-mutable global). Catch it
        // here with the same E009 locals get.
        let is_known_binding = self.env.lookup_var(target.as_str()).is_some()
            || self.env.top_lets.contains_key(&sym(target.as_str()));
        if is_known_binding && !self.env.mutable_vars.contains(target) {
            let mut diag = super::err(
                format!("cannot mutate immutable binding '{}'", target),
                format!("Use 'var {} = ...' to declare a mutable variable", target),
                format!("{}[...] = ...", target)).with_code("E009");
            if let Some(&(line, col)) = self.env.var_decl_locs.get(target) {
                diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", target));
            }
            self.emit(diag);
        }
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
        self.infer_expr(else_);
        self.env.define_var(name, bound_ty);
    }

    // ── Pattern binding ──

    pub(crate) fn bind_pattern(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        match pattern {
            ast::Pattern::Wildcard => {}
            ast::Pattern::Ident { name } => { self.env.define_var(name, ty.clone()); }
            ast::Pattern::Constructor { name, args } => self.bind_pattern_constructor(name, args, ty),
            ast::Pattern::RecordPattern { name, fields, .. } => self.bind_pattern_record(name, fields, ty),
            ast::Pattern::Tuple { elements } => self.bind_pattern_tuple(elements, ty),
            ast::Pattern::List { elements } => self.bind_pattern_list(elements, ty),
            ast::Pattern::Some { inner } => self.bind_pattern_some(inner, ty),
            ast::Pattern::Ok { inner } => self.bind_pattern_ok(inner, ty),
            ast::Pattern::Err { inner } => self.bind_pattern_err(inner, ty),
            ast::Pattern::None => {
                let resolved = resolve_ty(ty, &self.uf);
                self.reject_ctor_pattern_on_scalar("none", "an Option", &resolved);
            }
            ast::Pattern::Literal { .. } => {}
        }
    }

    /// `ast::Pattern::Constructor` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_constructor(&mut self, name: &Sym, args: &[ast::Pattern], ty: &Ty) {
        let resolved = self.env.resolve_named(ty);
        // Normalize module-qualified names: "binary.Unreachable" → "Unreachable"
        let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
        let payload_tys: Vec<Ty> = match &resolved {
            Ty::Variant { cases, .. } => cases.iter()
                .find(|c| c.name == bare_name)
                .map(|c| match &c.payload {
                    VariantPayload::Tuple(tys) => tys.clone(),
                    // #488: a paren pattern on a RECORD-payload case
                    // (`SetEmotion(_)`) bound nothing on native (rustc
                    // E0164) while wasm accepted it — reject with the
                    // brace spelling, both targets agree at check time.
                    VariantPayload::Record(_) => {
                        self.emit(super::err(
                            format!("case '{}' has named fields — use a record pattern", name),
                            format!("Match it as `{} {{ .. }}` (or bind fields: `{} {{ field }}`)", name, name),
                            format!("pattern {}(...)", name),
                        ).with_code("E021"));
                        vec![]
                    }
                    _ => vec![],
                })
                .unwrap_or_default(),
            // Opaque alias destructure: SafeHtml(s) → inner type
            Ty::Named(tname, _) => {
                if let Some(target) = self.env.opaque_alias_targets.get(tname).cloned() {
                    vec![target]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };
        for (i, arg) in args.iter().enumerate() {
            self.bind_pattern(arg, payload_tys.get(i).unwrap_or(&Ty::Unknown));
        }
    }

    /// `ast::Pattern::RecordPattern` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_record(&mut self, name: &Sym, fields: &[ast::FieldPattern], ty: &Ty) {
        let resolved = self.env.resolve_named(ty);
        // Normalize module-qualified names: "varlib.Circle" → "Circle" (mirrors
        // the Constructor arm above, and `lower_pattern`'s already-fixed #412
        // equivalent in `lower/statements.rs`). Without this a cross-module
        // record-variant pattern's case lookup below always misses (`c.name` is
        // the case's bare name), silently leaving every bound field `Ty::Unknown`
        // — which then leaks through `bind_pattern` into the arm body's `Ident`
        // inference and poisons the whole match expression's type (#412).
        let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
        let field_tys: Vec<(Sym, Ty)> = match &resolved {
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.clone(),
            Ty::Variant { cases, .. } => {
                // Find the specific case matching the pattern name
                cases.iter()
                    .find(|c| c.name == bare_name)
                    .and_then(|c| match &c.payload {
                        VariantPayload::Record(fs) => Some(fs.iter().map(|(n, t)| (*n, t.clone())).collect()),
                        _ => None,
                    })
                    .unwrap_or_default()
            }
            _ => vec![],
        };
        for f in fields {
            let ft = field_tys.iter().find(|(n, _)| *n == f.name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
            if let Some(ref p) = f.pattern { self.bind_pattern(p, &ft); }
            else { self.env.define_var(&f.name, ft); }
        }
    }

    /// `ast::Pattern::Tuple` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_tuple(&mut self, elements: &[ast::Pattern], ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        if let Ty::Tuple(tys) = &resolved {
            for (i, e) in elements.iter().enumerate() { self.bind_pattern(e, tys.get(i).unwrap_or(&Ty::Unknown)); }
        } else if super::types::is_inference_var(&resolved).is_some() {
            // Type is an unresolved inference var (e.g., lambda parameter).
            // Create fresh vars for each element and constrain: ?N = (?a, ?b, ...).
            // When the outer call context later resolves ?N, the element vars
            // get their correct types through the constraint chain.
            let elem_vars: Vec<Ty> = elements.iter().map(|_| self.fresh_var()).collect();
            self.constrain(resolved, Ty::Tuple(elem_vars.clone()), "tuple destructure");
            for (e, ev) in elements.iter().zip(elem_vars.iter()) { self.bind_pattern(e, ev); }
        } else {
            for e in elements { self.bind_pattern(e, &Ty::Unknown); }
        }
    }

    /// `ast::Pattern::List` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_list(&mut self, elements: &[ast::Pattern], ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        let elem_ty = match &resolved {
            Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].clone(),
            // Subject is still an unresolved inference var (e.g. a
            // lambda param not yet linked to its call site). Matching
            // `[..]` asserts it is a List, so pin its structure to
            // List[?elem] — otherwise the element pattern var
            // dead-ends at Unknown and leaks to the IR.
            Ty::TypeVar(_) => {
                let elem_v = self.fresh_var();
                self.unify_infer(&resolved, &Ty::list(elem_v.clone()));
                elem_v
            }
            _ => Ty::Unknown,
        };
        for e in elements { self.bind_pattern(e, &elem_ty); }
    }

    /// `ast::Pattern::Some` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_some(&mut self, inner: &ast::Pattern, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        let it = match &resolved {
            Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
            // See List arm: pin an unresolved subject to Option[?inner].
            Ty::TypeVar(_) => {
                let inner_v = self.fresh_var();
                self.unify_infer(&resolved, &Ty::option(inner_v.clone()));
                inner_v
            }
            _ => {
                self.reject_ctor_pattern_on_scalar("some(..)", "an Option", &resolved);
                Ty::Unknown
            }
        };
        self.bind_pattern(inner, &it);
    }

    /// A Option/Result CONSTRUCTOR pattern over a plain scalar subject is a
    /// type error, not a silent `Unknown` bind — silence here let the wrong
    /// mental model (`fan.bounded` "returns an Option") sail through to a
    /// backend wall (dojo budget-units, MSR round 3). The classic source is
    /// an effect fn's auto-`?`: the bound value is ALREADY the payload.
    fn reject_ctor_pattern_on_scalar(&mut self, ctor: &str, want: &str, resolved: &Ty) {
        if matches!(
            resolved,
            Ty::Int | Ty::Bool | Ty::Float | Ty::String
                | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                | Ty::Float32 | Ty::Float64
        ) {
            self.emit(super::err(
                format!(
                    "pattern `{ctor}` cannot match {} — the subject is not {want}",
                    resolved.display()
                ),
                format!(
                    "the value is already a plain {}. If it comes from an effect-fn call, \
                     auto-`?` has unwrapped it — use the value directly, or `?? <default>` \
                     on the producing call for a fallback",
                    resolved.display()
                ),
                "match pattern".to_string(),
            ));
        }
    }

    /// `ast::Pattern::Ok` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_ok(&mut self, inner: &ast::Pattern, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        let it = match &resolved {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            Ty::TypeVar(_) => {
                let ok_v = self.fresh_var();
                let err_v = self.fresh_var();
                self.unify_infer(&resolved, &Ty::result(ok_v.clone(), err_v));
                ok_v
            }
            _ => {
                self.reject_ctor_pattern_on_scalar("ok(..)", "a Result", &resolved);
                Ty::Unknown
            }
        };
        self.bind_pattern(inner, &it);
    }

    /// `ast::Pattern::Err` arm of [`Self::bind_pattern`]. Verbatim text move.
    fn bind_pattern_err(&mut self, inner: &ast::Pattern, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        let it = match &resolved {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(),
            Ty::TypeVar(_) => {
                let ok_v = self.fresh_var();
                let err_v = self.fresh_var();
                self.unify_infer(&resolved, &Ty::result(ok_v, err_v.clone()));
                err_v
            }
            _ => {
                self.reject_ctor_pattern_on_scalar("err(..)", "a Result", &resolved);
                Ty::Unknown
            }
        };
        self.bind_pattern(inner, &it);
    }

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
