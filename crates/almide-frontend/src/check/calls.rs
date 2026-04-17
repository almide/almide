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
    pub(crate) fn check_call_with_type_args(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr], type_args: Option<&[Ty]>) -> Ty {
        let arg_tys: Vec<Ty> = args.iter_mut().map(|a| self.infer_expr(a)).collect();
        match &mut callee.kind {
            ExprKind::Ident { name, .. } => {
                let name = name.clone();
                // Register callee's type for variables that hold function values
                // (Skip for builtins/functions — they don't need ExprId registration)
                if self.env.lookup_var(&name).is_some() {
                    let _ = self.infer_expr(callee);
                }
                self.check_named_call_with_type_args(&name, &arg_tys, type_args)
            }
            ExprKind::TypeName { name, .. } => {
                if let Some((type_name, case)) = self.env.constructors.get(&sym(name)).cloned() {
                    self.check_constructor_args(name, &case, &arg_tys);
                    // Instantiate parent type's generics with fresh inference vars
                    let generic_args = self.instantiate_type_generics(type_name.as_str());
                    // Unify constructor payload types with arg types to resolve generic vars.
                    // e.g., Leaf(1) where Leaf(T) → unify T=?fresh with Int → ?fresh=Int
                    if !generic_args.is_empty() {
                        if let Some(ty_def) = self.env.types.get(&sym(type_name.as_str())).cloned() {
                            let mut type_var_names = Vec::new();
                            crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
                            // Build substitution map: named TypeVar name → fresh inference var
                            let subst: std::collections::HashMap<almide_base::intern::Sym, Ty> = type_var_names.iter()
                                .zip(generic_args.iter())
                                .map(|(tv, fresh)| (*tv, fresh.clone()))
                                .collect();
                            // Get expected payload types for this case
                            if let crate::types::VariantPayload::Tuple(expected) = &case.payload {
                                for (aty, ety) in arg_tys.iter().zip(expected.iter()) {
                                    // Substitute named TypeVars with fresh inference vars, then unify
                                    let substituted = subst_ty(ety, &subst);
                                    self.unify_infer(aty, &substituted);
                                }
                            }
                        }
                    }
                    Ty::Named(type_name, generic_args)
                } else { Ty::Named(sym(name), vec![]) }
            }
            // Module call: string.trim(s), list.map(xs, f), etc.
            ExprKind::Member { object, field, .. } => {
                // Try static resolution: module.func, alias.func, TypeName.method, codec.encode
                if let Some(result) = self.resolve_static_member(object, field, &arg_tys) {
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
                        diag = diag.with_try(format!(
                            "// x.{field}()  →  {m}.{close}(x)\n{m}.{close}(x)",
                            m = module, close = close, field = field
                        ));
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
            // TypeVars and inference vars are not concrete — skip protocol checking
            Ty::TypeVar(_) | Ty::Unknown => None,
            _ => None,
        }
    }

    pub(crate) fn check_named_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        self.check_named_call_with_type_args(name, arg_tys, None)
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
        let sig = self.env.functions.get(&sym(name)).cloned().or_else(|| {
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
            // No function signature found — try constructor, variable, or report error
            if let Some((type_name, case)) = self.env.constructors.get(&sym(name)).cloned() {
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
                return ty;
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
            if let Some(rich) = rich_snippet {
                diag = diag.with_try(rich.to_string());
            } else if let Some(fix) = fix_name {
                diag = diag.with_try(format!("// {wrong}(...)  →  {right}(...)\n{right}(...)", wrong = name, right = fix));
            }
            self.emit(diag);
            return Ty::Unknown;
        };

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
        let concrete_args: Vec<Ty> = arg_tys.iter().map(|a| resolve_ty(a, &self.uf)).collect();
        for ((pname, pty), aty) in sig.params.iter().zip(concrete_args.iter()) {
            self.unify_call_arg(name, pname, pty, aty, &sig.structural_bounds, &mut bindings);
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
        for ((_, pty), aty) in sig.params.iter().zip(arg_tys.iter()) {
            let expected = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
            if expected != Ty::Unknown {
                self.constrain(expected, aty.clone(), format!("call to {}()", name));
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
        // In test blocks, user-defined effect fn calls that return non-Result T are reported as Result[T, String].
        // This removes the implicit auto-unwrap and lets users choose: `!` to unwrap, or match on ok/err.
        // Only user-defined fns are wrapped — stdlib effect fns are not lifted by codegen,
        // so their runtime implementations return raw values (not Result).
        if self.env.in_test_block && sig.is_effect && !ret.is_result()
            && self.env.functions.contains_key(&sym(name))
        {
            return Ty::result(ret, Ty::String);
        }
        ret
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
    fn unify_call_arg(
        &mut self, fn_name: &str, param_name: &Sym,
        param_ty: &Ty, arg_ty: &Ty,
        structural_bounds: &HashMap<Sym, Ty>,
        bindings: &mut HashMap<Sym, Ty>,
    ) {
        if let Ty::TypeVar(tv) = param_ty {
            if let Some(bound) = structural_bounds.get(tv) {
                let resolved = self.env.resolve_named(arg_ty);
                if bound.compatible(&resolved) || bound.compatible(arg_ty) {
                    bindings.insert(*tv, arg_ty.clone());
                } else {
                    self.emit(super::err(
                        format!("argument '{}' does not satisfy bound {}: got {}", param_name, bound.display(), arg_ty.display()),
                        "The argument must have the required fields",
                        format!("call to {}()", fn_name)));
                }
            } else {
                crate::types::unify(param_ty, arg_ty, bindings);
            }
        } else {
            crate::types::unify(param_ty, arg_ty, bindings);
            let expected = if bindings.is_empty() { param_ty.clone() } else { crate::types::substitute(param_ty, bindings) };
            let expected_resolved = self.env.resolve_named(&expected);
            let arg_resolved = self.env.resolve_named(arg_ty);
            if types_mismatch(&expected_resolved, &arg_resolved) {
                let hint = if let Ty::Named(name, args) = &expected {
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
                self.emit(diag);
            }
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
