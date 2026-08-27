//! The emission entry: the two-pass reachability-pruned pipeline —
//! split from lib.rs for the file budget.

use std::collections::{HashMap, HashSet};

use almide_ir::IrProgram;

use crate::types_table::TypeTable;
use crate::*;

/// Emit a core wasm module for `ir`, or say precisely why not yet.
/// Two passes: the first loads the WHOLE linked registry graph (so
/// resolution and the refusal BFS see everything) and reports which
/// program fns main actually reaches; when dead fns exist, a second
/// pass re-emits with ONLY the reachable set in the table — real
/// dead-code elimination (the type/decl/stub bookkeeping of a dead
/// registry graph once quadrupled a small module).
pub fn emit_program(ir: &IrProgram) -> Result<Vec<u8>, EmitError> {
    let (bytes, visited, total) = emit_program_pass(ir, None)?;
    if visited.len() >= total {
        return Ok(bytes);
    }
    let (bytes, _, _) = emit_program_pass(ir, Some(&visited))?;
    Ok(bytes)
}

#[allow(clippy::type_complexity)]
fn emit_program_pass(
    ir: &IrProgram,
    keep: Option<&HashSet<usize>>,
) -> Result<(Vec<u8>, HashSet<usize>, usize), EmitError> {
    let Some(main) = ir.functions.iter().find(|f| f.name.as_str() == "main") else {
        return unsup("no main function");
    };
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
        let key = qual.clone().unwrap_or_else(|| f.name.as_str().to_string());
        // impl_index carries ONLY registry implementation symbols — a
        // global simple-name index over ALL module fns collides across
        // modules (two self-host modules both defining __len_loop made
        // cross_module fixtures call the WRONG module's helper).
        if qual.is_some() && registry_impl_names().contains(f.name.as_str()) {
            table.impl_index.insert(f.name.as_str().to_string(), i);
        }
        table.by_name.insert(key, i);
        table.infos.push(FnInfo { wasm_index: F_FN_BASE + i as u32, params, ret, refuse });
    }
    let main_index = F_FN_BASE + program_fns.len() as u32;

    let mut pool = Pool::new();
    // Interned eagerly so $append_bool can carry their fixed addresses.
    let true_base = pool.intern("true");
    let false_base = pool.intern("false");

    let (global_map, global_decls, init_lets) = build_globals(ir, &types);

    // Function-VALUE work shared by every lowering below (funcref table,
    // call_indirect types, lifted lambdas).
    let work = FnWork::default();
    // Calls made from display-helper bodies (BFS roots).
    let mut display_helper_calls: std::collections::HashSet<usize> = HashSet::new();
    work.itype_base.set(T_FN_BASE + table.infos.len() as u32);
    work.helper_base.set(F_FN_BASE + table.infos.len() as u32 + 1);

    // Lower every callable function; a body that doesn't lower yet is
    // recorded (not fatal) — fatal only if `main` can reach it.
    let mut lowered: Vec<Result<(Function, HashSet<usize>), String>> = Vec::new();
    for (i, (f, qual, space)) in program_fns.iter().enumerate() {
        if let Some(r) = &table.infos[i].refuse {
            lowered.push(Err(r.clone()));
            continue;
        }
        let params: Vec<(VarId, SliceTy)> =
            f.params.iter().zip(&table.infos[i].params).map(|(p, &t)| (p.var, t)).collect();
        let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map };
        let cur_module = qual.as_ref().and_then(|q| q.split('.').next());
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
        };
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
                }
            }
            Err(EmitError::Unsupported(r)) => lowered.push(Err(r)),
        }
    }

    // `main`: top-lets as the eager prelude, then the body. Failure here is
    // fatal — main is always reachable.
    let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map };
    let main_plan = FnPlan {
        ret: None,
        cur_module: None,
        var_space: 0,
        effect_raw: None,
        in_main: true,
        env_captures: None,
        // main is user code (its loop heads charge) but is never CALLED,
        // so no entry charge — the 1002-unit ledger counts the callee's.
        metered: !meter.user.is_empty(),
        charge_entry: false,
    };
    let (main_fn, main_calls) =
        lower_fn(&[], main_plan, &main.body, &init_lets, &ctx, &mut pool)?;
    display_helper_calls.extend(display::build_display_helpers(&table, &types, &work, &mut pool)?);

    // Lift lambdas to extra functions (they may register further lambdas
    // or table entries — iterate to the fixed point).
    let mut lifted_fns: Vec<LoweredLifted> = Vec::new();
    loop {
        let pending: Vec<LiftedLambda> = {
            let all = work.lifted.borrow();
            all[lifted_fns.len()..].to_vec()
        };
        if pending.is_empty() {
            break;
        }
        for ll in pending {
            let plan = FnPlan {
                ret: ll.ret,
                cur_module: None,
                var_space: ll.var_space,
                effect_raw: ll.effect_raw,
                in_main: false,
                env_captures: Some(ll.captures.clone()),
                // Closure hops always charge (TailCallee::Clo mirror) —
                // unless the whole meter elided (no regions anywhere).
                metered: !meter.user.is_empty(),
                charge_entry: !meter.user.is_empty(),
            };
            let (f, calls) = lower_fn(&ll.params, plan, &ll.body, &[], &ctx, &mut pool)?;
            display_helper_calls
                .extend(display::build_display_helpers(&table, &types, &work, &mut pool)?);
            // Uniform convention: env i32 leads every table signature.
            let mut ps: Vec<ValType> = vec![ValType::I32];
            ps.extend(ll.params.iter().map(|(_, t)| t.val_type()));
            lifted_fns.push((ps, ll.ret.map(SliceTy::val_type), f, calls));
        }
    }

    // Reachability: refuse the program iff a call chain from `main` lands
    // on a function whose body did not lower (its stub would trap).
    let mut queue: Vec<usize> = main_calls.iter().copied().collect();
    queue.extend(display_helper_calls.iter().copied());
    for (_, _, _, calls) in &lifted_fns {
        queue.extend(calls.iter().copied());
    }
    for e in work.entries.borrow().iter() {
        match e {
            TableEntry::Fn(i) | TableEntry::Adapter { target: i, .. } => queue.push(*i),
            TableEntry::Lambda(_) => {}
        }
    }
    let mut visited: HashSet<usize> = HashSet::new();
    // The per-fn call sets are HashSets, so traversal order is
    // process-seeded — complete the walk and report the reachable
    // failure with the SMALLEST function index (program order), never
    // whichever the walk happened to step on first (the gauntlet's
    // functional_port refused with three different reasons across
    // twelve runs before this pick was made deterministic).
    let mut first_err: Option<usize> = None;
    while let Some(i) = queue.pop() {
        if !visited.insert(i) {
            continue;
        }
        match &lowered[i] {
            Err(_) => first_err = Some(first_err.map_or(i, |p| p.min(i))),
            Ok((_, calls)) => queue.extend(calls.iter().copied()),
        }
    }
    if let Some(i) = first_err
        && let Err(reason) = &lowered[i]
    {
        return unsup(reason);
    }

    // #457: exported pub fns are DCE ROOTS — the host calls them without
    // main ever reaching them (render_frame, on_pointer_*, any host-called
    // pub fn). An entry-program fn exports when its WHOLE call closure
    // lowers; one that (transitively) hits an unlowered body is simply not
    // exported — main-reachable strictness above is untouched.
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
        let mut sub: HashSet<usize> = HashSet::new();
        let mut q = vec![i];
        let mut clean = true;
        while let Some(j) = q.pop() {
            if !sub.insert(j) {
                continue;
            }
            match &lowered[j] {
                Err(_) => {
                    clean = false;
                    break;
                }
                Ok((_, calls)) => q.extend(calls.iter().copied()),
            }
        }
        if clean {
            visited.extend(sub);
            export_fns.push((name.to_string(), table.infos[i].wasm_index));
        }
    }

    // Extra functions (ok-wrap adapters + lifted lambdas) resolve BEFORE
    // the type section is built — their call_indirect/type interning must
    // land inside it. Indices start right after main.
    let (extra_fns, entry_fn_indices) = resolve_extras(&table, &work, &lifted_fns);

    let oom_msg = pool.intern("Error: out of memory");
    let total = lowered.len();
    let bytes = assemble_module(AssembleIn {
        table: &table,
        work: &work,
        pool: &pool,
        oom_msg,
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
    Ok((bytes, visited, total))
}
