// The NATIVE rung pipeline — the `try_render_rust_source` entry and its
// gates: the per-fn signature-subset PRECISION WALL, the T1-3 Result-window
// rewrite, the SIG-KIND table, and the `ALMIDE_DUMP_MIR` debug dump.
// include!-spliced into `pipeline.rs` next to the wasm-side pipeline_c.rs,
// so it shares that module's imports (the pipeline_b/pipeline_c precedent).

/// NATIVE leg of the trust spine (#764, rung 1): lower `.almd` source through the
/// SAME Perceus MIR the wasm leg uses and render it to native Rust — `Dup` as
/// `.clone()`, `Drop` erased to Rust's scope-end drop, the runtime boundary mapped
/// to a closed native shim floor. `verify_ownership` certifies the Perceus balance
/// on the same ops before the erasure. WALLS (`Err`) on anything outside the
/// rung-1 subset — the CLI falls back to v0, so a rendered program is never wrong.
/// Debug probe: dump the lowered MIR ops of every non-test fn (walls listed
/// per fn). Used by examples/probe_native.rs during rung development.
pub fn debug_dump_mir(source: &str) -> Result<String, LowerError> {
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = source_to_ir_with(source, &[])?;
    let globals = std::collections::HashMap::new();
    let global_inits = std::collections::HashMap::new();
    // The REAL pipeline's layout registries — without them a record literal
    // lowers as an Opaque skeleton and a field read walls (the very first
    // rung-5 records probe misread that as a lowering gap).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    let mut out = String::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        match crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, &record_layouts, &variant_layouts) {
            Ok(all) => {
                for f in all {
                    out.push_str(&format!("== fn {} ==\n", f.name));
                    for op in &f.ops {
                        out.push_str(&format!("  {op:?}\n"));
                    }
                }
            }
            Err(e) => out.push_str(&format!("== fn {} LOWER-WALL: {e:?}\n", func.name)),
        }
    }
    Ok(out)
}

/// The per-fn PRECISION WALL of [`try_render_rust_source`] (rung 2): the
/// native renderer types a heap `Repr::Ptr` param/result by DECLARED type,
/// so any signature outside the rung subset walls here, where the
/// Almide-level `Ty` is still visible (`MirParam` carries only reprs).
fn native_sig_subset_check(
    func: &almide_ir::IrFunction,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Result<(), LowerError> {
    // PRECISION WALL (rung 2): the native renderer types a heap `Repr::Ptr`
    // param/result as a STRING — only sound when the DECLARED type says so.
    // Any signature outside {Int, Bool, String, Unit-ret} walls here, where
    // the Almide-level `Ty` is still visible (`MirParam` carries only reprs).
    use almide_lang::types::Ty;
    use almide_lang::types::constructor::TypeConstructorId;
    let is_scalar_list = |t: &Ty| {
        matches!(t, Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool))
    };
    // Rung-5 records slab: an ALL-SCALAR record is layout-identical to a
    // scalar list (the DynList block), so its params/returns ride the same
    // `&[i64]`/`Vec<i64>` convention on native.
    let is_scalar_record = |t: &Ty| -> bool {
        let Ty::Named(n, args) = t else { return false };
        if !args.is_empty() { return false; }
        record_layouts
            .get(n.as_str())
            .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
    };
    // A FLAT variant (every ctor scalar-only) is likewise one slot block
    // (tag@0, payload@1+) — same `&[i64]` convention (rung-5 variants slab).
    let is_flat_variant = |t: &Ty| -> bool {
        let Ty::Named(n, args) = t else { return false };
        if !args.is_empty() { return false; }
        variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
            layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
        })
    };
    // Rung-5 closures slab: a SCALAR closure type (`(Int) -> Int` — every param
    // and the return scalar) travels as its env block (`Vec<i64>`: [fnidx,
    // drop-header, captures…]); invocation dispatches through the generated
    // `__almd_ci_*` tables. Heap-param/-return closures stay wasm-only.
    let is_scalar_fn = |t: &Ty| {
        matches!(t, Ty::Fn { is_effect: _, params, ret }
            if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                && matches!(**ret, Ty::Int | Ty::Bool))
    };
    // T1-3: Result[scalar, String] returns ride a dedicated native
    // carrier (NTy::Res — Rust Result<i64, String>). String-Ok payloads
    // stay walled (their tag/payload windows are ambiguous with Err's).
    let is_native_result = |t: &Ty| {
        matches!(t, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2
                && matches!(a[0], Ty::Int | Ty::Bool)
                && matches!(a[1], Ty::String))
    };
    // #4006 List[String] slab: an owned `Vec<AlmideString>` / a borrowed
    // `&[AlmideString]` (the slot block's handles, rendered by the address
    // algebra in render_native_addr.rs).
    let is_str_list = |t: &Ty| {
        matches!(t, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String))
    };
    // #4006 A5: a record with heap fields that has a generated struct.
    let is_struct_record = |t: &Ty| {
        matches!(t, Ty::Named(n, args) if args.is_empty() && crate::render_native_rec::rec_index(n.as_str()).is_some())
    };
    let sig_ok = |t: &Ty| {
        matches!(t, Ty::Int | Ty::Bool | Ty::Float | Ty::String)
            || is_scalar_list(t)
            || is_str_list(t)
            || is_struct_record(t)
            || is_scalar_record(t)
            || is_flat_variant(t)
            || is_scalar_fn(t)
    };
    let sig_ok_ret = |t: &Ty| sig_ok(t) || is_native_result(t);
    for p in &func.params {
        if !sig_ok(&p.ty) {
            return Err(LowerError::Unsupported(format!(
                "native: fn `{}` param `{:?}` type — outside the native rung subset",
                func.name, p.ty
            )));
        }
    }
    if !matches!(func.ret_ty, Ty::Unit) && !sig_ok_ret(&func.ret_ty) {
        return Err(LowerError::Unsupported(format!(
            "native: fn `{}` return type {:?} — outside the native rung subset",
            func.name, func.ret_ty
        )));
    }
    Ok(())
}


/// #3550: a self call left in tail position after lowering — the MIR loop
/// conversion (`try_tco_rewrite`) declined the shape — with a heap argument.
/// The argument is released AFTER the call (`CallFn`, then `Drop`), so the
/// call is not a jump and the function recurses frame by frame, overflowing
/// at a depth the wasm leg runs in constant stack. The standard codegen
/// loop-converts it (`TailCallOptPass`), so the program walls onto it.
/// A self call whose result is used further (non-tail recursion) is not
/// refused: no leg turns that into a loop.
fn recurses_on_a_heap_arg(f: &crate::MirFunction) -> bool {
    use crate::{CallArg, Op};
    f.ops.iter().enumerate().any(|(i, op)| {
        let Op::CallFn { name, args, dst: Some(d), .. } = op else { return false };
        if *name != f.name || !args.iter().any(|a| matches!(a, CallArg::Handle(_))) {
            return false;
        }
        let rest = f.ops[i + 1..].iter().find(|o| !matches!(o, Op::Consume { .. } | Op::Drop { .. }));
        match rest {
            None => f.ret == Some(*d),
            Some(Op::Else { val } | Op::EndIf { val }) => *val == Some(*d),
            Some(_) => false,
        }
    })
}

/// T1-3: rewrite the stereotyped Result block windows onto the native
/// carrier prims (NTy::Res). Native-leg only — the wasm renderer never sees
/// these ops. Runs BEFORE verification/render (both are downstream).
fn rewrite_native_result_windows(ir: &almide_ir::IrProgram, functions: &mut [crate::MirFunction]) {
    rewrite_native_result_windows_over(ir.functions.iter(), functions)
}

/// [`rewrite_native_result_windows`] over the declarations of several
/// programs (the user program and every linked self-host source).
fn rewrite_native_result_windows_over<'a>(
    decls: impl Iterator<Item = &'a almide_ir::IrFunction>,
    functions: &mut [crate::MirFunction],
) {
// T1-3: rewrite the stereotyped Result block windows onto the native
// carrier prims (NTy::Res). Native-leg only — the wasm renderer never
// sees these ops. Runs BEFORE verification/render (both are downstream).
{
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    let result_fns: std::collections::BTreeSet<String> = decls
        .filter(|f| {
            matches!(&f.ret_ty, Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2
                    && matches!(a[0], Ty::Int | Ty::Bool)
                    && matches!(a[1], Ty::String))
                // A LIFTED effect fn (declared `-> Int|Bool`, `effect`) has
                // the same wrapped carrier ABI on this leg — the native
                // pipeline runs no never-err strip, so every lifted call
                // site reads the stereotyped consumer windows. Declared
                // ret_ty alone misses them (the lift lives in the ABI, not
                // the signature), which left the windows unrewritten and
                // the verifier flagging the raw LoadHandle as UseAfterFree.
                // A NEVER-ERR one returns its raw scalar instead: the shared
                // chain settled every call site against that (#3058).
                || (f.is_effect
                    && matches!(f.ret_ty, Ty::Int | Ty::Bool)
                    && !crate::lower::is_never_err_lifted(f.name.as_str()))
        })
        .map(|f| f.name.as_str().to_string())
        .collect();
    for f in functions.iter_mut() {
        crate::native_result_rewrite::rewrite_result_ops(f, &result_fns);
    }
}
}

/// The declared-type → native sig-kind mapping of [`build_native_sig_table`]
/// (return position admits the Res carrier; the param gate rejected Result
/// params before this table is read).
fn native_sig_kind(
    t: &almide_lang::types::Ty,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<crate::render_native::NativeSigKind> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;

    match t {
        Ty::Int | Ty::Bool => Some(NativeSigKind::I64),
        Ty::Float => Some(NativeSigKind::F64),
        Ty::String => Some(NativeSigKind::Str),
        // T1-3: the native Result carrier (return position only — the
        // param gate rejects Result params before this table is read).
        Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2
                && matches!(a[0], Ty::Int | Ty::Bool)
                && matches!(a[1], Ty::String) =>
        {
            Some(NativeSigKind::Res)
        }
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool) =>
        {
            Some(NativeSigKind::ListI64)
        }
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String) => {
            Some(NativeSigKind::ListStr)
        }
        // An all-scalar record travels as its slot block (see sig_ok).
        Ty::Named(n, args)
            if args.is_empty()
                && record_layouts
                    .get(n.as_str())
                    .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft))) =>
        {
            Some(NativeSigKind::ListI64)
        }
        // A flat variant travels as its tag+payload slot block.
        Ty::Named(n, args)
            if args.is_empty()
                && variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                    layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
                }) =>
        {
            Some(NativeSigKind::ListI64)
        }
        // A scalar closure travels as its env block (rung-5 closures slab).
        Ty::Fn { is_effect: _, params, ret }
            if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                && matches!(**ret, Ty::Int | Ty::Bool) =>
        {
            Some(NativeSigKind::ListI64)
        }
        Ty::Named(n, args) if args.is_empty() => crate::render_native_rec::rec_index(n.as_str()).map(NativeSigKind::Rec),
        _ => None,
    }
}

/// The SIG-KIND table (rung 4) of [`try_render_rust_source`]: the declared
/// param/return kinds, computed where the Almide-level `Ty` is visible, so
/// the native render can type a heap `Repr::Ptr` as `&str` vs `&[i64]` per
/// the declaration. Lifted effect fns widen to the Res carrier; lifted
/// lambdas (no IR sig) declare env-block + scalar params.
fn build_native_sig_table(
    ir: &almide_ir::IrProgram,
    functions: &[crate::MirFunction],
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Result<crate::render_native::NativeSigs, LowerError> {
// The SIG-KIND table (rung 4): the declared param/return kinds, computed here
// where the Almide-level `Ty` is visible, so the native render can type a heap
// `Repr::Ptr` as `&str` vs `&[i64]` per the declaration.
let mut sigs: crate::render_native::NativeSigs = Default::default();
{
    
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        let params: Option<Vec<_>> = func.params.iter().map(|p| native_sig_kind(&p.ty, record_layouts, variant_layouts)).collect();
        let ret = if matches!(func.ret_ty, Ty::Unit) {
            Some(None)
        } else if func.is_effect
            && matches!(func.ret_ty, Ty::Int | Ty::Bool)
            && !crate::lower::is_never_err_lifted(func.name.as_str())
        {
            // A LIFTED effect fn returns the wrapped carrier on this leg
            // (the same widening `result_fns` applies above): its declared
            // scalar would type the call dst I64 while the value is Res.
            Some(Some(NativeSigKind::Res))
        } else {
            native_sig_kind(&func.ret_ty, record_layouts, variant_layouts).map(Some)
        };
        if let (Some(ps), Some(r)) = (params, ret) {
            sigs.insert(func.name.as_str().to_string(), (ps, r));
        }
    }
    // LIFTED lambdas exist only as MirFunctions (no IR sig): param 0 is the env
    // block (`&[i64]`), the rest are the lambda's own params — SCALAR reprs only
    // in this slab (a heap-param lambda walls the program: all-or-nothing, and
    // its dispatch arm could not type). The return kind is body-derived by the
    // renderer, so it is not declared here.
    for f in functions {
        if !f.name.starts_with("__lambda_") {
            continue;
        }
        let mut ps = vec![crate::render_native::NativeSigKind::ListI64];
        for p in &f.params[1..] {
            match p.repr {
                crate::Repr::Scalar { .. } => {
                    ps.push(crate::render_native::NativeSigKind::I64)
                }
                _ => {
                    return Err(LowerError::Unsupported(format!(
                        "native: lambda `{}` heap param — outside the closures slab",
                        f.name
                    )))
                }
            }
        }
        sigs.insert(f.name.clone(), (ps, Some(crate::render_native::NativeSigKind::I64)));
    }
}
    Ok(sigs)
}

include!("pipeline_native_concurrency.rs");
include!("pipeline_native_selfhost.rs");

pub fn try_render_rust_source(source: &str) -> Result<String, LowerError> {
    try_render_rust_source_certified(source).map(|(rust, _)| rust)
}

/// [`try_render_rust_source`] with the ownership certificate of EVERY
/// function it rendered (#4006): the certificates are taken from the exact
/// MIR handed to the renderer — after the probe charges, the result-window
/// rewrite and the make-unique elision — so `proofs/native-wall.sh` judges
/// the functions the Rust source was rendered from, not a sibling lowering.
pub fn try_render_rust_source_certified(source: &str) -> Result<(String, Vec<(String, String)>), LowerError> {
    // Debug aid: ALMIDE_DUMP_MIR=1 prints every lowered fn's op stream (the
    // same view `debug_dump_mir` builds) before the native render runs.
    if almide_base::env::flag("ALMIDE_DUMP_MIR") {
        if let Ok(dump) = debug_dump_mir(source) {
            eprintln!("{dump}");
        }
    }
    crate::charge_probe::reset_budget_used();
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = source_to_ir_with(source, &[])?;
    crate::charge_probe::note_program_regions(almide_ir::meter_plan::program_has_regions(&ir));
    require_native_concurrency_support(&ir)?;
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    // #4006 A5: the generated record structs, derived from the declared layouts.
    let _records = crate::render_native_rec::install(crate::render_native_rec::derive_records(&record_layouts));
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    // #4006: an immutable top-level `let` lowers the way the witness path
    // threads it — each reference materializes the CONST initializer
    // (`value_or_global`), and a computed one walls in the lowering. A
    // mutable `var` lives in a storage slot the native render has no
    // address for, so it walls here.
    if ir.top_lets.iter().any(|tl| tl.mutable) {
        return Err(LowerError::Unsupported(
            "native: a mutable top-level var — outside the native rungs".into(),
        ));
    }
    // Top-level lets are evaluated EAGERLY, at program start, before `main`
    // (top_let_div_eager: an unused `let X = 1 / 0` aborts). Materializing at
    // each use is lazy, so it is only faithful for an initializer that can
    // neither fail nor be observed: literals, other top-level lets, and
    // operators that cannot abort (no integer `/`, `%` or `^`, no call).
    let top_vars: std::collections::HashSet<_> = ir.top_lets.iter().map(|tl| tl.var).collect();
    if let Some(tl) = ir.top_lets.iter().find(|tl| !total_const_init(&tl.value, &top_vars)) {
        return Err(LowerError::at(
            tl.value.span,
            "native: a top-level let whose initializer can fail or calls — it must run eagerly, before main",
        ));
    }
    let globals: std::collections::HashMap<_, _> = ir.top_lets.iter().map(|tl| (tl.var, tl.ty.clone())).collect();
    let global_inits: std::collections::HashMap<_, _> =
        ir.top_lets.iter().map(|tl| (tl.var, tl.value.clone())).collect();
    let mut functions = Vec::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        native_sig_subset_check(func, &record_layouts, &variant_layouts)?;
        // ALL-OR-NOTHING: any unlowerable fn walls the program (the native rungs
        // have no per-fn fallback — a partial native binary cannot call into v0).
        let all = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            &record_layouts,
            &variant_layouts,
        )
        .map_err(|e| {
            LowerError::Unsupported(format!("native: fn `{}`: {e:?}", func.name))
        })?;
        functions.extend(all);
    }
    // #4006: the registry bodies the program calls, linked as certified MIR.
    let linked = link_native_self_host(&mut functions)?;
    if let Some(f) = functions.iter().find(|f| recurses_on_a_heap_arg(f)) {
        return Err(LowerError::Unsupported(format!(
            "native: fn `{}` passes a heap value to its own tail call — no loop conversion on this render (#3550)",
            f.name
        )));
    }
    rewrite_native_result_windows_over(
        ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter())),
        &mut functions,
    );
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    let mut sigs = build_native_sig_table(&ir, &functions, &record_layouts, &variant_layouts)?;
    for l in &linked {
        for (name, sig) in build_native_sig_table(&l.ir, &[], &l.record_layouts, &l.variant_layouts)? {
            if functions.iter().any(|f| f.name == name) {
                sigs.entry(name).or_insert(sig);
            }
        }
    }
    // Stage 1 probe: same insertion point in pass order as the wasm leg.
    // T1-2 metered clones carry their base fn's declared signature — copy the
    // sig entry so a call to `__fuel__heavy` types exactly like `heavy` (without
    // this the repr fallback typed a Result-returning clone as String). The
    // clone→base pairs come from the pass that made the clones, never from a
    // name's spelling: a `__fuel` suffix test gave a user fn `heavy__fuel`
    // `heavy`'s signature (#3491).
    let exempt = crate::charge_probe::entry_exempt_set(&ir);
    for (clone, base) in crate::charge_probe::insert_probe_charges(&mut functions, &exempt) {
        if let Some(sig) = sigs.get(base.as_str()).cloned() {
            sigs.insert(clone, sig);
        }
    }
    // #824: see the wasm leg's call above — `Op::MakeUnique` already renders to
    // nothing on native (render_native.rs's `Op::Consume | Op::Borrow |
    // Op::MakeUnique => {}`), so this is a no-op cleanup here, kept only so both
    // legs run the identical target-agnostic MIR pass list.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    let certs = functions.iter().map(|f| (f.name.to_string(), crate::certificate::ownership_certificate(f))).collect();
    let rust = crate::render_native::try_render_native_program(
        &MirProgram {
            functions,
            exports: Vec::new(),
            // Rung 1 walls every top-let above, so there are no mutable-global slots.
            mutable_global_count: 0,
        },
        &sigs,
    )?;
    Ok((rust, certs))
}


/// Every function's ownership certificate, INCLUDING `test` bodies — the
/// pre-flight view of what `proofs/corpus-wall.sh` hands to the kernel-proven
/// checker.
///
/// `debug_dump_mir` skips test fns, and for a long time nothing else looked at
/// them locally either: the only thing that checked a test body's ownership was
/// the Coq-extracted checker in CI, which needs an opam/coqc toolchain most
/// working copies do not have. A leak reachable ONLY from a test block (the L9
/// fork keeps `!` as unwrap there, so a HOF callback that unwraps takes a
/// lowering path ordinary fn bodies never reach) was therefore invisible until
/// a push went red. This makes that view available in-process.
///
/// Returns `(function name, certificate)` pairs; a function outside the
/// lowering subset contributes nothing (an honest wall is not a certificate).
pub fn ownership_certificates(source: &str) -> Result<Vec<(String, String)>, LowerError> {
    Ok(lowered_functions(source)?
        .iter()
        .map(|f| (f.name.to_string(), crate::certificate::ownership_certificate(f)))
        .collect())
}

/// [`ownership_certificates`] with `verify_ownership`'s verdict on the same
/// lowered function: `(name, certificate, verify_ownership accepted)` (#3270).
pub fn ownership_verdicts(source: &str) -> Result<Vec<(String, String, bool)>, LowerError> {
    Ok(lowered_functions(source)?
        .iter()
        .map(|f| (f.name.to_string(), crate::certificate::ownership_certificate(f), crate::verify_ownership(f).is_ok()))
        .collect())
}

/// Every function of `source` that lowers, test bodies included — the MIR
/// [`ownership_certificates`] certifies, for a caller that also runs
/// [`crate::verify_ownership`] on it.
pub fn lowered_functions(source: &str) -> Result<Vec<crate::MirFunction>, LowerError> {
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = crate::pipeline::source_to_ir_for_certs(source)?;
    let globals = std::collections::HashMap::new();
    let global_inits = std::collections::HashMap::new();
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    let mut out = Vec::new();
    for func in &ir.functions {
        let Ok(all) = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            &record_layouts,
            &variant_layouts,
        ) else {
            continue;
        };
        out.extend(all);
    }
    Ok(out)
}

/// An initializer whose value does not depend on WHEN it is evaluated: no
/// call, and no operator that can abort (#4006, the top-level-let rule).
fn total_const_init(e: &almide_ir::IrExpr, top_vars: &std::collections::HashSet<almide_ir::VarId>) -> bool {
    use almide_ir::{BinOp, IrExprKind};
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } => true,
        IrExprKind::Var { id } => top_vars.contains(id),
        IrExprKind::UnOp { operand, .. } => total_const_init(operand, top_vars),
        IrExprKind::BinOp { op, left, right } => {
            !matches!(op, BinOp::DivInt | BinOp::ModInt | BinOp::PowInt)
                && total_const_init(left, top_vars)
                && total_const_init(right, top_vars)
        }
        IrExprKind::List { elements } => elements.iter().all(|x| total_const_init(x, top_vars)),
        _ => false,
    }
}
