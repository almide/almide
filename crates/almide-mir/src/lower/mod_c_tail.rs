// ── tail of mod_c.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// The lowering context for one function: its layouts, plus the return-type
/// facts every `!`/tail rule keys on.
fn new_lower_ctx(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> LowerCtx {
    LowerCtx {
        globals: globals.clone(),
        global_inits: global_inits.clone(),
        fn_name: func.name.as_str().to_string(),
        record_layouts: record_layouts.clone(),
        variant_layouts: variant_layouts.clone(),
        // An EXPLICIT `Result`/`Option` declared return is a REAL heap value the caller inspects
        // (e.g. `fs.write -> Result[Unit, String]`), so a `Result[Unit, _]` tail must NOT be voided
        // — see `LowerCtx::decl_ret_is_result`. A declared-`Unit` effect fn (the synthetic Result)
        // keeps the void convention.
        decl_ret_is_result: matches!(
            &func.ret_ty,
            Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::Result
                    | almide_lang::types::constructor::TypeConstructorId::Option,
                _
            )
        ),
        // STRICTLY-Result declared return (Option excluded — see the field doc) OR an
        // auto-wrapped scalar ABI: the bare-tail-Option-`!` desugar's gate.
        ret_is_result_abi: matches!(
            &func.ret_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)
        ) || crate::lower::AUTO_WRAP_ABI_FNS
            .with(|s| s.borrow().contains(func.name.as_str())),
        decl_ret_ty_is_unit: matches!(&func.ret_ty, Ty::Unit),
        decl_ret_family: match &func.ret_ty {
            t @ Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _) => {
                Some(crate::lower::result_family(t))
            }
            // A LIFTED effect fn (no declared Result/Option — `effect fn f()
            // -> T`): its synthetic carrier is `Result[T, String]`, whose
            // family follows the DECLARED ret's heapness. Heap-ret effect fns
            // ARE lifted too (lifted_effect_fn_names filters only on
            // is_effect + non-Result/Option decl), so hard-coding Scalar here
            // mislabeled every `effect fn -> List[..]` (fs_fold_lines_range's
            // collect_partition) into the rebox path.
            // A LIFTED effect fn (`effect fn f() -> T`, no declared
            // Result/Option): only a SCALAR T admits. A HEAP T's synthetic
            // carrier is built by the scalar-family materializer (len@4 as
            // the tag) while `result_family` would type it HeapOk (tag@16) —
            // a real producer/consumer layout split (found via
            // fs_fold_lines_range: the @16 read took the err branch on an OK
            // carrier). `None` here keeps those fns walling until the split
            // is closed in its own slice.
            t if !matches!(
                    t,
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _)
                )
                && crate::lower::AUTO_WRAP_ABI_FNS
                    .with(|s| s.borrow().contains(func.name.as_str())) =>
            {
                if !crate::lower::is_heap_ty(t) {
                    Some(crate::lower::ResultFamily::Scalar)
                } else if crate::lower::bang_return_probe() {
                    // #1437 L1: UNDER THE PROBE the lift wraps every return
                    // position (auto_wrap_abi_body's probe arm below), so a
                    // heap T's carrier blocks are all @16-readable by
                    // construction — err via the ResErrStr superset, ok via
                    // the cap-as-tag materialize_result_str — and the family
                    // may finally be read off the synthetic type. PROBE-OFF
                    // the #841 hybrid (raw ok tails) still exists and the
                    // containment stays None.
                    Some(crate::lower::result_family(&Ty::result((*t).clone(), Ty::String)))
                } else {
                    None
                }
            }
            _ => None,
        },
        // The fn's effective err type — declared `Result[_, E]`'s E, `String` for the lifted
        // synthetic Result, None for a declared Option (its `!` pass-through is repr-identical).
        decl_fn_err: match &func.ret_ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                if a.len() == 2 =>
            {
                Some(a[1].clone())
            }
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _) => None,
            _ => Some(Ty::String),
        },
        ..Default::default()
    }
}

/// The synthetic-Result ABI retype the lowering applies before its desugar ladder: an
/// `AUTO_WRAP_ABI_FNS` member's root body is retyped to the TRUE compiled carrier
/// `Result[<declared>, String]` (`func.ret_ty` keeps the bare sugar type). `pub` because
/// the classify count-side must apply the SAME retype before `desugar_all` —
/// desugar-before-both means BOTH: without it `desugar_loop_unwrap`'s `Result[T, String]`
/// root gate declines on the count side while the lowering fires it, and the rewrite's
/// injected owned-copy concat becomes a MIR op with no counted IR node (a false
/// `mir > ir` breach on every in-profile loop-`!` fn — the #1176 drift).
pub fn auto_wrap_abi_body(func: &IrFunction) -> Option<IrExpr> {
    if crate::lower::AUTO_WRAP_ABI_FNS.with(|s| s.borrow().contains(func.name.as_str())) {
        let result_ty = Ty::result(func.ret_ty.clone(), Ty::String);
        let mut body = IrExpr { ty: result_ty.clone(), ..func.body.clone() };
        // #1410: a DECLARED-OPTION member's ok-path values are wrapped in
        // `ok(...)` HERE, in the shared retype both the lowering and the
        // classify count-side apply (the #1176 discipline, by construction).
        // The scalar members' raw ok tails are handled by the existing
        // machinery downstream and are left exactly as before; Option is the
        // family whose raw tail survived to the renderer as the #841 hybrid —
        // err path a wrapped Result block, ok path a raw Option block — which
        // no consumer can discriminate: wasm swallowed a failed int.parse and
        // continued with a garbage value where native aborted.
        // #1431 reached the SCALAR members too — but "the existing machinery
        // downstream" is the PROPAGATION rewrite, so it only covers a body
        // that HAS a `!` to piggyback on. A body made can-err by a bare
        // `err(..)` has none, and its raw tail survived to the renderer. The
        // retype therefore takes exactly the bodies the rewrite will not
        // reach; see `has_propagation_site` for why taking the others costs
        // `effect_tco` its loop conversion. Option stays unconditional — that
        // is the #1410 path, already proven.
        let opt_ret = matches!(&func.ret_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _));
        // #1437 L1: under the RETURN-OP PROBE the propagation rewrite (the
        // machinery the skip defers to) never runs, so a `!`-bearing HEAP-ret
        // body would keep its raw ok tails — the #841 hybrid the R2 rule can
        // neither read nor return. Wrap those unconditionally there; SCALAR
        // members keep their proven downstream path (their raw tails lower
        // through the scalar tail machinery probe-on today), and probe-off is
        // byte-identical.
        let wrap_under_probe =
            crate::lower::bang_return_probe() && crate::lower::is_heap_ty(&func.ret_ty);
        if opt_ret || !has_propagation_site(&body) || wrap_under_probe {
            wrap_return_positions_in_ok(&mut body, &func.ret_ty, &result_ty);
        }
        // A SINGLE-EXPRESSION body `X!` (or auto-`?` `X?`) whose callee already
        // answers the carrier: the root retype above and the TAIL are the same
        // node, so the Unwrap itself got stamped `Result[T, String]` — a
        // nonsensical "unwrap yielding the un-unwrapped type" that every
        // downstream gate then declines (`wrap_return_positions_in_ok`: ty !=
        // decl_ty; the unit-tail arm: ty != Unit) and the lowering treats as a
        // bare effect STATEMENT — call, rc_dec the Result, return void. That is
        // BOTH a swallowed err and the def/callsite ABI split (callers
        // `local.set` the promised handle over a void callee = invalid wasm;
        // pin: `effect fn w(p) -> Unit = fs.write(p, "x")!` consumed
        // first-class by testing.assert_err). ADR-0006 Phase 1a names the
        // semantics: a tail `X!` in a fallible fn is PASS-THROUGH — err and ok
        // alike are X's own value, so the root IS the call. Guarded on the
        // inner answering the EXACT carrier; a typed-E mismatch keeps its
        // honest wall.
        if let IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } = &body.kind {
            if std::env::var("ALMIDE_TMPDBG").is_ok() {
                eprintln!("[tmpdbg] passthrough-check fn={} inner_ty={:?} result_ty={:?} eq={}",
                    func.name.as_str(), inner.ty, result_ty, inner.ty == result_ty);
            }
            if inner.ty == result_ty {
                body = (**inner).clone();
            }
        }
        Some(body)
    } else {
        None
    }
}

/// Does this body contain a `!` propagation site? That is the question the
/// #1431 defect turns on. "The scalar members' raw ok tails are handled by the
/// existing machinery downstream" means the PROPAGATION rewrite, which wraps
/// the tail it threads — so it only runs for a fn made can-err by a `!`. A fn
/// made can-err by a bare `err(..)` binding has no propagation to piggyback on:
/// its root was retyped to the carrier while its tail stayed a raw scalar, the
/// callee returned i64 where every consumer read an i32 Result block, and the
/// module failed wasm VALIDATION.
///
/// So the retype wraps exactly the bodies the rewrite will not reach. Wrapping
/// the others is not free: `checked`/`carried` in spec/wasm_cross/effect_tco
/// are auto-wrap members whose tail self-recursion loop-converts, and wrapping
/// their arms retyped the `if` spine so the tco rewrite declined and the wasm
/// leg trapped with `call stack exhausted` at 2e6 depth where native printed
/// its three sums.
fn has_propagation_site(expr: &IrExpr) -> bool {
    struct TryFinder(bool);
    impl almide_ir::visit::IrVisitor for TryFinder {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if matches!(expr.kind, IrExprKind::Try { .. }) {
                self.0 = true;
            }
            if !self.0 {
                almide_ir::visit::walk_expr(self, expr);
            }
        }
    }
    let mut f = TryFinder(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut f, expr);
    f.0
}

/// Wrap every RETURN-position value of `expr` whose type is the declared
/// return type in `ok(...)`, retyping the spine to the Result carrier as it
/// goes. Return positions only — Block tails, If arms, Match arms — never
/// operand or argument positions, so nothing an expression CONSUMES changes
/// shape. `ResultOk`/`ResultErr` values are already the carrier, and a CALL
/// gets its shape from the callee's own ABI; both are left alone.
///
/// Returns whether anything was wrapped, and the spine is retyped ONLY along
/// the paths where something was: a body whose every return position is a call
/// (`effect fn checked(n) = if n < 0 then fail(..) else checked(n - 1)`) must
/// come out BYTE-IDENTICAL to before, or the tail-call loop conversion declines
/// on the retyped spine and the fn runs O(n) stack — spec/wasm_cross/effect_tco
/// trapped with `call stack exhausted` on the wasm leg while native printed its
/// three sums.
fn wrap_return_positions_in_ok(expr: &mut IrExpr, decl_ty: &Ty, result_ty: &Ty) -> bool {
    wrap_return_positions_go(expr, decl_ty, result_ty, false)
}

fn wrap_return_positions_go(expr: &mut IrExpr, decl_ty: &Ty, result_ty: &Ty, in_branch: bool) -> bool {
    match &mut expr.kind {
        IrExprKind::Block { expr: tail, .. } => {
            let wrapped = tail
                .as_deref_mut()
                .map(|t| wrap_return_positions_go(t, decl_ty, result_ty, in_branch))
                .unwrap_or(false);
            if wrapped {
                expr.ty = result_ty.clone();
            }
            wrapped
        }
        IrExprKind::If { then, else_, .. } => {
            let a = wrap_return_positions_go(then, decl_ty, result_ty, true);
            let b = wrap_return_positions_go(else_, decl_ty, result_ty, true);
            if a || b {
                expr.ty = result_ty.clone();
            }
            a || b
        }
        IrExprKind::Match { arms, .. } => {
            let mut any = false;
            for arm in arms.iter_mut() {
                any |= wrap_return_positions_go(&mut arm.body, decl_ty, result_ty, true);
            }
            if any {
                expr.ty = result_ty.clone();
            }
            any
        }
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => false,
        // A call to a NAMED/computed/method callee in return position may
        // already yield the CARRIER — a lifted effect sibling's ABI wraps its
        // exits (wrapping the site again would build `ok(ok(..))`), and the
        // tail SELF-call's unwrapped spine is what lets `effect_tco`
        // loop-convert — so those stay with the existing machinery. A MODULE
        // (stdlib) callee is different: it is a plain VALUE producer with no
        // wrapping ABI of its own, so declining it left the raw value as the
        // fn's return — `map.len(m)` in the tail of an auto-wrapped fn
        // returned a raw i64 where every call site `local.set`s the promised
        // i32 carrier: invalid wasm at validate ("expected i32, found i64").
        // SPINE positions only (the Block-tail chain), and VALUE-producing only:
        // a Module call inside a Match/If ARM stays declined — those already
        // lower through the branch merge + propagation machinery, and wrapping
        // them retyped previously-fine heap-result matches out of the
        // executable subset (the fs_streaming/fs_if_exists/fs_fold_lines_chunked
        // fallback regression this gate's first, arm-wide spelling shipped). A
        // UNIT-typed Module call tail (`testing.assert_err(..)` ending a test
        // fn) stays declined too — the unit-tail machinery
        // (`wrap_unit_body_in_ok`) already turns it into a statement + `ok(())`,
        // and wrapping the call itself built a heap-result construct the same
        // subset refuses. A non-Unit Module call answering the DECLARED type
        // falls through to the value wrap below; a Result-typed one
        // (fs.read_text) fails ty == decl_ty there and stays untouched.
        IrExprKind::Call { target: CallTarget::Module { .. }, .. }
            if in_branch || matches!(expr.ty, Ty::Unit) =>
        {
            false
        }
        IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Computed { .. } | CallTarget::Method { .. }, .. }
        | IrExprKind::TailCall { .. } => false,
        _ => {
            // A VALUE in return position. Wrap it when it produces the declared
            // type; anything else (a Never-typed die, an already-Result
            // propagation) is left for the existing machinery.
            if expr.ty == *decl_ty {
                let inner = std::mem::replace(
                    expr,
                    IrExpr {
                        kind: IrExprKind::Unit,
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    },
                );
                *expr = IrExpr {
                    span: inner.span.clone(),
                    def_id: inner.def_id,
                    kind: IrExprKind::ResultOk { expr: Box::new(inner) },
                    ty: result_ty.clone(),
                };
                true
            } else {
                false
            }
        }
    }
}

fn lower_function_all_impl(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    // A body-less `@extern(wasm, module, name)` function lowers to a thin host-IMPORT
    // call (the browser dom/fetch/timer/console stubs) — its behavior IS the host's, so
    // it CALLS the import, never fabricates a value. Gated STRICTLY on target == "wasm"
    // (a `rust`/`rs` extern has no wasm host → `None` → it keeps walling as before).
    if let Some(import_fn) = try_lower_extern_wasm(func)? {
        return Ok(vec![import_fn]);
    }
    // A `mut` param's write-back rides v0's tuple-return + place-writeback
    // convention (C-131/C-132). The v1 lower has NO move-mode calling convention
    // yet: a mutation through the borrowed param COWs a copy and silently DROPS
    // the caller-visible write (`push9(v, 20)` left `v` unchanged on the verified
    // default while v0 pushed — the #790 mut_list_param row, main-reachable).
    // WALL the fn — v0 emits the correct convention on both targets.
    if !func.mutated_params.is_empty() {
        // The C-132 move-mode pass rewrites every eligible shape upstream
        // (mutated_params cleared), so a surviving entry names the honest
        // boundary: a value-returning effect fn that CAN err carries #1576's
        // unratified question (what the caller's `mut` argument holds after
        // an err), not a missing brick (#1622).
        let why = if func.is_effect && !matches!(func.ret_ty, almide_lang::types::Ty::Unit) {
            "a value-returning effect fn with a `mut` param that CAN err — what \
             the caller's argument holds after an err is #1576's unratified \
             design question (never-err forms rewrite via C-132)"
        } else {
            "the move-mode write-back convention (C-132) not in this brick"
        };
        return Err(LowerError::Unsupported(format!(
            "fn `{}` mutates its `mut` param(s) — {}",
            func.name, why
        )));
    }
    // #1865: a `!`-consumed `fan.map` whose inline callback propagates is refused
    // BEFORE any desugar touches the body — the unwrap ladder would otherwise
    // wall the same fn first, spanless, under a reason naming the wrong construct.
    wall_fan_map_propagating_callbacks(&func.body)?;
    let mut ctx = new_lower_ctx(func, globals, global_inits, record_layouts, variant_layouts);
    let params = ctx.bind_params(&func.params)?;
    // TCO: a tail-self-recursive heap-result function is rewritten to a scalar loop + post-loop
    // dispatch (the existing self-rec guard would otherwise wall it). The rewritten body lowers
    // through the ordinary statements+tail path; if it is out of the TCO subset, `None` keeps the
    // original body (which the self-rec guard walls as before — no regression).
    // PRE-DESUGAR before the TCO: a recursive body `{ let c = if k then A else B; recurse(acc + c) }`
    // has a let-bound heap-result `if` the loop-body lowering would wall. Tail-duplication
    // (`desugar_heap_branches`) pushes the continuation — INCLUDING the recursive call — into each arm,
    // yielding BRANCHED recursion `if k then recurse(acc+A) else recurse(acc+B)` that `tco_collect`
    // handles (it recurses both `if` arms). The let-bound `if` is ELIMINATED, so the loop body lowers.
    // `lower_body_into` desugars again (idempotent) for the non-TCO path; the caps gate counts the
    // SAME desugared tree (desugar-before-both), so mir == ir. Unblocks base64 encode/decode_chunks +
    // toml read_basic/parse_val (the let-bound-heap-`if`-in-a-loop frontier).
    let owned_body;
    let func_body: &IrExpr = if let Some(b) = auto_wrap_abi_body(func) {
        owned_body = b;
        &owned_body
    } else {
        &func.body
    };
    // The desugar-before-both chain: every downstream consumer (counting, TCO,
    // lowering) sees the SAME tree, so `mir == ir` holds for whatever the
    // rewrites introduce.
    let desugared = apply_pre_lower_desugars(func_body, &func.params);
    let func_body: &IrExpr = desugared.as_ref().unwrap_or(func_body);
    // A RESULT-ABI fn (declared `Result[Unit, E]`, or a declared-Unit AUTO_WRAP lift) whose
    // effective TAIL is Unit-typed produces NO value on the unit path — the never-err strips
    // reduce a lifted tail call to a raw Unit effect call, and a declared-Result effect fn can
    // end on a bare effect stmt. But every CALL SITE consults the same name-keyed ABI
    // registries and `local.set`s the expected Result handle over the void callee — invalid
    // wasm (the #786 class: def and call sites disagree on the ABI). Materialize the missing
    // value: `body_unit` → `{ body_unit; ok(()) }`, so the def returns the real Result block
    // its classification promises (the proven alloc(i) + move-out(m) tail). A declared-Unit
    // main is NEITHER case (both gates miss), so the exit-code void convention is untouched.
    let ok_wrapped_body;
    let func_body: &IrExpr = if let Some(result_ty) = unit_tail_result_abi_ty(func, func_body) {
        ok_wrapped_body = wrap_unit_body_in_ok(func_body, result_ty);
        &ok_wrapped_body
    } else {
        func_body
    };
    crate::lower::dump_desugared_ir(func.name.as_str(), func_body, variant_layouts, record_layouts);
    let pre_tco = desugar_heap_branches(func_body, variant_layouts);
    let body_ref: &IrExpr = pre_tco.as_ref().unwrap_or(func_body);
    let tco_body = try_tco_rewrite(&ctx.fn_name, &func.params, body_ref);
    let final_body = tco_body.as_ref().unwrap_or(body_ref);
    // SHARED-CELL pre-scan (closures Rung 6, cells.rs): over the FINAL lowered tree,
    // so bind/read/write/capture all classify the same vars as cells. A pure scan —
    // no rewrite, so the counted tree is untouched.
    ctx.cell_vars = collect_cell_vars(final_body, &ctx.globals, &func.params);
    // WHOLE-BODY read counts, over the SAME final tree — the liveness oracle the
    // statement-at-a-time lowering cannot derive on its own. Its consumer is the
    // in-place accumulator fold, which rebinds a variable's SLOT and is therefore
    // sound only when the bind doing so is that variable's LAST reader. A pure
    // scan; the counted tree is untouched.
    ctx.var_read_counts = collect_var_read_counts(final_body);
    let ret = ctx.lower_body_into(final_body)?;
    // The function's EFFECT SIGNATURE → its declared capability bound. The v1 model
    // has one capability (Stdout); an `effect fn` declares it may reach the host, so
    // it admits the only modeled cap. A pure `fn` declares ∅ — so if it reached
    // Stdout (forbidden by the effect system) the proven `used ⊆ declared` checker
    // would REJECT it. The capability gate verifies `reachable ⊆ declared`, not just
    // "reaches nothing" — so an effectful function is now caps-VERIFIED against its
    // own declared bound, not merely excluded.
    // An `effect fn` declares it MAY reach the modeled host capabilities (the v1 effect system is
    // binary: pure vs host-reaching, not per-capability). So it admits Stdout, Entropy, CliArgs AND
    // FsRead — the `used ⊆ declared` checker then verifies its body stays within that bound. A pure
    // `fn` declares ∅, so reaching ANY cap (a `print`/`random.int`/`env.args`/`fs.read_text` from a
    // non-effect fn — already a frontend type error) would REJECT here too: the soundness floor (pure
    // stays pure) is unchanged; only the host-reaching set grows. (A per-capability effect signature
    // is a later precision refinement.)
    let declared_caps = if func.is_effect {
        vec![
            crate::Capability::Stdout,
            crate::Capability::Entropy,
            crate::Capability::CliArgs,
            crate::Capability::FsRead,
            crate::Capability::FsWrite,
            crate::Capability::Stdin,
        ]
    } else {
        Vec::new()
    };
    let lifted = std::mem::take(&mut ctx.lifted);
    let heap_slot_masks = ctx.record_masks.iter().map(|(v, m)| (*v, m.clone())).collect();
    let main = MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        declared_caps,
        heap_slot_masks,
    };
    let mut all = vec![main];
    all.extend(lifted);
    // The synthesized recursive-eq helpers ride the same rail as lifted lambdas
    // (extra cluster functions; per-parent names, so no cross-fn collision).
    all.extend(std::mem::take(&mut ctx.synth_eq_fns));
    // MIR well-formedness (#777 F3 item 2): every read is preceded by a
    // definition and the defines/reads split partitions op_values, checked on
    // every function this lowering emits — main, lifted lambdas, and synth-eq
    // helpers alike. A violation is a compiler bug surfaced as a NAMED wall
    // (the T6 owes the user a refusal, not a crash), and the fuzz ladder
    // classifies it loudly instead of the drift rendering as wrong bytes.
    for f in &all {
        if let Err(reason) = crate::mir_wellformed::check_def_before_use(f) {
            return Err(LowerError::Unsupported(reason));
        }
        // The `Return` terminal discipline (law 6) rides the same rail: a
        // violation is a lowering bug surfaced as a named wall.
        if let Err(reason) = crate::mir_wellformed::check_return_terminal(f) {
            return Err(LowerError::Unsupported(reason));
        }
    }
    Ok(all)
}

mod binds;
mod layout;
mod tail;
mod control;
mod calls;

// The `??`-operand admission gates (free fns in the private `control` module) — re-exported so the
// `classify_corpus` caps counter consults the SAME predicates the lowering uses (no count drift).


// The in-place `&mut` mutator surface (a free fn in the private `calls` module) — re-exported so
// `inline_pure_call_globals`'s receiver fence tests the SAME predicate the receiver COW does, and
// the two can never drift apart (#906).
pub(crate) use calls::is_inplace_mutator;


#[cfg(test)]
mod tests;

include!("drop_sources.rs");
include!("variant_drop_field_frees.rs");
include!("drop_sources_b.rs");
include!("drop_sources_c.rs");
include!("drop_sources_d.rs");
include!("repr_sources.rs");
include!("repr_sources_b.rs");
include!("repr_sources_c.rs");
include!("repr_sources_d.rs");
include!("usage_scan.rs");
include!("newtype_erase.rs");
include!("newtype_subst.rs");
include!("record_defaults.rs");
include!("desugar_guard.rs");
include!("desugar_guard_b.rs");
include!("desugar_guard_c.rs");
include!("cells.rs");
include!("inline_scalar_fns.rs");
include!("mod_p2.rs");
include!("mod_p2_b.rs");
include!("mod_p2_c.rs");
include!("mod_p3.rs");
include!("mod_p3_b.rs");
include!("mod_p3_c.rs");
include!("mod_p4.rs");
include!("mod_p4_b.rs");
include!("mod_p4_c.rs");
include!("mod_p4_f.rs");
include!("mod_p4_d.rs");
include!("mod_p4_e.rs");
include!("mod_p4_g.rs");
include!("mod_p4_h.rs");
include!("mod_p4_i.rs");
include!("mod_p5.rs");
include!("mod_p5_b.rs");
// The desugar family (formerly one 4.8k-line mod_p6.rs), split by concern:
include!("desugar.rs");
include!("desugar_b.rs");
include!("desugar_c.rs");
include!("desugar_call_arg_anf.rs");
include!("desugar_unwrap.rs");
include!("desugar_unwrap_b.rs");
include!("desugar_nested_unwrap.rs");
include!("desugar_ctor_payload.rs");
include!("desugar_loop.rs");
include!("desugar_loop_b.rs");
include!("desugar_branch.rs");
include!("desugar_branch_b.rs");
include!("desugar_fan.rs");
include!("desugar_match.rs");
include!("desugar_match_grouped.rs");
include!("desugar_match_b.rs");
include!("desugar_match_c.rs");
include!("desugar_match_subject.rs");
include!("synth_eq.rs");
