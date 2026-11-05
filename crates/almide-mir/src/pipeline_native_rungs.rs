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
    let is_typed_fn = |t: &Ty| matches!(t, Ty::Fn { .. }) && native_sig_kind(t, record_layouts, variant_layouts).is_some();
    let is_scalar_fn = |t: &Ty| {
        matches!(t, Ty::Fn { is_effect: _, params, ret }
            if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                && matches!(**ret, Ty::Int | Ty::Bool))
    };
    // T1-3: Result[scalar, String] returns ride a dedicated native
    // carrier (NTy::Res — Rust Result<i64, String>). String-Ok payloads
    // stay walled (their tag/payload windows are ambiguous with Err's).
    let is_native_result = is_native_result_ty;
    // #4006 List[String] slab: an owned `Vec<AlmideString>` / a borrowed
    // `&[AlmideString]` (the slot block's handles, rendered by the address
    // algebra in render_native_addr.rs).
    let is_str_list = |t: &Ty| {
        matches!(t, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String))
    };
    // #4006 A5: a record with heap fields that has a generated struct.
    let is_struct_record = |t: &Ty| {
        matches!(t, Ty::Named(n, args) if args.is_empty() && crate::render_native_rec::rec_index(n.as_str()).is_some())
            || matches!(t, Ty::Bytes | Ty::Matrix)
            || crate::render_native_rec::is_scalar_instance(t)
            || (matches!(t, Ty::Tuple(_) | Ty::Record { .. } | Ty::Applied(TypeConstructorId::List, _) | Ty::Named(..))
                && crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)).is_some())
            || heap_carrier_of(t).is_some()
            || crate::lower::is_value_ty(t)
            || native_map_of(t).is_some()
            || native_set_of(t).is_some()
    };
    let sig_ok = |t: &Ty| {
        crate::render_native_rec::is_lane_scalar(t) || matches!(t, Ty::String)
            || is_scalar_option(t)
            || is_native_result(t)
            || is_string_result_ty(t)
            || is_scalar_list(t)
            || is_str_list(t)
            || is_struct_record(t)
            || is_scalar_record(t)
            || is_flat_variant(t)
            || is_scalar_fn(t)
            || is_typed_fn(t)
            || native_sig_kind(t, record_layouts, variant_layouts).is_some()
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
        // Every release that erases to scope end (a typed list or carrier
        // drop as much as a plain one) still runs after the call.
        let rest = f.ops[i + 1..].iter().find(|o| !matches!(o, Op::Consume { .. }) && !crate::render_native_move::bookkeeping(o));
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
/// The synthesized slot initializer's name: the shared lowering already
/// knows it (an assignment inside it skips the old-value take of a heap slot).
const SLOT_INIT: &str = "__mg_init";

/// `__mg_init`: every slot-held top-level binding (a mutable `var`, or a `let`
/// that must run eagerly) assigned its initializer, in declaration order,
/// through the same slot-routed assignment user code uses. `None` when the
/// program holds no such binding.
fn synthesize_slot_init(ir: &almide_ir::IrProgram) -> Option<almide_ir::IrFunction> {
    let stmts: Vec<almide_ir::IrStmt> = ir
        .top_lets
        .iter()
        .filter(|tl| tl.mutable)
        .map(|tl| almide_ir::IrStmt {
            kind: almide_ir::IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
            span: None,
        })
        .collect();
    if stmts.is_empty() {
        return None;
    }
    let body = almide_ir::IrExpr {
        kind: almide_ir::IrExprKind::Block { stmts, expr: None },
        ty: almide_lang::types::Ty::Unit,
        span: Default::default(),
        def_id: None,
    };
    Some(almide_ir::IrFunction {
        name: almide_lang::intern::sym(SLOT_INIT),
        params: vec![],
        ret_ty: almide_lang::types::Ty::Unit,
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: almide_ir::IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: vec![], // fresh-fn: synthesized zero-param slot initializer
    })
}

/// A `Result` whose Ok payload is a native scalar (Int, Bool, Float as its
/// bits, Unit as 0) and whose Err is a String: the T1-3 carrier
/// `Result<i64, AlmideString>` (#4006 family C widened it past Int | Bool).
fn is_native_result_ty(t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && (crate::render_native_rec::is_lane_scalar(&a[0]) || matches!(a[0], Ty::Unit))
            && matches!(a[1], Ty::String))
}

/// `Result[String, String]`: the String-Ok carrier `Result<AlmideString,
/// AlmideString>` (#4006 family C).
fn is_string_result_ty(t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String))
}

/// #4006 family C: rewrite every String-Ok Result window onto its carrier.
fn rewrite_native_result_str_windows(ir: &almide_ir::IrProgram, functions: &mut [crate::MirFunction]) {
    let fns: std::collections::BTreeSet<String> = ir
        .functions
        .iter()
        .filter(|f| is_string_result_ty(&f.ret_ty))
        .map(|f| f.name.as_str().to_string())
        .collect();
    let decls: std::collections::HashMap<&str, &almide_ir::IrFunction> =
        ir.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    for f in functions.iter_mut() {
        let params: Vec<crate::ValueId> = match decls.get(f.name.as_str()) {
            Some(d) if d.params.len() == f.params.len() => d
                .params
                .iter()
                .zip(&f.params)
                .filter(|(dp, _)| is_string_result_ty(&dp.ty))
                .map(|(_, mp)| mp.value)
                .collect(),
            _ => Vec::new(),
        };
        let carriers = heap_carrier_values(ir, f);
        crate::native_result_str_rewrite::rewrite_result_str_ops(f, &fns, &params, &carriers);
    }
}

/// The generated heap Option/Result carrier a declared type stands for.
fn heap_carrier_of(t: &almide_lang::types::Ty) -> Option<u16> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _))
        .then(|| crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)))
        .flatten()
        .filter(|i| crate::render_native_addr::is_carrier(*i))
}

/// The generated `AlmideMap` alias a declared map type stands for.
fn native_set_of(t: &almide_lang::types::Ty) -> Option<u16> {
    crate::render_native_rec::is_native_set(t)
        .then(|| crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)))
        .flatten()
        .filter(|i| crate::render_native_rec::set_elem(*i).is_some())
}

fn native_map_of(t: &almide_lang::types::Ty) -> Option<u16> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Map, _))
        .then(|| crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)))
        .flatten()
        .filter(|i| crate::render_native_rec::map_kinds(*i).is_some())
}

/// The values of `f` whose declared type is a generated heap carrier.
fn heap_carrier_values(ir: &almide_ir::IrProgram, f: &crate::MirFunction) -> std::collections::BTreeSet<crate::ValueId> {
    heap_carrier_values_in(&|name: &str| ir.functions.iter().find(|d| d.name.as_str() == name), f)
}

/// [`heap_carrier_values`] over any declaration lookup.
fn heap_carrier_values_in<'a>(
    decl: &dyn Fn(&str) -> Option<&'a almide_ir::IrFunction>,
    f: &crate::MirFunction,
) -> std::collections::BTreeSet<crate::ValueId> {
    let sig = |name: &str| -> Option<crate::render_native_addr::CarrierSig> {
        let d = decl(name)?;
        Some((d.params.iter().map(|p| heap_carrier_of(&p.ty)).collect(), heap_carrier_of(&d.ret_ty)))
    };
    let own = sig(f.name.as_str()).unwrap_or_default();
    crate::render_native_addr::carrier_values(f, &own, &sig).into_keys().collect()
}

/// A LIFTED effect fn: declared `-> Int | Bool | Unit` (not a Result), whose
/// body can err, so its compiled ABI is the wrapped `Result[T, String]`
/// carrier on this leg. A never-err one returns its raw value (#3058), and
/// the entry `main` keeps its own exit convention (the same exclusion
/// `effect_abi_facts` makes for the auto-wrap ABI).
fn returns_lifted_carrier(f: &almide_ir::IrFunction) -> bool {
    use almide_lang::types::Ty;
    f.is_effect
        && match f.ret_ty {
            Ty::Int | Ty::Bool => true,
            Ty::Unit => f.name.as_str() != "main",
            _ => false,
        }
        && !crate::lower::is_never_err_lifted(f.name.as_str())
}

/// An `Option` whose payload is one of the native scalar kinds (Int, Bool,
/// Float): the `Option<i64>` carrier (#4006 family B), read off the type.
fn is_scalar_option(t: &almide_lang::types::Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    matches!(t, Ty::Applied(TypeConstructorId::Option, a)
        if a.len() == 1 && crate::render_native_rec::is_lane_scalar(&a[0]))
}

/// #4006 family B: rewrite every scalar-Option window onto the native
/// carrier prims. The Option values are seeded from the declared types: a
/// call to a fn returning a scalar Option, and a param declared as one.
fn rewrite_native_option_windows(ir: &almide_ir::IrProgram, linked: &[LinkedSelfHost], functions: &mut [crate::MirFunction]) {
    // The declarations: the program's, and every linked self-host source's.
    let declared = || ir.functions.iter().chain(linked.iter().flat_map(|l| l.ir.functions.iter()));
    let mut option_fns: std::collections::BTreeSet<String> = declared()
        .filter(|f| is_scalar_option(&f.ret_ty))
        .map(|f| f.name.as_str().to_string())
        .collect();
    // A declared floor call whose declaration returns a scalar Option
    // (`Int?`, or the element `A?` of its `List[A]` param) binds the same
    // `Option<i64>` carrier (`native_floor::render_call`), so its `??` /
    // `match` windows are this carrier's too. An element Option over a
    // String list walls at the floor render, and every carrier read refuses
    // a value not typed `Opt`.
    for f in functions.iter() {
        for op in &f.ops {
            if let crate::Op::CallFn { name, .. } = op {
                let opt_ret = crate::native_floor::declared(name).is_some_and(|d| match d.ret {
                    crate::native_floor::FloorRet::Opt(_) => true,
                    crate::native_floor::FloorRet::Elem(crate::native_floor::ElemRet::Opt) => {
                        crate::native_floor::elem_option_is_scalar(name)
                    }
                    _ => false,
                });
                if opt_ret {
                    option_fns.insert(name.clone());
                }
            }
        }
    }
    let decls: std::collections::HashMap<&str, &almide_ir::IrFunction> =
        declared().map(|f| (f.name.as_str(), f)).collect();
    for f in functions.iter_mut() {
        let opt_params: Vec<crate::ValueId> = match decls.get(f.name.as_str()) {
            Some(d) if d.params.len() == f.params.len() => d
                .params
                .iter()
                .zip(&f.params)
                .filter(|(dp, _)| is_scalar_option(&dp.ty))
                .map(|(_, mp)| mp.value)
                .collect(),
            // A lifted lambda declares its params where the lowering lifts
            // it (`crate::lambda_sigs`), after its env param.
            None => match crate::lambda_sigs::get(&f.name) {
                Some((ps, _)) if ps.len() + 1 == f.params.len() => ps
                    .iter()
                    .zip(&f.params[1..])
                    .filter(|(t, _)| is_scalar_option(t))
                    .map(|(_, mp)| mp.value)
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let carriers = heap_carrier_values(ir, f);
        let filled = crate::option_blocks::of(&f.name);
        crate::native_option_rewrite::rewrite_option_ops(f, &option_fns, &opt_params, &carriers, &filled);
    }
}

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
    let decl_list: Vec<&almide_ir::IrFunction> = decls.collect();
    let result_fns: std::collections::BTreeSet<String> = decl_list
        .iter()
        .filter(|f| {
            is_native_result_ty(&f.ret_ty)
                // A LIFTED effect fn (declared `-> Int|Bool`, `effect`) has
                // the same wrapped carrier ABI on this leg — the native
                // pipeline runs no never-err strip, so every lifted call
                // site reads the stereotyped consumer windows. Declared
                // ret_ty alone misses them (the lift lives in the ABI, not
                // the signature), which left the windows unrewritten and
                // the verifier flagging the raw LoadHandle as UseAfterFree.
                // A NEVER-ERR one returns its raw scalar instead: the shared
                // chain settled every call site against that (#3058).
                || returns_lifted_carrier(f)
        })
        .map(|f| f.name.as_str().to_string())
        .collect();
    let decls: std::collections::HashMap<&str, &almide_ir::IrFunction> =
        decl_list.iter().map(|f| (f.name.as_str(), *f)).collect();
    for f in functions.iter_mut() {
        let res_params: Vec<crate::ValueId> = match decls.get(f.name.as_str()) {
            Some(d) if d.params.len() == f.params.len() => d
                .params
                .iter()
                .zip(&f.params)
                .filter(|(dp, _)| is_native_result_ty(&dp.ty))
                .map(|(_, mp)| mp.value)
                .collect(),
            _ => Vec::new(),
        };
        let carriers = heap_carrier_values_in(&|n: &str| decls.get(n).copied(), f);
        crate::native_result_rewrite::rewrite_result_ops(f, &result_fns, &res_params, &carriers);
    }
}
}

include!("pipeline_native_sigs.rs");
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
///
/// Each certificate is `(property, function, witness)`: an `ownership`
/// witness per function, and one `native-calls` witness for the whole program
/// (function `program`) binding every call site's parameter modes and moves
/// (#4006 class P, proofs/NativeCalls.v).
pub fn try_render_rust_source_certified(source: &str) -> Result<(String, Vec<NativeCert>), LowerError> {
    render_native_parts(source).map(|(rust, certs, _)| (rust, certs))
}

/// The capabilities `main` reaches in the native render of `source`, as
/// capability ids: `certificate::reachable_caps` over the exact MIR the
/// native renderer rendered from (host ops counted at their call sites, as
/// `cap_witness` counts them). The native leg's capability report, held to
/// the wasm leg's (`cert_project::caps`) for the same program.
pub fn native_capability_reach(source: &str) -> Result<std::collections::BTreeSet<u32>, LowerError> {
    let (_, _, functions) = render_native_parts(source)?;
    let program: std::collections::BTreeMap<String, crate::MirFunction> =
        functions.into_iter().map(|f| (f.name.to_string(), f)).collect();
    let mut seen = std::collections::BTreeSet::new();
    Ok(crate::certificate::reachable_caps("main", &program, &mut seen).into_iter().map(|c| c.id()).collect())
}

include!("pipeline_native_render.rs");

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
