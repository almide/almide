
/// Rewrite `match <never-err lifted-effect call> { ok(pat) => A, err(_) => B }` to `{ let pat =
/// <call>; A }`. A never-err lifted effect fn always returns Ok (its body builds no `err`), and its
/// v1 result is the RAW `T` — so the `match` has no real Result tag to dispatch on (reading the raw
/// handle as a Result block TRAPs / linearizes both arms). The sound, byte-matching lowering is: bind
/// the Ok arm's pattern to the raw call result and run the Ok arm; the `err` arm is dead. Handles the
/// common `ok(x)` (Bind) and `ok(_)` (Wildcard) Ok patterns; an Ok arm with a NESTED/structured
/// pattern, a guard, or no Ok arm is LEFT untouched so it stays a `match` that the call-type-unwrap
/// SKIPS and the match-subject lowering WALLs cleanly (never a trap). Runs BEFORE
/// `unwrap_never_err_call_types`.
pub fn rewrite_never_err_effect_match(
    body: &mut IrExpr,
    can_err: &std::collections::HashSet<String>,
    lifted_effect_fns: &std::collections::HashSet<String>,
) {
    use almide_ir::{walk_expr_mut, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S<'a>(&'a std::collections::HashSet<String>, &'a std::collections::HashSet<String>);
    impl IrMutVisitor for S<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            let is_target = matches!(&expr.kind, IrExprKind::Match { subject, .. }
                if matches!(&subject.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.1.contains(name.as_str()) && !self.0.contains(name.as_str())));
            if !is_target {
                return;
            }
            let IrExprKind::Match { subject, arms } = &expr.kind else { return };
            let Some(ok_arm) = arms.iter().find(|a| matches!(&a.pattern, IrPattern::Ok { .. })) else {
                return;
            };
            if ok_arm.guard.is_some() {
                return;
            }
            let IrPattern::Ok { inner } = &ok_arm.pattern else { return };
            // The raw Ok payload type = the subject call's Ok type (Result[T, String] → T).
            let ok_ty = match &subject.ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[0].clone(),
                _ => return,
            };
            let raw_call = IrExpr { ty: ok_ty.clone(), ..(**subject).clone() };
            // Only the `ok(x)` BIND pattern is rewritten — the bound `var` gives the raw call result a
            // named owner with a sound scope-end drop. An `ok(_)` WILDCARD is LEFT as a `match` (it then
            // WALLs cleanly via the un-rewritten path): binding the result to a fresh throwaway var would
            // need a unique VarId the pre-pass cannot mint, and a bare `Expr`-statement call leaves the
            // heap result un-owned on the stack (invalid wat). A clean wall beats that.
            let bind_stmt = match &**inner {
                IrPattern::Bind { var, .. } => IrStmt {
                    kind: IrStmtKind::Bind {
                        var: *var,
                        mutability: almide_ir::Mutability::Let,
                        ty: ok_ty,
                        value: raw_call,
                    },
                    span: None,
                },
                _ => return,
            };
            let body_expr = ok_arm.body.clone();
            let result_ty = expr.ty.clone();
            expr.kind = IrExprKind::Block { stmts: vec![bind_stmt], expr: Some(Box::new(body_expr)) };
            expr.ty = result_ty;
        }
    }
    S(can_err, lifted_effect_fns).visit_expr_mut(body);
}

/// The set of user functions whose CALL type the frontend LIFTS to `Result[T, String]`: an
/// `effect fn` whose DECLARED return is non-Result (so the call site sees the lifted Result while the
/// body returns raw `T`). EXACTLY the predicate in `check/calls.rs` (`sig.is_effect &&
/// !ret.is_result()`). A pure fn, or an effect fn already declaring `Result`/`Option`, is excluded —
/// its return type is real and must not be unwrapped.
pub fn lifted_effect_fn_names(fns: &[IrFunction]) -> std::collections::HashSet<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    fns.iter()
        .filter(|f| {
            f.is_effect
                && !matches!(&f.ret_ty,
                    Ty::Applied(TypeConstructorId::Result | TypeConstructorId::Option, _))
        })
        .map(|f| f.name.as_str().to_string())
        .collect()
}

/// PROGRAM-level pre-pass: inline a MUTUAL-recursive tail SIBLING so the caller becomes DIRECT
/// self-recursive — exposing the parser loops (`flow_rec ⇄ flow_step`, `collect_seq ⇄ seq_item`, …)
/// to the append-accumulator TCO, which only fires on a SELF-call.
///
/// For a function F that calls a sibling G where G calls F back (a mutual pair) and G is called by
/// ONLY F (so dead after inlining), every `G(args)` in F is replaced by G's body with G's parameters
/// substituted by the call's `args`, and G is dropped. Semantics-preserving (a plain inline).
///
/// TRY-LOWER GUARD (no regression by construction): the inline is applied ONLY when F currently WALLS
/// *and* the inlined F then LOWERS — so a function that already lowers (e.g. `esc_rec`, `collect_block`)
/// is NEVER touched (inlining could make it self-recursive and push it into a TCO path that walls). The
/// guard lowers F and inlined-F with the program's `globals`/`record_layouts`, exactly as the real
/// lowering will, so its verdict matches.
/// Populate the name-keyed ABI registries (never-err lifted / auto-wrap / declared-Option)
/// from `fns` — extracted from [`inline_mutual_tail_recursion`] so the pipeline can WIDEN the
/// registries over the WHOLE program (main + mangled module siblings) without feeding module
/// bodies through the main pre-pass rewrites (which regressed the intra-module tail-call shape).
/// A snapshot of the AUTO_WRAP registry — the pipeline's populate→rewrite fixpoint
/// compares successive snapshots to detect stability (the registry is thread-local
/// and `pub(crate)`, so the pipeline reads it through this accessor).
pub fn auto_wrap_abi_snapshot() -> std::collections::HashSet<String> {
    AUTO_WRAP_ABI_FNS.with(|s| s.borrow().clone())
}

pub fn populate_abi_registries(fns: &[IrFunction], _record_layouts: &RecordLayouts) {
    let can_err = compute_can_err(fns);
    let lifted_effect_fns = lifted_effect_fn_names(fns);
    // Publish the never-err lifted set (lifted ∖ can-err) for the match-subject wall (the rare residue
    // `rewrite_never_err_effect_match` cannot turn into a `let`-block — `ok(_)`/structured/guarded Ok).
    NEVER_ERR_LIFTED_FNS.with(|s| {
        *s.borrow_mut() =
            lifted_effect_fns.iter().filter(|n| !can_err.contains(*n)).cloned().collect();
    });
    if std::env::var("ALMIDE_ABI_PROBE").is_ok() {
        eprintln!("[abi] can_err={can_err:?} lifted={lifted_effect_fns:?}");
    }
    AUTO_WRAP_ABI_FNS.with(|s| {
        *s.borrow_mut() = fns
            .iter()
            .filter(|f| f.name.as_str() != "main")
            .filter(|f| {
                !matches!(
                    &f.ret_ty,
                    Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Result
                            | almide_lang::types::constructor::TypeConstructorId::Option,
                        _
                    )
                ) && (body_has_stmt_position_propagating_unwrap(&f.body)
                    || body_has_tail_position_option_unwrap(&f.body)
                    || body_has_tail_position_canerr_try(&f.body, &can_err))
            })
            .map(|f| f.name.as_str().to_string())
            .collect();
    });
    DECLARED_OPTION_FNS.with(|s| {
        *s.borrow_mut() = fns
            .iter()
            .filter(|f| {
                matches!(&f.ret_ty,
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _))
            })
            .map(|f| f.name.as_str().to_string())
            .collect();
    });
}

pub fn inline_mutual_tail_recursion(
    fns: &[IrFunction],
    globals: &HashMap<VarId, Ty>,
    record_layouts: &RecordLayouts,
) -> Vec<IrFunction> {
    use std::collections::{HashMap as Map, HashSet};
    fn named_calls(body: &IrExpr) -> HashSet<String> {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C {
            names: HashSet<String>,
        }
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
                    self.names.insert(name.as_str().to_string());
                }
                walk_expr(self, e);
            }
        }
        let mut c = C { names: HashSet::new() };
        c.visit_expr(body);
        c.names
    }
    // NEVER-ERR `!` STRIP (sound, the scoped form of the reverted blanket strip): an effect call whose
    // callee provably never returns `Err` has a no-op `!`, so `let pat = f()!` → `let pat = f()` and a
    // `f()!` self-call → bare `f()` (which `tco_collect` then recognizes). This is what lets the yaml
    // parser cluster (entirely never-err) TCO; `safe_div` & co. (can-err) keep their `!` and stay walled.
    // Done HERE, before the inline guard's try-lower, so inlined-F sees the stripped body and lowers.
    populate_abi_registries(fns, record_layouts);
    let can_err = compute_can_err(fns);
    let lifted_effect_fns = lifted_effect_fn_names(fns);
    let stripped: Vec<IrFunction> = fns
        .iter()
        .map(|f| {
            let mut nf = f.clone();
            strip_declared_option_trys(&mut nf.body);
            strip_never_err_unwraps(&mut nf.body, &can_err, &lifted_effect_fns, f.name.as_str());
            rewrite_fan_map_pure(&mut nf.body);
            crate::lower::desugar_option_str_literal_match(&mut nf.body);
            rewrite_never_err_effect_match(&mut nf.body, &can_err, &lifted_effect_fns);
            unwrap_never_err_call_types(&mut nf.body, &can_err, &lifted_effect_fns);
            rewrap_never_err_into_result_targets(
                &mut nf.body,
                &can_err,
                &lifted_effect_fns,
                record_layouts,
            );
            nf
        })
        .collect();
    let fns: &[IrFunction] = &stripped;
    let lowers =
        |f: &IrFunction| lower_function_all_with_types(f, globals, record_layouts).is_ok();
    let calls: Map<String, HashSet<String>> =
        fns.iter().map(|f| (f.name.as_str().to_string(), named_calls(&f.body))).collect();
    let mut callers: Map<String, HashSet<String>> = Map::new();
    for (f, cs) in &calls {
        for c in cs {
            callers.entry(c.clone()).or_default().insert(f.clone());
        }
    }
    let by_name: Map<&str, &IrFunction> = fns.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut rewritten: Map<String, IrFunction> = Map::new();
    let mut dropped: HashSet<String> = HashSet::new();
    for f in fns {
        let fname = f.name.as_str();
        if dropped.contains(fname) {
            continue;
        }
        // G: F calls G, G calls F back, G ≠ F, G local, ONLY F calls G (droppable).
        let g = calls[fname].iter().find(|g| {
            g.as_str() != fname
                && !dropped.contains(g.as_str())
                && by_name.contains_key(g.as_str())
                && calls.get(*g).is_some_and(|gc| gc.contains(fname))
                && callers.get(*g).is_some_and(|cs| cs.len() == 1 && cs.contains(fname))
        });
        if let Some(g) = g {
            // Guard: only inline if F WALLS now and the inlined F LOWERS (else leave both untouched —
            // no regression of an already-lowering function).
            if !lowers(f) {
                let mut nf = f.clone();
                inline_sibling_calls(&mut nf.body, g, by_name[g.as_str()]);
                if lowers(&nf) {
                    rewritten.insert(fname.to_string(), nf);
                    dropped.insert(g.clone());
                }
            }
        }
    }
    fns.iter()
        .filter(|f| !dropped.contains(f.name.as_str()))
        .map(|f| rewritten.remove(f.name.as_str()).unwrap_or_else(|| f.clone()))
        .collect()
}

/// Replace every `Call(callee_name, args)` in `body` with `callee`'s body, its parameters substituted
/// by `args` (a single-level inline; the inlined body's calls — back to the OUTER fn — are left as-is,
/// turning the caller into a direct self-recursion).
fn inline_sibling_calls(body: &mut IrExpr, callee_name: &str, callee: &IrFunction) {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    struct V<'a> {
        name: &'a str,
        callee: &'a IrFunction,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind {
                if name.as_str() == self.name && args.len() == self.callee.params.len() {
                    let mut b = self.callee.body.clone();
                    for (p, a) in self.callee.params.iter().zip(args.iter()) {
                        b = almide_ir::substitute_var_in_expr(&b, p.var, a);
                    }
                    *expr = b;
                }
            }
        }
    }
    V { name: callee_name, callee }.visit_expr_mut(body);
}

/// Lower a function body expression to MIR (the param-free testable core;
/// `lower_function` is the wrapper that seeds parameters first).
pub fn lower_body(body: &IrExpr, name: &str) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx::default();
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

/// Like [`lower_body`] but returns the main function PLUS any lambda-lifted auxiliaries
/// the body produced (index 0 is the main). The plain [`lower_body`] discards the lifted
/// set, so a test that lifts a closure must use this to see (and verify) the lifted
/// function where the closure's body — and its captured calls — now live.
#[cfg(test)]
pub(crate) fn lower_body_all(body: &IrExpr, name: &str) -> Result<Vec<MirFunction>, LowerError> {
    let mut ctx = LowerCtx { fn_name: name.to_string(), ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    let lifted = std::mem::take(&mut ctx.lifted);
    let mut all =
        vec![MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() }];
    all.extend(lifted);
    Ok(all)
}

/// Like [`lower_body`] but seeds the declared GLOBAL set (top-level `let`s) so a
/// reference to one is admitted by `value_or_global` instead of walled. Test/diagnostic
/// entry — `lower_function` builds the same context for real programs.
#[cfg(test)]
pub(crate) fn lower_body_with_globals(
    body: &IrExpr,
    name: &str,
    globals: HashMap<VarId, Ty>,
) -> Result<MirFunction, LowerError> {
    let mut ctx = LowerCtx { globals, ..Default::default() };
    let ret = ctx.lower_body_into(body)?;
    Ok(MirFunction { name: name.to_string(), ops: ctx.ops, ret, ..Default::default() })
}

#[derive(Default)]
pub(crate) struct LowerCtx {
    ops: Vec<Op>,
    /// VarId → the MIR value it denotes. Aliases map to the SAME ValueId.
    value_of: HashMap<VarId, ValueId>,
    /// Heap handles in binding order, for scope-end drops (one Drop per handle).
    live_heap_handles: Vec<ValueId>,
    /// The MIR values that are BORROWED heap parameters (the v1 calling
    /// convention): the caller owns the reference. A direct move-out/return or
    /// in-place mutation of one needs an explicit acquire (`Dup`) the body does
    /// not perform, so it is walled — never lowered to an unbacked cert event.
    param_values: HashSet<ValueId>,
    next_value: u32,
    /// recursive-eq brick (synth_eq.rs): the variant types whose synthesized eq
    /// helper exists / is being generated in THIS fn (a self-typed field inside
    /// a generating body emits the helper CALL instead of re-inlining), and the
    /// generated helper MirFunctions (returned with the cluster like `lifted`).
    synth_eq_types: std::collections::BTreeSet<String>,
    synth_eq_fns: Vec<MirFunction>,
    /// Depth of enclosing control-flow FRAMES (branch arms / loop bodies). A heap
    /// reassignment at depth > 0 must NOT rebind `value_of` — the new handle would
    /// be frame-local (dropped at the frame's end), yet the var is read on the next
    /// iteration or after the branch merges, dereferencing a freed handle (a UAF the
    /// flat fold cannot see). Inside a frame such a reassignment is DEFERRED: the var
    /// keeps its still-live handle and the new value is carried like every `Opaque`.
    in_frame: u32,
    /// Depth of enclosing DEFUNCTIONALIZED HOF bodies (`list.map((x) => …)`) being lowered inline.
    /// When > 0, a SELF-RECURSIVE call in a heap-result body is BOUNDED (the map iterates a finite
    /// list; a `render_el(child, …)` recurses to the tree's depth, not unbounded), so it is ADMITTED —
    /// unlike a function-tail self-call (the unbounded TCO shape that overflows the stack), which the
    /// `lower_heap_result_arm` self-call gate still WALLS when this is 0.
    in_defunc_body: u32,
    /// Depth of enclosing SCALAR-STATE loops being lowered with real markers
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd`). When > 0, a scalar `Assign` reassigns
    /// the var's STABLE local via [`Op::SetLocal`] (the loop-carried state) instead of
    /// rebinding `value_of` to a fresh value (which a loop back-edge could not see), and a
    /// HEAP reassignment ERRORS — that aborts the scalar-loop attempt so `lower_while`
    /// falls back to its sound model-one-iteration form (a heap accumulator is deferred,
    /// not run, exactly as before).
    scalar_loop_depth: u32,
    /// Depth of enclosing EXECUTABLE Unit (statement) `if`/`match` arms — lowered with
    /// real markers (`IfThen`/`Else`/`EndIf`) so exactly ONE arm runs at runtime. When
    /// > 0, a scalar `Assign` to a var that ALREADY has a stable local (declared outside
    /// the arm — `var r = 0`) mutates that local via [`Op::SetLocal`] instead of rebinding
    /// `value_of` to a fresh value. A fresh rebind is frame-local: `value_of[var]` ends up
    /// pointing at whichever arm was lowered LAST (last-writer-wins), so a read after the
    /// branch sees a local only that arm's `local.set` wrote — but at runtime the OTHER
    /// arm ran, leaving it unset (the `match n { 0 => {r=100}, x => {r=999} }` 0-vs-999
    /// silent miscompile). SetLocal-to-the-stable-local is the faithful in-place mutation
    /// v0 performs. Distinct from `scalar_loop_depth` (loops also block heap rebinds and
    /// roll back the whole attempt); here a heap reassignment keeps the existing branch-arm
    /// DEFER behavior. Cert-neutral: a scalar `SetLocal` carries no heap ownership (the
    /// same no-op `verify_ownership` already proves for the loop-carried SetLocal).
    unit_arm_depth: u32,
    /// The module's top-level `let` bindings (VarId → declared Ty). A reference to one
    /// of these resolves to no FUNCTION-local `value_of` entry; this DECLARED set lets
    /// `value_or_global` distinguish a legitimate global reference (materialize a fresh
    /// external value) from a genuine lowering gap (a local that should have been bound
    /// — still WALLED). Confirming against the declared set, not merely a `value_of`
    /// miss, is what keeps the boundary a wall instead of a silent hole.
    globals: HashMap<VarId, Ty>,
    /// The module-level globals' INITIALIZER expressions (VarId → its `let` value). A HEAP
    /// global reference materializes a FRESH OWNED copy by lowering this initializer in place
    /// (`value_or_global`) — sound value semantics (each reference is an independent copy,
    /// dropped at scope end). Only initializers inside the lowering subset (a string literal,
    /// `bytes.from_list([…])`, a scalar-list literal) succeed; anything else keeps the wall.
    /// Empty for the simple entries; populated by the real pipeline from the program's top_lets.
    global_inits: HashMap<VarId, IrExpr>,
    /// MIR values KNOWN to be MATERIALIZED Options (the 0-or-1-element-list layout:
    /// `Some(x)` = `Init::OptSome` len=1, `None` = `Init::Opaque` len=0). A variant
    /// `match` may EXECUTE (read `len` as the tag, extract `data[0]`) ONLY over a
    /// subject in this set — every other Option (a closure/range/deferred `Opaque`, a
    /// non-self-host Option-returning call) is `Opaque` with len=0 and would MISREAD as
    /// `None`, so it keeps the sound LINEARIZED match. This is the gate that makes the
    /// len-as-tag execution safe without any global materialization invariant.
    materialized_options: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED Results (the DynListStr len-as-tag layout: `Ok(int)` =
    /// len 0 with the value in slot 0, `Err(string)` = len 1 owning the message). An `Ok`/`Err`
    /// `match` may EXECUTE (read `len` as the tag — len 0 → Ok, len != 0 → Err — and extract slot
    /// 0) ONLY over a subject in this set; any other Result is a deferred `Opaque` (len 0 → MISREADS
    /// as Ok) and keeps the sound LINEARIZED match. The Result analogue of `materialized_options`.
    materialized_results: HashSet<ValueId>,
    /// MIR values KNOWN to be MATERIALIZED HEAP-Ok Results (`Result[String, String]` etc.): a 1-slot
    /// DynListStr (cap 1, len 1 — IDENTICAL block size to every String, so the free-list reuses it)
    /// that ALWAYS owns one String in slot 0's LOW 32 bits (@12 — Ok's value OR Err's message), with
    /// the Ok/Err TAG in slot 0's HIGH 32 bits (@16: 0=Ok, 1=Err). `DropListStr` `i32.wrap`s the slot
    /// to the low-32 handle, so the high-32 tag is inert. An `Ok`/`Err` `match` reads @16 and binds
    /// the @12 handle as a borrowed String. The heap-Ok-payload analogue of `materialized_results`.
    materialized_results_str: HashSet<ValueId>,
    /// Lambda-lifted auxiliary functions produced while lowering this function's body
    /// (a non-capturing `let f = (x) => …` or a lambda call-argument lifts its body to a
    /// fresh MirFunction here, bound via `Op::FuncRef`). `lower_function_all` returns these
    /// alongside the main function so the program assembler tables + verifies them.
    lifted: Vec<crate::MirFunction>,
    /// The enclosing source function's name — the file-unique prefix for lifted lambda
    /// names (`__lambda_<fn_name>_<n>`). The corpus harness keys the in-profile map by name
    /// within a file, so two source functions each lifting `__lambda_0` would COLLIDE
    /// without this prefix (one lambda's certificate silently lost). Set by
    /// `lower_function_all`; empty for the param-free testable `lower_body` entry.
    fn_name: String,
    /// MIR values that denote a CLOSURE BLOCK — the uniform first-class function
    /// representation: a heap `[rc][len][cap][fnidx][captured…]` block (`lift_lambda`).
    /// A later call whose callee is one of these (`f(args)` where `f` bound a lambda /
    /// a function-typed param or call result) lowers to `Op::CallIndirect` through the
    /// block (fnidx loaded from slot 0, the block passed as the borrowed env arg —
    /// `emit_closure_call`) instead of deferring — the closure EXECUTES.
    closure_values: HashSet<ValueId>,
    /// C1 DIRECT-CALL INLINE: source-`VarId` → the INLINE lambda (`params`, `body`) a `let f =
    /// (x) => body` statically bound. A later DIRECT call `f(args)` whose callee is this `f`
    /// is DEFUNCTIONALIZED — the body is lowered INLINE with each param bound to its arg, and
    /// the captures resolve through `value_of` (they are in scope at the call site). This is
    /// what makes `let s = "ab"; let f = (x) => string.len(s) + x; f(1)` EXECUTE (return 3)
    /// instead of deferring the capturing lambda to an Opaque + `Const 0`. A lambda that ALSO
    /// lifts (non-capturing) keeps its `funcref_values` CallIndirect path; this map is the
    /// inline route for the CAPTURING / non-lifted case (recorded for BOTH, the call site
    /// prefers inline). Cleared per function (Default).
    lambda_bindings: HashMap<VarId, (Vec<(VarId, Ty)>, IrExpr)>,
    /// Every local's DECLARED type, recorded at its `Bind` (reassignment keeps the type).
    /// `FieldAssign` resolves the TARGET's record layout through this — the stmt itself
    /// carries only the VarId, and the field-slot store (`r.f = v` → `ListSetScalar`)
    /// needs the container type's field offsets.
    var_decl_tys: HashMap<VarId, Ty>,
    /// SHARED-CELL vars (closures Rung 6): locals that are BOTH captured by some lambda
    /// AND mutated (an `Assign`/in-place mutator anywhere in the fn — enclosing scope or
    /// any lambda body). A plain env value-copy capture silently LOSES such mutations
    /// (the closure rebinds its copy — the container-stored-closure miscompile class), so
    /// these vars live in a heap CELL instead: a 1-slot block holding the current
    /// value/handle, read fresh at every reference and written through at every assign —
    /// the LOCAL analogue of the mutable-global slot machinery (`value_or_global` /
    /// `__mg_take`+Store). Computed by `collect_cell_vars` over the final desugared body
    /// before lowering, so bind/read/write/capture all agree on which vars are cells.
    cell_vars: HashSet<VarId>,
    /// var → its live CELL BLOCK value (`cell_vars` members only, populated at the
    /// `Bind`). Reads load slot 0 fresh (never cached in `value_of` — an intervening
    /// closure call may have written it); assigns take+drop the old slot value and
    /// store the new one; `lift_lambda` captures the CELL handle (rc-shared), so the
    /// closure and the enclosing scope address the same storage.
    cell_of: HashMap<VarId, ValueId>,
    /// MIR values that are `List[String]` (NESTED-OWNERSHIP lists — their i64 slots hold OWNED
    /// String handles). A scope-end drop of one emits [`Op::DropListStr`] (recursive free),
    /// not a flat [`Op::Drop`] — so the element Strings are reclaimed. Populated when an
    /// `alloc_list_str` result or a `List[String]`-typed bind is created (Machinery 2).
    heap_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[List[String]]` (the csv `rows` shape: a list whose element slots
    /// hold owned `List[String]` blocks). A scope-end drop emits [`Op::DropListListStr`] (a NESTED
    /// free: each row's cell Strings, then each row block, then the outer block) — a flat
    /// `DropListStr` would only `rc_dec` each inner-list handle, LEAKING the cells. Populated by the
    /// list-of-lists concat (`rows + [cur]`).
    list_list_str_lists: HashSet<ValueId>,
    /// MIR values that are a `Result[Value, String]` (the `ok(value.array(...))` shape). A scope-end
    /// drop emits [`Op::DropResultValue`] (tag-dispatch: Ok → `$__drop_value`, Err → `rc_dec`) — a
    /// flat `DropListStr` would leak the Ok Value's nested payload.
    value_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(String, Int), String]` wrapper (toml `parse_key_part`'s
    /// `ok((slice, pos))`). A scope-end drop emits [`Op::DropResultStrInt`] (tag-dispatch: Ok → free
    /// the `(String, Int)` tuple @12 recursively (its String slot only), Err → `rc_dec` the String) —
    /// a flat `DropListStr` would leak the Ok tuple's String + free the tuple block as if it were one.
    str_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(Value, Int), String]` wrapper (toml `parse_val`'s
    /// `ok((value.…, pos))`). A scope-end drop emits [`Op::DropResultValueInt`] (Ok → free the
    /// (Value, Int) tuple recursively via `$__drop_value_tuple`, Err → `rc_dec` the String) — a flat
    /// `DropListStr` would leak the Value's nested payload.
    value_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(List[Value], Int), String]` wrapper (toml `collect_array_items`'s
    /// `ok((items, np))`). A scope-end drop emits [`Op::DropResultListValueInt`] (Ok → free the
    /// (List[Value], Int) tuple recursively via `$__drop_list_value_tuple`, Err → `rc_dec` the String).
    list_value_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[(List[String], Int), String]` wrapper (toml `parse_key` /
    /// `parse_table_key`'s `ok((keys, pos))`). A scope-end drop emits [`Op::DropResultListStrInt`]
    /// (Ok → free the (List[String], Int) tuple recursively: each element String, the List block, the
    /// tuple block; Err → `rc_dec` the String) — a flat `DropListStr` would leak the List's Strings.
    list_str_int_result_results: HashSet<ValueId>,
    /// MIR values that are a `Result[List[String], String]` wrapper (the `fs.list_dir` `ok([name,…])`
    /// shape — NO tuple). A scope-end drop emits [`Op::DropResultListStr`] (Ok → free the List[String]
    /// payload recursively: each element String, then the List block; Err → `rc_dec` the String) — a
    /// flat `DropListStr` would `rc_dec` only the @12 List HANDLE, leaking the element Strings + block.
    list_str_result_results: HashSet<ValueId>,
    /// MIR values KNOWN to be a REAL, POPULATED list block (a list LITERAL, a heap-list PARAM —
    /// the v1 convention passes a genuine block —, or a self-host list-returning CALL whose closure
    /// args ALL lifted, so the callee actually fills it). A direct `xs[i]` (`lower_scalar_index_access`)
    /// computes a bounds-checked `$elem_addr` load that TRAPS on `i >= cap`, so it may fire ONLY over
    /// a value in this set: an Opaque/deferred list (a `list.map` whose param-invoking lambda could
    /// NOT lift → an empty/garbage block) has cap 0 and would TRAP at `xs[0]`, a NEW crash where the
    /// deferred `Const 0` merely mis-valued. Gating on real materialization keeps `xs[i]` from
    /// regressing an unmaterialized-list program to a runtime trap.
    materialized_lists: HashSet<ValueId>,
    /// Heap binds that fell to the DEFERRED `Alloc{Opaque}` model (an EMPTY block whose
    /// content is never populated — sound only while nothing READS through it). A
    /// custom-variant `match` over such a subject would read a garbage tag and execute
    /// the wrong arm (the record-ctor mt2 miscompile), so the match paths WALL on it.
    pub(crate) deferred_opaque_binds: HashSet<ValueId>,
    /// Set true by `lower_pure_module_call_args` when a closure ARGUMENT to a pure combinator could
    /// NOT be lifted to a FuncRef (a capturing / param-invoking lambda — `list.map(fns, (f) => f(10))`)
    /// and so fell back to `record_elided_calls`. The auto-linked self-host combinator then runs with
    /// a MISSING closure slot → an empty / garbage result list, NOT a faithfully-filled one. The
    /// `list.map` bind reads this to decide whether the result is a `materialized_lists` member (safe
    /// to index directly) — a genuinely-lifted map fills the list (admit `xs[i]`), an unlifted one
    /// does not (defer `xs[i]` to `Const 0`, no trap). Reset before each module-call arg lowering.
    last_call_had_unlifted_closure: bool,
    /// MIR values of the dynamic `Value` type (the Codec data model). A scope-end drop emits
    /// [`Op::DropValue`] (runtime-tag-dispatched: a Str/Array/Object Value frees its one heap
    /// payload, a scalar Value just frees the block) instead of a flat [`Op::Drop`]. Populated
    /// when a `Value`-typed bind is created.
    value_handles: HashSet<ValueId>,
    /// MIR values that are `List[Value]` (a list whose i64 slots hold OWNED dynamic `Value` handles,
    /// each itself possibly a heap-payload Str/Array). A scope-end drop emits [`Op::DropListValue`]
    /// (recursive `$__drop_value` per element) instead of the flat [`Op::DropListStr`], which would
    /// leak each element Value's nested payload. Populated when a `List[Value]` literal/arg is
    /// materialized. Distinct from `heap_elem_lists` (String elements, whose `rc_dec` is the full free).
    value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, Value)]` whose element slots hold owned (String, Value)
    /// TUPLE blocks (the yaml `pairs` shape). A scope-end drop emits [`Op::DropListStrValue`]
    /// (`$__drop_list_str_value`: per tuple, rc_dec the String slot + recursive `$__drop_value` the Value
    /// slot, then the tuple, then the list) — a flat [`Op::DropListStr`] would leak each tuple's payloads.
    /// Populated when a `List[(String,Value)]` concat is materialized via `__list_concat_rc`.
    str_value_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `List[(String, String)]` (the `map.entries` / svg render_attrs shape) —
    /// element slots hold owned (String, String) TUPLE blocks. A scope-end drop emits
    /// [`Op::DropListStrStr`] (`$__drop_list_str_str`: per tuple, rc_dec BOTH String slots, then the
    /// tuple, then the list). The (String,String) counterpart of `str_value_elem_lists`.
    str_str_elem_lists: HashSet<ValueId>,
    /// MIR values that are a `value.as_array` Result `Result[List[Value], String]` (the cap-as-tag
    /// 1-slot block whose Ok payload @12 is a `List[Value]`). A scope-end drop emits
    /// [`Op::DropResultListValue`] (`$__drop_result_lv`: Ok → recursive list free, Err → String free)
    /// instead of the flat [`Op::DropListStr`] (which leaks the list's element Values). Read by the
    /// SAME cap@16 match machinery as a str-result (`materialized_results_str`); only the DROP differs.
    value_result_lists: HashSet<ValueId>,
    /// MIR values KNOWN to be a record/tuple block this brick MATERIALIZED with the uniform
    /// slot layout (`try_lower_scalar_record_construct` / `try_lower_record_construct` /
    /// `try_lower_scalar_tuple_construct` / scalar-tuple/list-slot), plus aggregate-typed
    /// params (the v1 convention passes the same-layout block pointer). A PRECISE field read
    /// that DEREFERENCES a loaded slot — a heap-field BORROW (`b.label`), which passes the
    /// loaded handle to a String/List consumer — is admitted ONLY over a value in this set:
    /// a DEFERRED `Alloc{Opaque}` aggregate (a spread record / a call result) has ZERO
    /// (garbage) slot handles, so loading + dereferencing one would TRAP at `rc_dec`. (A
    /// scalar field read does not dereference, so it tolerates a 0 slot as a benign mis-read;
    /// but a heap-field deref must be gated on REAL materialization.)
    materialized_aggregates: HashSet<ValueId>,
    /// MIR values that are MIXED scalar+heap record/tuple blocks → the i64-SLOT INDICES that
    /// hold an OWNED heap handle (a `String`/`List`/nested-aggregate field). Such a value's
    /// scope-end / per-iteration drop emits a [`Op::DropListStr`] (cert = the SAME single `d`
    /// as any drop — each heap field was accounted `m` when stored), and the render frees
    /// exactly these slots (then the block) via the per-value mask carried on the
    /// [`MirFunction::heap_slot_masks`] side table. A value here is treated like a
    /// `heap_elem_lists` member for drop-op SELECTION, but the mask makes the recursive free
    /// touch only the heap slots (NOT every slot — the scalar fields must not be `rc_dec`'d).
    record_masks: HashMap<ValueId, Vec<usize>>,
    /// The CURRENT binding (`lower_bind`) is a MUTABLE `var` (set by `lower_stmt` from the
    /// `Bind` mutability). A `var b = r.items` heap-field extraction may be COW-mutated later,
    /// so it must take an OWNED container-grain `Dup` (mutable in place), NOT a precise borrow
    /// (a shared field handle the value-model refuses to mutate). Read by `lower_heap_extraction`.
    binding_is_mutable: bool,
    /// Count of SYNTHETIC temp VarIds allocated while lowering this function (for ANF-lifting a
    /// Call-result whose heap field/element/tuple is extracted directly — `f(x).field`). Each
    /// synthetic id is `u32::MAX - n`, descending from the top of the VarId space so it can never
    /// collide with a frontend-assigned source VarId. See [`LowerCtx::fresh_synth_var`].
    synth_var_count: u32,
    /// The VarId of the CURRENT `Bind` being lowered (set by `lower_stmt`), so the value-lowering
    /// can ask whether THIS var is loop-reassigned (`loop_reassigned_vars`). `None` outside a
    /// statement bind (a sub-expression / argument materialization).
    binding_var: Option<VarId>,
    /// VarIds that are the TARGET of an `Assign` lexically INSIDE a loop (`while`/`for`) in the
    /// current function body — the loop-carried-reassignment (option-C) slots. A MUTABLE `var x =
    /// r.field` whose `x` is in this set must NOT take the owned field-`Dup` (the initial owned copy
    /// + the option-C per-iteration drop are an UNPROVEN ownership coordination that the kernel cert
    /// REJECTS as a leak); such a bind WALLS (`lower_heap_extraction`). A var reassigned only at
    /// straight-line top level is NOT here (that owned-Dup + scope-end drop is balanced). Computed
    /// once per function in `lower_body_into`.
    loop_reassigned_vars: std::collections::HashSet<VarId>,
    /// Named-record layout registry (the VALUE-MODEL field structure): type NAME →
    /// (declared generic param names, declared fields in declaration order). A record
    /// literal / field access typed `Ty::Named(name, args)` resolves its fields here
    /// (substituting the generic params with `args`), so `r.x` loads from the same slot
    /// construction stored to. Empty when lowering without a type registry (the
    /// param-free testable entry) — a `Ty::Named` aggregate then stays walled, a
    /// `Ty::Record`/`Ty::Tuple` (structurally typed) still resolves directly.
    record_layouts: RecordLayouts,
    /// Custom-variant (ADT) layout registry (the tag + per-constructor field structure):
    /// type NAME → its [`VariantLayout`], with a ctor-name → type reverse index. A variant
    /// CONSTRUCT / `match` resolves its tag and field slots here, the value-model sibling of
    /// `record_layouts`. Empty when lowering without a type registry — a variant value then
    /// stays walled (the pre-ADT-brick status quo). Populated by [`build_variant_layouts`]
    /// and threaded via [`lower_function_all_with_layouts`].
    variant_layouts: VariantLayouts,
    /// Constructed CUSTOM-VARIANT values whose scope-end drop must be the RECURSIVE
    /// [`Op::DropVariant`] (a nested-variant type — `Add(Expr, Expr)` — whose flat free would leak
    /// child blocks), mapped to their TYPE NAME (so the render calls the generated `$__drop_<ty>`).
    /// `drop_op_for` consults this before the flat/masked drops. Populated by
    /// `try_lower_variant_ctor` for a type that [`VariantLayouts::needs_recursive_drop`] (ADT brick 5b).
    variant_drop_handles: HashMap<ValueId, String>,
    /// True when THIS function's DECLARED return type is an explicit `Result`/`Option` (e.g.
    /// `effect fn fs.write(...) -> Result[Unit, String]`). Such a return is a REAL inspectable
    /// heap value the caller `match`es on — so a `Result[Unit, _]` TAIL must produce the heap
    /// Result (the heap path), NOT be voided. It is ONLY the SYNTHETIC `Result[Unit, _]` of an
    /// `effect fn … -> Unit` (declared return `Unit`, this flag FALSE) that the
    /// `is_unit_result_ty` voiding in `lower_tail` should turn into a void wasm function. Without
    /// this discriminator a `-> Result[Unit, String]` fn (fs.write) would be emitted void while
    /// its call site treats it as a heap result — a `(local.set $r (call $void))` type mismatch
    /// (invalid wasm). EXACTLY the `lifted_effect_fn_names` predicate's complement (an effect fn
    /// already declaring Result/Option is NOT lifted — its return is real). Set in
    /// `lower_function_all_impl`; defaults false (the void convention) for the bare entries.
    decl_ret_is_result: bool,
    /// The fn's REAL compiled ABI returns `Result[T, String]` — either GENUINELY declared
    /// `-> Result[..]` (STRICTLY Result: an `-> Option[..]` fn is excluded, since a bare tail
    /// Option-`!` there is a same-repr pass-through, which is already correct), or auto-wrapped
    /// via [`AUTO_WRAP_ABI_FNS`]. Gates the bare-tail-Option-`!` desugar
    /// (`desugar_tail_effect_unwrap`): under a Result ABI that pass-through returns the RAW
    /// Option handle AS the Result — a confirmed silent wrong-value. Threaded as an explicit
    /// per-fn FACT (the `unit_main` pattern) so the desugar never trusts a tree-local `.ty`.
    ret_is_result_abi: bool,
    /// This fn's effective ERR type: the declared `Result[_, E]`'s `E`, or `String` for a
    /// lifted effect fn (the synthetic `Result[T, String]`); `None` when no Result ABI applies
    /// (a declared `-> Option[..]` fn, a lifted lambda sub-ctx). Gates the tail-`!` pass-through
    /// (tail.rs): `f() = g()!` returns g's Result AS f's, which is sound only when the err
    /// components match — v0 coerces a mismatch with `.map_err(...)` at the `?` site, so the
    /// unchecked pass-through type-punned the err payload (the collect_map! class, 2026-07-17).
    decl_fn_err: Option<Ty>,
}

/// Type NAME → (generic param names, declaration-ordered fields) — the VALUE-MODEL
/// field registry threaded into lowering (see [`LowerCtx::record_layouts`]).
pub type RecordLayouts =
    HashMap<String, (Vec<almide_lang::intern::Sym>, Vec<(almide_lang::intern::Sym, Ty)>)>;

/// One constructor of a variant type, as the value model sees it: its name, its `tag`
/// (the declaration index — `type E = Lit(Int) | Add(E,E) | Neg(E)` gives Lit=0, Add=1,
/// Neg=2), and its declaration-ordered fields. A TUPLE constructor's positional fields
/// are named `_0`, `_1`, … and a RECORD constructor keeps its declared names — the same
/// synthesis v0 (`emit_wasm` variant registration) uses, so the two backends agree on
/// field identity. A UNIT constructor has no fields.
#[derive(Clone, Debug)]
pub struct VariantCaseLayout {
    pub ctor: almide_lang::intern::Sym,
    pub tag: u32,
    pub fields: Vec<(almide_lang::intern::Sym, Ty)>,
}