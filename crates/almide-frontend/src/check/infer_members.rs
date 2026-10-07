// Member access inference: `object.field`, module-qualified members, the
// unknown-member and missing-field reports, tuple indexing and optional
// chaining. `include!`d by infer.rs (the 800-line file budget); it shares
// that module's scope and imports.

impl Checker {
    /// Whether `object` is an identifier a local binding owns — then `object.x`
    /// goes THROUGH that binding and never resolves as a module reference, even
    /// when a module has the same name. The one rule for both the call path
    /// (`object.f(..)`, #1441) and the member path (`object.f`, #3030).
    pub(crate) fn object_shadowed_by_local(&self, object: &ast::Expr) -> bool {
        matches!(&object.kind, ExprKind::Ident { name, .. } if self.env.lookup_var(name).is_some())
    }

    fn infer_expr_member(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::Member { object, field, .. } = &mut expr.kind else { unreachable!("infer_expr_member called on the wrong ExprKind") };
        // `infer_expr(object)` below overwrites `current_span` with the object's
        // range, so capture the Member expr's own span now. E013 uses it to
        // position the `try_replace` rewrite that covers `object.field`.
        let member_span = self.current_span;
        // A module-qualified reference (`string.len`, `utils.CATEGORY_ORDER`) is
        // resolved BEFORE the object is inferred, because inferring it would fail:
        // `string` is a module name, not a variable. A local of the module's name
        // takes the spelling first, as it does on the call path (#3030, #1441).
        if !self.object_shadowed_by_local(object)
            && let Some(ty) = self.infer_module_qualified_member(object, field)
        {
            return ty;
        }
        let obj_ty = self.infer_expr(object);
        let concrete = resolve_ty(&obj_ty, &self.uf);
        let field_ty = self.resolve_field_type(&concrete, field);
        if matches!(field_ty, Ty::Unknown) {
            self.report_unknown_member(object, field, &concrete, member_span);
        }
        field_ty
    }

    /// Resolve `mod.name` where `mod` names a module rather than a value.
    ///
    /// Covers a stdlib signature, a user module's fn, and a cross-module
    /// top-level `let` — the Visibility section of the spec applies to `fn`,
    /// `type` AND `let`. `None` means the object is not a module reference and
    /// the caller should infer it as an ordinary expression.
    fn infer_module_qualified_member(
        &mut self,
        object: &mut ast::Expr,
        field: &almide_base::intern::Sym,
    ) -> Option<Ty> {
        if let ExprKind::Ident { name: mod_name, .. } = &object.kind {
            self.reject_dead_try_spelling(mod_name, field, object.id, object.span, None);
            self.reject_user_prim(mod_name, field, object.span);
            // Every resolved branch below marks the import used (#3241): a
            // member taken as a VALUE (`let f = process.pid`, an argument, a
            // pipe RHS) is as much a use as a call, which marks on its own path.
            if let Some(sig) = crate::stdlib::lookup_sig(mod_name, field) {
                self.record_purity_ref(super::pure_attr::PurityCallee::Stdlib(*mod_name, *field), object.span);
                self.type_map.insert(object.id, Ty::Unit); // placeholder; object isn't evaluated
                self.env.import_table.mark_used(mod_name);
                self.reject_mut_param_fn_value(&format!("{}.{}", mod_name, field), &sig, false);
                return Some(self.fn_value_ty(&sig));
            }
            let resolved_mod_name = self.env.import_table.resolve(mod_name)
                .map(|s| s.to_string())
                .unwrap_or_else(|| mod_name.to_string());
            let key = format!("{}.{}", resolved_mod_name, field);
            if let Some(sig) = self.env.functions.get(&sym(&key)).cloned() {
                self.record_purity_ref(super::pure_attr::PurityCallee::User(sym(&key)), object.span);
                self.type_map.insert(object.id, Ty::Unit);
                self.env.import_table.mark_used(mod_name);
                self.reject_mut_param_fn_value(&format!("{}.{}", mod_name, field), &sig, true);
                return Some(self.fn_value_ty(&sig));
            }
            // Cross-module top-level `let` access: `utils.CATEGORY_ORDER`.
            // Spec Visibility section applies to fn, type, AND let.
            if let Some(let_ty) = self.env.top_lets.get(&sym(&key)).cloned() {
                super::debug_trace("ALMIDE_TOPLET_DEBUG", || format!("reader: key={} -> {:?}", key, let_ty));
                self.type_map.insert(object.id, Ty::Unit);
                self.env.import_table.mark_used(mod_name);
                return Some(let_ty);
            }
            // Cross-module variant constructor as value: dispatch.Never, binary.ImportFunc.
            // Owner-filtered (#1426): resolve inside the named module alone.
            let resolved_mod = self.env.import_table.resolve(mod_name)
                .unwrap_or(sym(mod_name));
            if let Some((type_name, case)) = self.env.lookup_ctor_owned(&sym(field), resolved_mod.as_str()) {
                let qualified = format!("{}.{}", resolved_mod.as_str(), type_name.as_str());
                if self.env.types.contains_key(&sym(&qualified)) {
                    self.type_map.insert(object.id, Ty::Unit);
                    self.env.import_table.mark_used(mod_name);
                    // #433: return the qualified `mod.Type` (it exists and was
                    // just confirmed) so the binding mangles to the namespaced
                    // struct, not the ambiguous bare name.
                    let qual_ty = sym(&qualified);
                    // A payload-carrying case is the same function value the
                    // bare `Ctor` is — its params instantiated with the SAME
                    // fresh vars as its result (#2925's sweep: they were the
                    // declaration's own `T`, so `list.map(xs, m.Box)` left the
                    // result's element unconstrained).
                    return Some(match &case.payload {
                        VariantPayload::Tuple(_) => self.ctor_fn_value_ty(&case, &qualified, qual_ty),
                        VariantPayload::Unit | VariantPayload::Record(_) => {
                            let generic_args = self.instantiate_type_generics(&qualified);
                            Ty::Named(qual_ty, generic_args)
                        }
                    });
                }
            }
        }
        None
    }

    /// Emit the E013 for a field access that resolved to no field.
    ///
    /// LLMs trained on Haskell / Python / Ruby write `xs.head`, `xs.tail`,
    /// `xs.length`, `s.length`. In Almide those are stdlib calls, so the
    /// diagnostic is intercepted here and carries the mechanical rewrite —
    /// otherwise rustc leaks `error[E0609]: no field 'head' on type 'Vec<i64>'`
    /// from generated code the user never wrote.
    fn report_unknown_member(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
        self.report_missing_record_field(object, field, concrete, member_span);
        self.suggest_stdlib_for_member(object, field, concrete, member_span);
    }

    /// #847: a MISSING field on a CLOSED record used to sail through as
    /// `Unknown` with no diagnostic at all — the failure surfaced as a codegen
    /// postcondition ICE, or leaked rustc's E0609 from code the user never wrote.
    /// Reported here with the record's field roster.
    fn report_missing_record_field(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
    // #1120: `.field` on an Option (forgetting the `?`) used to sail through
    // as Unknown and die at the ConcretizeTypes wall. Suggest the operator
    // that exists for exactly this: `?.` (ADR-0005 D2).
    if let Ty::Applied(crate::types::TypeConstructorId::Option, args) = &concrete {
        let inner_display = args.first().map(|t| t.display()).unwrap_or_else(|| "T".to_string());
        let mut diag = super::err(
            format!("field access '.{}' on {} — the value is optional", field, concrete.display()),
            format!("Use optional chaining: `?.{f}` yields Option[field type] ({inner} may be absent). \
                     To unwrap first: `?? fallback`, or `match` on some/none.", f = field, inner = inner_display),
            format!("field access .{}", field),
        ).with_code("E013");
        // SUGGESTION, not machine-applicable (#1312): `?.` yields
        // `Option[field]` where the surrounding code asked for the field
        // itself, so applying it moves the problem one expression out.
        // Unwrapping (`??`, `match`) is the other legal reading.
        if let (Some(span), Some(obj_src)) = (member_span, object.span.and_then(|s| self.source_slice(s))) {
            diag = diag.with_suggested_fix(
                span.line, span.col, span.end_col,
                format!("{}?.{}", obj_src, field),
            );
        }
        self.emit(diag);
        return;
    }
    // #847: a MISSING field on a closed record used to sail
    // through as Unknown (no diagnostic at all — the failure
    // surfaced as a codegen postcondition ICE, or leaked
    // rustc's E0609). Report it here with the field roster.
    let record_shape = self.env.resolve_named(&concrete);
    if let Ty::Record { fields } = &record_shape {
        let available = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        let suggestion = almide_base::diagnostic::suggest(
            field, fields.iter().map(|(n, _)| n.as_str()));
        let hint = match &suggestion {
            Some(close) => format!("Did you mean `{}`? Available fields: {}", close, available),
            None => format!("Available fields: {}", available),
        };
        let hint = format!("{}{}", hint, self.stdlib_shadow_note(concrete).unwrap_or_default());
        let mut diag = super::err(
            format!("no field '{}' on {}", field, concrete.display()),
            hint,
            format!("field access .{}", field),
        ).with_code("E013");
        // SUGGESTION: the field name came from an edit distance over the
        // record's roster — a plausible neighbour, not a known intent.
        if let (Some(close), Some(span)) = (&suggestion, member_span) {
            if let Some(obj_src) = object.span.and_then(|s| self.source_slice(s)) {
                diag = diag.with_suggested_fix(
                    span.line, span.col, span.end_col,
                    format!("{}.{}", obj_src, close),
                );
            }
        }
        self.emit(diag);
        return;
    }
    // #1521: a field access on a concrete NON-record (`3.value`, a Bool, a
    // Map…) fell through EVERY reporter above — Option and closed records
    // here, List/String in `suggest_stdlib_for_member` — with no diagnostic,
    // and died at codegen behind the COMPILER BUG banner. Anything still
    // concrete at this point has no fields at all.
    let handled_elsewhere = matches!(&concrete,
        Ty::Applied(crate::types::TypeConstructorId::List, _) | Ty::String);
    let opaque = concrete.contains_typevar()
        || matches!(&concrete, Ty::Unknown | Ty::Never)
        || matches!(&record_shape, Ty::OpenRecord { .. });
    if !handled_elsewhere && !opaque {
        // #2771: on an UNDECLARED type this is a consequence of the E029 —
        // held, and dropped once the E029 names the root cause.
        let diag = super::err(
            format!("no field '{}' on {} — the type has no fields", field, concrete.display()),
            format!(
                "Almide values outside records have no fields. Use the type's stdlib module functions instead.{}",
                self.stdlib_shadow_note(concrete).unwrap_or_default()
            ),
            format!("field access .{}", field),
        ).with_code("E013");
        self.emit_unless_unknown_type(concrete, diag);
    }
    }

    /// #1828: the value is the STDLIB's `Value` / `FileStat` / … while this
    /// program also declares a type of that name. The two spell alike, so
    /// say which one this is and where the user's own type lives — a
    /// same-name declaration never rebinds a stdlib type.
    fn stdlib_shadow_note(&self, concrete: &Ty) -> Option<String> {
        let Ty::Named(n, _) = concrete else { return None };
        let owner = almide_lang::stdlib_info::stdlib_owned_type_owner(n.as_str())?;
        let mut shadows: Vec<&str> = self.env.types.keys()
            .map(|k| k.as_str())
            .filter(|k| k.rsplit_once('.').is_some_and(|(p, base)| {
                base == n.as_str() && !almide_lang::stdlib_info::is_bundled_module(p)
            }))
            .collect();
        shadows.sort_unstable();
        let shadow = shadows.first()?;
        Some(format!(
            " Here `{n}` is the `{owner}` module's type, not your `type {n}` (which is `{shadow}`): \
             a same-name declaration never rebinds a stdlib type, so read this value through \
             `{owner}.*` — or rename your type to keep the two apart."
        ))
    }

    /// Rewrite a Haskell/Python/Ruby-style field access into the Almide stdlib
    /// call it means (`xs.head` → `list.first(xs)`).
    fn suggest_stdlib_for_member(
        &mut self,
        object: &ast::Expr,
        field: &almide_base::intern::Sym,
        concrete: &Ty,
        member_span: Option<crate::ast::Span>,
    ) {
    let module_and_subs: Option<(&str, &[MemberRewrite])> = match &concrete {
        Ty::Applied(TypeConstructorId::List, _) => Some(("list", LIST_MEMBER_REWRITES)),
        Ty::String => Some(("string", STRING_MEMBER_REWRITES)),
        _ => None,
    };
    if let Some((module, subs)) = module_and_subs {
        let matched = subs.iter().find(|r| r.field == field.as_str());
        let hint = if matched.is_some() {
            format!(
                "Almide values have no fields — use the `{m}` stdlib module. No method-call or field-access syntax is supported.",
                m = module
            )
        } else {
            format!(
                "Almide values have no fields. Use `{m}.<fn>(x)` (or `x |> {m}.<fn>`) — see docs/stdlib/{m}.md for available functions.",
                m = module
            )
        };
        let mut diag = super::err(
            format!("no field '{}' on {}", field, module),
            hint,
            format!("field access .{}", field),
        ).with_code("E013");
        if let Some(rule) = matched {
            // Mechanical rewrite: substitute the object's
            // source text into `args_tpl`. `member_span`
            // now covers the full `object.field` (parser
            // upgrade from the E002 arc), so replacing
            // that range leaves the surrounding source
            // intact. Falls back to a display-only
            // snippet when source text isn't available.
            let rewrite = object.span
                .and_then(|s| self.source_slice(s))
                .and_then(|obj_src| {
                    let span = member_span?;
                    let args = rule.args_tpl.replace("{0}", &obj_src);
                    Some((span, format!("{}{}", rule.fn_name, args)))
                });
            if let Some((span, snippet)) = rewrite {
                // #1312: the per-cell applicability decides whether
                // `almide fix` may apply this unattended.
                diag = if rule.applicability.is_machine_applicable() {
                    diag.with_machine_fix(span.line, span.col, span.end_col, snippet)
                } else {
                    diag.with_suggested_fix(span.line, span.col, span.end_col, snippet)
                };
            } else {
                let display = format!(
                    "{}{}{}",
                    rule.fn_name,
                    rule.args_tpl.replace("{0}", "xs"),
                    rule.display_suffix,
                );
                diag = diag.with_try(display);
            }
        }
        self.emit(diag);
    }
    }

    fn infer_expr_tuple_index(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::TupleIndex { object, index, .. } = &mut expr.kind else { unreachable!("infer_expr_tuple_index called on the wrong ExprKind") };
                let obj_ty = self.infer_expr(object);
                if let Ty::Tuple(elems) = &obj_ty {
                    if *index < elems.len() { return elems[*index].clone(); }
                }
                let concrete = resolve_ty(&obj_ty, &self.uf);
                match &concrete {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    // Out of range on a KNOWN tuple: a check-time error, never a
                    // silent `Unknown` (#1266 — the Unknown sailed through check
                    // and died at build as a [COMPILER BUG] banner that told the
                    // user their own type error was ours).
                    Ty::Tuple(elems) => {
                        self.emit(
                            super::err(
                                format!(
                                    "tuple index .{index} is out of range for {} (valid: .0 through .{})",
                                    concrete.display(),
                                    elems.len() - 1
                                ),
                                format!("the tuple has {} element(s)", elems.len()),
                                "tuple index",
                            )
                            .with_code("E045"),
                        );
                        Ty::Unknown
                    }
                    // Object's type is still an open inference var (e.g. a
                    // fresh lambda param yet to be bound by its call site).
                    // Park a fresh result var and resolve it once the
                    // union-find binds the object to a concrete `Tuple`
                    // (see `Checker::resolve_deferred_tuple_indices`).
                    // Without this deferral the body type freezes to
                    // `Unknown` here and propagates outward — breaking
                    // chains like `xs |> list.map((p) => p.1) |>
                    // list.fold(0.0, (a, b) => a + b)` where the fold's
                    // element-typed lambda param gets no constraint.
                    Ty::TypeVar(name) if name.starts_with('?') => {
                        let result = self.fresh_var();
                        self.deferred_tuple_indices.push((obj_ty, *index, result.clone()));
                        result
                    }
                    // An object that already failed to type keeps its silence —
                    // the upstream error owns the report, a second one is noise.
                    Ty::Unknown => Ty::Unknown,
                    // `.k` on a concrete NON-tuple (`n.0` over Int): the other
                    // half of #1266, also a check-time error now.
                    other => {
                        self.emit(
                            super::err(
                                format!("tuple index .{index} on non-tuple type {}", other.display()),
                                "only tuple values support positional .k access",
                                "tuple index",
                            )
                            .with_code("E045"),
                        );
                        Ty::Unknown
                    }
                }
    }

    fn infer_expr_optional_chain(&mut self, expr: &mut ast::Expr) -> Ty {
        let ExprKind::OptionalChain { expr: inner, field, .. } = &mut expr.kind else { unreachable!("infer_expr_optional_chain called on the wrong ExprKind") };
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner() {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    return self.fresh_var();
                } else {
                    // ADR-0005 D2 (#1107): the Result misuse gets its own
                    // code and the canonical unwrap ladder as the hint —
                    // `?.` is Option-only by definition (`o?.f ≡
                    // option.map(o, (v) => v.f)`).
                    let hint = if resolved.is_result() {
                        "'?.' is Option-only. For a Result, convert first: `r?` turns \
                         Result into Option (err → none), so `r?.field` becomes \
                         `(r?)?.field`; or unwrap with `?? fallback` / `match` / `!` \
                         (effect fn) and access the field directly."
                    } else {
                        "Use '?.' only on Option[T] values"
                    };
                    self.emit(super::err(
                        format!("operator '?.' requires Option type but got {}", resolved.display()),
                        hint,
                        "operator ?.",
                    ).with_code("E055"));
                    return Ty::Unknown;
                };
                // Resolve field type from inner_ty
                match &inner_ty {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => {
                        if let Some((_, field_ty)) = fields.iter().find(|(n, _)| n == field) {
                            Ty::option(field_ty.clone())
                        } else {
                            self.emit(super::err(
                                format!("field '{}' not found on type {}", field, inner_ty.display()),
                                "Check the field name",
                                format!("field {}", field),
                            ));
                            Ty::Unknown
                        }
                    }
                    _ => {
                        let field_ty = self.resolve_field_type(&inner_ty, field);
                        if !matches!(field_ty, Ty::Unknown) {
                            Ty::option(field_ty)
                        } else {
                            self.emit(super::err(
                                format!("cannot access field '{}' on type {}", field, inner_ty.display()),
                                "Optional chaining requires a record type inside Option",
                                format!("field {}", field),
                            ).with_code("E013"));
                            Ty::Unknown
                        }
                    }
                }
    }
}
