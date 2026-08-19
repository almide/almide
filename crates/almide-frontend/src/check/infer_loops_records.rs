// `infer_expr_inner` group 3, part 2 — the loop / pipe / record-validation
// helpers and the literal-context machinery. Split out of `infer_calls_closures.rs` at a
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
            // #1521: a CONCRETE non-iterable head (`for x in 5`) used to fall
            // through as Unknown and die at codegen behind the COMPILER BUG
            // banner. Opaque types stay silent — an unresolved var is the
            // inference's business, not the loop's.
            Ty::Unknown | Ty::TypeVar(_) | Ty::Never => Ty::Unknown,
            other => {
                self.emit(super::err(
                    format!("`for` cannot iterate `{}` — the loop head must be a List or a Map", other.display()),
                    "Iterate a collection: a List (`for x in xs`), a range (`for i in 0...n`), or a Map (`for (k, v) in m`)".to_string(),
                    "for loop head".to_string()).with_code("E059"));
                Ty::Unknown
            }
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
    /// An `if`/`match` RHS whose branches are EXPLICIT `ok(..)`/`err(..)`
    /// constructors is Result-TYPED, but nothing downstream unwraps it: the
    /// per-branch auto-`?` only fires for branches that are effect CALLS (the
    /// #717 family — `let v = if c then boom(x) else boom(y)`, which really does
    /// short-circuit), and `auto_try::is_result_value` is kind-driven at the top
    /// level (`Call` / `ok(..)` / `err(..)`), which an `if`/`match` is not. So a
    /// constructor-armed branch stays a `Result` at runtime on every backend —
    /// verified observable: `let r = if c then ok(..) else err(..)` does NOT
    /// early-return on the err branch; a statement after the binding still runs,
    /// identically on native and wasm.
    ///
    /// Unwrapping the TYPE here anyway made the checker the only party claiming
    /// the payload: every occurrence of the var got a payload type over a Result
    /// value, which the type system then could not catch — `int.to_string(r)`
    /// type-checked and exploded in the generated Rust instead (E0308), and an
    /// effect-fn tail re-yielding the var was double-`Ok`-wrapped, so the whole
    /// function failed to compile for every payload type.
    ///
    /// Keeping the Result for exactly this shape makes the checker agree with
    /// what the backends do. A CALL-armed branch keeps unwrapping (#717's
    /// `via_if`/`pick_mixed`/`pick_both` pin that short-circuit), as does a
    /// direct `let x = eff_call()`, and the annotation / match-subject escape
    /// hatches are untouched. Pinned by `spec/lang/effect_if_value_test.almd`.
    pub(crate) fn rhs_keeps_result_shape(value: &almide_lang::ast::Expr) -> bool {
        use almide_lang::ast::ExprKind;
        fn is_ctor(e: &almide_lang::ast::Expr) -> bool {
            matches!(&e.kind, ExprKind::Ok { .. } | ExprKind::Err { .. })
        }
        match &value.kind {
            ExprKind::If { then, else_, .. } => is_ctor(then) && is_ctor(else_),
            ExprKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|a| is_ctor(&a.body))
            }
            _ => false,
        }
    }

    /// [`Self::effect_unwrap_rhs`] + the #1123 E041 queue: when the strip
    /// will fire (auto_unwrap, target doesn't keep, t is Result), record the
    /// site so post-solve emits the deprecation warning with the `!` insert.
    fn effect_unwrap_rhs_warned(&mut self, t: Ty, span: Option<ast::Span>, what: &'static str, mechanical: bool, target_keeps_result: bool) -> Ty {
        if self.env.auto_unwrap && !target_keeps_result {
            self.deferred_implicit_prop_checks.push((t.clone(), span, what, mechanical, false));
        }
        self.effect_unwrap_rhs(t, target_keeps_result)
    }

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
        // A sized element pins its literals directly; an AGGREGATE element is
        // recursed into, because the sized slot may sit inside it —
        // `List[(Int8, Int)]` narrows the tuple's first component exactly as
        // `List[Int8]` narrows the element. Returning `None` for the aggregate
        // case stopped the walk at the list and left the component unchecked
        // (differential fuzz, seed 1784958133509490474).
        matches!(elem,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Tuple(_) | Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Named(..)
            | Ty::Applied(TC::List | TC::Set, _)
        ).then(|| elem.clone())
    }

    /// Pair each part of an aggregate literal with the type its annotation
    /// declares for that part.
    ///
    /// Tuples pair positionally and records pair BY NAME — a record literal's
    /// fields are in source order while the declaration's are in declaration
    /// order, so position would pair the wrong types. `None` when the literal
    /// and the annotation are not the same aggregate shape: that is a type
    /// error the checker reports elsewhere, and pinning a literal against a
    /// mismatched slot would report a second, misleading one.
    fn annotated_component_tys<'a>(
        declared: &Ty,
        value: &'a ast::Expr,
    ) -> Option<Vec<(&'a ast::Expr, Ty)>> {
        match (&value.kind, declared) {
            (ExprKind::Tuple { elements, .. }, Ty::Tuple(tys)) if elements.len() == tys.len() => {
                Some(elements.iter().zip(tys.iter().cloned()).collect())
            }
            (ExprKind::Record { fields, .. }, Ty::Record { fields: decl } | Ty::OpenRecord { fields: decl }) => {
                Some(fields.iter().filter_map(|f| {
                    decl.iter()
                        .find(|(n, _)| *n == f.name)
                        .map(|(_, t)| (&f.value, t.clone()))
                }).collect())
            }
            _ => None,
        }
    }

    pub(crate) fn record_int_literal_context(&mut self, value: &ast::Expr, declared: &Ty) {
        // A COLLECTION literal against an annotated element type pins each
        // ELEMENT: `let bs: List[Int8] = [1, 256]` narrows every element to i8 in
        // codegen, so `256` must face the Int8 range check here — it did not, and
        // rustc rejected `256i8` after `check` accepted (differential-fuzz, seed
        // 1784965680755102000; the same check-vs-build gap #626/index 92 closed for
        // scalar bindings). Recurses so a nested annotation reaches through.
        if let ExprKind::Paren { expr } = &value.kind {
            self.record_int_literal_context(expr, declared);
            return;
        }
        // #1521: BRANCH-RESULT positions carry the annotation too — `let b:
        // Int8 = if c then 300 else 1` narrows each branch tail in codegen
        // (`300i8`, rejected by rustc while wasm ran and printed 300: a parity
        // hole AND a divergence in one shape), but this walk stopped at the
        // `if`. Same for match-arm bodies and a block's tail expression. A MAP
        // literal against `Map[K, V]` pins each entry's key AND value — the
        // VALUE slot was the unchecked one (`["k": 300]` into Map[String,
        // Int8] was check-green, rustc E0308).
        match &value.kind {
            ExprKind::If { then, else_, .. } => {
                self.record_int_literal_context(then, declared);
                self.record_int_literal_context(else_, declared);
                return;
            }
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    self.record_int_literal_context(&arm.body, declared);
                }
                return;
            }
            ExprKind::Block { expr: Some(tail), .. } => {
                self.record_int_literal_context(tail, declared);
                return;
            }
            ExprKind::MapLiteral { entries } => {
                use almide_lang::types::constructor::TypeConstructorId as TC;
                if let Ty::Applied(TC::Map, args) = declared {
                    if let [k_ty, v_ty] = args.as_slice() {
                        for (k, v) in entries {
                            self.record_int_literal_context(k, k_ty);
                            self.record_int_literal_context(v, v_ty);
                        }
                        return;
                    }
                }
            }
            _ => {}
        }
        if let Some(elem_ty) = Self::annotated_element_ty(declared) {
            if let ExprKind::List { elements, .. } = &value.kind {
                for e in elements {
                    self.record_int_literal_context(e, &elem_ty);
                }
                return;
            }
        }
        // A TUPLE literal against an annotated tuple type pins each component,
        // and a RECORD literal against a declared record pins each field BY
        // NAME. Both narrow in codegen exactly as a collection element does, so
        // both must face the same range check — `let r: Rec = { b: 65535 }` with
        // `Rec.b: Int8` was accepted here and then rejected by rustc as
        // `65535i8` (differential fuzz, seed 1784958133509490474; the same
        // check-vs-build gap #626 closed for scalar bindings).
        // A NOMINAL annotation (`let r: Rec = { … }`) must be expanded to its
        // structural form first — the literal's fields pair against the
        // declaration's, and `Ty::Named` carries none.
        let structural = self.env.resolve_named(declared);
        if let Some(parts) = Self::annotated_component_tys(&structural, value) {
            for (part, ty) in parts {
                self.record_int_literal_context(part, &ty);
            }
            return;
        }
        // Reaches through any paren/unary-minus chain, so `let m: Int8 = -(300)`
        // and `let m: Int8 = --300` face the same range check `-300` does. The
        // NET sign comes from the same walk; the `Unary` inference writes it onto
        // the site independently, and this branch keeps it in step for the site
        // it has to enqueue itself.
        // The FLOAT twin (Wave 4 L7): pin the annotated context onto an enqueued
        // out-of-f32-range float literal — a bare literal's own solved type stays
        // `Float`, so `let p: Float32 = 1e300` is only decidable with the pin.
        if let Some((fid, _v)) = super::float_literal_chain(value) {
            if let Some(site) =
                self.deferred_float_overflow_checks.iter_mut().find(|s| s.expr_id == fid)
            {
                site.context_ty = Some(declared.clone());
            }
        }
        let Some((id, raw, negated)) = super::int_literal_chain(value) else { return };
        if let Some(site) = self.deferred_int_overflow_checks.iter_mut().find(|s| s.expr_id == id) {
            site.context_ty = Some(declared.clone());
            return;
        }
        // A literal that fits i64 was never enqueued — but a SIZED context
        // can still overflow it (`neg_one_i8(128)`: check accepted, native
        // rustc rejected `128i8` — the check-vs-build gap, fuzz
        // seed-20260718 index 92). Enqueue a site so the post-solve E024
        // range check runs against the sized context.
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
        // The CALLER's identity is the module currently being inferred
        // (`current_module_prefix`; `None` = the entry program). Every file
        // loads as its own namespace module, so module identity IS file
        // identity — `local fn` allows exactly caller == callee (#870; the
        // old `self_module_name` bypass let the ENTRY program reach any
        // module's `local fn`). `mod fn` allows the same PROJECT: the
        // caller's and callee's PACKAGE agree — a dotted module belongs to
        // its first segment's package, a bare module is a dep package iff
        // it is a known dep ROOT (`env.dep_root_modules`), else the SELF
        // package (self submodules load bare, dep submodules dotted). The
        // old module-equality rule wrongly rejected a same-project
        // cross-file `mod fn` call.
        let caller = self.current_module_prefix.clone();
        let pkg_of = |m: Option<&str>| -> String {
            let Some(m) = m else { return String::new() };
            if let Some((root, _)) = m.split_once('.') {
                return root.to_string();
            }
            if self.env.dep_root_modules.contains(&sym(m)) {
                m.to_string()
            } else {
                String::new()
            }
        };
        match vis {
            ast::Visibility::Public => return,
            ast::Visibility::Local => {
                if caller.as_deref() == Some(callee_module) {
                    return;
                }
            }
            ast::Visibility::Mod => {
                if pkg_of(caller.as_deref()) == pkg_of(Some(callee_module)) {
                    return;
                }
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
