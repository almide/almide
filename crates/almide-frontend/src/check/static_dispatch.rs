/// Static member resolution — fan.*, codec.*, module/alias dispatch, TypeName.method.

use almide_lang::ast;
use almide_lang::ast::ExprKind;
use almide_base::intern::sym;
use crate::types::{Ty, TypeConstructorId};
use super::types::resolve_ty;
use super::Checker;

/// Extract the effective return type from a function type, auto-unwrapping Result.
fn unwrap_fn_return(fn_ty: &Ty) -> Option<Ty> {
    if let Ty::Fn { ret, .. } = fn_ty {
        Some(match ret.as_ref() {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            other => other.clone(),
        })
    } else {
        None
    }
}

/// Extract the Result type from List[Fn() -> Result[T, E]] -> Result[T, E]
fn unwrap_list_fn_result_ty(list_ty: &Ty) -> Ty {
    match list_ty {
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            match &args[0] {
                Ty::Fn { ret, .. } => match ret.as_ref() {
                    r @ Ty::Applied(TypeConstructorId::Result, _) => r.clone(),
                    other => Ty::result(other.clone(), Ty::String),
                },
                _ => Ty::Unknown,
            }
        }
        _ => Ty::Unknown,
    }
}

/// Extract the element's effective return type from List[Fn() -> Result[T, E]] -> T
fn unwrap_list_fn_return(list_ty: &Ty) -> Ty {
    match list_ty {
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            unwrap_fn_return(&args[0]).unwrap_or(Ty::Unknown)
        }
        _ => Ty::Unknown,
    }
}

impl Checker {
    /// Resolve a member call statically (module.func, alias, TypeName.method, codec).
    /// Returns Some(Ty) if resolved, None to fall through to UFCS/convention dispatch.
    pub(super) fn resolve_static_member(&mut self, object: &ast::Expr, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        // Detect dot-chain submodule access and emit helpful error
        if let Some(dotted) = self.resolve_dotted_module(&object.kind) {
            let key = format!("{}.{}", dotted, field);
            if self.env.functions.contains_key(&sym(&key)) {
                // Extract the last segment of the dotted path for the import suggestion
                let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
                self.emit(super::err(
                    format!("dot-chain submodule access is no longer supported"),
                    format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
                    format!("call to {}.{}", dotted, field),
                ));
                // Still resolve so codegen doesn't break
                return Some(self.check_named_call(&key, arg_tys));
            }
        }

        // `module.Type.method(...)` — a convention/Codec method on a cross-module
        // type, e.g. `shapes.Dot.encode(d)`. The object is `Member(Ident(mod), Type)`;
        // the method is registered (by the Codec derive / an impl) under the bare key
        // `Type.method`. Resolve it before UFCS infers `module` as a variable (E003).
        if let ExprKind::Member { object: inner, field: type_name } = &object.kind {
            if let ExprKind::Ident { name: module, .. } = &inner.kind {
                if self.env.import_table.resolve(module).is_some() {
                    let key = format!("{}.{}", type_name, field);
                    if self.env.functions.contains_key(&sym(&key)) {
                        self.env.import_table.mark_used(module);
                        return Some(self.check_named_call(&key, arg_tys));
                    }
                }
            }
        }

        let module_name = match &object.kind {
            ExprKind::Ident { name, .. } => Some(name.as_str()),
            _ => None,
        };

        if let Some(module) = module_name {
            // fan.map / fan.race — compiler-known concurrency primitives
            if module == "fan" {
                return self.resolve_fan_call(field, arg_tys);
            }

            // Codec convenience: json.encode(t) -> String when t has T.encode
            if field == "encode" && arg_tys.len() == 1 {
                let arg_concrete = resolve_ty(&arg_tys[0], &self.uf);
                if self.has_codec_encode(&arg_concrete) {
                    return Some(Ty::String);
                }
            }

            if let Some(result) = self.resolve_module_member(module, field, arg_tys) {
                return Some(result);
            }
        }

        // TypeName.method() — direct convention call
        if let ExprKind::TypeName { name: type_name, .. } = &object.kind {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(self.check_named_call(&key, arg_tys));
            }
        }

        None
    }

    /// `fan.*` dispatch of [`Self::resolve_static_member`] — compiler-known
    /// concurrency primitives (`map`/`race`/`any`/`settle`), the removed
    /// `timeout` tombstone, and the unknown-fan-fn diagnostic. Verbatim text
    /// move: every arm ends in `return Some(..)`, so this always resolves
    /// (never falls through to UFCS).
    fn resolve_fan_call(&mut self, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        if !self.env.can_call_effect {
            self.emit(super::err(
                format!("fan.{}() can only be used inside an effect fn", field),
                "Mark the enclosing function as `effect fn`",
                format!("call to fan.{}()", field)));
        }
        match field {
            "map" => {
                // fan.map(xs, f) -> Result[List[B], String] where xs: List[A],
                // f: Fn(A) -> Result[B, String]. EFFECTFUL: the first element
                // Err (in list order) propagates as the whole map's Err. The
                // Result is auto-unwrapped in effect-fn bindings and auto-`?`
                // propagated, exactly like a user effect fn call.
                if arg_tys.len() != 2 {
                    self.emit(super::err(
                        format!("fan.map() expects 2 arguments but got {}", arg_tys.len()),
                        "Usage: fan.map(list, fn(item) => result)",
                        "call to fan.map()".to_string()));
                    return Some(Ty::Unknown);
                }
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                let elem_ty = match &list_ty {
                    Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                    _ => Ty::Unknown,
                };
                // Pin the callback's full type — `Fn(elem_ty) -> Result[B, String]`
                // — UNCONDITIONALLY, mirroring the normal `list.map` rule
                // (check/calls.rs constrains the arg to `Fn { params: arg_tys, .. }`),
                // with fan.map's added contract that the callback returns a Result.
                // Two things hinge on this being unconditional, not a fallback:
                //   - Param pinning: an inline lambda whose return type resolves on
                //     its own — e.g. `(x) => ok(x * 10)` — would otherwise leave `x`
                //     a free var that resolves to Ty::Unknown in the IR. WASM closure
                //     registration then falls back to i32 for the param while the body
                //     emits i64 for `x * 10` (validator: i32 != i64).
                //   - Return contract: a callback returning a bare Int or an Option
                //     (e.g. `(x) => x * 10` / `(x) => some(...)`) is ill-typed and is
                //     now reported at check time, instead of silently lowering to
                //     invalid Rust (E0308: expected Result, found Int/Option).
                // #547: a PURE mapper (`(x) => x * 10`) is rejected by
                // design, but pushing that through the generic constraint
                // produced a garbled expected/actual pair (the param slot
                // rendered as Result). When the callback's return type is
                // already resolved to a concrete non-Result, state the
                // ACTUAL RULE directly instead.
                if let Ty::Fn { ret, .. } = resolve_ty(&arg_tys[1], &self.uf) {
                    let cb_ret = resolve_ty(&ret, &self.uf);
                    let concrete_non_result = !cb_ret.is_result()
                        && !matches!(cb_ret, Ty::Unknown | Ty::TypeVar(_));
                    if concrete_non_result {
                        self.emit(super::err(
                            format!(
                                "fan.map callback must return Result but returns {}",
                                cb_ret.display()
                            ),
                            "Wrap the value: `(x) => ok(x * 10)` — fan.map mappers are \
                             effectful by contract (race/any/settle thunks auto-wrap, \
                             map mappers do not)",
                            "fan.map callback".to_string()));
                        return Some(Ty::Unknown);
                    }
                }
                let result_elem = self.fresh_var();
                let callback_ret = Ty::result(result_elem.clone(), Ty::String);
                self.constrain(arg_tys[1].clone(),
                    Ty::Fn { params: vec![elem_ty], ret: Box::new(callback_ret) },
                    "fan.map callback");
                Some(Ty::result(Ty::list(resolve_ty(&result_elem, &self.uf)), Ty::String))
            }
            "race" => {
                // fan.race(thunks) -> Result[T, String] — the FIRST thunk in
                // LIST ORDER to SETTLE (deterministic, NOT wall-clock): thunk[0]'s
                // result, Ok(v) or Err(e). Distinct from fan.any, which SKIPS
                // failures to find the first Ok. EFFECTFUL like fan.any: a head Err
                // is auto-`?` propagated to the unified main-error exit. (The
                // wall-clock "fastest wins" has no portable, deterministic meaning;
                // every async model's deterministic kernel is source/list order.)
                if arg_tys.len() != 1 {
                    self.emit(super::err(
                        format!("fan.race() expects 1 argument but got {}", arg_tys.len()),
                        "Usage: fan.race([fn() => a, fn() => b])",
                        "call to fan.race()".to_string()));
                    return Some(Ty::Unknown);
                }
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                Some(Ty::result(unwrap_list_fn_return(&list_ty), Ty::String))
            }
            "any" => {
                // fan.any(thunks) -> Result[T, String] — try thunks in LIST
                // ORDER, return the FIRST Ok (deterministic); if ALL fail,
                // return a defined Err ("fan.any: all candidates failed").
                // EFFECTFUL: auto-unwrapped in effect-fn bindings and auto-`?`
                // propagated, like a user effect fn call.
                if arg_tys.len() != 1 {
                    self.emit(super::err(
                        format!("fan.any() expects 1 argument but got {}", arg_tys.len()),
                        "Usage: fan.any([() => a, () => b])",
                        "call to fan.any()".to_string()));
                    return Some(Ty::Unknown);
                }
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                Some(Ty::result(unwrap_list_fn_return(&list_ty), Ty::String))
            }
            "settle" => {
                // fan.settle(thunks) -> List[Result[T, String]]
                if arg_tys.len() != 1 {
                    self.emit(super::err(
                        format!("fan.settle() expects 1 argument but got {}", arg_tys.len()),
                        "Usage: fan.settle([() => a, () => b])",
                        "call to fan.settle()".to_string()));
                    return Some(Ty::Unknown);
                }
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                let inner_result = unwrap_list_fn_result_ty(&list_ty);
                Some(Ty::list(inner_result))
            }
            "timeout" => {
                // Tombstone (contract C-006): `fan.timeout` was REMOVED in 0.29.0.
                // A wall-clock timeout has no portable cross-target meaning (wasm
                // has no clock, scheduler, or threads), and it was the sole stdlib
                // surface whose result was not a function of the program + its
                // inputs. Deadlines belong at the host boundary that invokes the
                // program. The dedicated arm (instead of the unknown-member arm
                // below) keeps the migration actionable.
                self.emit(super::err(
                    "fan.timeout was removed: a wall-clock timeout has no portable cross-target meaning",
                    "Enforce deadlines at the host boundary that invokes the program \
                     (e.g. `timeout 5 ./app`). Inside Almide every fan combinator is \
                     deterministic by list order: fan.map, fan.race, fan.any, fan.settle.",
                    "call to fan.timeout()".to_string()).with_code("E027"));
                Some(Ty::Unknown)
            }
            _ => {
                self.emit(super::err(
                    format!("unknown function 'fan.{}'", field),
                    "Available: fan.map, fan.race, fan.any, fan.settle",
                    format!("call to fan.{}()", field)));
                Some(Ty::Unknown)
            }
        }
    }

    /// Direct stdlib/user module call or resolved alias of
    /// [`Self::resolve_static_member`] (`string.trim(s)`, `alias.func(x)`,
    /// a cross-module variant constructor). Only imported modules are
    /// accessible (no phantom dependencies) — `None` when `module` isn't a
    /// resolved import, matching the caller's UFCS/TypeName fallthrough.
    /// Verbatim text move.
    fn resolve_module_member(&mut self, module: &str, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        let m = self.env.import_table.resolve(module).map(|s| {
            self.env.import_table.mark_used(module);
            s.to_string()
        })?;
        // Cross-module variant constructor call: binary.ImportFunc(0)
        if let Some((type_name, case)) = self.env.lookup_ctor(&sym(field)) {
            let qualified = format!("{}.{}", m, type_name.as_str());
            if self.env.types.contains_key(&sym(&qualified)) {
                self.check_constructor_args(field, &case, arg_tys);
                let generic_args = self.instantiate_type_generics(type_name.as_str());
                if !generic_args.is_empty() {
                    if let Some(ty_def) = self.env.types.get(&type_name).cloned() {
                        let mut type_var_names = Vec::new();
                        crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
                        let subst: std::collections::HashMap<almide_base::intern::Sym, Ty> = type_var_names.iter()
                            .zip(generic_args.iter())
                            .map(|(tv, fresh)| (*tv, fresh.clone()))
                            .collect();
                        if let crate::types::VariantPayload::Tuple(expected) = &case.payload {
                            for (aty, ety) in arg_tys.iter().zip(expected.iter()) {
                                let substituted = super::calls::subst_ty(ety, &subst);
                                self.unify_infer(aty, &substituted);
                            }
                        }
                    }
                }
                // #433: the binding/result takes the qualified `mod.Type`
                // (just confirmed to exist) so it mangles to the namespaced
                // enum, not the ambiguous bare name.
                return Some(Ty::Named(sym(&qualified), generic_args));
            }
        }
        let key = format!("{}.{}", m, field);
        // Enforce cross-module visibility (`mod fn` / `local fn`)
        // before lowering the call — the key now lives in
        // `env.fn_visibility` thanks to registration.
        self.check_fn_visibility(&m, field, &key);
        Some(self.check_named_call(&key, arg_tys))
    }

    /// Resolve a nested Member chain to a dotted module path.
    /// e.g. Member(Member(Ident("bindgen"), "bindings"), "python") → "bindgen.bindings.python"
    /// Returns None if the chain doesn't start with a known module name.
    fn resolve_dotted_module(&self, kind: &ExprKind) -> Option<String> {
        match kind {
            ExprKind::Member { object, field, .. } => {
                if let ExprKind::Ident { name: root, .. } = &object.kind {
                    let resolved_root = self.env.import_table.resolve(root)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| root.to_string());
                    let candidate = format!("{}.{}", resolved_root, field);
                    if self.env.import_table.accessible.contains(&sym(&candidate)) {
                        return Some(candidate);
                    }
                    let prefix = format!("{}.", candidate);
                    if self.env.import_table.accessible.iter().any(|m| m.as_str().starts_with(&prefix)) {
                        return Some(candidate);
                    }
                }
                if let Some(parent) = self.resolve_dotted_module(&object.kind) {
                    let candidate = format!("{}.{}", parent, field);
                    if self.env.import_table.accessible.contains(&sym(&candidate)) {
                        return Some(candidate);
                    }
                    let prefix = format!("{}.", candidate);
                    if self.env.import_table.accessible.iter().any(|m| m.as_str().starts_with(&prefix)) {
                        return Some(candidate);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Check if a type has a Codec encode function registered.
    fn has_codec_encode(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Named(name, _) => self.env.functions.contains_key(&sym(&format!("{}.encode", name))),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().any(|(name, t)| t == ty && self.env.functions.contains_key(&sym(&format!("{}.encode", name))))
            }
            _ => false,
        }
    }
}
