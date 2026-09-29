
// UNPOPULATED since #2950. Every registry below except `EAGER_ABORT_GLOBALS`
// was filled only by the incumbent wasm pipeline's program passes
// (`populate_abi_registries`, `inline_mutual_tail_recursion`,
// `set_mutable_global_vars`, `set_derived_type_owners`), which were deleted
// with that pipeline (#2935, #2950). The native MIR leg and the witness
// producer never ran them, so on every product path these sets are empty and
// each membership test below reads `false`/`None`. The per-registry docs keep
// the contract they were written against; removing the registries and the
// branches that read them is follow-up work.
thread_local! {
    /// The names of NEVER-ERR LIFTED user effect fns (an `effect fn` whose declared return is
    /// non-Result, so the frontend lifts its call type to `Result[T, String]`, but whose body builds
    /// no `err` and returns raw `T`). Populated by `inline_mutual_tail_recursion` (which knows the
    /// `can_err` × `lifted_effect_fns` sets) and read by the match-subject lowering so an UN-REWRITTEN
    /// `match <such call> {…}` (an `ok(_)`/structured/guarded Ok arm the `rewrite_never_err_effect_match`
    /// pass left in place) WALLs cleanly instead of reading the raw handle as a Result block (a trap).
    /// A common `ok(x)` match is already rewritten away to a `let`-block, so this only catches the rare
    /// residue. Thread-local because lowering runs single-threaded per program right after the pre-pass.
    pub(crate) static NEVER_ERR_LIFTED_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// The names of AUTO-WRAP ABI functions — an `effect fn` declared with a bare scalar return
    /// (`-> Int`, not `-> Result[Int, String]`) whose body contains a STATEMENT-position
    /// propagating `!`/auto-`?` (`body_has_stmt_position_propagating_unwrap`, mod.rs), so its
    /// TRUE compiled ABI is `Result[<declared>, String]` even though `func.ret_ty` stays the bare
    /// sugar type. Populated by `inline_mutual_tail_recursion` (the SAME program-wide pre-pass
    /// `NEVER_ERR_LIFTED_FNS` uses, run once before any per-function lowering, so it is fully
    /// populated before ANY caller's own lowering — including a caller that is itself never-err
    /// lifted and processed before this callee). EXCLUDES `main` — see the population site.
    pub(crate) static AUTO_WRAP_ABI_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// MAIN-region top-lets whose PURE, call-bearing initializer CAN ABORT
    /// (`init_can_abort`: an integer `/` or `%`) — C-007, #2571. Their VALUE is
    /// inlined at each use site by `inline_pure_call_globals` (pure ⇒ the same
    /// value each time), which alone would defer the abort to the first read:
    /// `before use` printed where native and the structural leg abort at
    /// startup. `synthesize_global_init` re-evaluates every member once in
    /// `__global_init`, exactly as it already re-evaluates the call-free scalar
    /// inits for their abort, so the abort fires before `main`. Populated by
    /// `inline_pure_call_globals`, read by `synthesize_global_init`.
    pub(crate) static EAGER_ABORT_GLOBALS: std::cell::RefCell<std::collections::HashSet<almide_ir::VarId>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// Per user fn (by its lowered name), which parameter positions the callee
    /// WRITES BACK into (#2503 / C-033): `p.is_mut` on a PURE fn. The C-132
    /// move-mode rewrite keeps `is_mut` on the param and clears only
    /// `mutated_params`, so this is read after the rewrite. Effect callees are
    /// excluded exactly as the structural leg excludes them (`param_mut` in
    /// crates/almide-wasm/src/emit.rs): the argument's own credit makes
    /// rc >= 2 there, so a call-site copy would fire on every call. Populated by
    /// `populate_abi_registries`, read by `cow_mut_param_call_args`.
    pub(crate) static MUT_PARAM_FNS: std::cell::RefCell<std::collections::HashMap<String, Vec<bool>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());

    /// Effect fns whose DECLARED return is `Option[..]` — in the v1 model they are NOT
    /// lifted (the Option IS the real return; there is no err channel), so a caller's
    /// frontend auto-`?` (`Try`) over such a call is a NO-OP and must be STRIPPED: left
    /// in place, the effect-unwrap desugar built an err/ok match over the raw OPTION
    /// block (read with Result polarity/offsets — r5's `hit=999` silent wrong value +
    /// rc_dec trap). A SPELLED `!` is different (unwrap-the-Option, die on none) and is
    /// NOT stripped.
    /// The EFFECT members of [`DECLARED_OPTION_FNS`] (#1573): the callees
    /// whose `??` the checker types as the EFFECT err-fallback (identity —
    /// `T? ?? T?`; a PURE declared-Option fn's `??` types as the
    /// Option-unwrap `T? ?? T`, so a `?? none` there is a type error).
    pub(crate) static DECLARED_OPTION_EFFECT_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    pub(crate) static DECLARED_OPTION_FNS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    /// MUTABLE module-level `var` globals (program + module top_lets, `tl.mutable ==
    /// true`): VarId → (storage-slot index, declared Ty). Cross-function shared state
    /// lives in a dedicated linear-memory slot (`crate::mg_slot_addr(index)`): a read
    /// loads the slot fresh each time (a scalar `Load`, a heap `$__mg_get` owned Dup),
    /// an assign stores through it (`$__mg_take` + type-routed drop of the old value +
    /// `Store`+`Consume` of the new). WITHOUT slot routing, a read materialized the
    /// const initializer and an assign rebound a function-local copy (`var counter = 0;
    /// bump(); bump()` printed `5 3 0` where native says `5 8 8` — a LIVE miscompile).
    /// Populated by the pipeline / classify globals collection (the same pre-lowering
    /// point the maps are built); shapes beyond the slot subset still WALL.
    /// Cross-module DERIVED-METHOD owners (#790 codec bridge): base type name → the ONE
    /// non-stdlib module that declares it (unique owners only; main-declared types are
    /// excluded by the pipeline's population). The MIR desugar consults this when it
    /// forms a `T.encode`/`T.decode` Named target from a Method call, resolving it to
    /// the module-mangled derived fn instead of an unlinked bare name.
    pub(crate) static DERIVED_TYPE_OWNERS: std::cell::RefCell<std::collections::HashMap<String, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    pub(crate) static MUTABLE_GLOBAL_VARS: std::cell::RefCell<std::collections::HashMap<u32, (u32, Ty)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Resolve a freshly-formed `T.encode`/`T.decode` (or `mod.T.<m>`) Named target through
/// the derived-method owner map: a uniquely-owned module type's codec method maps to the
/// module-mangled derived fn (`almide_rt_<m>_T_<method>` — dots become underscores, the
/// `user_module_fn_name` convention). Everything else passes through unchanged.
pub fn resolve_derived_method_owner(name: String) -> String {
    let resolved = {
        let parts: Vec<&str> = name.split('.').collect();
        let (qualifier, ty, method) = match parts.as_slice() {
            [t, m] => (None, *t, *m),
            [q, t, m] => (Some(*q), *t, *m),
            _ => return name,
        };
        if method != "encode" && method != "decode" {
            return name;
        }
        DERIVED_TYPE_OWNERS.with(|o| {
            let o = o.borrow();
            match o.get(ty) {
                Some(m) if qualifier.is_none() || qualifier == Some(m.as_str()) => {
                    // The DEFINITION side mangles the QUALIFIED type name (`varlib.Pigment`
                    // → `varlib_Pigment`) under the module prefix, so the derived fn is
                    // `almide_rt_varlib_varlib_Pigment_encode` (module twice — observed in
                    // the linked IR). Mirror that exactly or the call dangles unlinked.
                    let mm = m.replace('.', "_");
                    Some(format!("almide_rt_{mm}_{mm}_{ty}_{method}"))
                }
                _ => None,
            }
        })
    };
    resolved.unwrap_or(name)
}

/// Is `var` a mutable module-level `var` (slot-routed cross-function state)?
pub(crate) fn is_mutable_global(var: almide_ir::VarId) -> bool {
    MUTABLE_GLOBAL_VARS.with(|s| s.borrow().contains_key(&var.0))
}

/// The (slot index, declared Ty) of a mutable module-level `var`, if `var` is one.
pub(crate) fn mutable_global_info(var: almide_ir::VarId) -> Option<(u32, Ty)> {
    MUTABLE_GLOBAL_VARS.with(|s| s.borrow().get(&var.0).cloned())
}

/// Does this lambda body PROPAGATE — carry an `Unwrap` (`!`) of its own, outside any
/// nested lambda (a nested lambda's `!` rides that lambda's own Result channel)? A
/// never-err `!` has already been stripped by `strip_never_err_unwraps` before any
/// reader of this asks, so a hit means the body can genuinely exit through `err`.
pub(crate) fn lambda_body_propagates(body: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find {
        found: bool,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found || matches!(e.kind, IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(e.kind, IrExprKind::Unwrap { .. }) {
                self.found = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { found: false };
    f.visit_expr(body);
    f.found
}

/// `e` as `fan.map(xs, <inline lambda whose body propagates>)!` / `?`: the callback.
fn consumed_propagating_fan_map_callback(e: &IrExpr) -> Option<&IrExpr> {
    let (IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }) = &e.kind else {
        return None;
    };
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &inner.kind
    else {
        return None;
    };
    if module.as_str() != "fan" || func.as_str() != "map" {
        return None;
    }
    let callback = args.get(1)?;
    let IrExprKind::Lambda { body, .. } = &callback.kind else { return None };
    lambda_body_propagates(body).then_some(callback)
}

/// The #1865 wall: `fan.map(xs, <inline callback whose body propagates>)!` — the
/// call CONSUMED by `!` (or `?`), in every callback spelling (`(p) =>
/// fs.read_text(p)!`, the helper call, a compound body) — is refused by this
/// brick, fn-wide and BEFORE any desugar. Exactly this shape is what
/// `rewrite_fan_map_pure` used to strip into a String-returning `list.map`
/// closure (nondeterministic garbage bytes per element); with the strip
/// declined, the unwrap ladder walls the fn anyway — spanless, under a reason
/// naming a match it desugared, not the construct the author wrote. The
/// UN-consumed forms (`?? fb`, a match over the Result, an effect-fn VALUE
/// callback) ride the self-host `fan.map` route with the callback's own Result
/// channel intact and stay lowered. The structural leg (the default route) runs
/// the shape byte-identically to native; the incumbent retires under #1696
/// rather than growing a lowering. Span: the callback itself.
pub(crate) fn wall_fan_map_propagating_callbacks(body: &IrExpr) -> Result<(), LowerError> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find {
        hit: Option<Option<almide_ir::Span>>,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.hit.is_some() {
                return;
            }
            if let Some(callback) = consumed_propagating_fan_map_callback(e) {
                self.hit = Some(callback.span);
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { hit: None };
    f.visit_expr(body);
    match f.hit {
        None => Ok(()),
        Some(span) => Err(LowerError::at(
            span,
            "fan.map consumed by `!` with a propagating (!) callback body is not lowered by \
             the incumbent leg (the structural leg serves it)",
        )),
    }
}

/// A call to a never-err DECLARED-Option effect fn still typed as its lifted `Result`: the
/// `match` residue [`rewrite_never_err_effect_match`] leaves, which the match lowering walls.
/// Keyed on the type as well, because a stripped `f()!` subject is the Option itself.
pub(crate) fn is_unstripped_declared_option_call(e: &IrExpr) -> bool {
    is_result_ty(&e.ty)
        && matches!(&e.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
            if DECLARED_OPTION_EFFECT_FNS.with(|s| s.borrow().contains(name.as_str())))
}

