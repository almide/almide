/// Call type checking — resolves function calls, builtins, variant constructors.

use std::collections::HashMap;
use almide_lang::ast;
use almide_lang::ast::ExprKind;
use almide_base::intern::{Sym, sym};
use crate::types::Ty;
use super::types::resolve_ty;
use super::Checker;
pub(crate) use super::builtin_calls::{builtin_module_for_type, types_mismatch};

/// Substitute named TypeVars in a type with replacement types.
pub(crate) fn subst_ty(ty: &Ty, subst: &HashMap<Sym, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Applied(id, args) => Ty::Applied(id.clone(), args.iter().map(|a| subst_ty(a, subst)).collect()),
        Ty::Named(name, args) => Ty::Named(*name, args.iter().map(|a| subst_ty(a, subst)).collect()),
        Ty::Fn { params, ret } => Ty::Fn { params: params.iter().map(|p| subst_ty(p, subst)).collect(), ret: Box::new(subst_ty(ret, subst)) },
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| subst_ty(e, subst)).collect()),
        Ty::Record { fields } => Ty::Record { fields: fields.iter().map(|(n, t)| (*n, subst_ty(t, subst))).collect() },
        _ => ty.clone(),
    }
}

impl Checker {
    /// Report a bare constructor name declared in more than one variant type
    /// (e.g. a local type and a dependency's) — an ambiguous name (#413). The
    /// caller still resolves to the first candidate; this surfaces the conflict as
    /// a clear source-level error so the user qualifies/renames, instead of the
    /// silent wrong-type resolution that later fails as a cryptic generated-Rust
    /// E0769. Returns true if it was ambiguous.
    pub(crate) fn report_ambiguous_ctor(&mut self, name: &str) -> bool {
        let key = sym(name);
        if self.env.ctor_candidate_count(&key) > 1 {
            let types = self.env.ctor_candidate_types(&key).iter()
                .map(|t| t.as_str().to_string())
                .collect::<Vec<_>>().join(" and ");
            self.emit(super::err(
                format!("ambiguous constructor '{}': declared in {}", name, types),
                format!("Rename the constructor in one of them so its name is unique (a qualified `Type.{}` is not yet supported)", name),
                format!("constructor {}", name),
            ).with_code("E019"));
            true
        } else {
            false
        }
    }

    /// Resolve the callee of a call to its function signature, for the two
    /// shapes that can name a higher-order function with a `Fn`-typed parameter:
    /// a bare `Ident` (user fn / selectively-imported stdlib fn) or
    /// `module.field` (`list.map`, an aliased import, or a user `module.fn`).
    /// Returns the signature so the eager-arg pass can pin an inferred lambda
    /// param to the element type BEFORE the lambda body is checked. Returns
    /// `None` for anything else (the call then infers args bottom-up as before).
    fn lookup_call_sig(&self, callee: &ast::Expr) -> Option<crate::types::FnSig> {
        match &callee.kind {
            ExprKind::Ident { name, .. } => {
                self.env.functions.get(&sym(name)).cloned()
            }
            ExprKind::Member { object, field, .. } => {
                let module = match &object.kind {
                    ExprKind::Ident { name, .. } => name.as_str(),
                    _ => return None,
                };
                // Honor import aliases (e.g. `gpu` -> `snaidhm.web.gpu`).
                let canonical = self.env.import_table.resolve(module)
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| module.to_string());
                let key = format!("{}.{}", canonical, field);
                self.env.functions.get(&sym(&key)).cloned()
                    .or_else(|| crate::stdlib::lookup_sig(&canonical, field))
            }
            _ => None,
        }
    }

    pub(crate) fn check_call_with_type_args(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr], type_args: Option<&[Ty]>) -> Ty {
        // Expected-type-directed argument inference (#653). The default is
        // strictly-left-to-right bottom-up inference of every argument. The one
        // place that breaks down is an INFERRED lambda param passed to a
        // higher-order function inside a generic body: `list.map(xs, (e) =>
        // e.name())` where `xs: List[T]`, `T: Labelled`. Inferred bottom-up,
        // `e` is a fresh var, so `e.name()` cannot see the protocol bound and
        // collapses `e` into a closure type (`Fn() -> String`) -- the later
        // `(T)->U` constraint can no longer undo that, yielding a spurious
        // native E0308. Fix: resolve the callee's signature up front; as we
        // infer args left-to-right we unify each non-lambda arg against its
        // declared param to learn the generic bindings (`A := T`), then, just
        // before inferring a lambda arg whose param slot is a `Fn`, pin the
        // lambda's (unannotated) params to the substituted expected element
        // type (`T`, carrying the bound). The lambda body then resolves
        // `e.name()` via the protocol path. Calls without a `Fn`-param sig are
        // unaffected -- they take the plain bottom-up path below.
        let call_sig = self.lookup_call_sig(callee);
        let arg_tys: Vec<Ty> = {
            let mut bindings: HashMap<Sym, Ty> = HashMap::new();
            let mut tys: Vec<Ty> = Vec::with_capacity(args.len());
            for (i, a) in args.iter_mut().enumerate() {
                // Pin an unannotated lambda's params to the expected element
                // types substituted with bindings learned from earlier args.
                // A slot whose substituted type still mentions one of the
                // CALLEE's OWN unbound generics (`A` when arg0 was itself an
                // unresolved inference var) gets NO pin: writing the literal
                // sig generic into the lambda param disconnects it from the
                // union-find, so it never picks up the element type that flows
                // in later and silently defaults to Int (nn variance_rows:
                // `let sq = list.map(row, (x) => …)` inside a map lambda).
                let pinned = if matches!(&a.kind, ExprKind::Lambda { .. }) {
                    call_sig.as_ref()
                        .and_then(|sig| {
                            let (_, pty) = sig.params.get(i)?;
                            let pty = crate::types::substitute(pty, &bindings);
                            let Ty::Fn { params, .. } = pty else { return None };
                            // A callee generic that no earlier arg pinned is
                            // MEANINGLESS in the caller's scope — unless its name
                            // happens to denote an IN-SCOPE rigid generic (the
                            // enclosing fn's own `T`, registered in env.types as
                            // a TypeVar), in which case the pin is exactly the
                            // #653 protocol-bound case and must survive.
                            let unbound: std::collections::HashSet<Sym> = sig.generics.iter().copied()
                                .filter(|g| !bindings.contains_key(g))
                                .filter(|g| !matches!(self.env.types.get(g), Some(Ty::TypeVar(n)) if n == g))
                                .collect();
                            let mentions_unbound = |t: &Ty| -> bool {
                                let hit = |t: &Ty| matches!(t, Ty::TypeVar(n) if unbound.contains(n));
                                hit(t) || t.any_child_recursive(&hit)
                            };
                            Some(params.into_iter()
                                .map(|t| if mentions_unbound(&t) { None } else { Some(t) })
                                .collect::<Vec<Option<Ty>>>())
                        })
                } else { None };
                let prev_hint = self.lambda_arg_hint.take();
                self.lambda_arg_hint = pinned;
                let aty = self.infer_expr(a);
                self.lambda_arg_hint = prev_hint;
                // Accumulate generic bindings from this arg so later lambda
                // params can be pinned. Lambdas contribute nothing new here.
                if let Some(sig) = &call_sig {
                    if let Some((_, pty)) = sig.params.get(i) {
                        crate::types::unify(pty, &resolve_ty(&aty, &self.uf), &mut bindings);
                    }
                }
                tys.push(aty);
            }
            tys
        };
        let callee_span_snapshot = callee.span;
        match &mut callee.kind {
            ExprKind::Ident { name, .. } => {
                let name = name.clone();
                // Register callee's type for variables that hold function values
                // (Skip for builtins/functions — they don't need ExprId registration)
                if self.env.lookup_var(&name).is_some() {
                    let _ = self.infer_expr(callee);
                }
                self.arg_spans = args.iter().map(|a| a.span).collect();
                let ret = self.check_named_call_spanned(&name, &arg_tys, type_args, callee_span_snapshot);
                let arg_refs: Vec<&ast::Expr> = args.iter().collect();
                self.validate_mut_args(&name, &arg_refs);
                ret
            }
            ExprKind::TypeName { name, .. } => {
                // #631: pin the constructed value's `.ty` to the OWNER-qualified
                // type name (`mod.Type`) via the module-aware lookup, exactly as
                // the bare-value ctor path (infer.rs) and `lookup_ctor_in` for
                // record ctors already do. A bare `lookup_ctor` here left the call
                // expression's type bare `Type` even when the only declaration is
                // `mod.Type`, so a producer fn INSIDE its owning submodule that
                // constructs the variant tripped the #433 name-pinning guard at
                // codegen (both targets aborted after `check` said clean).
                if let Some((type_name, case)) =
                    self.env.lookup_ctor_in(&sym(name), self.current_module_prefix.as_deref())
                {
                    self.report_ambiguous_ctor(name);
                    self.check_constructor_args(name, &case, &arg_tys);
                    // Instantiate parent type's generics with fresh inference vars
                    let generic_args = self.instantiate_type_generics(type_name.as_str());
                    // Unify each constructor arg with its payload type. For a GENERIC
                    // variant this resolves the parent's vars (Leaf(1) → T=Int); for
                    // ANY variant it also propagates a CONCRETE payload type — e.g. a
                    // function payload `Tick((Unit) -> Int)` — into a lambda arg's
                    // otherwise-unconstrained params. Without it a closure payload's
                    // unused param stays unresolved and the WASM closure signature
                    // mismatched the call site (an indirect-call trap). Was gated on
                    // `!generic_args.is_empty()`, so non-generic variants were skipped.
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
                } else if let Some(target_ty) = self.env.opaque_alias_targets.get(&sym(name)).cloned() {
                    // Opaque alias constructor: SafeHtml("hello")
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
                } else {
                    // #488: nothing claimed this TypeName call — not a variant
                    // ctor, not an opaque alias, and the record paths were
                    // intercepted before infer_call. Letting it through here is
                    // how unvalidated constructions reached rustc/wasm; make
                    // the unknown name a checker error instead.
                    self.emit(super::err(
                        format!("unknown type or constructor '{}' in call position", name),
                        format!("No type, variant constructor, or opaque alias named '{}' is in scope. Check the spelling or add the missing import.", name),
                        format!("call to {}()", name),
                    ).with_code("E003"));
                    Ty::Named(sym(name), vec![])
                }
            }
            // Module call: string.trim(s), list.map(xs, f), etc.
            ExprKind::Member { object, field, .. } => {
                self.arg_spans = args.iter().map(|a| a.span).collect();
                // Try static resolution: module.func, alias.func, TypeName.method, codec.encode
                // Thread the callee's span so `E002` can emit a
                // mechanically-applicable `try_replace` when the stdlib
                // alias map supplies a clean rename target.
                let prev = self.callee_span_hint.take();
                self.callee_span_hint = callee_span_snapshot;
                let resolved = self.resolve_static_member(object, field, &arg_tys);
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
                // Record field call: h.run("hello") where run is a Fn-typed field
                // Must check before UFCS so field-access + call takes priority
                let field_ty = self.resolve_field_type(&obj_concrete, &field);
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
                    return ret.as_ref().clone();
                }
                // Built-in generic types -> stdlib module UFCS
                let builtin_module = builtin_module_for_type(&obj_concrete);
                if let Some(module) = builtin_module {
                    let key = format!("{}.{}", module, field);
                    if self.env.functions.contains_key(&sym(&key))
                        || crate::stdlib::resolve_ufcs_candidates(&field).contains(&module)
                    {
                        let mut all_args = vec![obj_ty];
                        all_args.extend(arg_tys.iter().cloned());
                        return self.check_named_call(&key, &all_args);
                    }
                }
                // Convention method: dog.repr() -> Dog.repr(dog)
                let type_name_opt = self.resolve_type_name(&obj_concrete);
                if let Some(type_name) = type_name_opt {
                    let convention_key = format!("{}.{}", type_name, field);
                    if self.env.functions.contains_key(&sym(&convention_key)) {
                        let mut all_args = vec![obj_ty];
                        all_args.extend(arg_tys.iter().cloned());
                        return self.check_named_call(&convention_key, &all_args);
                    }
                }
                // Protocol method on TypeVar: item.show() where item: T, T: Showable
                if let Ty::TypeVar(tv) = &obj_concrete {
                    if let Some(proto_names) = self.env.generic_protocol_bounds.get(tv).cloned() {
                        for proto_name in &proto_names {
                            if let Some(proto_def) = self.env.protocols.get(proto_name).cloned() {
                                if let Some(method_sig) = proto_def.methods.iter().find(|m| m.name == field) {
                                    // Resolve method return type: substitute Self -> T (the TypeVar)
                                    let ret = self.substitute_self_in_ty(&method_sig.ret, &obj_concrete);
                                    return ret;
                                }
                            }
                        }
                    }
                }
                // UFCS: user-defined function obj.func(args) -> func(obj, args)
                if self.env.functions.contains_key(&sym(&field)) {
                    let mut all_args = vec![obj_ty];
                    all_args.extend(arg_tys.iter().cloned());
                    return self.check_named_call(&field, &all_args);
                }
                // Cross-module UFCS: find the module that defines the object's type,
                // then check if module.method exists.
                let cross_type_name = match &obj_concrete {
                    Ty::Named(n, _) => Some(n.to_string()),
                    _ => None,
                };
                if let Some(type_name) = cross_type_name {
                    let defining_module = self.env.types.keys()
                        .find(|k| {
                            let s = k.as_str();
                            s.ends_with(&format!(".{}", type_name))
                                && s.len() > type_name.len() + 1
                        })
                        .map(|k| k.as_str()[..k.as_str().len() - type_name.len() - 1].to_string());
                    if let Some(module) = defining_module {
                        let key = format!("{}.{}", module, field);
                        if self.env.functions.contains_key(&sym(&key)) {
                            let mut all_args = vec![obj_ty];
                            all_args.extend(arg_tys.iter().cloned());
                            return self.check_named_call(&key, &all_args);
                        }
                    }
                }
                // Almide-specific hint: method-call syntax isn't supported.
                // If obj_ty maps to a stdlib module, suggest the module-call
                // form (plus the closest existing name if there's a typo).
                if let Some(module) = builtin_module {
                    // Use the *full* surface (TOML + bundled `.almd`) so fns
                    // migrated through the Stdlib Unification arc still power
                    // the E002 suggestion. `module_functions` only sees TOML,
                    // so after `stdlib/string.almd` replaced the TOML the
                    // method-call try-snippet silently disappeared.
                    let module_funcs = crate::stdlib::module_functions_all(module);
                    let suggestion = almide_base::diagnostic::suggest(&field, module_funcs.iter().copied());
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
                        // Mechanical rewrite path: if we have the object's
                        // source text AND the full call span, substitute
                        // `x.field()` → `module.close(x)` in place. Falls
                        // back to the comment-headed display form when
                        // the source isn't reachable (IDE / playground).
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
                    return Ty::Unknown;
                }
                let ret = self.fresh_var();
                self.constrain(obj_ty, Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "method call");
                ret
            }
            _ => {
                let ct = self.infer_expr(callee);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "function call");
                ret
            }
        }
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

    /// Resolve a type to its name for protocol checking purposes.
    /// Handles Named types, Records/Variants (by looking up type definitions),
    /// and TypeVars (which are not concrete — returns None to skip checking).
    fn resolve_type_name_for_protocol(&self, ty: &Ty) -> Option<Sym> {
        match ty {
            Ty::Named(name, _) => Some(*name),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().find_map(|(name, def)| {
                    (def == ty && name.starts_with(|c: char| c.is_uppercase())).then(|| *name)
                })
            }
            // Numeric primitive types — canonicalised so `T: Numeric`
            // bounds can look them up in `env.type_protocols` (the
            // `register_builtin_protocols` pass seeds this table with
            // the primitive ↔ `Numeric` links).
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
            // TypeVars and inference vars are not concrete — skip protocol checking
            Ty::TypeVar(_) | Ty::Unknown => None,
            _ => None,
        }
    }

    pub(crate) fn check_named_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        self.check_named_call_with_type_args(name, arg_tys, None)
    }

    /// Like `check_named_call_with_type_args`, but also records the
    /// callee's source span so `E002` can emit a mechanically-applicable
    /// `try_replace` range when a rename suggestion is available.
    /// Prefer this over the plain variant from call sites that have the
    /// callee AST node in hand (`check_call_with_type_args` etc.).
    pub(crate) fn check_named_call_spanned(
        &mut self,
        name: &str,
        arg_tys: &[Ty],
        type_args: Option<&[Ty]>,
        callee_span: Option<ast::Span>,
    ) -> Ty {
        let prev = self.callee_span_hint.take();
        self.callee_span_hint = callee_span;
        let ty = self.check_named_call_with_type_args(name, arg_tys, type_args);
        self.callee_span_hint = prev;
        ty
    }

    pub(crate) fn check_named_call_with_type_args(&mut self, name: &str, arg_tys: &[Ty], type_args: Option<&[Ty]>) -> Ty {
        // Try builtin resolution first
        if let Some(ty) = self.check_builtin_call(name, arg_tys) {
            return ty;
        }

        // Try stdlib lookup for module-qualified calls (e.g. "string.trim").
        // Selective import (`import json.{from_string}`) lets bare `from_string`
        // resolve via direct map → "json.from_string".
        let qualified_via_direct = self.env.import_table.resolve_direct(name);
        // Resolve import alias in module-qualified calls: "gpu.create_buffer" → "snaidhm.web.gpu.create_buffer"
        let resolved_name = if let Some((module, func)) = name.split_once('.') {
            if let Some(canonical) = self.env.import_table.resolve(module) {
                Some(format!("{}.{}", canonical.as_str(), func))
            } else {
                None
            }
        } else {
            None
        };
        // DefId-based resolution: try def_map first for canonical lookup
        let sig = self.env.def_map.get(&sym(name))
            .and_then(|_did| self.env.functions.get(&sym(name)).cloned())
            .or_else(|| {
                if let Some(ref rn) = resolved_name {
                    self.env.def_map.get(&sym(rn))
                        .and_then(|_did| self.env.functions.get(&sym(rn)).cloned())
                        .or_else(|| self.env.functions.get(&sym(rn)).cloned())
                } else {
                    None
                }
            })
            .or_else(|| self.env.functions.get(&sym(name)).cloned())
            .or_else(|| {
                let (module, func) = name.split_once('.')?;
                crate::stdlib::lookup_sig(module, func)
            }).or_else(|| {
                let q = qualified_via_direct.as_ref()?;
                let (module, func) = q.split_once('.')?;
                crate::stdlib::lookup_sig(module, func)
                    .or_else(|| self.env.functions.get(&sym(q)).cloned())
            });

        // Mark the source module as used (for unused-import diagnostic).
        if qualified_via_direct.is_some() {
            if let Some(module) = self.env.import_table.direct.get(&sym(name)).copied() {
                self.env.import_table.used.insert(module);
            }
        }

        let Some(sig) = sig else {
            self.last_mut_params = vec![];
            // No function signature found — try constructor, variable, or report error
            if let Some((type_name, case)) = self.env.lookup_ctor(&sym(name)) {
                self.check_constructor_args(name, &case, arg_tys);
                let generic_args = self.instantiate_type_generics(type_name.as_str());
                return Ty::Named(type_name, generic_args);
            }
            if let Some(ty) = self.env.lookup_var(name).cloned() {
                if let Ty::Fn { params, ret } = &ty {
                    arg_tys.iter().zip(params.iter()).for_each(|(aty, pty)| {
                        self.constrain(pty.clone(), aty.clone(), format!("call to {}()", name));
                    });
                    return ret.as_ref().clone();
                }
                // #558: `n(args)` where `n` is a NON-function local — the call
                // position makes this an error. Previously this returned the
                // var's own type unchecked, so the program passed `check` and
                // then ICE'd in the wasm emitter (`call target not in func_map`)
                // / leaked a raw rustc E0425 natively.
                let rty = resolve_ty(&ty, &self.uf);
                if !matches!(rty, Ty::Unknown | Ty::TypeVar(_)) {
                    self.emit(super::err(
                        format!("`{}` is not a function — it has type {}", name, rty.display()),
                        format!("`{}` is a value; only functions and closures can be called", name),
                        format!("call to {}()", name)).with_code("E002"));
                    return Ty::Unknown;
                }
                // #623: `f` is an as-yet-unresolved inference var being CALLED — so
                // it MUST be a function. Constrain it to `(arg_tys) -> ?ret` and
                // return `?ret`, not `f`'s own type. Returning `ty` typed the call
                // result as f's CLOSURE type (e.g. `(f) => f(10)` in a `list.map`
                // lambda became `((Int)->Int) -> ((Int)->Int)` instead of
                // `((Int)->Int) -> Int`), so codegen emitted a closure body that
                // returns a closure where it returns the call result (invalid Rust
                // / wrong wasm). `?ret` is resolved from context — e.g. the element
                // type `(Int)->Int` flowing in from `list.map` pins `?ret = Int`.
                let ret = self.fresh_var();
                let fn_ty = Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) };
                self.constrain(fn_ty, ty, format!("call to {}()", name));
                return ret;
            }
            // Triple: (hint, clean fn-name fix for simple try:, rich multi-line snippet).
            // `rich_snippet` overrides `fix_name` when present — used for hallucinations
            // that need a conversion wrapper or operator rewrite rather than a rename.
            let (hint, fix_name, rich_snippet): (String, Option<String>, Option<&'static str>) = {
                // For module-qualified calls (e.g. "string.uppercase"), narrow candidates
                // to the same module and compare only the function part for better suggestions.
                if let Some((module, func)) = name.split_once('.') {
                    // Use the *full* surface (TOML + bundled) so diagnostic
                    // suggestions see fns migrated to `stdlib/<m>.almd` even
                    // after their TOML entries have been deleted.
                    let module_funcs = crate::stdlib::module_functions_all(module);
                    if !module_funcs.is_empty() {
                        // Check known alias map first (catches common hallucinations)
                        if let Some(alias) = crate::stdlib::suggest_alias(module, func) {
                            // Aliases can be free text like "xs + [x]"; only treat as a
                            // copy-pasteable fn name if it matches `module.func` form.
                            let fix = is_clean_fn_name(alias).then(|| alias.to_string());
                            let rich = crate::stdlib::try_snippet_for_alias(module, func);
                            (format!("Did you mean `{}`?", alias), fix, rich)
                        } else if let Some(suggestion) = almide_base::diagnostic::suggest(func, module_funcs.iter().copied()) {
                            let full = format!("{}.{}", module, suggestion);
                            (format!("Did you mean `{}`?", full), Some(full), None)
                        } else {
                            (format!("No function '{}' in module '{}'. See docs/CHEATSHEET.md for available functions", func, module), None, None)
                        }
                    } else {
                        // Unknown module — suggest across all candidates
                        let candidates = self.env.all_visible_names();
                        if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                            (format!("Did you mean `{}`?", suggestion), Some(suggestion.to_string()), None)
                        } else {
                            ("Check the function name".to_string(), None, None)
                        }
                    }
                } else {
                    let candidates = self.env.all_visible_names();
                    if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                        (format!("Did you mean `{}`?", suggestion), Some(suggestion.to_string()), None)
                    } else {
                        ("Check the function name".to_string(), None, None)
                    }
                }
            };
            // Cascade suppression: if `name` belongs to a fn whose body failed
            // to parse, the real error is already on top. Skip emitting E002
            // so the LLM focuses on the parse error, not N identical cascades.
            if self.env.failed_fn_names.contains(name) {
                return Ty::Unknown;
            }
            let mut diag = super::err(format!("undefined function '{}'", name), hint, format!("call to {}()", name)).with_code("E002");
            // `try_replace` (Phase 3): when the hint is a clean rename
            // and the callee's source span is available, emit both a
            // concise `try` and the exact replacement range so
            // `Diagnostic::apply_try_to` can rewrite the source.
            // Rich multi-line snippets (conversion wrappers, operator
            // suggestions) stay display-only via `with_try`.
            if let Some(rich) = rich_snippet {
                diag = diag.with_try(rich.to_string());
            } else if let (Some(fix), Some(span)) = (&fix_name, self.callee_span_hint) {
                // Almide `Span::end_col` is the column one past the last
                // char (same convention as lexer emit — `end_col = col +
                // token_len`). `apply_try_to` wants the exclusive upper
                // bound, so use `end_col` directly.
                diag = diag.with_try_replace(span.line, span.col, span.end_col, fix.clone());
            } else if let Some(fix) = &fix_name {
                // Fallback: no span available — fall back to the
                // comment-headed display form.
                diag = diag.with_try(format!("// {wrong}(...)  →  {right}(...)\n{right}(...)", wrong = name, right = fix));
            }
            self.emit(diag);
            return Ty::Unknown;
        };

        self.last_mut_params = sig.mut_params.clone();

        // Effect isolation: pure fn cannot call effect fn
        if sig.is_effect && !self.env.can_call_effect {
            let mut diag = super::err(
                format!("cannot call effect function '{}' from a pure function", name),
                "Mark the calling function as `effect fn`",
                format!("call to {}()", name)).with_code("E006");
            if let Some(&(line, col)) = self.env.fn_decl_spans.get(&sym(name)) {
                diag = diag.with_secondary(line, Some(col), format!("'{}' declared as effect fn here", name));
            }
            self.emit(diag);
        }

        // Validate argument count
        let min_params = match name.split_once('.') {
            Some((module, func)) => crate::stdlib::min_params(module, func).unwrap_or(sig.params.len()),
            None => self.env.fn_min_params.get(&sym(name)).copied().unwrap_or(sig.params.len()),
        };
        if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
            // Build a placeholder call showing the full signature so LLMs
            // can see exactly which args are missing / extraneous.
            let placeholder = sig.params.iter()
                .map(|(pname, pty)| format!("<{}: {}>", pname.as_str(), pty.display()))
                .collect::<Vec<_>>()
                .join(", ");
            let snippet = format!(
                "// {name}() takes {n} arg(s) — you passed {got}\n\
                {name}({placeholder})",
                name = name, n = sig.params.len(), got = arg_tys.len(),
                placeholder = placeholder,
            );
            self.emit(super::err(
                format!("{}() expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                "Check the number of arguments", format!("call to {}()", name)
            ).with_code("E004").with_try(snippet));
        }
        // Validate argument types and infer generics
        let mut bindings: HashMap<Sym, Ty> = HashMap::new();
        if let Some(ta) = type_args {
            for (gname, gty) in sig.generics.iter().zip(ta.iter()) {
                bindings.insert(*gname, gty.clone());
            }
        }
        let mut concrete_args: Vec<Ty> = arg_tys.iter().map(|a| resolve_ty(a, &self.uf)).collect();
        // #558: realign named args to the parameter they NAME before validating
        // (they were appended in source order). Reorder both the arg types and
        // their spans so E005 points at the right expression. Slots a named call
        // skips (relying on a default) are filled with the param's own type so
        // the zip below validates them trivially.
        // `aligned_raw[i] = Some(arg inference ty)` when param i was supplied
        // (positional or named); `None` when it relies on a default. The
        // constraint-propagation loop below uses this so back-propagation
        // targets the right inference var, and default slots add no constraint.
        let mut aligned_raw: Option<Vec<Option<Ty>>> = None;
        if let Some((named_start, ref names)) = self.named_arg_meta {
            if named_start <= concrete_args.len() {
                let param_index: std::collections::HashMap<Sym, usize> =
                    sig.params.iter().enumerate().map(|(i, (pn, _))| (*pn, i)).collect();
                let mut aligned: Vec<Ty> = sig.params.iter().map(|(_, t)| t.clone()).collect();
                let mut aligned_spans: Vec<Option<crate::ast::Span>> = vec![None; sig.params.len()];
                let mut raw: Vec<Option<Ty>> = vec![None; sig.params.len()];
                let mut ok = true;
                for i in 0..named_start.min(aligned.len()) {
                    aligned[i] = concrete_args[i].clone();
                    aligned_spans[i] = self.arg_spans.get(i).copied().flatten();
                    raw[i] = arg_tys.get(i).cloned();
                }
                for (k, nm) in names.iter().enumerate() {
                    let src = named_start + k;
                    match param_index.get(nm) {
                        Some(&pi) if src < concrete_args.len() => {
                            aligned[pi] = concrete_args[src].clone();
                            aligned_spans[pi] = self.arg_spans.get(src).copied().flatten();
                            raw[pi] = arg_tys.get(src).cloned();
                        }
                        _ => { ok = false; break; }
                    }
                }
                if ok {
                    concrete_args = aligned;
                    self.arg_spans = aligned_spans;
                    aligned_raw = Some(raw);
                }
            }
        }
        let mut e005_fired: Vec<bool> = Vec::new();
        for (i, ((pname, pty), aty)) in sig.params.iter().zip(concrete_args.iter()).enumerate() {
            // Point caret at the exact argument expression for E005
            let saved_span = self.current_span;
            if let Some(sp) = self.arg_spans.get(i).copied().flatten() {
                self.current_span = Some(sp);
            }
            let fired = self.unify_call_arg(name, pname, pty, aty, &sig.structural_bounds, &mut bindings);
            if !fired { self.current_span = saved_span; }
            e005_fired.push(fired);
        }
        self.arg_spans.clear();
        // #620: a generic param that NO argument pinned (because the arg was an
        // unresolved inference var — e.g. `unbox(b)` where `b` is a `list.map`
        // lambda's not-yet-resolved element) leaves its name UNBOUND in
        // `bindings`. The back-prop below would then `substitute` the param type
        // to its LITERAL generic name (`Box[TypeVar("T")]`) and leak it into the
        // union-find, where it can never be solved to the concrete type that
        // flows in later (from `list.map`'s collection). Bind each such generic
        // to a FRESH inference var (shared by the back-prop AND the return type),
        // so the relation becomes solvable. Generics an argument DID pin keep
        // their concrete binding; a concrete call is unaffected.
        for g in &sig.generics {
            bindings.entry(*g).or_insert_with(|| self.fresh_var());
        }
        // Verify protocol bounds on generic type parameters
        for (tv_name, proto_names) in &sig.protocol_bounds {
            if let Some(concrete_ty) = bindings.get(tv_name) {
                let type_name = self.resolve_type_name_for_protocol(concrete_ty);
                if let Some(type_name) = type_name {
                    for proto in proto_names {
                        let has_proto = self.env.type_protocols
                            .get(&type_name)
                            .map_or(false, |ps| ps.contains(proto));
                        if !has_proto {
                            self.emit(super::err(
                                format!("type '{}' does not implement protocol '{}'", type_name, proto),
                                format!("Add `: {}` to the type declaration: type {}: {} = ...", proto, type_name, proto),
                                format!("call to {}()", name)));
                        }
                    }
                }
            }
        }
        // Propagate resolved types back to inference variables
        // Skip constraint for args where E005 already fired (avoids duplicate E001)
        for (i, (_, pty)) in sig.params.iter().enumerate() {
            if e005_fired.get(i).copied().unwrap_or(false) { continue; }
            // The arg inference ty for param i — realigned for named calls; a
            // None slot (default-filled) gets no constraint.
            let aty = match &aligned_raw {
                Some(raw) => match raw.get(i).and_then(|o| o.clone()) { Some(t) => t, None => continue },
                None => match arg_tys.get(i) { Some(t) => t.clone(), None => continue },
            };
            let expected = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
            if expected != Ty::Unknown {
                self.constrain(expected, aty, format!("call to {}()", name));
            }
        }
        // Instantiate unresolved generics with fresh vars
        let mut final_bindings = bindings.clone();
        for g in &sig.generics {
            if !final_bindings.contains_key(g) {
                final_bindings.insert(*g, self.fresh_var());
            }
        }
        let ret = if final_bindings.is_empty() { sig.ret.clone() } else { crate::types::substitute(&sig.ret, &final_bindings) };
        // User-defined effect fn calls that return non-Result T are reported
        // as Result[T, String] in two contexts:
        //   1. test blocks — there's no enclosing effect fn to auto-`?`
        //      against, so the test sees the raw lifted Result.
        //   2. lambda bodies — codegen's ResultPropagation lifts the callee's
        //      return type but doesn't recurse into lambdas (closures can't
        //      `?`-propagate to the enclosing fn). Letting the lambda body's
        //      type stay `T` here means a `(n) => worker(n)` passes
        //      type-checking against `list.map`'s `(A) -> B` slot, only to
        //      blow up at codegen with `expected Vec<i64>, found
        //      Vec<Result<i64, String>>`. Surfacing the Result at the call
        //      site instead steers the user toward `match worker(n)
        //      { ok(v) => v, err(_) => ... }` — a real type error, not an
        //      "Almide bug" diagnostic.
        // Bundled stdlib effect fns are excluded — their `@inline_rust` /
        // `@intrinsic` templates carry their own propagation and never get
        // lifted by ResultPropagation, so their callers see raw `T`.
        // User-defined effect fns are lifted to Result[T, String] by
        // ResultPropagation. Make the checker's type match: callers always
        // see Result[T, String]. auto_unwrap in let/var bindings and
        // match arms transparently extracts T.
        // Bundled stdlib effect fns (@intrinsic/@inline_rust) are NOT
        // lifted — they carry their own Result/Option types already.
        let is_bundled_stdlib_call = name.split_once('.')
            .map(|(m, _)| almide_lang::stdlib_info::is_bundled_module(m))
            .unwrap_or(false);
        if sig.is_effect && !ret.is_result()
            && self.env.functions.contains_key(&sym(name))
            && !is_bundled_stdlib_call
        {
            return Ty::result(ret, Ty::String);
        }
        ret
    }

    /// Validate that arguments passed to `mut` parameters are mutable (`var`) bindings.
    /// Called after `check_named_call_with_type_args` which populates `self.last_mut_params`.
    pub(crate) fn validate_mut_args(&mut self, fn_name: &str, arg_exprs: &[&ast::Expr]) {
        let mut_params = std::mem::take(&mut self.last_mut_params);
        for &idx in &mut_params {
            if idx >= arg_exprs.len() { continue; }
            let arg = arg_exprs[idx];
            match &arg.kind {
                ExprKind::Ident { name, .. } => {
                    if !self.env.mutable_vars.contains(&sym(name)) {
                        self.emit(super::err(
                            format!("cannot pass immutable binding '{}' to `mut` parameter of {}()", name, fn_name),
                            format!("Declare '{}' with `var` instead of `let` to allow mutation", name),
                            format!("call to {}()", fn_name),
                        ).with_code("E007"));
                    }
                }
                // A field/element of a mutable place is itself a mutable place:
                // `list.push(box.items, x)` with `var box` (or a `mut box` param)
                // lowers to `&mut box.items`, valid Rust. Walk the member/index
                // chain down to its root identifier.
                ExprKind::Member { .. } | ExprKind::TupleIndex { .. } => {
                    match Self::place_root(arg) {
                        Some(root) if self.env.mutable_vars.contains(&sym(root)) => {}
                        Some(root) => {
                            self.emit(super::err(
                                format!("cannot mutate a field of immutable binding '{}' via `mut` parameter of {}()", root, fn_name),
                                format!("Declare '{}' with `var` instead of `let`", root),
                                format!("call to {}()", fn_name),
                            ).with_code("E007"));
                        }
                        None => {
                            self.emit(super::err(
                                format!("cannot pass a temporary expression to `mut` parameter of {}()", fn_name),
                                "Pass a mutable `var` binding (or a field/element of one)",
                                format!("call to {}()", fn_name),
                            ).with_code("E007"));
                        }
                    }
                }
                _ => {
                    self.emit(super::err(
                        format!("cannot pass a temporary expression to `mut` parameter of {}()", fn_name),
                        "Pass a mutable `var` binding instead",
                        format!("call to {}()", fn_name),
                    ).with_code("E007"));
                }
            }
        }
    }

    /// Root identifier of a place expression (member/tuple-index chain), or None
    /// if it doesn't bottom out at a plain identifier (i.e. a temporary).
    fn place_root(expr: &ast::Expr) -> Option<&str> {
        match &expr.kind {
            ExprKind::Ident { name, .. } => Some(name.as_str()),
            ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. } => {
                Self::place_root(object)
            }
            _ => None,
        }
    }

    /// Create fresh inference variables for a type's generic parameters.
    pub(crate) fn instantiate_type_generics(&mut self, type_name: &str) -> Vec<Ty> {
        // Count generics by finding TypeVars in the type definition
        if let Some(ty_def) = self.env.types.get(&sym(type_name)).cloned() {
            let mut type_vars = Vec::new();
            crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_vars);
            type_vars.iter().map(|_| {
                self.fresh_var()
            }).collect()
        } else {
            vec![]
        }
    }

    pub(super) fn check_constructor_args(&mut self, name: &str, case: &crate::types::VariantCase, arg_tys: &[Ty]) {
        if let crate::types::VariantPayload::Tuple(expected_tys) = &case.payload {
            if arg_tys.len() != expected_tys.len() {
                self.emit(super::err(
                    format!("{}() expects {} argument(s) but got {}", name, expected_tys.len(), arg_tys.len()),
                    "Check the number of arguments", format!("constructor {}()", name)));
            }
            for (i, (aty, ety)) in arg_tys.iter().zip(expected_tys.iter()).enumerate() {
                let concrete_arg = resolve_ty(aty, &self.uf);
                if concrete_arg != Ty::Unknown && !ety.compatible(&concrete_arg) {
                    // Richer hint: show the constructor signature + a conversion
                    // suggestion when the argument type is numeric / string-like.
                    let sig_shape = expected_tys.iter()
                        .map(|t| t.display()).collect::<Vec<_>>().join(", ");
                    let base = format!(
                        "{}({}) expects argument #{} to be {}, got {}",
                        name, sig_shape, i + 1, ety.display(), concrete_arg.display()
                    );
                    let hint = Self::hint_with_conversion(&base, ety, &concrete_arg);
                    self.emit(super::err(
                        format!("{}() argument {} expects {} but got {}", name, i + 1, ety.display(), concrete_arg.display()),
                        hint, format!("constructor {}()", name)).with_code("E005"));
                }
            }
        }
    }

    /// Unify a single call argument against its parameter type, updating bindings.
    /// Reports diagnostics for structural bound violations and type mismatches.
    /// Returns true if E005 was emitted (caller should skip redundant E001 constraint).
    fn unify_call_arg(
        &mut self, fn_name: &str, param_name: &Sym,
        param_ty: &Ty, arg_ty: &Ty,
        structural_bounds: &HashMap<Sym, Ty>,
        bindings: &mut HashMap<Sym, Ty>,
    ) -> bool {
        if let Ty::TypeVar(tv) = param_ty {
            if let Some(bound) = structural_bounds.get(tv) {
                let resolved = self.env.resolve_named(arg_ty);
                if bound.compatible(&resolved) || bound.compatible(arg_ty) {
                    bindings.insert(*tv, arg_ty.clone());
                    return false;
                } else {
                    self.emit(super::err(
                        format!("argument '{}' does not satisfy bound {}: got {}", param_name, bound.display(), arg_ty.display()),
                        "The argument must have the required fields",
                        format!("call to {}()", fn_name)));
                    return true;
                }
            } else {
                crate::types::unify(param_ty, arg_ty, bindings);
                return false;
            }
        } else {
            crate::types::unify(param_ty, arg_ty, bindings);
            let expected = if bindings.is_empty() { param_ty.clone() } else { crate::types::substitute(param_ty, bindings) };
            let expected_resolved = self.env.resolve_named(&expected);
            let arg_resolved = self.env.resolve_named(arg_ty);
            if types_mismatch(&expected_resolved, &arg_resolved) {
                // #740: an Int-only math builtin given a Float — point at the
                // Float-preserving sibling, not the truncating `float.to_int`.
                let float_sibling = if matches!(arg_resolved, Ty::Float)
                    && matches!(expected_resolved, Ty::Int)
                {
                    Self::math_float_sibling(fn_name)
                } else {
                    None
                };
                let hint = if let Some(sib) = float_sibling {
                    format!(
                        "`{}` is Int-only. For Floats use `{}(x)`, which preserves the Float — \
                         not `float.to_int`, which truncates",
                        fn_name, sib
                    )
                } else if let Ty::Named(name, args) = &expected {
                    let n = name.as_str();
                    let is_likely_typevar = args.is_empty()
                        && !n.is_empty()
                        && n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
                        && !self.env.types.contains_key(name)
                        && !self.env.constructors.contains_key(name);
                    if is_likely_typevar {
                        format!("'{}' is not a known type. To use it as a type parameter, declare it: fn {}[{}](...)", n, fn_name, n)
                    } else {
                        Self::hint_with_conversion("Fix the argument type", &expected, arg_ty)
                    }
                } else {
                    Self::hint_with_conversion("Fix the argument type", &expected, arg_ty)
                };
                let mut diag = super::err(
                    format!("argument '{}' expects {} but got {}", param_name, expected.display(), arg_ty.display()),
                    hint,
                    format!("call to {}()", fn_name)).with_code("E005");
                if let Some(&(line, col)) = self.env.fn_decl_spans.get(&sym(fn_name)) {
                    diag = diag.with_secondary(line, Some(col), format!("fn {}() defined here", fn_name));
                }
                // Show fix code: replace argument with conversion expression.
                // Suppressed for the math-Float-sibling case (#740): the fix is to
                // change the function, not to wrap the arg in a truncating cast.
                if float_sibling.is_none() {
                    if let Some(span) = self.current_span {
                        if let Some((_, template)) = Self::conversion_template(&expected, arg_ty) {
                            if let Some(src) = self.source_slice(span) {
                                let fixed = template.replace("{}", &src);
                                diag = diag.with_try(format!("// Try:\n{}", fixed));
                            }
                        }
                    }
                }
                self.emit(diag);
                return true;
            }
            false
        }
    }

    /// Substitute Ty::TypeVar("Self") with a concrete type in a protocol method return type.
    fn substitute_self_in_ty(&self, ty: &Ty, replacement: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(name) if name == "Self" => replacement.clone(),
            _ => ty.map_children(&|child| self.substitute_self_in_ty(child, replacement)),
        }
    }
}

/// Whether a string is a plain dotted identifier (e.g. `list.len`) safe to
/// drop into a copy-pasteable `try:` snippet as `fn(...)`. Rejects aliases
/// that are free-text hints (e.g. `"xs + [x]"`, `"string.chars + list.all"`).
fn is_clean_fn_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !s.starts_with('.')
        && !s.ends_with('.')
}
