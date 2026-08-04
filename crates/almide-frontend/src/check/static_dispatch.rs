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
        if let Some(ty) = self.resolve_dot_chain_submodule(object, field, arg_tys) {
            return Some(ty);
        }
        if let Some(ty) = self.resolve_cross_module_convention(object, field, arg_tys) {
            return Some(ty);
        }
        if let ExprKind::Ident { name, .. } = &object.kind {
            if let Some(ty) = self.resolve_module_call_member(name.as_str(), field, arg_tys) {
                return Some(ty);
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

    /// Dot-chain submodule access (`a.b.fn(...)`), which the module system no
    /// longer supports.
    ///
    /// The call still RESOLVES after the diagnostic is emitted, so lowering and
    /// codegen see a well-formed call and the user gets the one real error rather
    /// than a cascade behind it.
    fn resolve_dot_chain_submodule(&mut self, object: &ast::Expr, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        let dotted = self.resolve_dotted_module(&object.kind)?;
        let key = format!("{}.{}", dotted, field);
        if !self.env.functions.contains_key(&sym(&key)) {
            return None;
        }
        let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
        self.emit(super::err(
            "dot-chain submodule access is no longer supported".to_string(),
            format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
            format!("call to {}.{}", dotted, field),
        ));
        Some(self.check_named_call(&key, arg_tys))
    }

    /// `module.Type.method(...)` — a convention or Codec method on a cross-module
    /// type, e.g. `shapes.Dot.encode(d)`.
    ///
    /// The object parses as `Member(Ident(mod), Type)` while the method is
    /// registered under the BARE key `Type.method` (by the Codec derive or an
    /// impl). Resolving here, before UFCS runs, is what stops `module` being
    /// inferred as a variable and reported as E003.
    fn resolve_cross_module_convention(&mut self, object: &ast::Expr, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        let ExprKind::Member { object: inner, field: type_name } = &object.kind else { return None };
        let ExprKind::Ident { name: module, .. } = &inner.kind else { return None };
        self.env.import_table.resolve(module)?;
        let key = format!("{}.{}", type_name, field);
        if !self.env.functions.contains_key(&sym(&key)) {
            return None;
        }
        self.env.import_table.mark_used(module);
        Some(self.check_named_call(&key, arg_tys))
    }

    /// `module.fn(...)` where the object is a plain module name.
    fn resolve_module_call_member(&mut self, module: &str, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        // fan.map / fan.any / fan.settle — compiler-known concurrency primitives.
        if module == "fan" {
            return self.resolve_fan_call(field, arg_tys);
        }
        // compute.ms / duration.ms — the ADR-0001 time constructors. Compiler-known
        // NOMINAL types: the value erases to an Int of nanoseconds in lowering; the
        // Compute/Duration distinction lives here, in the checker, as the clock
        // firewall (bare Int and cross-clock arguments are type errors at the
        // consuming heads).
        if module == "compute" || module == "duration" {
            return Some(self.resolve_time_ctor(module, field, arg_tys));
        }
        // Codec convenience: `json.encode(t)` is String when `t` has `T.encode`.
        // This arm returns before `resolve_module_member`, which is where the
        // import is normally marked used — so `import json` was reported
        // unused on a file whose very next token used it, and following the
        // hint broke the build in a package (#1089).
        if field == "encode" && arg_tys.len() == 1 {
            let arg_concrete = resolve_ty(&arg_tys[0], &self.uf);
            if self.has_codec_encode(&arg_concrete) {
                self.env.import_table.mark_used(module);
                return Some(Ty::String);
            }
        }
        self.resolve_module_member(module, field, arg_tys)
    }

    /// `fan.*` dispatch of [`Self::resolve_static_member`] — compiler-known
    /// concurrency primitives (`map`/`any`/`settle`), the removed `race` and
    /// `timeout` tombstones, and the unknown-fan-fn diagnostic. Verbatim text
    /// move: every arm ends in `return Some(..)`, so this always resolves
    /// (never falls through to UFCS).
    /// The closed unit set of ADR-0001 S2 — 2 clocks x 6 units, gate-checked.
    /// Unknown units are a diagnostic naming the whole legal set (LLMs invent
    /// `msec`/`5m`; the matrix answer beats a nearest-match guess).
    fn resolve_time_ctor(&mut self, module: &str, field: &str, arg_tys: &[Ty]) -> Ty {
        let ty_name = almide_lang::time_units::clock_type_of_module(module)
            .expect("resolve_time_ctor called for a non-clock module");
        if almide_lang::time_units::unit_factor(field).is_none() {
            self.emit(super::err(
                format!("unknown unit '{}.{}'", module, field),
                almide_lang::time_units::unit_set_hint(module),
                format!("call to {}.{}()", module, field)));
            return Ty::Named(sym(ty_name), vec![]);
        }
        if arg_tys.len() != 1 {
            self.emit(super::err(
                format!("{}.{}() expects 1 argument but got {}", module, field, arg_tys.len()),
                format!("Usage: {}.{}(100)", module, field),
                format!("call to {}.{}()", module, field)));
            return Ty::Named(sym(ty_name), vec![]);
        }
        self.constrain(arg_tys[0].clone(), Ty::Int, "time constructor argument");
        Ty::Named(sym(ty_name), vec![])
    }

    fn resolve_fan_call(&mut self, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        if !self.env.can_call_effect {
            self.emit(super::err(
                format!("fan.{}() can only be used inside an effect fn", field),
                "Mark the enclosing function as `effect fn`",
                format!("call to fan.{}()", field)));
        }
        if let Some(ty) = self.resolve_fan_mapping(field, arg_tys) { return Some(ty); }
        if let Some(ty) = self.resolve_fan_collecting(field, arg_tys) { return Some(ty); }
        // Every known fan fn is handled above, so reaching here means the name is
        // not one — and `fan.*` always RESOLVES rather than falling through to
        // UFCS, so the diagnostic is emitted and `Unknown` recovers.
        self.emit(super::err(
            format!("unknown function 'fan.{}'", field),
            // LIVE surfaces only. `fan.race` is tombstoned (E027) and naming it here sent a
            // user who merely mistyped toward a function that no longer exists — the same
            // defect class as a tombstone whose migration target is itself removed.
            "Available: fan.map(xs, f), and the block heads fan.any / fan.settle / fan.race / fan.bounded",
            format!("call to fan.{}()", field)));
        Some(Ty::Unknown)
    }

    /// `fan.map` and the tombstoned `fan.race` — the fan arms that consume a mapper.
    ///
    /// One group of `resolve_fan_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group"; the router tries the groups in that
    /// order and only then reports the unknown-fan-fn diagnostic.
    fn resolve_fan_mapping(&mut self, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
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
                // fan.race was REMOVED (0.42.0). Under the concurrency stance
                // (docs/roadmap/active/concurrency-stance.md, the answer to #1000) Almide's
                // model is DETERMINISTIC DATA-PARALLELISM: `fan` may execute in parallel,
                // but observable behaviour is defined to be sequential evaluation in list
                // order. A name promising "first to complete wins" has no meaning in that
                // model — and the implementation never raced anyway. `desugar_fan.rs`'s
                // `rewrite_race_head` replaced `fan.race([t0, t1, …])` with `t0`; the other
                // thunks were not even evaluated, so the combinator was a no-op wrapper
                // whose name was the only thing it added. SPEC.md sold it as a race in two
                // places while C-004 correctly documented list order — the prose and the
                // contract contradicted each other, and the prose was wrong.
                //
                // Same treatment as fan.timeout (E027, 0.29.0): a check-time tombstone with
                // an actionable migration, not an alias. No coexistence.
                self.emit(super::err(
                    "fan.race changed signature: the thunk-list form was removed; race is now a deterministic block head",
                    "New form: `fan.race { a(); b() }` — the winner is the branch that \
                     completes with the LEAST deterministic computation ((spend, index) \
                     minimum; ties go to source order). An optional per-branch budget is \
                     `fan.race(compute.ms(5)) { … }`. If you meant the first candidate \
                     that SUCCEEDS in list order, use `fan.any`.",
                    "call to fan.race()".to_string()).with_code("E027"));
                Some(Ty::Unknown)
            }
            _ => return None,
        }
    }

    /// `fan.any`, `fan.settle`, and the removed `fan.timeout` tombstone.
    ///
    /// One group of `resolve_fan_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group"; the router tries the groups in that
    /// order and only then reports the unknown-fan-fn diagnostic.
    fn resolve_fan_collecting(&mut self, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        match field {
            // The Wave 1 BLOCK forms (parser-synthesized internal names): typed
            // exactly like the legacy combinators they compile to.
            "__any_block" => {
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                Some(Ty::result(unwrap_list_fn_return(&list_ty), Ty::String))
            }
            "__settle_block" => {
                let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                Some(Ty::list(unwrap_list_fn_result_ty(&list_ty)))
            }
            "any" => {
                // Wave 1: the thunk-list SPELLING is removed; the block form is
                // the surface. 2 args = the declared (not yet implemented)
                // mapper form.
                if arg_tys.len() == 1 {
                    self.emit(super::err(
                        "fan.any changed signature: the thunk-list form was removed; any is now a block head",
                        "New form: `fan.any { a(); b() }` — first Ok in source order. \
                         The dynamic mapper form `fan.any(xs, f)` is declared for Wave 2.",
                        "call to fan.any()".to_string()).with_code("E027"));
                    return Some(Ty::Unknown);
                }
                if arg_tys.len() == 2 {
                    // fan.any(xs, f) -> Result[B, String] (T2-3, the Wave 2
                    // mapper form): apply f in LIST ORDER, first Ok wins, an
                    // element's Err disqualifies that element only; all-fail
                    // (and empty) is the ledger-constant Err. Same callback
                    // contract as fan.map: f returns Result.
                    let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                    let elem_ty = match &list_ty {
                        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                        _ => Ty::Unknown,
                    };
                    let result_elem = self.fresh_var();
                    let callback_ret = Ty::result(result_elem.clone(), Ty::String);
                    self.constrain(arg_tys[1].clone(),
                        Ty::Fn { params: vec![elem_ty], ret: Box::new(callback_ret) },
                        "fan.any callback");
                    return Some(Ty::result(resolve_ty(&result_elem, &self.uf), Ty::String));
                }
                self.emit(super::err(
                    format!("fan.any() expects a block but got {} arguments", arg_tys.len()),
                    "Usage: fan.any { a(); b() }",
                    "call to fan.any()".to_string()));
                Some(Ty::Unknown)
            }
            "settle" => {
                if arg_tys.len() == 1 {
                    self.emit(super::err(
                        "fan.settle changed signature: the thunk-list form was removed; settle is now a block head",
                        "New form: `fan.settle { a(); b() }` — collects every result in \
                         source order. The dynamic mapper form `fan.settle(xs, f)` is \
                         declared for Wave 2.",
                        "call to fan.settle()".to_string()).with_code("E027"));
                    return Some(Ty::Unknown);
                }
                if arg_tys.len() == 2 {
                    // fan.settle(xs, f) -> List[Result[B, String]] (T2-3):
                    // apply f in LIST ORDER, collecting EVERY element's Result
                    // (Errs captured, never propagated). Lowering desugars it
                    // to `list.map(xs, f)` — that IS the semantics.
                    let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                    let elem_ty = match &list_ty {
                        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                        _ => Ty::Unknown,
                    };
                    let result_elem = self.fresh_var();
                    let callback_ret = Ty::result(result_elem.clone(), Ty::String);
                    self.constrain(arg_tys[1].clone(),
                        Ty::Fn { params: vec![elem_ty], ret: Box::new(callback_ret.clone()) },
                        "fan.settle callback");
                    return Some(Ty::list(Ty::result(
                        resolve_ty(&result_elem, &self.uf),
                        Ty::String,
                    )));
                }
                self.emit(super::err(
                    format!("fan.settle() expects a block but got {} arguments", arg_tys.len()),
                    "Usage: fan.settle { a(); b() }",
                    "call to fan.settle()".to_string()));
                Some(Ty::Unknown)
            }
            "timeout" => {
                // fan.timeout RETURNED in T5-1 as the ORACLE-tier block head
                // (`fan.timeout(duration.ms(n)) { body }` — a cooperative
                // wall-clock deadline checked at charge sites, ω-relative per
                // ADR-0001 S8; record/replay makes an observed ω
                // reproducible). The legacy CALL spelling gets a
                // signature-migration hint, mirroring race's E027 revision.
                self.emit(super::err(
                    "fan.timeout changed signature: it is now a block head with a Duration deadline",
                    "New form: `fan.timeout(duration.ms(5000)) { work(x) }` — the deadline \
                     is checked cooperatively at charge sites (never mid-operation), and \
                     the verdict is host-relative (record/replay reproduces it)",
                    "call to fan.timeout()".to_string()).with_code("E027"));
                Some(Ty::Unknown)
            }
            _ => return None,
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
                self.unify_ctor_payload_generics(type_name, &case, arg_tys, &generic_args);
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
            Ty::Named(name, _) => crate::canonicalize::registration::convention_fn_key(&self.env, &name.to_string(), "encode").is_some(),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().any(|(name, t)| t == ty && self.env.functions.contains_key(&sym(&format!("{}.encode", name))))
            }
            _ => false,
        }
    }

    /// Unify a generic variant constructor's arguments against its payload types
    /// with the call's fresh type variables substituted in.
    ///
    /// The payload types are written in terms of the declaration's own type vars
    /// (`Box[T]`'s `T`), while the call site has fresh ones; unifying without the
    /// substitution would bind the arguments to the declaration's vars and leak
    /// them across call sites. A non-generic constructor has nothing to
    /// substitute, and a record or unit payload has no positional arguments.
    fn unify_ctor_payload_generics(
        &mut self,
        type_name: almide_base::intern::Sym,
        case: &crate::types::VariantCase,
        arg_tys: &[Ty],
        generic_args: &[Ty],
    ) {
        if generic_args.is_empty() {
            return;
        }
        let Some(ty_def) = self.env.types.get(&type_name).cloned() else { return };
        let crate::types::VariantPayload::Tuple(expected) = &case.payload else { return };
        let mut type_var_names = Vec::new();
        crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
        let subst: std::collections::HashMap<almide_base::intern::Sym, Ty> = type_var_names.iter()
            .zip(generic_args.iter())
            .map(|(tv, fresh)| (*tv, fresh.clone()))
            .collect();
        for (aty, ety) in arg_tys.iter().zip(expected.iter()) {
            let substituted = super::calls::subst_ty(ety, &subst);
            self.unify_infer(aty, &substituted);
        }
    }

}
