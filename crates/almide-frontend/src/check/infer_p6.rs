// `infer_expr_inner` group 3, part 2 — the loop / pipe / record-validation
// helpers and the literal-context machinery. Split out of `infer_p3.rs` at a
// method boundary; both halves are `impl Checker` blocks `include!`d into
// `infer.rs`, so imports come from there.

impl Checker {
    fn infer_for_in(
        &mut self,
        var: &str,
        var_tuple: &Option<Vec<almide_base::intern::Sym>>,
        iterable: &mut Box<ast::Expr>,
        body: &mut Vec<ast::Stmt>,
    ) -> Ty {
        // An empty-list iterable (`for _ in []`) registers a generic ListLiteral
        // site via `infer_expr` below; retag it as `ForInEmpty` so the E018 hint
        // suggests the for-position fix `for _ in ([] : List[Int])` rather than a
        // `let`-binding example.
        let iterable_is_empty_list = matches!(&iterable.kind,
            ExprKind::List { elements, .. } if elements.is_empty());
        let iter_ty = self.infer_expr(iterable);
        if iterable_is_empty_list {
            if let Some(last) = self.deferred_empty_collection_checks.last_mut() {
                last.kind = super::EmptyCollectionKind::ForInEmpty;
            }
        }
        self.env.push_scope();
        let iter_resolved = resolve_ty(&iter_ty, &self.uf);
        let elem_ty = match &iter_resolved {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
            Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
            _ => Ty::Unknown,
        };
        self.bind_for_in_var(var, var_tuple, elem_ty);
        for stmt in body.iter_mut() { self.check_stmt(stmt); }
        self.env.pop_scope();
        Ty::Unit
    }

    /// Bind the loop variable(s) of a `for` statement: a plain `var` name,
    /// or `var_tuple` destructuring (`for (a, b) in xs`) against a Tuple
    /// element type. Verbatim text move out of [`Self::infer_for_in`].
    fn bind_for_in_var(&mut self, var: &str, var_tuple: &Option<Vec<almide_base::intern::Sym>>, elem_ty: Ty) {
        if let Some(names) = var_tuple {
            // Destructure tuple: for (a, b) in xs
            if let Ty::Tuple(tys) = &elem_ty {
                for (i, n) in names.iter().enumerate() {
                    self.env.define_var(n, tys.get(i).cloned().unwrap_or(Ty::Unknown));
                }
            } else {
                for n in names { self.env.define_var(n, Ty::Unknown); }
            }
        } else {
            self.env.define_var(var, elem_ty);
        }
    }

    // ── Statement checking ──

    /// Reject a binding whose type uses a function in a position that demands
    /// equality/hashing: a `Set` element or a `Map` key. Closures have neither,
    /// so such a type is meaningless — and the two targets disagree (native
    /// rustc rejects it, WASM silently drops the inserts). Closures are fine as
    /// `Map` *values*.
    pub(crate) fn check_collection_element_types(&mut self, ty: &Ty) {
        let resolved = resolve_ty(ty, &self.uf);
        if let Some((msg, hint)) = invalid_collection_type(&resolved) {
            self.emit(super::err(msg, hint, "collection element type").with_code("E016"));
        }
    }

    /// Record an empty-collection producer to re-check after constraint solving
    /// (the undecidable-empty-collection / E018 rule). The current span is
    /// captured now; the element type is verified post-solve in
    /// [`Checker::validate_empty_collection_elements`].
    pub(crate) fn register_empty_collection(&mut self, ty: Ty, kind: super::EmptyCollectionKind) {
        self.deferred_empty_collection_checks.push(super::EmptyCollectionSite {
            ty,
            kind,
            span: self.current_span,
        });
    }

    /// #488: classify a `TypeName(...)` call. All-named args on a record
    /// type or record-payload variant case rewrite the node in place to the
    /// brace `ExprKind::Record` form (one construction pipeline, both
    /// spellings); positional args on those, or named args on a tuple
    /// constructor, are E021. Returns true when the node was rewritten.
    fn normalize_ctor_paren_call(&mut self, expr: &mut ast::Expr) -> bool {
        let ExprKind::Call { callee, args, named_args, .. } = &expr.kind else { return false };
        // Both spellings of a constructor callee: bare/dotted `TypeName`, and
        // the cross-module `m.Cfg(...)` form, which parses as a MEMBER access
        // on the module ident — without this arm the paren-named normalization
        // only covered the same-file spelling (caught by the §2 matrix gate).
        let n = match &callee.kind {
            ExprKind::TypeName { name } => *name,
            ExprKind::Member { object, field }
                if field.as_str().chars().next().map_or(false, |c| c.is_uppercase()) =>
            {
                let ExprKind::Ident { name: obj, .. } = &object.kind else { return false };
                sym(&format!("{}.{}", obj, field))
            }
            _ => return false,
        };
        let bare = n.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(n);
        // Record-payload variant case? (ctor table is keyed by bare name)
        let ctor_payload_record = self.env.lookup_ctor_in(&bare, self.current_module_prefix.as_deref())
            .map(|(_, case)| matches!(case.payload, crate::types::VariantPayload::Record(_)));
        // Record TYPE? (resolve through the same canonicalization annotations use)
        let is_record_type = ctor_payload_record.is_none() && {
            let key = match n.as_str().rsplit_once('.') {
                Some(_) => sym(n.as_str()),
                None => crate::canonicalize::resolve::canonical_user_type_sym(
                    n.as_str(), &self.env.types, self.current_module_prefix.as_deref(),
                ).unwrap_or(n),
            };
            matches!(self.env.resolve_named(&Ty::Named(key, vec![])), Ty::Record { .. } | Ty::OpenRecord { .. })
        };
        if ctor_payload_record == Some(true) || is_record_type {
            if !args.is_empty() {
                self.emit(super::err(
                    format!("'{}' takes named fields, not positional arguments", n),
                    format!("Name every field: `{}(field: value, ...)` or `{} {{ field: value, ... }}`", n, n),
                    format!("constructor {}(...)", n),
                ).with_code("E021"));
                return false;
            }
            // Rewrite to the brace form in place; re-inference routes it
            // through the Record arm (defaults, field validation, #433
            // qualification, both backends' Record emission — for free).
            let ExprKind::Call { named_args, .. } = std::mem::replace(&mut expr.kind, ExprKind::Unit) else { unreachable!() };
            let fields = named_args.into_iter()
                .map(|(fname, value)| ast::FieldInit { name: fname, value })
                .collect();
            expr.kind = ExprKind::Record { name: Some(n), fields };
            return true;
        }
        if ctor_payload_record == Some(false) && !named_args.is_empty() {
            self.emit(super::err(
                format!("constructor '{}' takes positional arguments, not named ones", n),
                format!("Drop the names: `{}(value, ...)`", n),
                format!("constructor {}(...)", n),
            ).with_code("E021"));
        }
        false
    }

    /// #488: validate a record construction's field set against the declared
    /// fields: duplicates always; unknown + missing-without-default when the
    /// declaration is CLOSED (a plain record or a record-payload case).
    fn validate_record_fields(
        &mut self,
        type_label: &str,
        given: &[ast::FieldInit],
        decl_fields: &[(Sym, Ty)],
        closed: bool,
        defaults: &std::collections::HashSet<Sym>,
    ) {
        let mut seen: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        for f in given {
            if !seen.insert(f.name) {
                self.emit(super::err(
                    format!("field '{}' given more than once in '{}' construction", f.name, type_label),
                    "Remove the duplicate field",
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        if !closed { return; }
        let available = || decl_fields.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>().join(", ");
        for f in given {
            if !decl_fields.iter().any(|(d, _)| *d == f.name) {
                self.emit(super::err(
                    format!("'{}' has no field '{}'", type_label, f.name),
                    format!("Available fields: {}", available()),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
        for (d, _) in decl_fields {
            if !given.iter().any(|f| f.name == *d) && !defaults.contains(d) {
                self.emit(super::err(
                    format!("missing field '{}' in '{}' construction", d, type_label),
                    format!("Provide it: `{} {{ {}: ..., ... }}` (fields without defaults are required)", type_label, d),
                    format!("record literal {}", type_label),
                ).with_code("E021"));
            }
        }
    }

    /// The effect-fn auto-unwrap rule, shared by every binding-shaped
    /// position (let / var / assign): a Result[T, E]-typed RHS unwraps to T
    /// — the lowering inserts the matching `?` — unless the target itself
    /// keeps the Result (declared Result annotation, Result-typed var, or a
    /// usage-skip like `match x { ok/err }`). One function so the positions
    /// can never diverge again (#485).
    fn effect_unwrap_rhs(&self, t: Ty, target_keeps_result: bool) -> Ty {
        if self.env.auto_unwrap && !target_keeps_result {
            match t {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 =>
                    args.into_iter().next().unwrap_or(Ty::Unknown),
                other => other,
            }
        } else { t }
    }

    /// Pin the declared type onto an int-overflow candidate when the literal is
    /// the DIRECT value of an annotated binding (`let x: T = 5…` or `= -5…`), so
    /// a wider `T` (e.g. `UInt64`) makes a >i64 literal valid post-solve (#626).
    /// Pin `ty` as an EXISTING literal site's range context (first pin wins —
    /// a binding/arg annotation set earlier stays authoritative). Every int
    /// literal has a site since the liberal enqueue, so a lookup miss is a
    /// no-op by construction.
    pub(crate) fn pin_int_literal_context(&mut self, id: almide_lang::ast::ExprId, ty: &Ty) {
        if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == id) {
            if site.context_ty.is_none() {
                site.context_ty = Some(ty.clone());
            }
        }
    }

    /// The ELEMENT type of an annotated homogeneous collection, when the element
    /// is a concrete SIZED integer — the only case where an element literal's
    /// range differs from the default `Int` context.
    fn annotated_element_ty(declared: &Ty) -> Option<Ty> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let Ty::Applied(TC::List | TC::Set, args) = declared else { return None };
        let [elem] = args.as_slice() else { return None };
        matches!(elem,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        ).then(|| elem.clone())
    }

    /// Identify the underlying `Int` literal site (id, raw text, negated) of
    /// `value` — either a bare literal, a `-<literal>` unary, or a
    /// parenthesized literal. Verbatim text move out of
    /// [`Self::record_int_literal_context`].
    fn int_literal_context_site(value: &ast::Expr) -> (Option<almide_lang::ast::ExprId>, Option<String>, bool) {
        match &value.kind {
            ExprKind::Int { raw, .. } => (Some(value.id), Some(raw.clone()), false),
            ExprKind::Unary { op, operand, .. } if op.as_str() == "-"
                && matches!(&operand.kind, ExprKind::Int { .. }) =>
            {
                let raw = if let ExprKind::Int { raw, .. } = &operand.kind { Some(raw.clone()) } else { None };
                (Some(operand.id), raw, true)
            }
            ExprKind::Paren { expr } if matches!(&expr.kind, ExprKind::Int { .. }) => {
                let raw = if let ExprKind::Int { raw, .. } = &expr.kind { Some(raw.clone()) } else { None };
                (Some(expr.id), raw, false)
            }
            _ => (None, None, false),
        }
    }

    pub(crate) fn record_int_literal_context(&mut self, value: &ast::Expr, declared: &Ty) {
        // A COLLECTION literal against an annotated element type pins each
        // ELEMENT: `let bs: List[Int8] = [1, 256]` narrows every element to i8 in
        // codegen, so `256` must face the Int8 range check here — it did not, and
        // rustc rejected `256i8` after `check` accepted (differential-fuzz, seed
        // 1784965680755102000; the same check-vs-build gap #626/index 92 closed for
        // scalar bindings). Recurses so a nested annotation reaches through.
        if let Some(elem_ty) = Self::annotated_element_ty(declared) {
            match &value.kind {
                ExprKind::List { elements, .. } => {
                    for e in elements {
                        self.record_int_literal_context(e, &elem_ty);
                    }
                    return;
                }
                ExprKind::Paren { expr } => {
                    self.record_int_literal_context(expr, declared);
                    return;
                }
                _ => {}
            }
        }
        let (lit_id, raw, negated) = Self::int_literal_context_site(value);
        if let Some(id) = lit_id {
            if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == id) {
                site.context_ty = Some(declared.clone());
                return;
            }
            // A literal that fits i64 was never enqueued — but a SIZED context
            // can still overflow it (`neg_one_i8(128)`: check accepted, native
            // rustc rejected `128i8` — the check-vs-build gap, fuzz
            // seed-20260718 index 92). Enqueue a site so the post-solve E024
            // range check runs against the sized context.
            if let Some(raw) = raw {
                if !matches!(declared, Ty::Int | Ty::Unknown | Ty::TypeVar(_)) {
                    self.deferred_int_overflow_checks.push(super::IntOverflowSite {
                        expr_id: id,
                        raw,
                        negated,
                        context_ty: Some(declared.clone()),
                        span: value.span,
                    });
                }
            }
        }
    }
    /// Resolve a module.func Member expression to a qualified call key.
    fn resolve_module_call(&mut self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ExprKind::Ident { name: module, .. } = &object.kind {
            if let Some(canonical) = self.env.import_table.resolve(module) {
                self.env.import_table.mark_used(module);
                let key = format!("{}.{}", canonical, field);
                self.check_fn_visibility(&canonical, field, &key);
                return Some(key);
            }
            // Check if Ident.field is a Type.method (protocol implementation)
            let key = format!("{}.{}", module, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        // Detect dot-chain submodule access (for pipe context)
        if let Some(dotted) = self.env.import_table.resolve_dotted_path(&object.kind) {
            let key = format!("{}.{}", dotted, field);
            if self.env.functions.contains_key(&sym(&key)) {
                let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
                self.emit(super::err(
                    format!("dot-chain submodule access is no longer supported"),
                    format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
                    format!("call to {}.{}", dotted, field),
                ));
                return Some(key);
            }
        }
        // TypeName.method (e.g. Val.double in pipe)
        if let ExprKind::TypeName { name: type_name, .. } = &object.kind {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        None
    }

    /// Reject cross-module access to `mod fn` / `local fn` functions.
    ///
    /// A function has `Public` visibility by default — we only store entries
    /// for restricted (`Mod` / `Local`) declarations in `env.fn_visibility`.
    /// If the caller's own module (`self_module_name`) matches the callee's
    /// canonical module, the call is intra-module and all visibilities are
    /// allowed. Otherwise only `Public` is reachable.
    pub(super) fn check_fn_visibility(&mut self, callee_module: &str, field: &str, key: &str) {
        let vis = match self.env.fn_visibility.get(&sym(key)) {
            Some(v) => *v,
            None => return,
        };
        // Intra-module access (same package) is always allowed, regardless of
        // whether it's `mod fn` or `local fn`. This matches the spec for
        // `mod fn`; for `local fn` it is a deliberate relaxation, because
        // strict same-file enforcement needs per-fn file tracking the checker
        // does not carry (issue #870).
        if let Some(self_mod) = self.env.self_module_name {
            if self_mod.as_str() == callee_module {
                return;
            }
        }
        let (kind, scope_hint) = match vis {
            ast::Visibility::Mod => (
                "mod fn",
                "accessible only within the same project",
            ),
            ast::Visibility::Local => (
                "local fn",
                "accessible only within the same file",
            ),
            ast::Visibility::Public => return,
        };
        self.emit(super::err(
            format!("function '{}.{}' is not accessible", callee_module, field),
            format!("'{}' is declared as `{}` ({})", field, kind, scope_hint),
            format!("call to {}.{}", callee_module, field),
        ).with_code("E420"));
    }
}
