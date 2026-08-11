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
    let sig_ok = |t: &Ty| {
        matches!(t, Ty::Int | Ty::Bool | Ty::Float | Ty::String)
            || is_scalar_list(t)
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

/// T1-3: rewrite the stereotyped Result block windows onto the native
/// carrier prims (NTy::Res). Native-leg only — the wasm renderer never sees
/// these ops. Runs BEFORE verification/render (both are downstream).
fn rewrite_native_result_windows(ir: &almide_ir::IrProgram, functions: &mut [crate::MirFunction]) {
// T1-3: rewrite the stereotyped Result block windows onto the native
// carrier prims (NTy::Res). Native-leg only — the wasm renderer never
// sees these ops. Runs BEFORE verification/render (both are downstream).
{
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    let result_fns: std::collections::BTreeSet<String> = ir
        .functions
        .iter()
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
                || (f.is_effect && matches!(f.ret_ty, Ty::Int | Ty::Bool))
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
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        let params: Option<Vec<_>> = func.params.iter().map(|p| native_sig_kind(&p.ty, record_layouts, variant_layouts)).collect();
        let ret = if matches!(func.ret_ty, Ty::Unit) {
            Some(None)
        } else if func.is_effect && matches!(func.ret_ty, Ty::Int | Ty::Bool) {
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

pub fn try_render_rust_source(source: &str) -> Result<String, LowerError> {
    // Debug aid: ALMIDE_DUMP_MIR=1 prints every lowered fn's op stream (the
    // same view `debug_dump_mir` builds) before the native render runs.
    if std::env::var("ALMIDE_DUMP_MIR").is_ok() {
        if let Ok(dump) = debug_dump_mir(source) {
            eprintln!("{dump}");
        }
    }
    crate::charge_probe::reset_budget_used();
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = source_to_ir_with(source, &[])?;
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    if !ir.top_lets.is_empty() {
        return Err(LowerError::Unsupported(
            "native: top-level lets — outside rung 1".into(),
        ));
    }
    let globals = std::collections::HashMap::new();
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
            &std::collections::HashMap::new(),
            &record_layouts,
            &variant_layouts,
        )
        .map_err(|e| {
            LowerError::Unsupported(format!("native: fn `{}`: {e:?}", func.name))
        })?;
        functions.extend(all);
    }
    rewrite_native_result_windows(&ir, &mut functions);
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    let mut sigs = build_native_sig_table(&ir, &functions, &record_layouts, &variant_layouts)?;
    // Stage 1 probe: same insertion point in pass order as the wasm leg.
    crate::charge_probe::insert_probe_charges(&mut functions);
    // T1-2 metered clones carry their base fn's declared signature — copy the
    // sig entry so a call to `heavy__fuel` types exactly like `heavy` (without
    // this the repr fallback typed a Result-returning clone as String).
    {
        let cloned: Vec<(String, _)> = functions
            .iter()
            .filter_map(|f| {
                let base = f.name.strip_suffix("__fuel")?;
                Some((f.name.as_str().to_string(), sigs.get(base)?.clone()))
            })
            .collect();
        for (name, sig) in cloned {
            sigs.insert(name, sig);
        }
    }
    // #824: see the wasm leg's call above — `Op::MakeUnique` already renders to
    // nothing on native (render_native.rs's `Op::Consume | Op::Borrow |
    // Op::MakeUnique => {}`), so this is a no-op cleanup here, kept only so both
    // legs run the identical target-agnostic MIR pass list.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);
    crate::render_native::try_render_native_program(
        &MirProgram {
            functions,
            exports: Vec::new(),
            // Rung 1 walls every top-let above, so there are no mutable-global slots.
            mutable_global_count: 0,
        },
        &sigs,
    )
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
        for f in all {
            out.push((
                f.name.to_string(),
                crate::certificate::ownership_certificate(&f),
            ));
        }
    }
    Ok(out)
}
