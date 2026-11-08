// Included by native_hof: checked adapters for first-class callback values.
/// What the function-typed arguments add before and beside the call: the
/// env arrays' bindings, and the shims they need.
#[derive(Default)]
struct FnArgOut {
    prelude: Vec<String>,
    shims: Vec<&'static str>,
    reads: Vec<crate::render_native_bounds::Slot>,
}

/// What rendering the function-typed argument needs to see.
struct FnArgCx<'a> {
    user_fns: &'a BTreeMap<&'a str, &'a MirFunction>,
    sigs: &'a NativeSigs,
    tys: &'a BTreeMap<ValueId, NTy>,
    origins: &'a BTreeMap<ValueId, (String, Vec<ValueId>)>,
    /// The op index, which names the env array.
    at: usize,
    bounds: &'a [(String, almide_lang::capability_bounds::Cap)],
}

/// The adapter for the function-typed argument `a` (kind `t`) of declared
/// type `(dps) -> dret`, binding the type params it fixes; the env array's
/// binding and any shim it needs go to `side`.
fn fn_arg(cx: &FnArgCx<'_>, env: &mut Env, (dps, dret): (&[DTy], &DTy), (a, t): (&CallArg, NTy), side: &mut FnArgOut) -> Option<String> {
    let (CallArg::Handle(blk) | CallArg::Scalar(blk)) = a else { return None };
    let origin = cx.origins.get(blk);
    let lambda = origin.map(|(name, _)| name.as_str());
    // A lambda borrows its env; a user param it takes over (a move param,
    // `native_move_params`) gets the value the runtime hands the adapter.
    let moved: Vec<bool> = match lambda {
        Some(lambda) => {
            let l = cx.user_fns.get(lambda)?;
            if l.params.first().is_some_and(|p| l.owned_params.contains(&p.value)) { return None; }
            clo::lambda_moves(l)
        }
        None => Vec::new(),
    };
    let declared = match t {
        NTy::Rec(i) | NTy::RecRef(i) => rec::closure_sig(i),
        _ => None,
    };
    let (ps, ret) = if let Some(lambda) = lambda { clo::lambda_sig(cx.sigs, lambda)? }
        else { let (ps, ret) = declared?; (ps.into_iter().map(clo::arg_kind).collect(), ret.map(clo::arg_kind)) };
    let ret = ret?;
    if !matches!(t, NTy::Vec | NTy::VecRef) && !rec::is_closure(t) { return None; }
    let layout = callback_layout(dps, ps.len())?;
    if !cx.bounds.is_empty() {
        let (ptys, rty) = if let Some(lambda) = lambda { crate::lambda_sigs::get(lambda)? }
            else { let (NTy::Rec(i) | NTy::RecRef(i)) = t else { return None }; rec::closure_source(i)? };
        if ptys.len() != layout.len() || !layout.iter().zip(&ptys).all(|((d, _), t)| bounded_type_agrees(cx.bounds, d, t))
            || !bounded_type_agrees(cx.bounds, dret, &rty) { return None; }
    }
    let tuples = dps.iter().any(has_runtime_tuple) || has_runtime_tuple(dret);
    let bits: Vec<Lane> = if tuples {
        for ((d, _), k) in layout.iter().zip(&ps) { bind_callback_shape(env, d, clo::owned_nty(*k))?; }
        bind_callback_shape(env, dret, clo::owned_nty(ret))?;
        Vec::new()
    } else { dps
        .iter()
        .zip(&ps)
        .map(|(dp, k)| match bind_lambda(env, dp, *k)? {
            true => Some(Lane::F64),
            false if *dp == DTy::Bool => Some(Lane::Bool),
            false => Some(Lane::Same),
        })
        .collect::<Option<_>>()? };
    let ret_lane = match dret {
        DTy::Float => Lane::F64,
        DTy::Bool => Lane::Bool,
        _ => Lane::Same,
    };
    if !tuples && ret_lane == Lane::F64 {
        side.shims.push(LANE_F64);
    } else if !tuples && bind_lambda(env, dret, ret) != Some(false) {
        return None;
    }
    // The env: a copy of the block's slots, every one a scalar.
    let slots: Option<Vec<String>> = origin.map(|(_, es)| es.as_slice()).unwrap_or(&[])
        .iter()
        .map(|e| match cx.tys.get(e) {
            Some(NTy::I64) => Some(var(*e)),
            Some(NTy::F64) => Some(format!("{}.to_bits() as i64", var(*e))),
            _ => None,
        })
        .collect::<Option<_>>();
    let name = format!("__env_{}_{}", cx.at, side.prelude.len());
    if tuples && ret == NativeSigKind::F64 { side.shims.push(LANE_F64); }
    let closure = if tuples { tuple_callback_adapter(env, lambda, &name, (dps, dret), (&ps, &moved), ret)? }
        else { adapter(lambda, &name, (&ps, &moved), &bits, (ret, ret_lane))? };
    if matches!(t, NTy::Vec | NTy::VecRef) {
        let slots = slots?;
        side.prelude.push(format!("let {name}: [i64; {}] = [{}];", slots.len(), slots.join(", ")));
    } else {
        side.prelude.push(format!("let {name} = &{};", var(*blk)));
        if lambda.is_none() {
            side.prelude.push(format!("if {name}.len() == 0 {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }}"));
            side.prelude.push(format!("let {name}_index: i64 = {name}.words()[0];"));
            side.reads.push(crate::render_native_bounds::Slot { base: *blk, idx: None, k: 0 });
        }
    }
    Some(closure)
}


/// One call expression prefix; the dynamic index is read and checked once
/// in the call's prelude, before the runtime invokes the callback.
fn callback_head(lambda: Option<&str>, env: &str, ps: &[NativeSigKind], ret: NativeSigKind) -> String {
    if let Some(lambda) = lambda { return format!("{}(&{env}", crate::render_native::mangle(lambda)); }
    let key = clo::sig_key(ps, &[], Some(ret)).unwrap_or_else(|| ps.len().to_string());
    format!("__almd_ci_{key}({env}_index, &{env}")
}
