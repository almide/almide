// Continuation of `impl Checker` from calls.rs — UFCS member-call target
// resolution (`object.field(...)`) and TypeName constructor-call resolution
// (`Ctor(...)`). Split out to keep calls.rs under the 800-line codopsy
// max-lines threshold; pure text move, same module scope via `include!`
// (no privacy boundary — mirrors the mod_p2.rs/mod_p3.rs pattern already
// used elsewhere in this crate).

impl Checker {
    /// The `Member { object, field }` callee arm of [`Self::check_call_with_type_args`] — `object.field(...)`: static module/alias/TypeName/codec resolution, then the UFCS ladder (Fn-typed record field, builtin-module method, convention method, protocol method on a TypeVar, user-fn UFCS, cross-module UFCS), falling back to the E002 "no method syntax" diagnostic or a callable-object constraint. Verbatim text move: each step is an independent guard that either returns a resolved `Ty` or falls through to the next.
    fn check_call_target_member(
        &mut self,
        object: &mut ast::Expr,
        field: &Sym,
        args: &[ast::Expr],
        arg_tys: &[Ty],
        callee_span_snapshot: Option<ast::Span>,
    ) -> Ty {
        self.arg_spans = args.iter().map(|a| a.span).collect();
        // Try static resolution: module.func, alias.func, TypeName.method, codec.encode Thread the callee's span so `E002` can emit a mechanically-applicable `try_replace` when the stdlib alias map supplies a clean rename target.
        let prev = self.callee_span_hint.take();
        self.callee_span_hint = callee_span_snapshot;
        let resolved = self.resolve_static_member(object, field, arg_tys);
        self.callee_span_hint = prev;
        if let Some(result) = resolved {
            let arg_refs: Vec<&ast::Expr> = args.iter().collect();
            self.validate_mut_args(&format!("{}.{}", if let ExprKind::Ident { name, .. } = &object.kind { name.as_str() } else { "?" }, field), &arg_refs);
            return result;
        }
        // UFCS method: obj.method(args) -> module.method(obj, args)
        let obj_ty = self.infer_expr(object);
        let obj_concrete = resolve_ty(&obj_ty, &self.uf);
        let field = field.clone();

        if let Some(ty) = self.check_call_target_record_field(&obj_concrete, &field, arg_tys) {
            return ty;
        }
        // Built-in generic types -> stdlib module UFCS
        let builtin_module = builtin_module_for_type(&obj_concrete);
        if let Some(ty) = self.check_call_target_builtin_ufcs(builtin_module, &field, &obj_ty, arg_tys) {
            return ty;
        }
        if let Some(ty) = self.check_call_target_convention(&obj_concrete, &field, &obj_ty, arg_tys) {
            return ty;
        }
        if let Some(ty) = self.check_call_target_typevar_protocol(&obj_concrete, &field) {
            return ty;
        }
        // UFCS: user-defined function obj.func(args) -> func(obj, args)
        if self.env.functions.contains_key(&sym(&field)) {
            let mut all_args = vec![obj_ty];
            all_args.extend(arg_tys.iter().cloned());
            return self.check_named_call(&field, &all_args);
        }
        if let Some(ty) = self.check_call_target_cross_module_ufcs(&obj_concrete, &field, &obj_ty, arg_tys) {
            return ty;
        }
        if let Some(ty) = self.check_call_target_e002_hint(builtin_module, &field, object) {
            return ty;
        }
        let ret = self.fresh_var();
        self.constrain(obj_ty, Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "method call");
        ret
    }
    /// Record field call: `h.run("hello")` where `run` is a Fn-typed field. Must be checked before UFCS so field-access + call takes priority. Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_record_field(&mut self, obj_concrete: &Ty, field: &Sym, arg_tys: &[Ty]) -> Option<Ty> {
        let field_ty = self.resolve_field_type(obj_concrete, field);
        if let Ty::Fn { params, ret } = &field_ty {
            // Validate argument count
            if arg_tys.len() != params.len() {
                self.emit(super::err(
                    format!("field '{}' expects {} argument(s) but got {}", field, params.len(), arg_tys.len()),
                    "Check the number of arguments", format!("call to .{}()", field)).with_code("E004"));
            }
            // Unify argument types with parameter types
            for (aty, pty) in arg_tys.iter().zip(params.iter()) {
                self.constrain(pty.clone(), aty.clone(), format!("call to .{}()", field));
            }
            return Some(ret.as_ref().clone());
        }
        None
    }
    /// Built-in generic types -> stdlib module UFCS (`xs.len()` -> `list.len(xs)`). Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_builtin_ufcs(&mut self, builtin_module: Option<&str>, field: &Sym, obj_ty: &Ty, arg_tys: &[Ty]) -> Option<Ty> {
        let module = builtin_module?;
        let key = format!("{}.{}", module, field);
        if self.env.functions.contains_key(&sym(&key))
            || crate::stdlib::resolve_ufcs_candidates(field).contains(&module)
        {
            let mut all_args = vec![obj_ty.clone()];
            all_args.extend(arg_tys.iter().cloned());
            return Some(self.check_named_call(&key, &all_args));
        }
        None
    }
    /// Convention method: `dog.repr()` -> `Dog.repr(dog)`. Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_convention(&mut self, obj_concrete: &Ty, field: &Sym, obj_ty: &Ty, arg_tys: &[Ty]) -> Option<Ty> {
        let type_name_opt = self.resolve_type_name(obj_concrete);
        if let Some(type_name) = type_name_opt {
            let convention_key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&convention_key)) {
                let mut all_args = vec![obj_ty.clone()];
                all_args.extend(arg_tys.iter().cloned());
                return Some(self.check_named_call(&convention_key, &all_args));
            }
        }
        None
    }
    /// Protocol method on TypeVar: `item.show()` where `item: T, T: Showable`. Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_typevar_protocol(&mut self, obj_concrete: &Ty, field: &Sym) -> Option<Ty> {
        if let Ty::TypeVar(tv) = obj_concrete {
            if let Some(proto_names) = self.env.generic_protocol_bounds.get(tv).cloned() {
                for proto_name in &proto_names {
                    if let Some(proto_def) = self.env.protocols.get(proto_name).cloned() {
                        if let Some(method_sig) = proto_def.methods.iter().find(|m| m.name == *field) {
                            // Resolve method return type: substitute Self -> T (the TypeVar)
                            let ret = self.substitute_self_in_ty(&method_sig.ret, obj_concrete);
                            return Some(ret);
                        }
                    }
                }
            }
        }
        None
    }
    /// Cross-module UFCS: find the module that defines the object's type, then check if `module.method` exists. Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_cross_module_ufcs(&mut self, obj_concrete: &Ty, field: &Sym, obj_ty: &Ty, arg_tys: &[Ty]) -> Option<Ty> {
        let cross_type_name = match obj_concrete {
            Ty::Named(n, _) => Some(n.to_string()),
            _ => None,
        };
        if let Some(type_name) = cross_type_name {
            // A pinned QUALIFIED type name (`box.Box` — the #433 canonical form every checked expr now carries) names its defining module directly. The suffix scan below only ever matched historical BARE names, so cross-module UFCS silently fell through to the callable-object fallback and E001'd (ceangal's `count.get()`).
            let defining_module = match type_name.rsplit_once('.') {
                Some((m, _)) => Some(m.to_string()),
                None => self.env.types.keys()
                    .find(|k| {
                        let s = k.as_str();
                        s.ends_with(&format!(".{}", type_name))
                            && s.len() > type_name.len() + 1
                    })
                    .map(|k| k.as_str()[..k.as_str().len() - type_name.len() - 1].to_string()),
            };
            if let Some(module) = defining_module {
                let key = format!("{}.{}", module, field);
                if self.env.functions.contains_key(&sym(&key)) {
                    let mut all_args = vec![obj_ty.clone()];
                    all_args.extend(arg_tys.iter().cloned());
                    return Some(self.check_named_call(&key, &all_args));
                }
            }
        }
        None
    }
    /// Almide-specific hint: method-call syntax isn't supported. If `obj_ty` maps to a stdlib module, suggest the module-call form (plus the closest existing name if there's a typo). Verbatim text move out of [`Self::check_call_target_member`].
    fn check_call_target_e002_hint(&mut self, builtin_module: Option<&str>, field: &Sym, object: &ast::Expr) -> Option<Ty> {
        let module = builtin_module?;
        // Use the *full* surface (TOML + bundled `.almd`) so fns migrated through the Stdlib Unification arc still power the E002 suggestion. `module_functions` only sees TOML, so after `stdlib/string.almd` replaced the TOML the method-call try-snippet silently disappeared.
        let module_funcs = crate::stdlib::module_functions_all(module);
        let suggestion = almide_base::diagnostic::suggest(field, module_funcs.iter().copied());
        let hint = if let Some(close) = &suggestion {
            format!(
                "Almide doesn't use method-call syntax. Write `{m}.{close}(x)` (or `x |> {m}.{close}`). Method syntax `x.{field}()` is not supported.",
                m = module, close = close, field = field
            )
        } else {
            format!(
                "Almide doesn't use method-call syntax. Write `{m}.<fn>(x)` (or `x |> {m}.<fn>`) — there is no method `{field}` on `{m}`. Run `almide explain E002` for examples.",
                m = module, field = field
            )
        };
        let mut diag = super::err(
            format!("undefined method '{}' on {}", field, module),
            hint,
            format!("method call .{}()", field)
        ).with_code("E002");
        if let Some(close) = suggestion {
            // Mechanical rewrite path: if we have the object's source text AND the full call span, substitute `x.field()` → `module.close(x)` in place. Falls back to the comment-headed display form when the source isn't reachable (IDE / playground).
            let rewrite = object.span
                .and_then(|s| self.source_slice(s))
                .and_then(|obj_src| {
                    let call_span = self.call_span_hint?;
                    Some((call_span, format!("{}.{}({})", module, close, obj_src)))
                });
            if let Some((call_span, snippet)) = rewrite {
                diag = diag.with_try_replace(
                    call_span.line, call_span.col, call_span.end_col,
                    snippet,
                );
            } else {
                diag = diag.with_try(format!(
                    "// x.{field}()  →  {m}.{close}(x)\n{m}.{close}(x)",
                    m = module, close = close, field = field
                ));
            }
        }
        self.emit(diag);
        Some(Ty::Unknown)
    }
    /// The `TypeName(..)` callee arm of [`Self::check_call_with_type_args`] — record/variant constructor calls. Verbatim text move (#781).
    fn check_type_name_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        // #631: pin the constructed value's `.ty` to the OWNER-qualified type name (`mod.Type`) via the module-aware lookup, exactly as the bare-value ctor path (infer.rs) and `lookup_ctor_in` for record ctors already do. A bare `lookup_ctor` here left the call expression's type bare `Type` even when the only declaration is `mod.Type`, so a producer fn INSIDE its owning submodule that constructs the variant tripped the #433 name-pinning guard at codegen (both targets aborted after `check` said clean).
        if let Some((type_name, case)) =
            self.env.lookup_ctor_in(&sym(name), self.current_module_prefix.as_deref())
        {
            self.check_type_name_variant_ctor(name, arg_tys, type_name, &case)
        } else if let Some(target_ty) = self.env.opaque_alias_targets.get(&sym(name)).cloned() {
            self.check_type_name_opaque_alias(name, arg_tys, target_ty)
        } else {
            // #488: nothing claimed this TypeName call — not a variant ctor, not an opaque alias, and the record paths were intercepted before infer_call. Letting it through here is how unvalidated constructions reached rustc/wasm; make the unknown name a checker error instead.
            self.emit(super::err(
                format!("unknown type or constructor '{}' in call position", name),
                format!("No type, variant constructor, or opaque alias named '{}' is in scope. Check the spelling or add the missing import.", name),
                format!("call to {}()", name),
            ).with_code("E003"));
            Ty::Named(sym(name), vec![])
        }
    }

    // Variant-ctor path of `check_type_name_call`: `name` names a record or
    // tuple variant case, e.g. `Leaf(1)` / `Tick((Unit) -> Int)`.
    fn check_type_name_variant_ctor(&mut self, name: &str, arg_tys: &[Ty], type_name: Sym, case: &crate::types::VariantCase) -> Ty {
        self.report_ambiguous_ctor(name);
        self.check_constructor_args(name, case, arg_tys);
        // Instantiate parent type's generics with fresh inference vars
        let generic_args = self.instantiate_type_generics(type_name.as_str());
        // Unify each constructor arg with its payload type. For a GENERIC variant this resolves the parent's vars (Leaf(1) → T=Int); for ANY variant it also propagates a CONCRETE payload type — e.g. a function payload `Tick((Unit) -> Int)` — into a lambda arg's otherwise-unconstrained params. Without it a closure payload's unused param stays unresolved and the WASM closure signature mismatched the call site (an indirect-call trap). Was gated on `!generic_args.is_empty()`, so non-generic variants were skipped.
        let subst: std::collections::HashMap<almide_base::intern::Sym, Ty> = if !generic_args.is_empty() {
            self.env.types.get(&sym(type_name.as_str())).cloned().map(|ty_def| {
                let mut type_var_names = Vec::new();
                crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
                type_var_names.iter().zip(generic_args.iter())
                    .map(|(tv, fresh)| (*tv, fresh.clone()))
                    .collect()
            }).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
        if let crate::types::VariantPayload::Tuple(expected) = &case.payload {
            for (aty, ety) in arg_tys.iter().zip(expected.iter()) {
                let substituted = subst_ty(ety, &subst);
                self.unify_infer(aty, &substituted);
            }
        }
        Ty::Named(type_name, generic_args)
    }

    // Opaque-alias-ctor path of `check_type_name_call`, e.g. `SafeHtml("hello")`.
    fn check_type_name_opaque_alias(&mut self, name: &str, arg_tys: &[Ty], target_ty: Ty) -> Ty {
        let vis = self.env.opaque_alias_visibility.get(&sym(name)).copied()
            .unwrap_or(ast::Visibility::Public);
        if !matches!(vis, ast::Visibility::Public) {
            // Check if we're in the defining module
            let defining_module = self.env.opaque_alias_module.get(&sym(name))
                .cloned().flatten();
            let current_module = self.env.self_module_name
                .or(self.current_module_prefix.as_ref().map(|p| sym(p)));
            let allowed = match (&defining_module, &current_module) {
                (None, None) => true,       // defined in main, used in main
                (Some(def), Some(cur)) => def == cur, // same module
                _ => false,                 // cross-module
            };
            if !allowed {
                self.emit(super::err(
                    format!("cannot construct opaque type '{}' outside its defining module", name),
                    format!("Use the module's public API to create '{}' values", name),
                    format!("constructor {}()", name),
                ).with_code("E008"));
            }
        }
        if arg_tys.len() != 1 {
            self.emit(super::err(
                format!("{}() takes exactly 1 argument but got {}", name, arg_tys.len()),
                "Opaque type constructor wraps a single value",
                format!("constructor {}()", name),
            ).with_code("E004"));
        } else {
            self.constrain(target_ty, arg_tys[0].clone(), format!("constructor {}()", name));
        }
        Ty::Named(sym(name), vec![])
    }
    /// Resolve a concrete type to its declared type name.
    fn resolve_type_name(&self, ty: &Ty) -> Option<String> {
        match ty {
            Ty::Named(name, _) => Some(name.to_string()),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().find_map(|(name, def)| {
                    (def == ty && name.starts_with(|c: char| c.is_uppercase())).then(|| name.to_string())
                })
            }
            _ => None,
        }
    }
    /// Resolve a type to its name for protocol checking purposes. Handles Named types, Records/Variants (by looking up type definitions), and TypeVars (which are not concrete — returns None to skip checking).
    fn resolve_type_name_for_protocol(&self, ty: &Ty) -> Option<Sym> {
        match ty {
            Ty::Named(name, _) => Some(*name),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().find_map(|(name, def)| {
                    (def == ty && name.starts_with(|c: char| c.is_uppercase())).then(|| *name)
                })
            }
            // TypeVars and inference vars are not concrete — skip protocol checking
            Ty::TypeVar(_) | Ty::Unknown => None,
            _ => Self::primitive_protocol_type_name(ty),
        }
    }

    // Numeric primitive types — canonicalised so `T: Numeric` bounds can look
    // them up in `env.type_protocols` (the `register_builtin_protocols` pass
    // seeds this table with the primitive ↔ `Numeric` links).
    fn primitive_protocol_type_name(ty: &Ty) -> Option<Sym> {
        match ty {
            Ty::Int => Some(sym("Int")),
            Ty::Float => Some(sym("Float")),
            Ty::Int8 => Some(sym("Int8")),
            Ty::Int16 => Some(sym("Int16")),
            Ty::Int32 => Some(sym("Int32")),
            Ty::UInt8 => Some(sym("UInt8")),
            Ty::UInt16 => Some(sym("UInt16")),
            Ty::UInt32 => Some(sym("UInt32")),
            Ty::UInt64 => Some(sym("UInt64")),
            Ty::Float32 => Some(sym("Float32")),
            Ty::String => Some(sym("String")),
            Ty::Bool => Some(sym("Bool")),
            Ty::Bytes => Some(sym("Bytes")),
            Ty::Matrix => Some(sym("Matrix")),
            Ty::Unit => Some(sym("Unit")),
            _ => None,
        }
    }
}
