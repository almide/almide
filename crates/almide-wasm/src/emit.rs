//! The emission entry: the two-pass reachability-pruned pipeline —
//! split from lib.rs for the file budget.

use std::collections::{HashMap, HashSet};

use almide_ir::IrProgram;

use crate::decline_site::DeclineSite;
use crate::types_table::TypeTable;
use crate::*;

/// #2865: package modules keyed like a compiler namespace (a pre-pass of the
/// leg's front; its display half is read by [`fn_site`] below).
#[path = "package_keys.rs"]
pub mod package_keys;

/// #2750: fn-value HOF callbacks eta-expanded to literal lambdas (a pre-pass
/// of the leg's front, split out for the file budget).
#[path = "eta.rs"]
mod eta;
/// #2747: surface forms rewritten to ones the arms lower (map-pair
/// loops, `?.`).
#[path = "front_desugar.rs"]
mod front_desugar;

/// Emit a core wasm module for `ir`, or say precisely why not yet.
/// Two passes: the first loads the WHOLE linked registry graph (so
/// resolution and the refusal BFS see everything) and reports which
/// program fns main actually reaches; when dead fns exist, a second
/// pass re-emits with ONLY the reachable set in the table — real
/// dead-code elimination (the type/decl/stub bookkeeping of a dead
/// registry graph once quadrupled a small module).
pub fn emit_program(ir: &IrProgram) -> Result<Vec<u8>, EmitError> {
    emit_program_with_ops(ir).map(|(b, _)| b)
}

/// [`emit_program`] plus the set of `almide.fs_call` HOST OP numbers the
/// emitted module spells (#1423/#1710): the build path audits it against
/// the stock p1 shim's served set BEFORE shipping — an op the shim cannot
/// serve refuses at build time with its name, never as a runtime refusal
/// on a runtime the developer never ran (the env.set lesson, 2026-08-31).
pub fn emit_program_with_ops(
    ir: &IrProgram,
) -> Result<(Vec<u8>, std::collections::BTreeSet<i32>), EmitError> {
    emit_with_ops(ir, false)
}

/// Library ABI: `_start` initializes globals; every public function must
/// export successfully. Ordinary program emission still requires `main`.
pub fn emit_library_with_ops(
    ir: &IrProgram,
) -> Result<(Vec<u8>, std::collections::BTreeSet<i32>), EmitError> {
    emit_with_ops(ir, true)
}

fn emit_with_ops(ir: &IrProgram, library: bool) -> Result<(Vec<u8>, std::collections::BTreeSet<i32>), EmitError> {
    // Transparent newtypes erase FIRST, so both passes read one tree
    // (#1423 stage 4: the html/path SafeHtml/SafePath rows).
    let erased = crate::newtype::erase_transparent_aliases(ir);
    let ir = erased.as_ref().unwrap_or(ir);
    let desugared = front_desugar::desugar(ir);
    let ir = desugared.as_ref().unwrap_or(ir);
    // #2750: a fn VALUE as a HOF callback becomes the literal lambda the
    // arms inline (eta.rs) — one HOF lowering, never a second call path.
    let expanded = eta::eta_expand_callbacks(ir);
    let ir = expanded.as_ref().unwrap_or(ir);
    // #2004: a call result consumed by a module op's argument gets an
    // owner — bound first, released by the frame's exit plan.
    let bound = crate::arg_temps::bind_native_temporaries(ir);
    let ir = bound.as_ref().unwrap_or(ir);
    // #2577: accumulator recursion elimination — the rewrite the native leg
    // gets from TailCallOpt, from the same shared precondition check.
    let accumulated = accumulate_binary_recursion(ir);
    let ir = accumulated.as_ref().unwrap_or(ir);
    // #2980: small scalar fns inline at their call sites, bound rather
    // than substituted (inline_calls.rs) — the stdlib's own kernels too.
    let inlined = crate::inline_calls::inline_small_scalar_calls(ir);
    let ir = inlined.as_ref().unwrap_or(ir);
    // Witness sweeps (#2754) see the pass boundaries and which pass shipped
    // (no-ops unless a sweep collects).
    use crate::witness::{mark_pass as mark, mark_shipped as ship};
    crate::decline_site::set(None);
    mark(1);
    let first = emit_program_pass(ir, None, library, true)?;
    let keep = (first.visited.len() < first.total).then_some(&first.visited);
    let (bounded, bounded_pass) = match keep {
        Some(k) => {
            mark(2);
            (emit_program_pass(ir, Some(k), library, true)?, 2)
        }
        None => (first.clone(), 1),
    };
    if !bounded.bounded_fired {
        ship(bounded_pass);
        return Ok((bounded.bytes, bounded.ops));
    }
    // #2312: the bounded-line rewrites (line_bounded.rs) usually shrink a
    // module — they can keep the allocator and the line buffer's grow path
    // out of it — but their helpers cost bytes when the checked machinery
    // ships anyway. Emit both and ship the smaller: never larger than the
    // checked emission, and the choice is deterministic.
    mark(crate::witness::CHECKED_PASS);
    let checked = emit_program_pass(ir, keep, library, false)?;
    let best = if bounded.bytes.len() < checked.bytes.len() {
        ship(bounded_pass);
        bounded
    } else {
        ship(crate::witness::CHECKED_PASS);
        checked
    };
    Ok((best.bytes, best.ops))
}

/// Rewrite every `almide_ir::accum_tre` candidate into its accumulator loop
/// (`None` when nothing qualifies, so the common program is not cloned).
/// Skipped whole when the program brackets a budget/timeout region: the
/// new loop head would be a charge the deterministic meter (ALS-DT2) sees,
/// and outside a region the meter is elided so no charge is observable.
fn accumulate_binary_recursion(ir: &IrProgram) -> Option<IrProgram> {
    use almide_ir::accum_tre;
    let any = ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter()));
    if !any.clone().any(accum_tre::is_candidate) || fuel::program_has_regions(ir) {
        return None;
    }
    let mut out = ir.clone();
    for f in out.functions.iter_mut() {
        accum_tre::rewrite(f, &mut out.var_table);
    }
    for m in out.modules.iter_mut() {
        for f in m.functions.iter_mut() {
            accum_tre::rewrite(f, &mut m.var_table);
        }
    }
    Some(out)
}

/// One emission pass's output.
#[derive(Clone)]
struct Pass {
    bytes: Vec<u8>,
    /// The program fns main reaches (pass 2 keeps only these).
    visited: HashSet<usize>,
    total: usize,
    ops: std::collections::BTreeSet<i32>,
    /// A bounded-line rewrite was emitted (`FnWork::bounded_fired`).
    bounded_fired: bool,
}

fn emit_program_pass(
    ir: &IrProgram,
    keep: Option<&HashSet<usize>>,
    library: bool,
    bounded_lines: bool,
) -> Result<Pass, EmitError> {
    let main = ir.functions.iter().find(|f| f.name.as_str() == "main");
    if main.is_none() && !library {
        return unsup("no main function");
    }
    let empty_main = almide_ir::IrExpr::default();
    let main_body = main.map_or(&empty_main, |f| &f.body);
    // Program functions PLUS every linked module's functions — module fns
    // register under their QUALIFIED name ("url.encode_component"), which
    // is exactly the `CallTarget::Module` lookup key. A module carrying
    // top-level lets is excluded whole (its init order is a later slice).
    let program_fns = collect_program_fns(ir);
    // Pass 2: only the fns pass 1 reached (positions are stable —
    // collect_program_fns is deterministic over the same IR).
    let program_fns: Vec<_> = match keep {
        Some(k) => program_fns
            .into_iter()
            .enumerate()
            .filter(|(i, _)| k.contains(i))
            .map(|(_, f)| f)
            .collect(),
        None => program_fns,
    };
    let types = TypeTable::build(ir);
    // Deterministic-meter plan (ALS-DT2): who charges, whose entry is exempt.
    let meter = fuel::meter_plan(ir, registry_impl_names());

    let mut table =
        FnTable { by_name: HashMap::new(), impl_index: HashMap::new(), infos: Vec::new() };
    for (i, (f, qual, _space)) in program_fns.iter().enumerate() {
        let (params, ret, refuse) = match fn_signature(f, &types) {
            Ok((p, r)) => (p, r, None),
            Err(reason) => (Vec::new(), None, Some(reason)),
        };
        // #2275: a body-less `@extern` is a declared import on the wasm
        // target, or a wall — never a hollow body.
        let (import, refuse) = match extern_import(f, &params, ret) {
            Ok(import) => (import, refuse),
            Err(reason) => (None, refuse.or(Some(reason))),
        };
        let key = qual.clone().unwrap_or_else(|| f.name.as_str().to_string());
        // impl_index carries ONLY registry implementation symbols — a
        // global simple-name index over ALL module fns collides across
        // modules (two self-host modules both defining __len_loop made
        // cross_module fixtures call the WRONG module's helper).
        if qual.is_some() && registry_impl_names().contains(f.name.as_str()) {
            table.impl_index.insert(f.name.as_str().to_string(), i);
        }
        table.by_name.insert(key, i);
        // #2503: which arguments the call site must make unique first. An
        // EFFECT callee is excluded, and the exclusion is measured, not
        // cautious: an argument's credit at a `!` call site is not released
        // on the ok path, so the buffer's count grows by one per call and
        // the rc-gated copy would fire on EVERY iteration of a loop like
        // `poke(b, i)!` — a 64 KiB buffer in a 20k-call loop ran out of
        // memory, and this corpus's mut_param_call_chain allocated 4.4x.
        // The alias rule therefore still diverges for an effect callee
        // (#2503 keeps that half), and closing it starts with that credit.
        let param_mut: Vec<bool> = f.params.iter().map(|p| p.is_mut && !f.is_effect).collect();
        table.infos.push(FnInfo {
            wasm_index: F_FN_BASE + i as u32,
            params,
            ret,
            refuse,
            param_owned: Vec::new(),
            param_mut,
            import,
            scoped_entry: f.is_scoped_block_entry(),
        });
    }
    // Which params each callee owns (#2028): computed once, over the whole
    // table, before any body lowers — the call sites and the exit plans
    // read the same vector.
    for (i, owned) in crate::param_borrow::infer(&program_fns, &table, &types).into_iter().enumerate() {
        table.infos[i].param_owned = owned;
    }
    let main_index = F_FN_BASE + program_fns.len() as u32;
    let region_pure = region::region_pure_fns(ir, &program_fns, &table);

    let mut pool = Pool::new();
    // Interned eagerly so $append_bool can carry their fixed addresses.
    let true_base = pool.intern("true");
    let false_base = pool.intern("false");

    let (global_map, global_decls, init_lets) = build_globals(ir, &types);

    // Function-VALUE work shared by every lowering below (funcref table,
    // call_indirect types, lifted lambdas).
    let reach = crate::global_reach::global_reach(&program_fns, &table, &global_map);
    let work = FnWork { region_pure: std::cell::RefCell::new(region_pure), global_reach: reach.into(), ..FnWork::default() };
    work.bounded_lines.set(bounded_lines);
    // Calls made from display-helper bodies (BFS roots).
    let mut display_helper_calls: std::collections::HashSet<usize> = HashSet::new();
    work.itype_base.set(T_FN_BASE + table.infos.len() as u32);
    work.helper_base.set(F_FN_BASE + table.infos.len() as u32 + 1);

    // Lower every callable function; a body that doesn't lower yet is
    // recorded (not fatal) — fatal only if `main` can reach it.
    let mut lowered: Vec<Result<(Function, HashSet<usize>), String>> = Vec::new();
    // Source names for the E083 diagnostic, by (var space, VarId): space 0
    // is the entry program, space i + 1 is modules[i] (collect_program_fns).
    let var_name = |space: u32, id: VarId| -> Option<String> {
        let vt = if space == 0 { &ir.var_table } else { &ir.modules.get(space as usize - 1)?.var_table };
        vt.entries.get(id.0 as usize).map(|v| v.name.as_str().to_string())
    };
    // The lifted lambdas each function body created (`fn_lambdas[i]`, a range
    // into `work.lifted`): a lambda is reachable only through its owner.
    let mut fn_lambdas: Vec<std::ops::Range<usize>> = Vec::new();
    // #2807: the source line each failed body refused at (decline_site.rs).
    let mut fail_lines: HashMap<usize, usize> = HashMap::new();
    for (i, (f, qual, space)) in program_fns.iter().enumerate() {
        let lifted_before = work.lifted.borrow().len();
        if let Some(r) = &table.infos[i].refuse {
            lowered.push(Err(r.clone()));
            fn_lambdas.push(lifted_before..lifted_before);
            continue;
        }
        if table.infos[i].import.is_some() {
            // A declared import's slot: the loud stub the post-pass removes.
            let mut stub = Function::new([]);
            stub.instructions().unreachable().end();
            lowered.push(Ok((stub, HashSet::new())));
            continue;
        }
        let params: Vec<(VarId, SliceTy)> =
            f.params.iter().zip(&table.infos[i].params).map(|(p, &t)| (p.var, t)).collect();
        let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map, var_name: &var_name };
        let cur_module = fn_module(qual.as_deref(), f);
        let effect_raw = if f.is_effect {
            match slice_ty_of(&f.ret_ty, &types) {
                Some(SliceTy::Unit) => Some(SliceTy::Unit),
                // A declared-Result effect fn is SINGLE-layer (probe:
                // `wrap_sum(p)!` strips once to Int): the body yields the
                // Result value itself via ok()/err() — no wrap. Declared-
                // Option and raw-T bodies yield the raw value and wrap
                // (call sites are annotated Result[T?, E] / Result[T, E]).
                Some(SliceTy::Result(..)) => None,
                other => other,
            }
        } else {
            None
        };
        let plan = FnPlan {
            ret: table.infos[i].ret,
            cur_module: cur_module.map(str::to_string),
            effect_raw,
            in_main: false,
            env_captures: None,
            metered: meter.user.contains(f.name.as_str()),
            charge_entry: meter.user.contains(f.name.as_str())
                && !meter.exempt.contains(f.name.as_str()),
            var_space: *space,
            name: qual.clone().unwrap_or_else(|| f.name.as_str().to_string()),
            witness_name: Some(
                qual.clone().unwrap_or_else(|| f.name.as_str().to_string()),
            ),
            self_index: Some(table.infos[i].wasm_index),
            param_owned: Some(table.infos[i].param_owned.clone()),
        };
        crate::decline_site::reset_pending();
        match lower_fn(&params, plan, &f.body, &[], &ctx, &mut pool) {
            Ok(ok) => {
                // Any display helpers this fn registered build NOW — a
                // failing body refuses THIS fn, not the program.
                match display::build_display_helpers(&table, &types, &work, &mut pool) {
                    Ok(calls) => {
                        display_helper_calls.extend(calls);
                        // Self-tail-recursion → loop (tco.rs): only fns
                        // whose call set includes THEMSELVES are scanned.
                        let (body, fcalls) = ok;
                        let body = if fcalls.contains(&i) {
                            let info = &table.infos[i];
                            let pvts: Vec<ValType> =
                                info.params.iter().map(|t| t.val_type()).collect();
                            let rvt = info.ret.map(SliceTy::val_type);
                            tco::loop_convert(&body, &pvts, rvt, info.wasm_index)
                                .unwrap_or(body)
                        } else {
                            body
                        };
                        lowered.push(Ok((body, fcalls)));
                    }
                    Err(EmitError::Unsupported(r)) => lowered.push(Err(r)),
            // E083: a compiler defect is fatal for the whole program — a
            // reachable-or-not leak is still a defect, never a wall.
            Err(e @ EmitError::OwnershipLowering(_)) => return Err(e),
                }
            }
            Err(EmitError::Unsupported(r)) => {
                if let Some(sp) = crate::decline_site::take_pending() {
                    fail_lines.insert(i, sp.line);
                }
                lowered.push(Err(r))
            }
            // E083: a compiler defect is fatal for the whole program — a
            // reachable-or-not leak is still a defect, never a wall.
            Err(e @ EmitError::OwnershipLowering(_)) => return Err(e),
        }
        fn_lambdas.push(lifted_before..work.lifted.borrow().len());
    }

    // `main`: top-lets as the eager prelude, then the body. Failure here is
    // fatal — main is always reachable.
    let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map, var_name: &var_name };
    let main_plan = FnPlan {
        ret: None,
        cur_module: None,
        var_space: 0,
        name: "main".to_string(),
        witness_name: Some("main".to_string()),
        effect_raw: None,
        in_main: true,
        env_captures: None,
        // main is user code (its loop heads charge) but is never CALLED,
        // so no entry charge — the 1002-unit ledger counts the callee's.
        metered: !meter.user.is_empty(),
        charge_entry: false,
        self_index: None,
        param_owned: None,
    };
    let main_lambdas_from = work.lifted.borrow().len();
    crate::decline_site::reset_pending();
    let (main_fn, main_calls) = lower_fn(&[], main_plan, main_body, &init_lets, &ctx, &mut pool)
        .inspect_err(|_| crate::decline_site::set(Some(crate::decline_site::take_main_site())))?;
    let main_lambdas = main_lambdas_from..work.lifted.borrow().len();
    display_helper_calls.extend(display::build_display_helpers(&table, &types, &work, &mut pool)?);

    // Lift lambdas to extra functions (they may register further lambdas
    // or table entries — iterate to the fixed point).
    let mut lifted_fns: Vec<LoweredLifted> = Vec::new();
    // Per lifted lambda: the lambdas ITS body lifted, and — when the body did
    // not lower — why. A failed lambda ships as an `unreachable` stub and
    // refuses the program only if a reachable owner created it (#2588: the
    // router's closures inside `http` must not wall a program that only
    // calls `http.get`).
    let mut lambda_children: Vec<std::ops::Range<usize>> = Vec::new();
    let mut lambda_errs: Vec<Option<String>> = Vec::new();
    let mut lambda_sites: Vec<Option<DeclineSite>> = Vec::new();
    loop {
        let pending: Vec<LiftedLambda> = {
            let all = work.lifted.borrow();
            all[lifted_fns.len()..].to_vec()
        };
        if pending.is_empty() {
            break;
        }
        for ll in pending {
            // #2758: a lambda body is a frame like any other — its params
            // callee-owned (the closure convention), its captures views of
            // the env block the closure holds.
            let lambda_name = format!("<lambda#{}>", lifted_fns.len());
            let plan = FnPlan {
                ret: ll.ret,
                cur_module: ll.cur_module.clone(),
                var_space: ll.var_space,
                name: "<lambda>".to_string(),
                witness_name: Some(lambda_name.clone()),
                effect_raw: ll.effect_raw,
                in_main: false,
                env_captures: Some(ll.captures.clone()),
                // Closure hops always charge (TailCallee::Clo mirror) —
                // unless the whole meter elided (no regions anywhere).
                // #1627 synthetic initializers never hop-charge (native
                // charges nothing for reaching a top-let's value), but
                // their bodies stay metered like the main prologue whose
                // inline lowering they replace.
                metered: !meter.user.is_empty(),
                charge_entry: !meter.user.is_empty() && ll.charge_hop,
                self_index: None,
        param_owned: None,
            };
            let children_from = work.lifted.borrow().len();
            crate::decline_site::reset_pending();
            let (f, calls, err) = match lower_fn(&ll.params, plan, &ll.body, &[], &ctx, &mut pool) {
                Ok((f, calls)) => (f, calls, None),
                Err(EmitError::Unsupported(r)) => {
                    // The stub ships unrecorded: counted, never certified.
                    crate::witness::decline_unrecorded(&lambda_name, "lambda:unlowered");
                    let mut stub = Function::new([]);
                    stub.instructions().unreachable().end();
                    (stub, HashSet::new(), Some(r))
                }
                Err(e) => return Err(e),
            };
            lambda_children.push(children_from..work.lifted.borrow().len());
            lambda_sites.push(err.as_ref().map(|_| DeclineSite {
                function: ll.site_name.clone().unwrap_or_else(|| "<lambda>".into()),
                module: ll.cur_module.clone(),
                line: crate::decline_site::take_pending().map(|s| s.line),
                top_let: ll.site_name.is_some(),
            }));
            lambda_errs.push(err);
            display_helper_calls
                .extend(display::build_display_helpers(&table, &types, &work, &mut pool)?);
            // Uniform convention: env i32 leads every table signature.
            let mut ps: Vec<ValType> = vec![ValType::I32];
            ps.extend(ll.params.iter().map(|(_, t)| t.val_type()));
            lifted_fns.push((ps, ll.ret.map(SliceTy::val_type), f, calls));
        }
    }

    // Reachability: refuse the program iff a call chain from `main` lands
    // on a function — or a lambda — whose body did not lower (its stub would
    // trap). A lambda is reached through the function or lambda that
    // created it; its calls count only then.
    // The walk reports the reachable failure with the SMALLEST index
    // (program order), never whichever the walk happened to step on first
    // (the gauntlet's functional_port refused with three different reasons
    // across twelve runs before this pick was made deterministic).
    let reach = |fn_roots: Vec<usize>, lambda_roots: Vec<usize>| {
        let (mut fq, mut lq) = (fn_roots, lambda_roots);
        let (mut fns, mut lams): (HashSet<usize>, HashSet<usize>) = (HashSet::new(), HashSet::new());
        let (mut fn_err, mut lam_err): (Option<usize>, Option<usize>) = (None, None);
        while !fq.is_empty() || !lq.is_empty() {
            while let Some(k) = lq.pop() {
                if !lams.insert(k) {
                    continue;
                }
                if lambda_errs[k].is_some() {
                    lam_err = Some(lam_err.map_or(k, |p| p.min(k)));
                }
                fq.extend(lifted_fns[k].3.iter().copied());
                lq.extend(lambda_children[k].clone());
            }
            while let Some(i) = fq.pop() {
                if !fns.insert(i) {
                    continue;
                }
                match &lowered[i] {
                    Err(_) => fn_err = Some(fn_err.map_or(i, |p| p.min(i))),
                    Ok((_, calls)) => fq.extend(calls.iter().copied()),
                }
                if let Some(r) = fn_lambdas.get(i) {
                    lq.extend(r.clone());
                }
            }
        }
        let err = fn_err
            .and_then(|i| lowered[i].as_ref().err().cloned())
            .or_else(|| lam_err.and_then(|k| lambda_errs[k].clone()));
        let site = match fn_err {
            Some(i) => Some(fn_site(&program_fns[i], fail_lines.get(&i).copied())),
            None => lam_err.and_then(|k| lambda_sites[k].clone()),
        };
        (fns, err, site)
    };
    let mut roots: Vec<usize> = main_calls.iter().copied().collect();
    roots.extend(display_helper_calls.iter().copied());
    for e in work.entries.borrow().iter() {
        match e {
            TableEntry::Fn(i) | TableEntry::Adapter { target: i, .. } => roots.push(*i),
            TableEntry::Lambda(_) | TableEntry::Direct(_) => {}
        }
    }
    let (mut visited, err, site) = reach(roots, main_lambdas.clone().collect());
    if let Some(reason) = err {
        crate::decline_site::set(site);
        return unsup(&reason);
    }

    // #457: exported pub fns are DCE ROOTS — the host calls them without
    // main ever reaching them (render_frame, on_pointer_*, any host-called
    // pub fn). An entry-program fn exports when its WHOLE call closure
    // lowers; one that (transitively) hits an unlowered body is simply not
    // exported — main-reachable strictness above is untouched.
    // `@export(wasm, "sym")` (#2752) names the export `sym` instead of the
    // fn's own name, and makes the export an obligation: a declared export
    // that does not lower refuses the module in either form, never ships an
    // artifact silently missing the entry point its host calls.
    let mut export_fns: Vec<(String, u32)> = Vec::new();
    for (i, (f, qual, _space)) in program_fns.iter().enumerate() {
        let name = f.name.as_str();
        if qual.is_some()
            || name == "main"
            || name.starts_with("__")
            || f.is_test
            || f.generics.as_ref().is_some_and(|g| !g.is_empty())
            || !matches!(f.visibility, almide_ir::IrVisibility::Public)
        {
            continue;
        }
        let declared = f.export_attrs.iter().find(|a| a.target.as_str() == "wasm").map(|a| a.symbol.to_string());
        let export_name = declared.clone().unwrap_or_else(|| name.to_string());
        let (sub, err, site) = reach(vec![i], Vec::new());
        if let Some(reason) = &err
            && (library || declared.is_some())
        {
            crate::decline_site::set(site);
            return unsup(&format!("exported function `{name}` cannot be lowered: {reason}"));
        }
        if err.is_none() {
            // A second export of one name is an invalid module — a wall,
            // never a silent dedup (the name is the host's contract).
            // `memory`, `main` and the `__`-prefixed runtime exports are the
            // module's own.
            let reserved = matches!(export_name.as_str(), "memory" | "main") || export_name.starts_with("__");
            if reserved || export_fns.iter().any(|(e, _)| *e == export_name) {
                return unsup(&format!(
                    "duplicate wasm export name `{export_name}` (fn `{name}`) — two exports claim it, which is an invalid module"
                ));
            }
            visited.extend(sub);
            crate::host_exports::note_export(&export_name, table.infos[i].param_owned.clone());
            export_fns.push((export_name, table.infos[i].wasm_index));
        }
    }

    // Extra functions (ok-wrap adapters + lifted lambdas) resolve BEFORE
    // the type section is built — their call_indirect/type interning must
    // land inside it. Indices start right after main.
    let (extra_fns, entry_fn_indices) = resolve_extras(&table, &work, &lifted_fns);

    let oom_msg = pool.intern("Error: out of memory");
    let repeat_msg = pool.intern("Error: repeat result too large");
    let total = lowered.len();
    let bytes = assemble_module(AssembleIn {
        table: &table,
        work: &work,
        pool: &pool,
        oom_msg,
        repeat_msg,
        lowered: &lowered,
        reachable: &visited,
        main_fn: &main_fn,
        entry_fn_indices: &entry_fn_indices,
        extra_fns: &extra_fns,
        global_decls: &global_decls,
        export_fns: &export_fns,
        main_index,
        true_base,
        false_base,
    })?;
    // #2275: the extern stubs become declared imports of the finished bytes.
    let declared: Vec<imports::Declared> = table
        .infos
        .iter()
        .filter_map(|info| {
            let (module, name) = info.import.clone()?;
            Some(imports::Declared { index: info.wasm_index, module, name })
        })
        .collect();
    let bytes = imports::declare(&bytes, &declared).map_err(|e| EmitError::Unsupported(format!("extern-import:{e}")))?;
    record_decls(&program_fns, (main, main_index), &work, &entry_fn_indices, &declared, &bytes);
    let host_ops = work.host_ops.borrow().clone();
Ok(Pass { bytes, visited, total, ops: host_ops, bounded_fired: work.bounded_fired.get() })
}

/// #2759: the declaration table the name and capability witnesses read
/// (witness_decls.rs) — each program fn's, `main`'s and lifted lambda's
/// index with what its source declares.
fn record_decls(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    (main, main_index): (Option<&IrFunction>, u32),
    work: &FnWork,
    entry_fn_indices: &[u32],
    stubs: &[imports::Declared],
    bytes: &[u8],
) {
    use crate::witness::decls::{DeclFn, Declared, PassDecls};
    if !crate::witness::decls::collecting() {
        return;
    }
    let stubs = stubs.iter().map(|d| d.index).collect();
    // #3041: a fn synthesized from an effect fn's body keeps its origin's
    // declaration (`IrFunction::declares_effect`).
    let class = |f: &IrFunction| Some(if f.declares_effect() { Declared::Effect } else { Declared::Pure });
    let meter = work.meter_reads.borrow();
    let reads = |name: &str| meter.get(name).copied().unwrap_or(0);
    let mut fns: Vec<DeclFn> = program_fns
        .iter()
        .enumerate()
        .map(|(i, (f, qual, _))| DeclFn {
            index: F_FN_BASE + i as u32,
            name: qual.clone().unwrap_or_else(|| f.name.as_str().to_string()),
            // A self-host registry body implements a SURFACE whose
            // declaration its callers were checked against; its own `fn`
            // keyword declares nothing about the host (random.int is an
            // `effect fn` implemented by a plain `fn random_int`).
            declared: if qual.as_deref().is_some_and(|q| q.starts_with("__selfhost_")) { None } else { class(f) },
            meter_clock_reads: reads(&qual.clone().unwrap_or_else(|| f.name.as_str().to_string())),
        })
        .collect();
    fns.push(DeclFn {
        index: main_index,
        name: "main".into(),
        declared: main.map_or(Some(Declared::Pure), class),
        meter_clock_reads: reads("main"),
    });
    for (pos, e) in work.entries.borrow().iter().enumerate() {
        if let (TableEntry::Lambda(j), Some(&index)) = (e, entry_fn_indices.get(pos)) {
            let name = format!("<lambda#{j}>");
            let meter_clock_reads = reads(&name);
            fns.push(DeclFn { index, name, declared: None, meter_clock_reads });
        }
    }
    crate::witness::decls::record(PassDecls { fns, stubs, bytes: bytes.to_vec() });
}

/// The module a program fn belongs to: its qualified key minus `.<fn name>`
/// (`collect_program_fns` keys a module fn `"{module}.{fn}"`). A module name
/// can itself be dotted (`gramide.lex`, a dependency's submodule), and a fn
/// name can be too (`Cfg.get`), so neither the first nor the last dot splits
/// it (#2904: `gramide.lex`'s own calls resolved against `gramide`).
pub(crate) fn fn_module<'a>(qual: Option<&'a str>, f: &IrFunction) -> Option<&'a str> {
    qual?.strip_suffix(f.name.as_str())?.strip_suffix('.')
}

/// Where a program function's wall points (#2807): its qualified name, its
/// module, and the line of the expression that refused — the body's first
/// line when the refusing node carried no span (a signature-level refusal).
fn fn_site(entry: &(&IrFunction, Option<String>, u32), refused_at: Option<usize>) -> DeclineSite {
    let (f, qual, _) = entry;
    // A re-keyed package module (#2865) is shown under the key its source wrote.
    let module = qual
        .as_ref()
        .and_then(|q| q.rsplit_once('.').map(|(m, _)| package_keys::display_key(m).to_string()));
    let function = match &module {
        Some(m) => format!("{m}.{}", f.name.as_str()),
        None => qual.clone().unwrap_or_else(|| f.name.as_str().to_string()),
    };
    DeclineSite { function, module, line: refused_at.or(f.body.span.map(|s| s.line)), top_let: false }
}

/// The `@extern(wasm, module, name)` import a body-less fn declares (#2275):
/// `Ok(Some((module, name)))` when its signature has the scalar host ABI
/// (`Int`/sized ints → i64, `Float` → f64, `Bool` → i32, `String` → i32
/// block, `Unit` → no result); `Ok(None)` for a fn with a body; `Err` for a
/// native (`rs`/`rust`) extern — there is no wasm host for it, so an import
/// would be a hollow lie — and for a param or return outside the ABI.
fn extern_import(f: &IrFunction, params: &[SliceTy], ret: Option<SliceTy>) -> Result<Option<(String, String)>, String> {
    if f.extern_attrs.is_empty() {
        return Ok(None);
    }
    let Some(a) = f.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") else {
        return Err(format!("extern-native:{}", f.name));
    };
    if !matches!(f.body.kind, IrExprKind::Hole) {
        return Ok(None);
    }
    for (p, t) in f.params.iter().zip(params) {
        if !matches!(t, SliceTy::Scalar(_)) {
            return Err(format!("extern-ty:{}:{}", f.name, p.name));
        }
    }
    if !matches!(ret, None | Some(SliceTy::Scalar(_))) {
        return Err(format!("extern-ret:{}", f.name));
    }
    Ok(Some((a.module.as_str().to_string(), a.function.as_str().to_string())))
}
