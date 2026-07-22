// Continuation of `impl Checker` — module-level type inference, declaration
// checking, and test where-clause evaluation. Split out of mod.rs to keep it
// under the 800-line codopsy max-lines threshold; pure text move, same
// module scope via `include!` (no privacy boundary since mod_p2.rs/mod_p3.rs
// are spliced directly into check/mod.rs's module, not separate submodules).

impl Checker {
    /// Type-check a module's declarations. Populates type_map for all expressions.
    /// Temporarily registers unprefixed declarations for intra-module resolution,
    /// then cleans them up.
    pub fn infer_module(&mut self, prog: &mut ast::Program, module_name: &str) {
        // Isolate module's constraint solving and type map from the main program
        let saved_constraints = std::mem::take(&mut self.constraints);
        let saved_uf = std::mem::replace(&mut self.uf, UnionFind::new());
        self.type_map.clear();

        // Build module's import table
        let self_name = self.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(module_name);
        let saved_import_table = std::mem::replace(&mut self.env.import_table, ImportTable::new());
        let (mod_table, diags) = build_import_table(prog, Some(import_table_name), &self.env.user_modules);
        self.env.import_table = mod_table;
        self.diagnostics.extend(diags);

        // Temporarily register unprefixed declarations for intra-module resolution
        let snapshot = self.env.snapshot_keys();
        crate::canonicalize::registration::register_decls(
            &mut self.env, &mut self.diagnostics, &prog.decls, None,
        );

        // Infer + solve + resolve
        let saved_prefix = std::mem::replace(
            &mut self.current_module_prefix,
            Some(module_name.to_string()),
        );
        for decl in prog.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        self.resolve_deferred_tuple_indices();
        self.flush_pending_toplet_tys();
        resolve_type_map(&mut self.type_map, &self.uf);
        self.validate_map_key_types();
        self.validate_empty_collection_elements();
        self.validate_int_overflow_literals();
        self.validate_unresolved_binding_types();
        self.current_module_prefix = saved_prefix;

        // Restore
        self.constraints = saved_constraints;
        self.uf = saved_uf;
        self.env.import_table = saved_import_table;
        self.env.restore_keys(&snapshot);
    }

    /// #785: re-infer ONLY this module's top-level `let`s so `env.top_lets`
    /// carries fully inferred types BEFORE the entry program is checked.
    /// The CLI drivers run `infer_program` (where every cross-module reader
    /// lives) before the `infer_module` loop, so without this pre-pass a
    /// reader sees the registration-time seed — `Unknown` for any
    /// non-literal initializer (`let K = neg_two()`). Same isolation
    /// bracket as `infer_module`; decls are cloned so the module AST stays
    /// pristine for the real inference later.
    pub fn refresh_module_top_lets(&mut self, prog: &ast::Program, module_name: &str) {
        if !prog.decls.iter().any(|d| matches!(d, ast::Decl::TopLet { .. })) {
            return;
        }
        let saved_constraints = std::mem::take(&mut self.constraints);
        let saved_uf = std::mem::replace(&mut self.uf, UnionFind::new());
        let saved_type_map = std::mem::take(&mut self.type_map);
        // This is a TYPE-EXTRACTION pre-pass, not the module's real check —
        // `infer_module` re-infers everything later and owns the reporting.
        // Emitting here would DOUBLE-report every real initializer error and
        // fire spurious ambiguity (the temp unprefixed decls coexist with the
        // canonical prefixed ones, so a bare ctor initializer like
        // `let MOOD = Happy` sees its own type twice). Discard everything
        // this pass emits.
        let saved_diag_len = self.diagnostics.len();
        // Deferred-check sites registered here carry TypeVars owned by THIS
        // pass's union-find; validating them later against the real pass's UF
        // would mis-resolve (false E018/E025 or silent misses). The real pass
        // re-registers every site, so truncate the pre-pass's additions away.
        let saved_deferred_lens = (
            self.deferred_tuple_indices.len(),
            self.deferred_field_accesses.len(),
            self.deferred_map_key_checks.len(),
            self.deferred_ord_elem_checks.len(),
            self.deferred_unknown_type_checks.len(),
            self.deferred_empty_collection_checks.len(),
            self.deferred_int_overflow_checks.len(),
            self.deferred_unresolved_binding_checks.len(),
        );

        let self_name = self.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(module_name);
        let saved_import_table = std::mem::replace(&mut self.env.import_table, ImportTable::new());
        let (mod_table, _diags) = build_import_table(prog, Some(import_table_name), &self.env.user_modules);
        self.env.import_table = mod_table;

        let snapshot = self.env.snapshot_keys();
        crate::canonicalize::registration::register_decls(
            &mut self.env, &mut Vec::new(), &prog.decls, None,
        );
        let saved_prefix = std::mem::replace(
            &mut self.current_module_prefix,
            Some(module_name.to_string()),
        );
        for decl in prog.decls.iter() {
            if matches!(decl, ast::Decl::TopLet { .. }) {
                let mut d = decl.clone();
                self.check_decl(&mut d);
            }
        }
        self.solve_constraints();
        self.flush_pending_toplet_tys();
        self.current_module_prefix = saved_prefix;

        self.constraints = saved_constraints;
        self.uf = saved_uf;
        self.type_map = saved_type_map;
        self.env.import_table = saved_import_table;
        self.env.restore_keys(&snapshot);
        self.diagnostics.truncate(saved_diag_len);
        self.deferred_tuple_indices.truncate(saved_deferred_lens.0);
        self.deferred_field_accesses.truncate(saved_deferred_lens.1);
        self.deferred_map_key_checks.truncate(saved_deferred_lens.2);
        self.deferred_ord_elem_checks.truncate(saved_deferred_lens.3);
        self.deferred_unknown_type_checks.truncate(saved_deferred_lens.4);
        self.deferred_empty_collection_checks.truncate(saved_deferred_lens.5);
        self.deferred_int_overflow_checks.truncate(saved_deferred_lens.6);
        self.deferred_unresolved_binding_checks.truncate(saved_deferred_lens.7);
    }

    /// Upgrade `env.top_lets` entries from the POST-solve resolution of their
    /// initializer types. The `TopLet` branch writes pre-solve (all it can do
    /// mid-pass), which leaves a generic-ctor initializer's payload Unknown;
    /// this pass replaces any still-partially-Unknown entry once the
    /// union-find actually knows the answer. Only a FULLY concrete resolution
    /// upgrades — swapping one partial type for another would churn without
    /// fixing readers. Must run before the calling flow swaps its UF back.
    fn flush_pending_toplet_tys(&mut self) {
        for (key, ity) in std::mem::take(&mut self.pending_toplet_tys) {
            let existing = self.env.top_lets.get(&key);
            let stale = match existing {
                None => true,
                Some(t) => t.contains_unknown() || t.contains_typevar(),
            };
            if !stale {
                continue;
            }
            let r = resolve_ty(&ity, &self.uf);
            if !r.contains_unknown() && !r.contains_typevar() {
                self.env.top_lets.insert(key, r);
            }
        }
    }

    // ── Declaration checking ──

    /// Push generic type vars, structural bounds, and protocol bounds into the environment.
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        use crate::canonicalize::registration::SCALAR_TYPE_NAMES;
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            // Check if this is a const param (single scalar type bound)
            let is_const = g.bounds.as_ref().map_or(false, |bs| {
                bs.len() == 1 && SCALAR_TYPE_NAMES.contains(&bs[0].as_str())
            });
            if is_const {
                let ty = self.resolve_type_expr(&ast::TypeExpr::Simple { name: sym(&g.bounds.as_ref().unwrap()[0]) });
                self.env.types.insert(sym(&g.name), Ty::ConstParam { name: sym(&g.name), ty: Box::new(ty) });
            } else {
                self.env.types.insert(sym(&g.name), Ty::TypeVar(sym(&g.name)));
            }
            if let Some(bte) = &g.structural_bound {
                let bt = self.resolve_type_expr(bte);
                self.env.structural_bounds.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
            }
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() && !is_const {
                    self.env.generic_protocol_bounds.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
                }
            }
        }
    }

    /// Remove generic type vars, structural bounds, and protocol bounds from the environment.
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.remove(&sym(&g.name));
            self.env.structural_bounds.remove(&sym(&g.name));
            self.env.generic_protocol_bounds.remove(&sym(&g.name));
        }
    }

    /// Constrain an effect fn body against its return type signature.
    /// Effect fns accept: Unit body (control-flow returns), unwrapped T, or full Result[T, E].
    fn constrain_effect_body(&mut self, name: &str, ret_ty: &Ty, body_ty: Ty) {
        let body_resolved = resolve_ty(&body_ty, &self.uf);
        if body_resolved == Ty::Unit { return; } // while loops, guard patterns return via control flow
        if let Ty::Applied(crate::types::TypeConstructorId::Result, args) = ret_ty {
            // ret_ty is Result[T, E]: body can be Result[T, E] or unwrapped T
            if args.len() >= 1 {
                let ok = &args[0];
                if body_resolved.is_result() {
                    self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
                } else {
                    self.constrain(ok.clone(), body_ty, format!("fn '{}'", name));
                }
                return;
            }
        }
        // ret_ty is non-Result (e.g. String): body can be T or Result[T, E] (auto-unwrapped)
        if let Ty::Applied(crate::types::TypeConstructorId::Result, ref args) = body_resolved {
            if args.len() >= 1 {
                self.constrain(ret_ty.clone(), args[0].clone(), format!("fn '{}'", name));
                return;
            }
        }
        self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
    }

    fn check_fn_decl(
        &mut self,
        name: &str,
        params: &mut [ast::Param],
        return_type: &ast::TypeExpr,
        body: &mut ast::Expr,
        effect: &Option<bool>,
        generics: &mut Option<Vec<ast::GenericParam>>,
    ) {
        self.env.push_scope();
        self.enter_generics(generics);
        // A bare `self` first param is sugar for `self: Self` (see
        // registration.rs's matching fix). `Self` only stays an unresolved
        // placeholder inside a `protocol { ... }` declaration; on an actual
        // convention method it must resolve to the enclosing type.
        let receiver_ty = name.split_once('.').map(|(ty_name, _)| Ty::Named(sym(ty_name), Vec::new()));
        for (i, p) in params.iter_mut().enumerate() {
            let ty = if i == 0 && p.name.as_str() == "self"
                && matches!(&p.ty, ast::TypeExpr::Simple { name: tn } if tn.as_str() == "Self")
            {
                receiver_ty.clone().unwrap_or_else(|| self.resolve_type_expr(&p.ty))
            } else {
                self.resolve_type_expr(&p.ty)
            };
            self.deferred_unknown_type_checks.push((
                ty.clone(), self.current_span, format!("parameter '{}'", p.name),
            ));
            self.env.define_var(&p.name, ty.clone());
            self.env.param_vars.insert(sym(&p.name));
            if p.is_mut { self.env.mutable_vars.insert(sym(&p.name)); }
            if let Some(ref mut default_expr) = p.default {
                let dty = self.infer_expr(default_expr);
                self.constrain(ty, dty, format!("default arg '{}'", p.name));
            }
        }
        let ret_ty = self.resolve_type_expr(return_type);
        self.deferred_unknown_type_checks.push((
            ret_ty.clone(), self.current_span, format!("return type of '{}'", name),
        ));
        let prev = (self.env.current_ret.take(), self.env.can_call_effect, self.env.auto_unwrap, self.env.lambda_depth);
        let is_effect = effect.unwrap_or(false);
        self.env.current_ret = Some(ret_ty.clone());
        self.env.can_call_effect = is_effect;
        self.env.auto_unwrap = is_effect;
        self.env.lambda_depth = 0;
        let body_ity = self.infer_expr(body);
        if effect.unwrap_or(false) {
            self.constrain_effect_body(name, &ret_ty, body_ity);
        } else {
            // Capture the trailing `let` binding name (if any) to specialize
            // the Unit-leak E001 try: snippet downstream.
            let hint = trailing_let_name(body).map(FixHint::LastLetName);
            self.constrain_with_hint(ret_ty, body_ity, format!("fn '{}'", name), hint);
        }
        self.env.current_ret = prev.0; self.env.can_call_effect = prev.1; self.env.auto_unwrap = prev.2; self.env.lambda_depth = prev.3;
        self.exit_generics(generics);
        self.env.pop_scope();
    }

    fn check_decl(&mut self, decl: &mut ast::Decl) {
        match decl {
            ast::Decl::Fn { name, params, return_type, body: Some(body), effect, generics, .. } => {
                self.check_fn_decl(name, params, return_type, body, effect, generics);
            }
            ast::Decl::Test { body, where_clauses, .. } => {
                self.check_decl_test(body, where_clauses);
            }
            ast::Decl::TestWhereDef { clauses, .. } => {
                let wcs = clauses.clone();
                for wc in &wcs { self.infer_test_where_inner(wc); }
            }
            ast::Decl::TopLet { name, ty, value, mutable, .. } => {
                self.check_decl_top_let(name, ty, value, *mutable);
            }
            ast::Decl::Type { ty, .. } => {
                // Infer types for default value expressions in variant record fields
                infer_default_exprs(self, ty);
            }
            _ => {}
        }
    }

    // `Test` arm of `check_decl`: infer the where-clauses (shared bind-type
    // map across the whole clause list, see `infer_test_where_collect`'s
    // Bind arm) then the test body, under effect-call and in-test-block
    // permissions scoped to this test.
    fn check_decl_test(&mut self, body: &mut ast::Expr, where_clauses: &Vec<ast::TestWhere>) {
        let wcs = where_clauses.clone();
        self.env.push_scope();
        let prev_call = self.env.can_call_effect; self.env.can_call_effect = true;
        let prev_test = self.env.in_test_block; self.env.in_test_block = true;
        let mut seen_binds = std::collections::HashMap::new();
        for wc in &wcs { self.infer_test_where_collect(wc, &mut seen_binds); }
        self.infer_expr(body);
        self.env.in_test_block = prev_test;
        self.env.can_call_effect = prev_call;
        self.env.pop_scope();
    }

    // `TopLet` arm of `check_decl`: infer the value, constrain it against a
    // declared annotation (pins e.g. an empty-collection element, #…), then
    // refresh `env.top_lets` under both the bare and module-prefixed keys.
    fn check_decl_top_let(&mut self, name: &Sym, ty: &Option<ast::TypeExpr>, value: &mut ast::Expr, mutable: bool) {
        if mutable { self.env.mutable_vars.insert(sym(name)); }
        let ity = self.infer_expr(value);
        // A declared type annotation on a top-level `let`/`var` is the
        // source of truth — flow it into the value so an annotated empty
        // collection (`var items: List[Int] = []`) pins its element, the
        // same as a local typed `let` binding. (Was dropped here, so the
        // element stayed undecidable and tripped E018.)
        if let Some(te) = ty {
            let declared = self.resolve_type_expr(te);
            self.constrain(declared, ity.clone(), format!("top let {}", name));
        }
        let resolved = resolve_ty(&ity, &self.uf);
        // Update env.top_lets with the fully inferred type.
        // `register_decls` seeds module top_lets under the
        // prefixed key (`util.ANON`), so without this we'd only
        // refresh the unprefixed intra-module alias — lowering
        // reads the prefixed key and gets `Ty::Unknown`.
        let prefixed_key = self.current_module_prefix.as_ref()
            .map(|p| sym(&format!("{}.{}", p, name)));
        if std::env::var_os("ALMIDE_TOPLET_DEBUG").is_some() {
            eprintln!("[toplet-debug] refresh: name={} prefix={:?} resolved={:?} existing_prefixed={:?}",
                name, self.current_module_prefix,
                resolved,
                prefixed_key.as_ref().map(|k| self.env.top_lets.get(k)));
        }
        if let Some(k) = prefixed_key {
            if matches!(self.env.top_lets.get(&k), Some(Ty::Unknown) | None) {
                self.env.top_lets.insert(k, resolved.clone());
            }
            self.pending_toplet_tys.push((k, ity.clone()));
        }
        if matches!(self.env.top_lets.get(&sym(name)), Some(Ty::Unknown) | None) {
            self.env.top_lets.insert(sym(name), resolved);
        }
        self.pending_toplet_tys.push((sym(name), ity));
    }

    // ── Exhaustiveness ──

    fn infer_test_where_inner(&mut self, wc: &ast::TestWhere) {
        let mut seen = std::collections::HashMap::new();
        self.infer_test_where_collect(wc, &mut seen);
    }

    fn infer_test_where_collect(
        &mut self,
        wc: &ast::TestWhere,
        seen: &mut std::collections::HashMap<Sym, Ty>,
    ) {
        match wc {
            ast::TestWhere::Bind { name, value } => {
                let mut val = value.clone();
                let ty = self.infer_expr(&mut val);
                // A `where greet = (name) => ...` binding shadows an existing
                // top-level function. Unify the inferred lambda type with that
                // function's signature so the lambda's parameter type variables
                // get pinned — otherwise they stay unbound and leak into the IR
                // as Unknown, tripping the ConcretizeTypes postcondition.
                self.unify_where_override_with_fn_sig(&[*name], &ty);
                // A CASE-table binding (`"add" [op = (a,b) => a+b, …]` / `"mul"
                // [op = …]`): each case re-binds the SAME name, but the test body
                // is inferred ONCE — against the LAST binding only. Unify every
                // same-name case binding with the first, so the body's call site
                // pins ALL cases' lambda param tyvars through the union-find (the
                // per-case lowering re-lowers the shared body, so the cases must
                // agree on types anyway — a heterogeneous table was never
                // lowerable). Without this, earlier cases' annotation-less lambda
                // params stayed unbound and leaked into the IR as Unknown.
                if let Some(prev) = seen.get(name) {
                    let prev = prev.clone();
                    self.unify_infer(&prev, &ty);
                } else {
                    seen.insert(*name, ty.clone());
                }
                let resolved = resolve_ty(&ty, &self.uf);
                self.env.define_var(name.as_str(), resolved);
            }
            ast::TestWhere::Override { path, value } => {
                let mut v = value.clone();
                let ty = self.infer_expr(&mut v);
                self.unify_where_override_with_fn_sig(path, &ty);
                let resolved = resolve_ty(&ty, &self.uf);
                let override_name = format!("__where_{}", path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("_"));
                self.env.define_var(&override_name, resolved);
            }
            ast::TestWhere::CallResponse { target, params, response } => {
                // Resolve param types from original function signature
                let target_name = if target.len() == 1 { *target.first().unwrap() }
                    else { sym(&target.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")) };
                let sig_params: Vec<Ty> = self.env.functions.get(&target_name)
                    .map(|sig| sig.params.iter().map(|(_, t)| t.clone()).collect())
                    .unwrap_or_default();
                let param_vars: Vec<_> = params.iter().filter_map(|pat| {
                    if let ast::Pattern::Ident { name } = pat { Some(*name) } else { None }
                }).collect();
                let param_tys: Vec<_> = param_vars.iter().enumerate().map(|(i, pname)| {
                    let ty = sig_params.get(i).cloned().unwrap_or_else(|| self.fresh_var());
                    self.env.define_var(pname.as_str(), ty.clone());
                    ty
                }).collect();
                let mut r = response.clone();
                let ret_ty = self.infer_expr(&mut r);
                let ret_resolved = resolve_ty(&ret_ty, &self.uf);
                let fn_ty = Ty::Fn { params: param_tys, ret: Box::new(ret_resolved) };
                let override_name = format!("__where_{}", target.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("_"));
                self.env.define_var(&override_name, fn_ty);
            }
            ast::TestWhere::Case { bindings, .. } => {
                for b in bindings { self.infer_test_where_collect(b, seen); }
            }
        }
    }

    /// Unify a `where`-clause override value's inferred type with the
    /// shadowed top-level function's signature, pinning the override
    /// lambda's parameter type variables. `path` is the overridden
    /// function's name path (`["greet"]` or `["http", "get"]`); `value_ty`
    /// is the inferred type of the override expression.
    ///
    /// Without this, an override like `where greet = (name) => ...` leaves
    /// `name`'s type variable unbound — it resolves to Unknown and leaks
    /// into the IR, tripping the ConcretizeTypes postcondition (and falling
    /// back to a wrong WASM ValType for non-i32 params).
    ///
    /// No-op when the path names no known function or the signature is
    /// generic: unifying a lambda's `?N` param with a named TypeVar would
    /// resolve it to `A`, which is itself unresolved at codegen time.
    fn unify_where_override_with_fn_sig(&mut self, path: &[Sym], value_ty: &Ty) {
        let name = if path.len() == 1 {
            path[0]
        } else {
            sym(&path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
        };
        let Some(sig) = self.env.functions.get(&name) else { return };
        let sig_ty = Ty::Fn {
            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
            ret: Box::new(sig.ret.clone()),
        };
        let mut typevars = Vec::new();
        TypeEnv::collect_typevars(&sig_ty, &mut typevars);
        if !typevars.is_empty() { return; }
        self.unify_infer(&sig_ty, value_ty);
    }
}
