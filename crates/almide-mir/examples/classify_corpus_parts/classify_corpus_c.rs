
/// The Pass-1 per-(inlined-)function lowering-and-witness-emission body of
/// [`classify_file`]'s main loop — verbatim move (`ctx.*` substituted for the
/// free variables it used to close over as a nested loop body). `t`/`s` are
/// accumulated into; `file_mirs`/`elided_call_fns` are appended/inserted into
/// by every arm (never read back here), so no state threads BETWEEN calls
/// beyond straightforward accumulation.
/// Extracted from `classify_lower_one_fn` (codopsy7 max-depth sweep): the per-`CallFn`-op
/// unlinkable-stdlib-call check + (c)-completeness backstop, verbatim (pure text move — was
/// nested inside `for mir in &mirs { for op in &mir.ops { if let CallFn { name, .. } = op {
/// .. } } }`, so the loop/if-let nesting alone pushed this classification past the depth
/// threshold). Same order, same bucket-(b)/(c) accounting, no behavior change.
fn classify_check_unlinkable_call(ctx: &FileCtx, t: &mut Tally, mir: &MirFunction, name: &str) {
    // Unlinkable stdlib call ⟺ a DOTTED name that is neither in the
    // self-host registry NOR a function defined in this file (a dotted
    // user PROTOCOL METHOD resolves to itself/a sibling, so it is NOT a
    // dangling stdlib call). This is exactly the class the render wall
    // rejects; a user method / cross-file call is out of scope here.
    let unlinkable = name.contains('.')
        && !ctx.auto_linkable.contains(name)
        && !ctx.file_fn_names.contains(name);
    if !unlinkable {
        return;
    }
    *t.would_wall_callees.entry(name.to_string()).or_insert(0) += 1;
    t.interp_walled += 1;
    // (c) completeness backstop: this site is genuinely unlinkable,
    // so the render wall MUST flag it. `unlinked_call_names` is the
    // wall's OWN predicate; if it does NOT contain the name, a site
    // escaped the wall → a real (c) breach. A single-fn probe is
    // sound here: the name is not file-defined, so adding sibling
    // functions could not make it resolve (only the registry could,
    // which `ctx.auto_linkable` already ruled out).
    let probe = MirProgram { functions: vec![mir.clone()], exports: vec![], mutable_global_count: 0 };
    if !almide_mir::render_wasm::unlinked_call_names(&probe).contains(name) {
        t.forbidden_unwalled.push(format!(
            "{}::{} -> {name} (escaped the render wall)",
            ctx.file.display(),
            mir.name
        ));
    }
}

fn classify_lower_one_fn(
    ctx: &FileCtx,
    func: &almide_ir::IrFunction,
    t: &mut Tally,
    s: &mut CertStreams,
    file_mirs: &mut Vec<(String, MirFunction)>,
    elided_call_fns: &mut HashSet<String>,
) {
    t.functions += 1;
    let lowered = catch_unwind(AssertUnwindSafe(|| {
        almide_mir::lower::lower_function_all_with_globals(
            func,
            ctx.globals,
            ctx.global_inits,
            ctx.record_layouts,
            ctx.variant_layouts,
        )
    }));
    match lowered {
        Ok(Ok(mirs)) => {
            // `mirs[0]` is the source function; `mirs[1..]` are lambda-lifted
            // auxiliaries (the closures machinery lifts `let f = (x) => …` bodies
            // into fresh functions). Every one is a real MIR function the proven
            // checker re-verifies, so backing / ownership / names witnesses are
            // emitted for ALL of them and the program assembler tables them by the
            // same position. With no lifting wired the vector is just `[main]` and
            // this is byte-identical to the prior single-function pass.
            t.in_profile += mirs.len();
            // The EFFECTIVE body the lowering actually saw: a let-bound heap-result
            // `if`/`match` is tail-duplicated PURELY in the IR before lowering, so the
            // caps `count_ir_calls` 1:1 gate and the interp-coverage count must read the
            // SAME rewritten tree (the duplicated continuation's calls / interps appear
            // once per arm in BOTH MIR and this counted IR — `mir == ir` by construction).
            // desugar-before-both: the SAME ANF-lift (call-arg heap-if → let) then
            // tail-duplication the lowering applies, so the duplicated calls are counted
            // 1:1 (mir == ir) and the caps gate stays exact.
            // desugar-before-both: read the SAME fully-desugared tree the lowering emits
            // its MIR from (the full guard → beta → tuple-unwrap → effect-unwrap →
            // heap-branches fixpoint), so a tail-duplicating rewrite duplicates a call in
            // BOTH the MIR and this counted IR — `mir == ir` by construction. A subset
            // (only guard + heap-branches) missed `desugar_tuple_unwrap_or`, so a
            // `let r = opt.unwrap_or((tuple)); f(r.0)` mir>ir-breached.
            let eff_body = almide_mir::lower::desugar_all(
                &func.body,
                func.name.as_str() == "main",
                ctx.variant_layouts,
                ctx.record_layouts,
                &func.params,
            );
            // INTERP COVERAGE (a): this function LOWERED, so its FULLY-LINKABLE
            // interps (Lit/String/Int/Bool parts) fold to a registered __str_concat /
            // int.to_string / bool.to_string chain (proven byte-match v0 by the
            // render_wasm detectors); a non-desugarable interp stays the sound Opaque
            // fallback, and a desugarable-but-UNLINKED one (Float/compound) walls at
            // render — both are (b) cleanly walled (no invalid wasm). The per-CallFn
            // loop below ADDS the unlinked-occurrence count to (b); this counts the
            // interp SITE once, as proven or walled, with no proven mis-bucket.
            let (proven, walled) = count_interp_sites(&eff_body, &ctx.auto_linkable, ctx.record_layouts);
            t.interp_lowered += proven;
            t.interp_walled += walled;
            // LINK COVERAGE: a LOWERED function emitting a dotted `Op::CallFn` whose
            // name the v1 linker cannot resolve (not in the self-host registry) would,
            // if rendered, emit a dangling `(call $name)` — invalid wasm. The render-
            // side `try_render_wasm_program` now WALLS the WHOLE program in that case
            // (a clean `LowerError::Unsupported`, never an `Ok` invalid module). So each
            // such site is a bucket-(b) cleanly-walled, NOT a (c) forbidden hole. We
            // MEASURE the distinct unlinkable callees (the visible self-host gap) and
            // fold each occurrence into (b). The wall's COMPLETENESS — that no such site
            // escapes to `Ok` — is what keeps (c) == 0 (asserted below).
            for mir in &mirs {
                for op in &mir.ops {
                    if let Op::CallFn { name, .. } = op {
                        classify_check_unlinkable_call(ctx, t, mir, name);
                    }
                }
            }
            for mir in &mirs {
                // The borrow-by-default soundness gate: every `+1` event must
                // be backed by a real runtime op (no synthetic param `+1`).
                if !plus_one_events_backed(mir) {
                    t.cert_backing_breaches
                        .push(format!("{}::{}", ctx.file.display(), mir.name));
                }
                // Ownership is one heap object per line; names are one line per
                // function. Both are LOCAL properties — no transitivity.
                let cert = ownership_certificate(mir);
                // Parallel name index (ownership.names): one `<file>::<fn>` line per
                // cert line, so a checker REJECT bisects straight to its function
                // (the anonymous 20k-line cert made a reject a needle hunt).
                for _ in cert.lines() {
                    s.ownership_names
                        .push_str(&format!("{}::{}\n", ctx.file.display(), mir.name));
                }
                s.ownership.push_str(&cert);
                s.names.push_str(&name_witness_string(mir));
                s.names.push('\n');
            }
            // CAPS SOUNDNESS: count the source's call nodes. A call ELIDED by
            // Opaque lowering (a list element, ctor payload, BinOp operand, …) is
            // absent from the MIR ops, so the transitive caps fold over CallFn /
            // FuncRef edges cannot see its effects — if it reached Stdout the
            // function would be falsely caps-verified. The IR call count covers the
            // WHOLE source body (including any lambda later lifted out), so the MIR
            // call count is summed across the main AND its lifted auxiliaries — a
            // lifted lambda carries its body's calls, and a `CallIndirect` (a
            // lowered closure invocation) is a genuine call counted here too. If the
            // cluster has MORE IR calls than MIR call-ops some call was elided
            // SOMEWHERE within it, so EVERY function of the cluster is conservatively
            // TAINTED below (we cannot tell which member hid it).
            let ir_calls = count_ir_calls(&eff_body, ctx.record_layouts, ctx.variant_layouts);
            let mir_calls = mirs
                .iter()
                .flat_map(|m| m.ops.iter())
                .filter(|o| {
                    matches!(
                        o,
                        Op::Call { .. } | Op::CallFn { .. } | Op::CallIndirect { .. }
                    )
                })
                // `$__mg_take` is a COMPILER-INJECTED slot accessor (a raw i32.load,
                // Stdout-free — the trusted-prim class), not a lowering of any IR
                // call node: a mutable-global heap assign injects one with no IR
                // counterpart, so counting it would false-breach `mir <= ir`.
                .filter(|o| !matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
                .count();
            if ir_calls > mir_calls {
                for mir in &mirs {
                    elided_call_fns.insert(mir.name.clone());
                }
            }
            // SOUNDNESS BACKSTOP for the elided-call effect markers: a marker
            // (`record_elided_calls`) may only surface a genuinely ELIDED
            // call, so the MIR call count can rise at most TO the IR's. If it
            // EXCEEDS, a marker double-counted a lowered call — which could
            // mask another elision and falsely de-taint. A wall breach.
            if mir_calls > ir_calls {
                t.call_count_breaches.push(format!(
                    "{}::{} (mir {mir_calls} > ir {ir_calls})",
                    ctx.file.display(),
                    func.name.as_str()
                ));
            }
            for mir in mirs {
                file_mirs.push((mir.name.clone(), mir));
            }
        }
        Ok(Err(almide_mir::lower::LowerError::Unsupported(reason))) => {
            // Categorize the wall: NATIVE-FFI (structural, excluded) iff this function is
            // in the transitive native-FFI closure; else REAL (a lowering gap to close).
            // A name absent from the node map (an inline_mutual_tail_recursion-synthesized
            // aux) defaults REAL (conservative — never over-excludes a real gap).
            let is_native = ctx.native_ffi_set.contains(func.name.as_str());
            if is_native {
                t.walled_native_ffi += 1;
            } else {
                t.walled_real += 1;
            }
            if std::env::var("WALL_NAMES").is_ok() {
                let tag = if is_native { "NATIVE-FFI" } else { "REAL" };
                eprintln!(
                    "WALLED {tag} {} :: {} :: {}",
                    ctx.file.display(),
                    func.name.as_str(),
                    reason
                );
            }
            *t.unsupported.entry(reason_key(&reason)).or_insert(0) += 1;
            // INTERP COVERAGE (b): every interp site inside a WALLED function is
            // cleanly walled too (its function emits no wasm) — never a miscompile.
            let (proven, walled) = count_interp_sites(&func.body, &ctx.auto_linkable, ctx.record_layouts);
            t.interp_walled += proven + walled;
        }
        Err(_) => {
            // THE wall breach: lowering must be total. Record file::func.
            t.lower_panics
                .push(format!("{}::{}", ctx.file.display(), func.name.as_str()));
        }
    }
}

/// The read-only shared inputs [`classify_fold_caps_one_fn`] threads through
/// the Pass-2 per-function capability-soundness fold of [`classify_file`] —
/// bundled to avoid a 9-parameter helper signature (the max-params trap: see
/// docs/roadmap/active/code-health-codopsy.md).
struct CapsFoldCtx<'a> {
    in_profile_map: &'a BTreeMap<String, MirFunction>,
    is_known_free: &'a dyn Fn(&str) -> bool,
    is_elided: &'a dyn Fn(&str) -> bool,
    cap_ids: &'a dyn Fn(&[Capability]) -> String,
}

/// The Pass-2 per-in-profile-function capability-soundness fold body of
/// [`classify_file`]'s second loop — verbatim move. `file_graph_clean` is an
/// output-only flag (every arm only ever clears it, never reads it back), so
/// it is safe as a plain `&mut bool` out-param.
fn classify_fold_caps_one_fn(
    ctx: &CapsFoldCtx,
    name: &str,
    mir: &MirFunction,
    t: &mut Tally,
    s: &mut CertStreams,
    file_graph_clean: &mut bool,
) {
    let mut visited = BTreeSet::new();
    match reachable_caps_or_tainted(name, ctx.in_profile_map, ctx.is_known_free, ctx.is_elided, &mut visited)
    {
        // Unanalyzable (an unknown/cross-file or elided callee hides effects).
        None => { t.caps_unverified += 1; *file_graph_clean = false; }
        // Fully-known reachable set. Caps-VERIFIED iff it is within the
        // DECLARED bound (`reachable ⊆ declared`): then emit the
        // `<declared>|<reachable>` witness for the proven `check_caps_cert` to
        // re-verify. A function reaching a capability it did NOT declare (e.g.
        // a non-`effect fn` that prints — `is_effect` does not capture every
        // Stdout reach) is conservatively caps-UNVERIFIED — it has an
        // undeclared effect, honestly not claimed safe (emitting it would
        // (correctly) fault the proven subset checker and fail the gate).
        Some(reachable) => {
            let declared: std::collections::BTreeSet<u32> =
                mir.declared_caps.iter().map(|c| c.id()).collect();
            if reachable.iter().all(|c| declared.contains(&c.id())) {
                s.caps.push_str(&format!(
                    "{}|{}\n",
                    (ctx.cap_ids)(&mir.declared_caps),
                    (ctx.cap_ids)(&reachable)
                ));
                t.caps_verified += 1;
            } else {
                t.caps_unverified += 1;
                *file_graph_clean = false;
            }
        }
    }
}

/// One corpus file's classification (the body of main's per-file loop —
/// decomposed #781, cog 199; early `continue`s became early `return`s).
fn classify_file(
    file: &std::path::Path,
    t: &mut Tally,
    s: &mut CertStreams,
    auto_linkable: &std::collections::HashSet<String>,
) {
    t.files += 1;
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            t.frontend_rejected += 1;
            return;
        }
    };
    let ir = match source_to_ir(file, &source) {
        FrontendOutcome::Ir(ir) => ir,
        FrontendOutcome::Rejected => {
            t.frontend_rejected += 1;
            return;
        }
        FrontendOutcome::Panicked => {
            t.frontend_panicked += 1;
            return;
        }
    };

    // The NATIVE-FFI closure over the linked IR (transitive over `@extern(rust/rs)` + the
    // enumerated no-wasm stdlib effects). A WALLED function in this set is a STRUCTURAL wall
    // (no wasm host equivalent), tagged NATIVE-FFI and excluded from the wall=0 metric; every
    // other wall is a REAL lowering gap. Computed once per file before the lowering loop.
    let native_ffi_set = compute_native_ffi_set(&ir);

    // Variant constructors of this program are PURE data builders (no host
    // effect), so a `CallFn` to one is Stdout-free — collected for the
    // capability soundness fold below.
    let ctors: HashSet<String> = ir
        .type_decls
        .iter()
        .flat_map(|td| match &td.kind {
            IrTypeDeclKind::Variant { cases, .. } => {
                cases.iter().map(|c| c.name.as_str().to_string()).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect();

    // Pass 1: lower every function; emit the LOCAL witnesses (ownership, names)
    // and collect the in-profile MIRs (the capability fold needs the whole
    // file's in-profile set before it can judge any one function's callees).
    let mut file_mirs: Vec<(String, MirFunction)> = Vec::new();
    // In-profile functions whose source had a call ELIDED by Opaque lowering —
    // their capability witness is incompletely captured, so the caps fold below
    // conservatively taints them (and their callers).
    let mut elided_call_fns: HashSet<String> = HashSet::new();
    // The module's top-level `let` globals (VarId -> declared Ty): a function that
    // references one resolves to no function-local binding, so the lowering needs
    // this DECLARED set to admit the reference (`value_or_global`) instead of
    // walling it. Union of program- and module-level top_lets.
    let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
        std::collections::HashMap::new();
    // The globals' INITIALIZERS too, so the gate VERIFIES the same heap-global materialization
    // render_program executes (a heap global lowers to its real const value, not a wall).
    let mut global_inits: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
        std::collections::HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    // MAIN-REGION precedence (this loop lowers the MAIN program's fns only): the raw
    // module union above stays as the FALLBACK (a shared-allocator id matches it — the
    // init_order shapes), the cross-module NAME bridge OVERRIDES a colliding module-raw
    // key (the byvalue shapes), and main's own top-lets (re-inserted last) win where the
    // name bridge would misfire — composition order: module union → bridge → main.
    almide_mir::lower::bridge_cross_module_toplets(&ir, &mut globals, &mut global_inits, &mut std::collections::HashMap::new());
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    // MUTABLE module-level `var`s route through their storage slots — publish the
    // VarId → (slot, Ty) map exactly as the render pipeline does (declaration order
    // = VarId order), so the gate classifies with the same slot lowering the real
    // emit performs (and the same walls for shapes beyond the slot subset).
    let mut mutable_tls: Vec<_> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .collect();
    mutable_tls.sort_by_key(|tl| tl.var.0);
    almide_mir::lower::set_mutable_global_vars(
        mutable_tls
            .iter()
            .enumerate()
            .map(|(i, tl)| (tl.var.0, (i as u32, tl.ty.clone())))
            .collect(),
    );
    // The functions DEFINED in this file (their names). A PROTOCOL METHOD is a
    // user-defined function whose name is dotted (`Type.method`, e.g. `MathExpr.eval`)
    // — it resolves to ITSELF / a sibling method, NOT a stdlib call. The unlinkable-
    // stdlib detector must exclude these (a dotted name is unlinkable only if it is also
    // NOT a function defined here), else a self-recursive method call falsely flags.
    let file_fn_names: HashSet<String> =
        ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    // The record-layout registry (type name → fields) for the VALUE MODEL, so the
    // corpus-wall exercises (and the proven checker re-verifies) record/`r.x`
    // materialization over the whole v0 corpus, not just the structurally-typed forms.
    let mut record_layouts = almide_mir::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
    }
    // The variant-layout registry (custom ADTs) — the value-model sibling of
    // `record_layouts`, so the corpus-wall exercises variant construct / `match` too.
    let mut variant_layouts = almide_mir::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = almide_mir::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
        variant_layouts.ctor_field_defaults.extend(m_vl.ctor_field_defaults);
    }
    // PROGRAM pre-pass: inline mutual-recursive tail siblings → direct self-recursion (exposed to
    // the append-accumulator TCO). Guarded: only where it makes a walled fn lower (no regression).
    let inlined_fns =
        almide_mir::lower::inline_mutual_tail_recursion(&ir.functions, &globals, &record_layouts);
    let ctx = FileCtx {
        file,
        globals: &globals,
        global_inits: &global_inits,
        record_layouts: &record_layouts,
        variant_layouts: &variant_layouts,
        auto_linkable,
        file_fn_names: &file_fn_names,
        native_ffi_set: &native_ffi_set,
    };
    for func in &inlined_fns {
        classify_lower_one_fn(&ctx, func, t, s, &mut file_mirs, &mut elided_call_fns);
    }

    // Pass 2 (capability SOUNDNESS): a function's empty capability witness is a
    // sound claim of Stdout-freedom ONLY if it reaches no Stdout TRANSITIVELY —
    // the direct witness alone misses what a callee reaches. Emit the witness
    // only for functions provably Stdout-free across `Op::CallFn` edges; the
    // rest are NOT claimed caps-safe (honest scope), never falsely accepted.
    let in_profile_map: BTreeMap<String, MirFunction> = file_mirs.iter().cloned().collect();
    // The conservative free-callee policy: a callee not in the in-profile map
    // is Stdout-free only if it is a pure stdlib `Module` call (a dotted name,
    // purity-gated at lowering), a variant constructor, or a known Stdout-free
    // builtin. Everything else (walled / cross-file user fns) is tainted.
    let is_known_free = |n: &str| {
        n.contains('.') || ctors.contains(n) || KNOWN_STDOUT_FREE_BUILTINS.contains(&n)
    };
    let is_elided = |n: &str| elided_call_fns.contains(n);
    let cap_ids =
        |c: &[Capability]| c.iter().map(|x| x.id().to_string()).collect::<Vec<_>>().join(" ");
    // Track whether the WHOLE file is analyzable + within-bound: only then is the
    // call-graph witness meaningful (any unanalyzable/over-bound function would route to
    // the UNIVERSE sentinel and reject — but unanalyzable is honest scope, not a failure).
    let mut file_graph_clean = !file_mirs.is_empty();
    let caps_ctx = CapsFoldCtx {
        in_profile_map: &in_profile_map,
        is_known_free: &is_known_free,
        is_elided: &is_elided,
        cap_ids: &cap_ids,
    };
    for (name, mir) in &file_mirs {
        classify_fold_caps_one_fn(&caps_ctx, name, mir, t, s, &mut file_graph_clean);
    }
    // The file is fully analyzable + within bound: emit its call-graph witness as ONE line
    // for the proven `check_prog_cert` (caps-transitive), which re-derives the transitive
    // reach itself. (UNIVERSE is unreferenced here — every callee is in-file or known-free.)
    if file_graph_clean {
        s.caps_graph
            .push_str(&program_cap_graph_witness(&in_profile_map, &is_known_free, &is_elided));
        s.caps_graph.push('\n');
    }
}

/// The five certificate output streams main accumulates across files.
#[derive(Default)]
struct CertStreams {
    ownership: String,
    ownership_names: String,
    names: String,
    caps: String,
    caps_graph: String,
}

fn main() {
    // Parse `--out DIR` (where the three witness `.cert` files are written); the
    // remaining args are corpus paths (files or dirs).
    let mut out_dir: Option<PathBuf> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--out" {
            out_dir = it.next().map(PathBuf::from);
        } else {
            paths.push(a);
        }
    }
    if paths.is_empty() || out_dir.is_none() {
        eprintln!("usage: classify_corpus --out DIR <file.almd | dir> ...");
        std::process::exit(2);
    }
    let out_dir = out_dir.expect("checked non-None by the usage-error exit(2) branch above");

    // The sweep catches panics deliberately; silence the default hook so a
    // walled-off panic does not spray a backtrace over the honest report.
    std::panic::set_hook(Box::new(|_| {}));

    let mut files = Vec::new();
    for a in &paths {
        collect_almd(Path::new(a), &mut files);
    }

    let mut t = Tally::default();
    // One witness stream per proven property. ownership = one heap object per
    // line; names/caps = one `<superset>|<subset>` line per in-profile function.
    let mut streams = CertStreams::default();
                // One line per FULLY-ANALYZABLE+within-bound file: the call-graph witness for the proven
    // `check_prog_cert` (caps-transitive), which COMPUTES the transitive reach itself — the
    // fold moves out of this untrusted classifier into the proof. Partially-analyzable files
    // stay on the per-function `caps.cert` (honest scope), not emitted here.
    
    // The render-resolvable name oracle for the interp-coverage (c) detector: a DOTTED
    // `CallFn` name (a stdlib `module.func`) renders to `(call $name)` and resolves ONLY
    // if the v1 linker auto-includes it (it is in the self-host registry). A dotted name
    // NOT here would dangle (invalid wasm). User functions can't hold a dot, so dotted +
    // not-auto-linkable is precisely the unlinkable-stdlib-call class (no cross-file user
    // false positives, which a per-file harness could not otherwise rule out).
    let auto_linkable = auto_linkable_call_names();

    for file in &files {
        classify_file(file, &mut t, &mut streams, &auto_linkable);
    }

    // Restore a sane hook before we print (catch window is over).
    let _ = std::panic::take_hook();

    // Write the three witness streams for the proven checker. ownership may be
    // empty if no in-profile function emits a heap object (trivially accepted);
    // names/caps have one line per in-profile function.
    let write = |name: &str, body: &str| {
        let p = out_dir.join(name);
        if let Err(e) = std::fs::write(&p, body) {
            eprintln!("cannot write {}: {e}", p.display());
            std::process::exit(2);
        }
    };
    write("ownership.cert", &streams.ownership);
    write("ownership.names", &streams.ownership_names);
    write("names.cert", &streams.names);
    write("caps.cert", &streams.caps);
    write("caps_graph.cert", &streams.caps_graph);

    // STDERR: the honest coverage report.
    eprintln!("== v0-corpus MIR-lowering wall report ==");
    eprintln!("files scanned          : {}", t.files);
    eprintln!("  frontend-rejected    : {}", t.frontend_rejected);
    eprintln!("  frontend-panicked    : {}", t.frontend_panicked);
    eprintln!("functions reaching MIR : {}", t.functions);
    eprintln!(
        "  in-profile (lowers)  : {}  <- proven-checker re-verifies these",
        t.in_profile
    );
    let walled: usize = t.unsupported.values().sum();
    eprintln!("  walled (Unsupported) : {walled}");
    eprintln!(
        "    walled real (lowering)   : {}  <- the wall=0 metric (pure/WASI-able gaps to close)",
        t.walled_real
    );
    eprintln!(
        "    walled native-FFI (excl) : {}  <- structural (@extern rust/rs + no-wasm stdlib effect); excluded",
        t.walled_native_ffi
    );
    for (reason, n) in &t.unsupported {
        eprintln!("      {n:>4}  {reason}");
    }
    eprintln!("  lower panics (BUG)   : {}", t.lower_panics.len());
    for p in &t.lower_panics {
        eprintln!("      PANIC {p}");
    }
    eprintln!(
        "  unbacked +1 (BUG)    : {}  <- borrow-by-default backing gate",
        t.cert_backing_breaches.len()
    );
    eprintln!(
        "  mir>ir calls (BUG)   : {}  <- elided-call marker double-count gate",
        t.call_count_breaches.len()
    );
    eprintln!(
        "  caps-verified        : {}  <- provably reach no Stdout (transitive); witness emitted",
        t.caps_verified
    );
    eprintln!(
        "  caps-unverified      : {}  <- call an unanalyzable callee; not claimed caps-safe (honest scope)",
        t.caps_unverified
    );
    // INTERP / LINK COVERAGE visibility metric (a/b/c) — measurement only, no soundness
    // DECISION (mir<=ir is unchanged; this neither weakens nor strengthens any detector).
    let would_wall_sites: usize = t.would_wall_callees.values().sum();
    eprintln!("-- interp / call-link coverage (visibility metric) --");
    eprintln!(
        "  (a) lowered (proven) : {}  <- lowerable interp in a lowered fn; folds to a registered chain (byte-match v0)",
        t.interp_lowered
    );
    eprintln!(
        "  (b) walled (no out)  : {}  <- non-subset interp stays Opaque, interp in a walled fn, OR an unlinkable stdlib call the render wall rejects; acceptable, never invalid wasm",
        t.interp_walled
    );
    eprintln!(
        "  (c) FORBIDDEN        : {}  <- a site that renders to dangling `(call $…)` AND escapes the render wall (invalid-wasm-as-Ok); MUST be 0",
        t.forbidden_unwalled.len()
    );
    eprintln!(
        "      of (b): {} unlinkable dotted stdlib call-site(s) across {} distinct callee(s) — the visible self-host-registry gap (render wall rejects each using program cleanly):",
        would_wall_sites,
        t.would_wall_callees.len()
    );
    for (callee, n) in t.would_wall_callees.iter() {
        eprintln!("        {n:>4}  {callee}");
    }
    for p in &t.cert_backing_breaches {
        eprintln!("      UNBACKED {p}");
    }
    for p in &t.call_count_breaches {
        eprintln!("      MIR>IR {p}");
    }
    for p in &t.forbidden_unwalled {
        eprintln!("      FORBIDDEN {p}");
    }

    let total_breaches = t.lower_panics.len()
        + t.cert_backing_breaches.len()
        + t.call_count_breaches.len()
        + t.forbidden_unwalled.len();
    if total_breaches == 0 {
        eprintln!(
            "WALL OK: lower_function was TOTAL over {} corpus functions \
             (Ok or explicit Unsupported, zero panics, zero undetected refusals \
             — a totality + certificate claim, NOT output correctness: that is \
             output-parity's, on its baseline set), and every in-profile \
             certificate `+1` is backed by a real runtime op \
             (no synthetic param ownership — the borrow-by-default gate).",
            t.functions
        );
    } else {
        if !t.lower_panics.is_empty() {
            eprintln!(
                "WALL BREACH: lower_function panicked on {} function(s) — must return \
                 Ok or Unsupported, never panic.",
                t.lower_panics.len()
            );
        }
        if !t.cert_backing_breaches.is_empty() {
            eprintln!(
                "WALL BREACH: {} function(s) emitted an UNBACKED certificate `+1` — \
                 a param or op injected ownership no runtime op performs \
                 (the gate-blind use-after-free class).",
                t.cert_backing_breaches.len()
            );
        }
        if !t.call_count_breaches.is_empty() {
            eprintln!(
                "WALL BREACH: {} function(s) have MORE MIR call-ops than IR call-nodes — \
                 an elided-call effect marker double-counted a lowered call, which could \
                 mask a real elision and falsely de-taint a Stdout-reaching function.",
                t.call_count_breaches.len()
            );
        }
        if !t.forbidden_unwalled.is_empty() {
            eprintln!(
                "WALL BREACH: {} unlinkable stdlib call-site(s) ESCAPED the render wall — a \
                 dangling `(call $…)` would render as a valid-looking `Ok` module (invalid wasm \
                 passing as Ok). `try_render_wasm_program` must reject EVERY unlinkable CallFn; \
                 a gap here is a wall-completeness bug.",
                t.forbidden_unwalled.len()
            );
        }
        std::process::exit(1);
    }
}
