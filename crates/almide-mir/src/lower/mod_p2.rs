
// The ABI registries below are DERIVED from the IR (#3058): the shared desugar
// chain calls [`settle_effect_abi`] on every product path and in the corpus
// classifier, which computes `almide_ir::effect_abi::effect_abi_facts` (never-err
// lifted / auto-wrap / declared-Option / mut params) and the mutable-global slot
// map as pure functions of the program and installs them for the per-fn
// lowering. The incumbent passes that used to fill them were deleted with that
// pipeline (#2935, #2950). The one registry nothing derived,
// `DERIVED_TYPE_OWNERS`, is folded away (#3000): it was never written, so its
// reader answered "unchanged" everywhere. A per-registry doc names the old
// populating pass; read it as "derived at the same point".
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
    pub(crate) static MUTABLE_GLOBAL_VARS: std::cell::RefCell<std::collections::HashMap<u32, (u32, Ty)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
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


/// Derive the effect-fn ABI facts of `program` ([`almide_ir::effect_abi`]),
/// settle every call site of a never-err lifted fn against its raw return, and
/// install the facts for the lowering of the program's functions (#3058). The
/// shared desugar chain runs this on every product path and in the corpus
/// classifier, just before the continuation lift (which lowers fns).
pub fn settle_effect_abi(program: &mut almide_ir::IrProgram) {
    let facts = almide_ir::effect_abi::effect_abi_facts(program);
    almide_ir::effect_abi::settle_never_err_calls(program, &facts);
    install(facts);
    install_mutable_globals(program);
}

/// The installed ABI facts, saved on creation and put back on drop: a nested
/// pipeline run (the registry signature lookup lowers a stdlib source in the
/// middle of a caller's per-fn lowering) installs ITS program's facts, which
/// must not outlive it.
pub(crate) struct AbiFactsScope(
    almide_ir::effect_abi::EffectAbiFacts,
    std::collections::HashMap<u32, (u32, Ty)>,
);

impl AbiFactsScope {
    pub(crate) fn save() -> Self {
        let facts = almide_ir::effect_abi::EffectAbiFacts {
            never_err_lifted: NEVER_ERR_LIFTED_FNS.with(|s| s.borrow().clone()),
            auto_wrap: AUTO_WRAP_ABI_FNS.with(|s| s.borrow().clone()),
            declared_option: DECLARED_OPTION_FNS.with(|s| s.borrow().clone()),
            declared_option_effect: DECLARED_OPTION_EFFECT_FNS.with(|s| s.borrow().clone()),
            mut_params: MUT_PARAM_FNS.with(|s| s.borrow().clone()),
        };
        AbiFactsScope(facts, MUTABLE_GLOBAL_VARS.with(|s| s.borrow().clone()))
    }
}

impl Drop for AbiFactsScope {
    fn drop(&mut self) {
        install(std::mem::take(&mut self.0));
        let slots = std::mem::take(&mut self.1);
        MUTABLE_GLOBAL_VARS.with(|s| *s.borrow_mut() = slots);
    }
}

/// Is `name` a never-err lifted effect fn of the program being lowered (its
/// body returns the raw value, and every call site was settled against it)?
pub(crate) fn is_never_err_lifted(name: &str) -> bool {
    NEVER_ERR_LIFTED_FNS.with(|s| s.borrow().contains(name))
}

/// Re-install the facts of `program` (after a pass added fns).
pub fn install_effect_abi_facts(program: &almide_ir::IrProgram) {
    install(almide_ir::effect_abi::effect_abi_facts(program));
    install_mutable_globals(program);
}

/// The storage slot of every MUTABLE module-level `var` (program and module
/// top-lets, `tl.mutable`): a pure function of the program, so a slot index
/// means the same thing to every fn lowered.
///
/// The map is keyed by the ENTRY file's VarIds — the region the lowered fns
/// speak. A module's top-let lives under an id of the MODULE's var table, so
/// it is reached through the entry's reference to it (the frontend's
/// `module_top_let_var`: same name, `module_origin` = the module's ident).
/// Keying the module's own ids here put two regions in one key space: an
/// entry id that happened to equal a module var's id read that var's slot
/// and type (`m.rec.n = 3` walled on `m.counts`' `Map` type, #2739).
fn install_mutable_globals(program: &almide_ir::IrProgram) {
    let mut entry: Vec<_> = program.top_lets.iter().filter(|tl| tl.mutable).collect();
    entry.sort_by_key(|tl| tl.var.0);
    let mut slots: std::collections::HashMap<u32, (u32, Ty)> =
        entry.iter().enumerate().map(|(i, tl)| (tl.var.0, (i as u32, tl.ty.clone()))).collect();
    let mut next = entry.len() as u32;
    // (module ident, exact name) → (slot, declared type); a name a module
    // defines twice is ambiguous and binds nothing.
    let mut by_name: std::collections::HashMap<(String, String), Option<(u32, Ty)>> =
        std::collections::HashMap::new();
    for m in &program.modules {
        let origin = crate::lower::crossmod_toplets::origin_key(m);
        let mut vars: Vec<_> = m.top_lets.iter().filter(|tl| tl.mutable).collect();
        vars.sort_by_key(|tl| tl.var.0);
        for tl in vars {
            let Some(info) = m.var_table.entries.get(tl.var.0 as usize) else { continue };
            let key = (origin.clone(), info.name.as_str().to_string());
            by_name.entry(key).and_modify(|e| *e = None).or_insert(Some((next, tl.ty.clone())));
            next += 1;
        }
    }
    for (i, info) in program.var_table.entries.iter().enumerate() {
        let Some(origin) = info.module_origin.as_deref() else { continue };
        if let Some(Some(slot)) = by_name.get(&(origin.to_string(), info.name.as_str().to_string())) {
            slots.entry(i as u32).or_insert_with(|| slot.clone());
        }
    }
    MUTABLE_GLOBAL_VARS.with(|s| *s.borrow_mut() = slots);
}

fn install(facts: almide_ir::effect_abi::EffectAbiFacts) {
    NEVER_ERR_LIFTED_FNS.with(|s| *s.borrow_mut() = facts.never_err_lifted);
    AUTO_WRAP_ABI_FNS.with(|s| *s.borrow_mut() = facts.auto_wrap);
    DECLARED_OPTION_FNS.with(|s| *s.borrow_mut() = facts.declared_option);
    DECLARED_OPTION_EFFECT_FNS.with(|s| *s.borrow_mut() = facts.declared_option_effect);
    MUT_PARAM_FNS.with(|s| *s.borrow_mut() = facts.mut_params);
}
