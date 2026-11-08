// The SIG-KIND table (rung 4) of the native rungs — include!-spliced into
// `pipeline.rs` through pipeline_native_rungs.rs, sharing that module's
// imports. A pure code move out of its parent (the 800-line ceiling the
// codopsy gate holds this crate to), with the two long functions split into
// named helpers; there is no boundary of meaning here.

/// The declared-type → native sig-kind mapping of [`build_native_sig_table`]
/// (return position admits the Res carrier; the param gate rejected Result
/// params before this table is read). The arms are tried in their original
/// order across three groups: lane scalars and carriers, slot-block kinds,
/// then the generated-type kinds (the last group's verdict is final).
fn native_sig_kind(
    t: &almide_lang::types::Ty,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<crate::render_native::NativeSigKind> {
    if let Some(k) = native_lane_sig_kind(t) {
        return Some(k);
    }
    if let Some(k) = native_block_sig_kind(t, record_layouts, variant_layouts) {
        return Some(k);
    }
    native_rec_sig_kind(t)
}

/// [`native_sig_kind`]'s first group: the lane scalars, `String` and the
/// scalar Result carriers. `None` = no arm of this group matched.
fn native_lane_sig_kind(t: &almide_lang::types::Ty) -> Option<crate::render_native::NativeSigKind> {
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;

    match t {
        // The 64-bit floats are real `f64`s; every other lane scalar (the
        // sized integers, a Float32's f32 bits) rides the i64 lane as is.
        t if crate::render_native_rec::is_f64_scalar(t) => Some(NativeSigKind::F64),
        // A narrow integer: its declared width (the one the lowering re-wraps
        // to) is its call-boundary type (#4006).
        t if crate::lower::narrow_int_width(t).is_some() => {
            let (bits, signed) = crate::lower::narrow_int_width(t)?;
            Some(NativeSigKind::Narrow { bits: bits as u8, signed })
        }
        t if crate::render_native_rec::is_lane_scalar(t) => Some(NativeSigKind::I64),
        Ty::String => Some(NativeSigKind::Str),
        // T1-3: the native Result carrier (return position only — the
        // param gate rejects Result params before this table is read).
        t if is_native_result_ty(t) => Some(NativeSigKind::Res),
        t if is_string_result_ty(t) => Some(NativeSigKind::ResStr),
        _ => None,
    }
}

/// [`native_sig_kind`]'s second group, the collection half: scalar lists, an
/// all-scalar tuple and a scalar Option. `None` = no arm matched.
fn native_list_sig_kind(t: &almide_lang::types::Ty) -> Option<crate::render_native::NativeSigKind> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;

    match t {
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && crate::render_native_rec::is_lane_scalar(&a[0]) =>
        {
            Some(NativeSigKind::ListI64)
        }
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String) => {
            Some(NativeSigKind::ListStr)
        }
        // #4006: an all-scalar tuple rides the same `Vec<i64>` (a List[Float]
        // already did above: each slot holds the f64 bits).
        // (One the render generates a by-value struct for travels as it, below.)
        Ty::Tuple(es)
            if !es.is_empty()
                && es.iter().all(crate::render_native_rec::is_lane_scalar)
                && crate::render_native_rec::rec_index(&crate::render_native_rec::block_key(t)).is_none() =>
        {
            Some(NativeSigKind::ListI64)
        }
        // #4006 family B: an Option of a scalar rides the `Option<i64>` carrier.
        t if is_scalar_option(t) => Some(NativeSigKind::Opt),
        _ => None,
    }
}

/// [`native_sig_kind`]'s second group: the slot-block kinds (collections,
/// all-scalar records, flat variants, closures). `None` = no arm matched.
fn native_block_sig_kind(
    t: &almide_lang::types::Ty,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<crate::render_native::NativeSigKind> {
    if let Some(k) = native_list_sig_kind(t) {
        return Some(k);
    }
    if let Some(k) = native_flat_named_sig_kind(t, record_layouts, variant_layouts) {
        return Some(k);
    }
    native_closure_sig_kind(t, record_layouts, variant_layouts)
}

/// The all-scalar record and flat-variant arms of [`native_block_sig_kind`].
/// `None` = no arm matched.
fn native_flat_named_sig_kind(
    t: &almide_lang::types::Ty,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<crate::render_native::NativeSigKind> {
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;

    match t {
        // An all-scalar record travels as its slot block (see sig_ok).
        Ty::Named(n, args)
            if args.is_empty()
                && crate::render_native_rec::rec_index(n.as_str()).is_none()
                && record_layouts
                    .get(n.as_str())
                    .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft))) =>
        {
            Some(NativeSigKind::ListI64)
        }
        // A flat variant travels as its tag+payload slot block.
        Ty::Named(n, args)
            if args.is_empty()
                && crate::render_native_rec::rec_index(n.as_str()).is_none()
                && variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                    layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
                }) =>
        {
            Some(NativeSigKind::ListI64)
        }
        _ => None,
    }
}

/// The closure arms of [`native_block_sig_kind`]. `None` = no arm matched.
fn native_closure_sig_kind(
    t: &almide_lang::types::Ty,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<crate::render_native::NativeSigKind> {
    crate::render_native_rec::closure_env_type(t, |p| native_sig_kind(p, record_layouts, variant_layouts).is_some())
        .then_some(crate::render_native::NativeSigKind::ListI64)
}

/// [`native_sig_kind`]'s last group: the generated-type kinds (`Value`,
/// records, tuples, bytes, heap carriers, maps). Its verdict is final.
fn native_rec_sig_kind(t: &almide_lang::types::Ty) -> Option<crate::render_native::NativeSigKind> {
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;

    match t {
        // The stdlib's dynamic Value: v0's runtime `AlmideValue`.
        t if crate::lower::is_value_ty(t) => Some(NativeSigKind::Rec(crate::render_native_rec::value_index())),
        Ty::Named(n, args) if args.is_empty() => crate::render_native_rec::rec_index(n.as_str()).map(NativeSigKind::Rec),
        Ty::Named(_, _) if crate::render_native_rec::is_scalar_instance(t) => Some(NativeSigKind::ListI64),
        Ty::Named(_, _) => crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)).map(NativeSigKind::Rec),
        Ty::Bytes => Some(NativeSigKind::Rec(crate::render_native_rec::bytes_index())),
        Ty::Matrix => Some(NativeSigKind::Rec(crate::render_native_rec::matrix_index())),
        // #4006: a heap Option / a Result outside the scalar carriers rides its
        // generated `Option<T>` / `Result<T, E>`.
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _) => heap_carrier_of(t).map(NativeSigKind::Rec),
        // A map rides v0's runtime `AlmideMap` (its generated alias).
        Ty::Applied(TypeConstructorId::Map, _) => native_map_of(t).map(NativeSigKind::Rec),
        // A set of Int or String rides v0's runtime `AlmideSet` (its alias).
        Ty::Applied(TypeConstructorId::Set, _) => native_set_of(t).map(NativeSigKind::Rec),
        Ty::Tuple(_) | Ty::Record { .. } | Ty::Applied(TypeConstructorId::List, _) => {
            crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(t)).map(NativeSigKind::Rec)
        }
        _ => None,
    }
}

/// One declared native signature: the param kinds and the return kind
/// (`None` = Unit).
type NativeSig = (Vec<crate::render_native::NativeSigKind>, Option<crate::render_native::NativeSigKind>);

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
    keep: &dyn Fn(&str) -> bool,
) -> Result<crate::render_native::NativeSigs, LowerError> {
    // The SIG-KIND table (rung 4): the declared param/return kinds, computed here
    // where the Almide-level `Ty` is visible, so the native render can type a heap
    // `Repr::Ptr` as `&str` vs `&[i64]` per the declaration. Only the fns `keep`
    // admits are visited: reading a signature's kinds registers its types in
    // the generated type table (#4514).
    let mut sigs: crate::render_native::NativeSigs = Default::default();
    declare_native_ir_sigs(ir, record_layouts, variant_layouts, &mut sigs, keep);
    widen_native_sigs_to_lane(functions, &mut sigs);
    declare_native_lambda_sigs(functions, record_layouts, variant_layouts, &mut sigs)?;
    Ok(sigs)
}

/// [`build_native_sig_table`]'s first step: every non-test IR function whose
/// params and return all have a native kind.
fn declare_native_ir_sigs(
    ir: &almide_ir::IrProgram,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
    sigs: &mut crate::render_native::NativeSigs,
    keep: &dyn Fn(&str) -> bool,
) {
    use almide_lang::types::Ty;
    use crate::render_native::NativeSigKind;
    for func in &ir.functions {
        if func.is_test || !keep(func.name.as_str()) {
            continue;
        }
        let params: Option<Vec<_>> = func.params.iter().map(|p| native_sig_kind(&p.ty, record_layouts, variant_layouts)).collect();
        let ret = if returns_lifted_carrier(func) {
            // A LIFTED effect fn returns the wrapped carrier on this leg
            // (the same widening `result_fns` applies above): its declared
            // scalar would type the call dst I64 while the value is Res.
            Some(Some(NativeSigKind::Res))
        } else if matches!(func.ret_ty, Ty::Unit) {
            Some(None)
        } else {
            native_sig_kind(&func.ret_ty, record_layouts, variant_layouts).map(Some)
        };
        if let (Some(ps), Some(r)) = (params, ret) {
            sigs.insert(func.name.as_str().to_string(), (ps, r));
        }
    }
}

/// [`build_native_sig_table`]'s second step. The narrow-integer call ABI
/// (#4006) is for an all-scalar signature called by name. A function value
/// shares the closure tables' i64 lane, and a signature with a heap kind
/// keeps its existing spelling, so both take the lane kinds.
fn widen_native_sigs_to_lane(functions: &[crate::MirFunction], sigs: &mut crate::render_native::NativeSigs) {
    use crate::render_native::NativeSigKind;
    let fn_values: std::collections::BTreeSet<&str> = functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            crate::Op::FuncRef { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    for (name, (ps, r)) in sigs.iter_mut() {
        let scalar = |k: &NativeSigKind| matches!(k, NativeSigKind::I64 | NativeSigKind::F64 | NativeSigKind::Narrow { .. });
        if fn_values.contains(name.as_str()) || !ps.iter().chain(r.iter()).all(scalar) {
            ps.iter_mut().for_each(|k| *k = k.lane());
            *r = r.map(NativeSigKind::lane);
        }
    }
}

/// [`build_native_sig_table`]'s last step. LIFTED lambdas exist only as
/// MirFunctions (no IR sig): param 0 is the env block (`&[i64]`), the rest are
/// the lambda's own params — SCALAR reprs only in this slab (a heap-param
/// lambda walls the program: all-or-nothing, and its dispatch arm could not
/// type). The return kind is body-derived by the renderer, so it is not
/// declared here.
fn declare_native_lambda_sigs(
    functions: &[crate::MirFunction],
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
    sigs: &mut crate::render_native::NativeSigs,
) -> Result<(), LowerError> {
    for f in functions {
        if !f.name.starts_with("__lambda_") {
            continue;
        }
        if let Some(sig) = heap_lambda_native_sig(f, record_layouts, variant_layouts) {
            sigs.insert(f.name.clone(), sig);
            continue;
        }
        let sig = scalar_lambda_native_sig(f)?;
        sigs.insert(f.name.clone(), sig);
    }
    Ok(())
}

/// A lambda with a heap or Float parameter/result is typed
/// from its declared signature (recorded at the lift). `None` = not such a
/// lambda, or a kind with no native spelling: it falls through to the scalar
/// slab, which walls a heap param as before.
fn heap_lambda_native_sig(
    f: &crate::MirFunction,
    record_layouts: &crate::lower::RecordLayouts,
    variant_layouts: &crate::lower::VariantLayouts,
) -> Option<NativeSig> {
    use crate::render_native::NativeSigKind;
    let (ptys, rty) = crate::lambda_sigs::get(&f.name)?;
    let heap = f.params[1..].iter().any(|p| !matches!(p.repr, crate::Repr::Scalar { .. }))
        || crate::lower::is_heap_ty(&rty);
    // A scalar MIR repr alone erases Float to i64. Generic runtime callbacks
    // must retain numeric equality (-0 == +0, NaN != NaN), not compare bits.
    let float = ptys.iter().chain([&rty]).any(crate::render_native_rec::is_f64_scalar);
    if !heap && !float {
        return None;
    }
    if ptys.len() + 1 != f.params.len() {
        return None;
    }
    // A kind with no native spelling falls through to the scalar
    // slab below, which walls a heap param as before.
    let ps: Option<Vec<_>> = ptys.iter().map(|t| native_sig_kind(t, record_layouts, variant_layouts)).collect();
    let ret = if matches!(rty, almide_lang::types::Ty::Unit) {
        Some(None)
    } else {
        native_sig_kind(&rty, record_layouts, variant_layouts).map(Some)
    };
    let (Some(ps), Some(ret)) = (ps, ret) else { return None };
    let mut all = vec![crate::render_native::NativeSigKind::ListI64];
    all.extend(ps.into_iter().map(NativeSigKind::lane));
    Some((all, ret.map(NativeSigKind::lane)))
}

/// The scalar closures slab: env block + `I64` params, `I64` result; a heap
/// param walls the program.
fn scalar_lambda_native_sig(f: &crate::MirFunction) -> Result<NativeSig, LowerError> {
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
    Ok((ps, Some(crate::render_native::NativeSigKind::I64)))
}
