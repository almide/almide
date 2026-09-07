
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

/// Publish the mutable module-level `var` map (VarId.0 → (slot index, declared Ty)) for
/// [`MUTABLE_GLOBAL_VARS`]. Called wherever the globals maps are collected (pipeline +
/// classify), BEFORE any per-function lowering.
/// Publish the derived-method owner map (see [`DERIVED_TYPE_OWNERS`]).
pub fn set_derived_type_owners(owners: std::collections::HashMap<String, String>) {
    DERIVED_TYPE_OWNERS.with(|o| *o.borrow_mut() = owners);
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

pub fn set_mutable_global_vars(vars: std::collections::HashMap<u32, (u32, Ty)>) {
    MUTABLE_GLOBAL_VARS.with(|s| *s.borrow_mut() = vars);
}

/// Is `var` a mutable module-level `var` (slot-routed cross-function state)?
pub(crate) fn is_mutable_global(var: almide_ir::VarId) -> bool {
    MUTABLE_GLOBAL_VARS.with(|s| s.borrow().contains_key(&var.0))
}

/// The (slot index, declared Ty) of a mutable module-level `var`, if `var` is one.
pub(crate) fn mutable_global_info(var: almide_ir::VarId) -> Option<(u32, Ty)> {
    MUTABLE_GLOBAL_VARS.with(|s| s.borrow().get(&var.0).cloned())
}

/// A function CAN-ERR (returns `Err` on some input) iff its body has a direct `err(…)` (`ResultErr`) OR
/// it `!`-PROPAGATES (an `Unwrap` over a `Named` call to) a can-err function. A function whose entire
/// `!`-call closure is err-free NEVER returns `Err`, so `let pat = f()!` over it is faithfully
/// `let pat = f()` (the same pass-through the tail `!` already uses). KEY: an error reached only through
/// a `match`/`??` (e.g. the yaml cluster calling the PURE `oct_rec`/`bin_rec` int parsers, which DO have
/// `err(…)`, but via `match` not `!`) is HANDLED, not propagated, so it does NOT make the caller can-err —
/// the yaml parser cluster is therefore entirely never-err.
fn has_result_err(body: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            // A direct `err(…)` (`ResultErr`) OR a RESULT-CONSTRUCTING PRIM whose Err arm is
            // runtime-reachable. `prim.read_text_file` builds an `Err(message)` inside the WASI
            // render (path_open failure) — invisible to the IR as a `ResultErr` node, but the fn
            // genuinely CAN return `Err`. So a `fn f() = prim.read_text_file(p)` (the self-host
            // `fs_read_text`) is can-err: its `!` must NOT be stripped as never-err (which would bind
            // the whole Result block where the Ok String was expected). `fs.read_text` reaches main
            // as a `Module` call so the strip already leaves it; this keeps the analysis honest if a
            // `Named`-call form ever appears (e.g. after inlining).
            if matches!(&e.kind, IrExprKind::ResultErr { .. }) {
                self.0 = true;
            }
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &e.kind
            {
                if module.as_str() == "prim"
                    && (func.as_str() == "read_text_file" || func.as_str() == "read_bytes_file")
                {
                    self.0 = true;
                }
            }
            // A `!` over a MODULE / runtime call propagates an error channel this
            // Named-call-only analysis cannot see (`json.parse(body)!` inside
            // porta's parse_and_wrap): the fn IS can-err. Without this it was
            // classified never-err, its callers' `!` got stripped, and the caller
            // read the REAL Result block as the raw payload (record fields off a
            // Result handle — the read_message `method=` garbage, 2026-07-03).
            // …and the same blindness holds for `!`/`?` over ANY non-Named subject: a
            // local Result var (`let r: Result[Int,String] = …; r!` — result_option_
            // matrix's unwrap_result_ok), a field, an Option unwrap (raises on none) —
            // all propagate an error channel the Named-call fixpoint cannot see.
            // Classifying them never-err stripped the CALLERS' `!` (bare i64 read)
            // while the def kept the Result-handle pass-through — the def/callsite
            // ABI split = invalid wasm (i64/i32, latent until the file first rendered).
            // Only a `Named` call stays out (the fixpoint tracks it precisely).
            if let IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } = &e.kind {
                if !matches!(
                    &inner.kind,
                    IrExprKind::Call { target: CallTarget::Named { .. }, .. }
                ) {
                    self.0 = true;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_expr(body);
    v.0 || returns_foreign_result(body)
}

/// The PASSTHROUGH seed (#1406): a fn whose RETURN position is a non-`Named`
/// call typed `Result[..]` — `effect fn helper(p) = fs.read_text(p)`, or the
/// lifted lambda `(p) => fs.read_text(p)` — CAN err (the callee's Err flows out
/// verbatim), yet it builds no `ResultErr` node and carries no `!`/`?`, so
/// neither of `has_result_err`'s walks sees it. Classified never-err it takes
/// the raw-payload ABI, which is consistent for direct calls (both sides read
/// the same table) but breaks the mapper FUNCREF contract — the callback must
/// return the uniform Result carrier — and the mapper result lands untracked
/// (the case-D wall: capability-bearing callback, `fan.any(xs, (p) =>
/// helper(p))`). TAIL positions only: a Module-Result call CONSUMED inside the
/// body (matched, `??`-defaulted) does not make the fn can-err, and widening
/// this to every node would flip the ABI of most of the corpus for nothing.
/// `Named` tail calls stay out — the `compute_can_err` fixpoint tracks them
/// precisely through `unwrap_named_callees`.
fn returns_foreign_result(body: &IrExpr) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn tail(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Call { target, .. } => {
                !matches!(target, CallTarget::Named { .. })
                    && matches!(&e.ty, Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2)
            }
            IrExprKind::Block { expr: Some(t), .. } => tail(t),
            IrExprKind::If { then, else_, .. } => tail(then) || tail(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| tail(&a.body)),
            _ => false,
        }
    }
    tail(body)
}

fn unwrap_named_callees(body: &IrExpr) -> std::collections::HashSet<String> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(std::collections::HashSet<String>);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            // `Try` is the frontend's auto-`?` — the SAME monadic err-propagation as a
            // spelled-out `!` (the effect-unwrap desugar treats them identically), so the
            // can-err fixpoint must see through BOTH. Missing `Try` classified `checked`
            // (whose only propagation is an auto-?'d `fail(..)` arm) as NEVER-ERR: its ABI
            // stripped to raw i64 while the Try arm produced fail's i32 Result — the
            // effect_tco invalid-wasm divergence (i64/i32 at wasm load).
            if let IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } = &e.kind {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind {
                    self.0.insert(name.as_str().to_string());
                }
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(std::collections::HashSet::new());
    v.visit_expr(body);
    v.0
}

/// The set of function names that CAN return `Err` — `has_result_err` seeds + `!`-propagation fixpoint.
pub fn compute_can_err(fns: &[IrFunction]) -> std::collections::HashSet<String> {
    use std::collections::HashSet;
    let mut can_err: HashSet<String> = fns
        .iter()
        .filter(|f| has_result_err(&f.body))
        .map(|f| f.name.as_str().to_string())
        .collect();
    let callees: Vec<(String, HashSet<String>)> = fns
        .iter()
        .map(|f| (f.name.as_str().to_string(), unwrap_named_callees(&f.body)))
        .collect();
    loop {
        let mut changed = false;
        for (name, cs) in &callees {
            if !can_err.contains(name) && cs.iter().any(|g| can_err.contains(g)) {
                can_err.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    can_err
}

/// Strip `Unwrap` (`!`) over a NEVER-ERR `Named` call: `let pat = f()!` → `let pat = f()` and a
/// `f()!` self-call → bare `f()` (so `tco_collect` sees the recursion). SOUND — a never-err callee always
/// returns `Ok`, so the `!` is a no-op; a CAN-ERR callee's `!` is LEFT untouched (it still walls in
/// `lower_destructure`/`lower_bind`), so its error is never silently dropped (the blanket strip that did
/// drop it byte-mismatched safe_div_chain & co. — see the roadmap note).
/// Strip the effect-Result layer over a call to a DECLARED-OPTION effect fn
/// (see [`DECLARED_OPTION_FNS`]): in the v1 model that callee returns the raw Option —
/// there is no err channel to propagate, so the propagation node is the identity.
/// BOTH spellings strip: the frontend's auto-`?` (`Try`) AND the spelled `!`
/// (`Unwrap`) — the checker resolves `f()!` on a declared-Option effect fn as the
/// effect-Result strip (the Unwrap's own ty IS the Option), the same identity as
/// the Try. Leaving the Unwrap in place fed `desugar_effect_unwrap`'s err/ok match
/// a RAW Option block read as a Result — `hit=999` + the scope-end rc_dec trap,
/// the #1125 silent-wrong class. The type gate (`expr.ty` is the Option) keeps a
/// genuine Option-payload unwrap (ty = the payload) out of the strip.
pub fn strip_declared_option_trys(body: &mut IrExpr) {
    strip_declared_option_trys_impl(body, false)
}

/// The program-wide fixpoint's variant (#1557 leg 2): strip ONLY calls to
/// MANGLED (cross-module) declared-Option callees. The pre-pass already ran
/// the unrestricted strip over main with its own-module registry; re-running
/// it unrestricted inside the fixpoint let the evolving can_err sets strip
/// main-region nodes the pre-pass deliberately left (the
/// option_to_result_generic wasm-leg regression). The mangled names are
/// exactly the set the pre-pass could not see by construction.
pub fn strip_declared_option_trys_mangled(body: &mut IrExpr) {
    strip_declared_option_trys_impl(body, true)
}

fn strip_declared_option_trys_impl(body: &mut IrExpr, mangled_only: bool) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        mangled_only: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let declared_option_call = |inner: &IrExpr| {
                matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if (!self.mangled_only || name.as_str().starts_with("almide_rt_"))
                        && DECLARED_OPTION_FNS.with(|s| s.borrow().contains(name.as_str())))
            };
            let strip = match &expr.kind {
                IrExprKind::Try { expr: inner } => declared_option_call(inner),
                IrExprKind::Unwrap { expr: inner } => {
                    declared_option_call(inner)
                        && matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1)
                }
                // #1573: the effect-`??` over a NEVER-ERR declared-Option
                // effect callee. The call site carries the LIFTED carrier
                // (`Result[Option[T], String]`) while the node's own type is
                // the Ok payload `Option[T]` — the err-fallback edge is
                // unreachable for a never-err callee, so the node strips to
                // the call RETYPED to the raw Option (the same
                // Result-to-raw retype the never-err `!` strip performs; the
                // def's never-err lowering returns exactly that raw Option).
                // Keyed on EFFECT callees only; a pure fn's `??` is
                // load-bearing Option-unwrap logic and types differently.
                IrExprKind::UnwrapOr { expr: inner, .. } => {
                    matches!(&inner.kind,
                        IrExprKind::Call { target: CallTarget::Named { name }, .. }
                            if DECLARED_OPTION_EFFECT_FNS.with(|s| s.borrow().contains(name.as_str())))
                        && matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1)
                        && matches!(&inner.ty, Ty::Applied(TypeConstructorId::Result, ra)
                            if ra.len() == 2 && ra[0] == expr.ty && matches!(ra[1], Ty::String))
                }
                _ => false,
            };
            if strip {
                if let IrExprKind::Try { expr: inner }
                | IrExprKind::Unwrap { expr: inner }
                | IrExprKind::UnwrapOr { expr: inner, .. } = &expr.kind
                {
                    let mut inner = (**inner).clone();
                    // The effect-`??` case: the call carries the lifted
                    // carrier type; the never-err def returns the RAW Option,
                    // so the stripped call takes the node's own (payload)
                    // type. Try/Unwrap inners are already raw-typed and the
                    // assignment is the identity there.
                    inner.ty = expr.ty.clone();
                    std::mem::swap(expr, &mut inner);
                }
            }
        }
    }
    S { mangled_only }.visit_expr_mut(body);
}

pub fn strip_never_err_unwraps(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
    self_name: &str,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct S<'a> {
        can_err: &'a std::collections::HashSet<String>,
        lifted: &'a std::collections::HashSet<String>,
    }
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            // The strip is REPRESENTATION-sound only when the callee's v1 body
            // returns the raw `T`: a LIFTED effect fn (the frontend added the
            // Result ABI; the MIR body never built a Result block). A pure /
            // effect fn DECLARED `-> Result[..]` builds a REAL Result block even
            // when it never errs (`ok(rec)`), so stripping its bind-position `!`
            // made the consumer read record fields off the Result handle (the
            // r8 / read_message miscompile, 2026-07-03). SELF-calls keep the
            // strip unconditionally — that is the tail-TCO shape (`f(..)!` →
            // `f(..)`), a same-Result pass-through the tail lowering already
            // treats as such (the yaml parser cluster).
            // `Try` (the frontend auto-`?`) over the same never-err lifted callee is
            // the identical no-op — `x = step(x)` carries `Try{step(x)}`, which an
            // unstripped path lowered to a deferred Const-0 (effect_assign_unwrap).
            // The SELF-call exception admits only a LIFTED self: a DECLARED-Result self
            // (`effect fn eval_e(..) -> Result[Int, String]`) builds a REAL Result block,
            // so stripping a BIND-position `eval_e(l)!` made the binder read the block
            // handle as the raw payload (i64 local ← i32 call: invalid wasm — the
            // box_deref_clone recursion). A lifted self keeps the strip everywhere (the
            // yaml TCO shape); a declared-Result tail self-`!` is pass-through in the
            // tail lowering without any strip.
            let never_err_named_call = |inner: &IrExpr| {
                matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if !self.can_err.contains(name.as_str())
                        && self.lifted.contains(name.as_str())
                        && !crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(name.as_str())))
            };
            let strip = matches!(&expr.kind,
                IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }
                if never_err_named_call(inner));
            if strip {
                if let IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } =
                    &expr.kind
                {
                    let inner = (**inner).clone();
                    *expr = inner;
                }
            }
            // `f() ?? d` over the same never-err lifted callee: the Err arm is
            // unreachable, so the `??` IS the raw call — the identical
            // representation argument as the `!`/`Try` strip above (the callee's
            // v1 value is raw `T`, so no path can read it as a Result block; the
            // #485 effect_assign test fns walled exactly here). Gated to a
            // CALL-FREE fallback (a literal / bare var): v0 evaluates `??`
            // operands eagerly, so a call-bearing fallback has observable
            // effects (and its dropped calls would also skew the caps counter).
            let strip_uo = matches!(&expr.kind,
                IrExprKind::UnwrapOr { expr: inner, fallback }
                if never_err_named_call(inner)
                    && match &fallback.kind {
                        IrExprKind::LitInt { .. }
                        | IrExprKind::LitFloat { .. }
                        | IrExprKind::LitBool { .. }
                        | IrExprKind::LitStr { .. }
                        | IrExprKind::Var { .. } => true,
                        // A negated literal (`?? -1`) — still call-free.
                        IrExprKind::UnOp { operand, .. } => matches!(&operand.kind,
                            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }),
                        _ => false,
                    });
            if strip_uo {
                if let IrExprKind::UnwrapOr { expr: inner, .. } = &expr.kind {
                    let inner = (**inner).clone();
                    *expr = inner;
                }
            }
        }
    }
    let _ = self_name; // kept in the signature: callers name the fn being stripped
    S { can_err, lifted: lifted_effect_fns }.visit_expr_mut(body);
}

/// Rewrite `Try/Unwrap { fan.map(xs, (x) => ok(E)) }` → `list.map(xs, (x) => E)` — a PURE
/// IR transformation. `fan.map` maps in list order and collects the first Err; a lambda
/// whose every exit is `ok(…)` can never Err, and fan lambdas cannot capture a `var`, so
/// parallelism is unobservable — the call IS list.map. This lets the C1 defunctionalization
/// / self-host list machinery execute it on the v1 path (it previously fell to the elided
/// deferred-Const and printed all-zero results — fan_map_inline_lambda, 2026-07-03).
/// Call-count-INVARIANT: one Module call becomes one Module call (`ok` is a constructor).
/// A lambda with a non-`ok` exit (a real Err path) is left untouched and keeps walling.
///
/// PRECONDITION, enforced (#1865): "every exit is `ok(…)`" is NOT "can never Err" when
/// the payload itself PROPAGATES — `(p) => fs.read_text(p)!` reaches here as
/// `ok(unwrap(fs.read_text(p)))`, and stripping the `ok` hands `list.map` a String-
/// returning closure whose `!` has no Result channel left to ride (the lifted body
/// printed garbage bytes per element). Such a body is left as `fan.map`, which
/// [`wall_fan_map_propagating_callbacks`] refuses honestly under its `!` consumer.
pub fn rewrite_fan_map_pure(body: &mut IrExpr) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    // The lambda's tail must be `ok(E)` (directly, or as a block tail). Returns the
    // rewritten body with the `ok` stripped, or None if any exit is not ok-wrapped.
    fn strip_ok(e: &IrExpr) -> Option<IrExpr> {
        match &e.kind {
            IrExprKind::ResultOk { expr } => Some((**expr).clone()),
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let new_tail = strip_ok(tail)?;
                Some(IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(new_tail)) },
                    ty: new_tail_ty(&e.ty),
                    span: e.span.clone(),
                    def_id: e.def_id,
                })
            }
            _ => None,
        }
    }
    fn new_tail_ty(t: &Ty) -> Ty {
        match t {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
            other => other.clone(),
        }
    }
    struct S;
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let (IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }) = &expr.kind
            else {
                return;
            };
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, type_args } =
                &inner.kind
            else {
                return;
            };
            if module.as_str() != "fan" || func.as_str() != "map" || args.len() != 2 {
                return;
            }
            let IrExprKind::Lambda { params, body, lambda_id } = &args[1].kind else { return };
            let Some(new_body) = strip_ok(body) else { return };
            if lambda_body_propagates(&new_body) {
                return;
            }
            let new_lambda_ty = match &args[1].ty {
                Ty::Fn { is_effect: _, params: ps, ret } => Ty::Fn { is_effect: false, params: ps.clone(), ret: Box::new(new_tail_ty(ret)) },
                other => other.clone(),
            };
            let new_lambda = IrExpr {
                kind: IrExprKind::Lambda {
                    params: params.clone(),
                    body: Box::new(new_body),
                    lambda_id: *lambda_id,
                },
                ty: new_lambda_ty,
                span: args[1].span.clone(),
                def_id: args[1].def_id,
            };
            let result_ty = expr.ty.clone(); // the Try-unwrapped List[B]
            *expr = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: almide_lang::intern::sym("list"),
                        func: almide_lang::intern::sym("map"),
                        def_id: None,
                    },
                    args: vec![args[0].clone(), new_lambda],
                    type_args: type_args.clone(),
                },
                ty: result_ty,
                span: expr.span.clone(),
                def_id: expr.def_id,
            };
        }
    }
    S.visit_expr_mut(body);
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

/// Rewrite a NEVER-ERR user `effect fn` `Named` CALL's result type from the lifted-ABI
/// `Result[T, String]` (what the frontend reports so consumers `auto_unwrap`) back to the RAW `T`
/// the v1 function body actually returns. A never-err effect fn's body returns the bare value (no
/// `ok`/`err` wrap — `$f` is `(result i64)`/raw String/List handle, NOT a Result block), so EVERY
/// value-position consumer must see raw `T`: an arg `g(f())`, `list.len(lst())`, a value tail, a
/// `let` — all read the call as a scalar/heap `T`, never a Result handle. Without this the consumer
/// keyed off the `Result[T, _]` `.ty` emitted Result-handle reads (`i32.load` + DropListStr / cap-tag)
/// over the raw i64/handle the never-err callee returns → INVALID WAT (scalar i32/i64 mismatch) or a
/// runtime TRAP (heap: the DropListStr walks the raw String's bytes as element pointers, hitting the
/// `$rc_dec` double-free sentinel). The bind position already works because `lower_bind` uses the
/// LET's unwrapped type; this extends that consistency to arg/value positions, for BOTH scalar and
/// heap `T` (the heap re-type is sound — corpus-wall ACCEPTs — because it is gated to LIFTED effect
/// fns only, whose body genuinely returns a raw handle, so the plain-heap drop is the CORRECT drop,
/// not the trapping Result-as-tag one). A CAN-ERR callee is LEFT untouched (real `Result[T, String]`
/// handle); a PURE `fn` returning a real Result is NOT in `lifted_effect_fns`; a bundled stdlib
/// effect fn is a `Module` call (never matched here). The `match` shape is handled by
/// `rewrite_never_err_effect_match` FIRST (it needs the Result tag to pick the Ok arm); a `Match`
/// SUBJECT call is SKIPPED here so an un-rewritten `match` keeps its lifted type and WALLs cleanly.
pub fn unwrap_never_err_call_types(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S<'a>(&'a std::collections::HashSet<String>, &'a std::collections::HashSet<String>);
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            // A `match SUBJ {…}` whose SUBJ is a never-err lifted-effect call: do NOT unwrap the
            // subject's lifted Result type here. `rewrite_never_err_effect_match` (run before this)
            // already turned every REWRITABLE such `match` into `{ let ok-pat = call; ok-arm }`; any
            // `match` that REMAINS (an un-rewritable Ok pattern) must keep its lifted Result type so
            // the match-subject lowering WALLs it (a clean Unsupported) rather than reading a raw
            // handle as a Result block (a trap). Still recurse into the arm bodies.
            if matches!(&expr.kind, IrExprKind::Match { .. }) {
                if let IrExprKind::Match { subject, arms } = &mut expr.kind {
                    let skip_subject = matches!(&subject.kind,
                        IrExprKind::Call { target: CallTarget::Named { name }, .. }
                            if self.1.contains(name.as_str()) && !self.0.contains(name.as_str()));
                    if skip_subject {
                        for arm in arms.iter_mut() {
                            self.visit_expr_mut(&mut arm.body);
                        }
                        return;
                    }
                }
            }
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind {
                // Unwrap ONLY a call to a LIFTED user effect fn that is also NEVER-err. (Pure Result
                // fns are excluded — not in `lifted_effect_fns` — the list_iter_tco regression fix.)
                if self.1.contains(name.as_str())
                    && !self.0.contains(name.as_str())
                    && !crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(name.as_str()))
                {
                    if let Ty::Applied(TypeConstructorId::Result, a) = &expr.ty {
                        if a.len() == 2 && matches!(a[1], Ty::String) {
                            expr.ty = a[0].clone();
                        }
                    }
                }
            }
        }
    }
    S(can_err, lifted_effect_fns).visit_expr_mut(body);
}

include!("mod_p2_rewrap.rs");
