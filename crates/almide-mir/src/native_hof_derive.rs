// Included from native_hof.rs; a text move preserving function scope.
fn derive_explained(impl_fn: &str) -> Result<Hof, String> {
    let call = registry_call(impl_fn).ok_or("no registry call")?;
    let target = crate::native_floor::specializes(&call).unwrap_or_else(|| call.to_string());
    let (module, func) = target.split_once('.').ok_or("not a module.fn")?;
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached).ok_or("no module source")?;
    let mut decls = program.decls.iter().filter_map(|d| match d {
        Decl::Fn { name, effect, attrs, generics, params, return_type, .. } if name.as_str() == func => {
            Some((effect, attrs, generics, params, return_type))
        }
        _ => None,
    });
    let (effect, attrs, generics, params, ret) = decls.next().ok_or_else(|| format!("no declaration of `{target}`"))?;
    if decls.next().is_some() {
        return Err("declared twice".into());
    }
    let symbol = attrs
        .iter()
        .find(|a| a.name.as_str() == "intrinsic")
        .and_then(|a| a.args.first())
        .and_then(|a| match &a.value {
            AttrValue::String { value } => Some(value.to_string()),
            _ => None,
        })
        .ok_or("no @intrinsic")?;
    let bounds = almide_lang::capability_bounds::of(module, func).map(|b| {
        generics.as_ref().and_then(|gs| gs.get(b.generic))
            .map(|g| (g.name.as_str().to_string(), b.cap)).ok_or("runtime bound has no declared generic")
    }).collect::<Result<Vec<_>, _>>()?;
    // Keep the upstream rejection unless declaration-level specialization
    // establishes a Float value adapter instead of comparing erased bits.
    let needs_float_comparison = !bounds.is_empty() && target != call
        && crate::native_floor::specialized_list_elem_holds_float(&call);
    let tps: BTreeSet<String> = generics.iter().flatten().map(|g| g.name.as_str().to_string()).collect();
    let dparams: Vec<DTy> = params.iter().map(|p| declared_dty(&p.ty, &tps, &program.decls)).collect::<Option<_>>().ok_or("a param type outside the map")?;
    let mut dret = declared_dty(ret, &tps, &program.decls).ok_or("a return type outside the map")?;
    // Effectful callbacks require the same named host declaration and
    // capability row as ordinary native host calls. Plain effect calls
    // continue through their existing floor, rather than this adapter.
    if effect.unwrap_or(false) && !crate::host_ops::host_op(module, func).is_some_and(|o|
        o.symbol == symbol && o.intrinsic_fn == func && !o.caps.is_empty()
        && (crate::host_ops::is_native_fs_host_call(&symbol)
            || dparams.iter().any(|d| matches!(d, DTy::Fn(..))))) {
        return Err("effect declaration outside the declared native host boundary".into());
    }
    let (rust_params, rust_ret, items) = if almide_codegen::generated::runtime_fn_modes::crosses_threads(&symbol) {
        crate::native_floor::parallel_runtime_body(&symbol)?
    } else { crate::native_floor::runtime_body_explained(&symbol)? };
    let matrix_ref_lists = dparams.iter().zip(&rust_params)
        .map(|(d, r)| matrix_ref_list(d, r, &symbol, &items))
        .collect::<Option<Vec<_>>>().ok_or("matrix-list runtime representation disagrees")?;
    if rust_params.len() != dparams.len() || rust_agrees(&dret, &rust_ret) != Some(false) {
        return Err(format!("runtime signature {rust_params:?} -> {rust_ret} disagrees"));
    }
    let generic_callbacks: Vec<bool> = dparams.iter().zip(&rust_params)
        .map(|(d, r)| generic_runtime_callback(d, r, &symbol, &items)).collect();
    let mutable: Vec<bool> = params.iter().map(|p| p.is_mut).collect();
    if dparams.iter().zip(&mutable).any(|(d, m)| *m && has_runtime_tuple(d)) {
        return Err("mutable tuple arguments need a write-back adapter".into());
    }
    let mut params: Vec<(DTy, bool)> = dparams
        .into_iter()
        .zip(&rust_params)
        .zip(&mutable).zip(&generic_callbacks).zip(&matrix_ref_lists)
        .map(|((((d, r), m), generic), matrix)| {
            if *matrix && !*m { return Some((d, true)); }
            if *generic && !*m { Some((d, false)) }
            else { declared_param_mode(&d, *m, r).map(|b| (d, b)) }
        })
        .collect::<Option<_>>()
        .ok_or_else(|| format!("runtime params {rust_params:?} disagree"))?;
    specialize_float_params(impl_fn, &mut params, &mut dret);
    if needs_float_comparison && !matches!(params.first(), Some((DTy::List(e), _)) if **e == DTy::Float) {
        return Err(format!("a Float specialization of `{target}` lacks a value comparison adapter"));
    }
    // A combinator takes its closure statically (`static_twin`); a call with
    // no function-typed param calls the runtime body as it is.
    if params.iter().zip(&generic_callbacks).all(|((d, _), generic)| !matches!(d, DTy::Fn(..)) || *generic) {
        return Ok(Hof { twin: symbol.clone(), symbol, params, mutable, matrix_ref_lists, ret: dret, items, bounds, owned: None });
    }
    let (twin, mut items) = static_runtime_closure(&symbol, items).ok_or("the runtime body has no static callback route")?;
    let owned = reuse_route(&symbol, &params, &dret, &mut items);
    Ok(Hof { symbol, params, mutable, matrix_ref_lists, ret: dret, items, twin, bounds, owned })
}

/// The static route of `symbol`'s `@reuse` owned twin: the reused param and
/// the twin's static name, adding the twin's items when the borrowing body
/// does not already forward to it. Only an update that returns the same
/// container it borrows qualifies, as on the declared floor
/// (`native_floor::takes_by_value`).
fn reuse_route(symbol: &str, params: &[(DTy, bool)], ret: &DTy, items: &mut Vec<&'static str>) -> Option<(usize, String)> {
    let r = almide_lang::reuse::of_intrinsic(symbol)?;
    let t = r.owned_twin?;
    let (d, by_ref) = params.get(r.param)?;
    if !*by_ref || d != ret || !matches!(d, DTy::List(_) | DTy::Set(_) | DTy::Map(..) | DTy::Bytes) {
        return None;
    }
    let (tparams, _, titems) = crate::native_floor::runtime_body(t)?;
    if tparams.len() != params.len() || tparams.get(r.param)?.trim_start().starts_with('&') {
        return None;
    }
    let name = format!("{t}_static");
    let carried = |items: &[&str]| items.iter().any(|i| crate::native_floor::runtime_fn_name(i).as_deref() == Some(name.as_str()));
    if !carried(items.as_slice()) {
        let (tw, more) = static_runtime_closure(t, titems)?;
        if tw != name {
            return None;
        }
        for i in more {
            if !items.contains(&i) {
                items.push(i);
            }
        }
    }
    carried(items.as_slice()).then_some((r.param, name))
}

