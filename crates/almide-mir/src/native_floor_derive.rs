// ── the declaration-to-floor derivation of native_floor.rs, include!-spliced back at module level ──
//
// A code move under the 800-line ceiling the codopsy gate holds this crate
// to: there is no boundary of meaning here.

/// `str_elems`: the call is a self-hosted body over a String list that
/// specializes this declaration, so an equality or ordering bound on its
/// element meets a String's, never a Float's bits.
fn derive_for(call: &str, str_elems: bool) -> Result<FloorFn, String> {

    let Some((module, func)) = call.split_once('.') else {
        // A bare registry helper (one generated code calls by its own name)
        // that declares the stdlib fn it specializes is served as that fn.
        return match specialized_target(call) {
            Some(target) if target.contains('.') => {
                derive_for(&target, str_elems).map_err(|e| format!("specializes `{target}`: {e}"))
            }
            _ => Err("not a module.fn call".into()),
        };
    };
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return Err("not a bundled stdlib module".into());
    }
    let program = almide_lang::stdlib_info::bundled_source(module)
        .and_then(almide_lang::parse_cached)
        .ok_or("the module's source does not parse")?;
    let mut decls = program.decls.iter().filter_map(|d| match d {
        Decl::Fn { name, effect, attrs, generics, params, return_type, body, .. } if name.as_str() == func => {
            Some((effect, attrs, generics, params, return_type, body))
        }
        _ => None,
    });
    let Some((effect, attrs, generics, params, ret, body)) = decls.next() else {
        // A self-hosted representation-specialized body (`map.set_skv`)
        // declares the stdlib fn it specializes (`@specializes("map.set")`):
        // the floor serves it as that fn.
        return match specialized_target(call) {
            Some(target) if target != call => {
                let str_elems = specialized_list_elem(call).as_deref() == Some("String");
                derive_for(&target, str_elems).map_err(|e| format!("specializes `{target}`: {e}"))
            }
            _ => Err("no declaration".into()),
        };
    };
    if decls.next().is_some() {
        return Err("declared more than once".into());
    }
    // A body that is one call of another stdlib fn with its own params, in
    // order, IS that fn: its floor entry is the callee's.
    if !attrs.iter().any(|a| a.name.as_str() == "intrinsic") {
        if let Some(target) = body.as_ref().and_then(|b| forwarded_call(b, params)) {
            return derive_explained(&target).map_err(|e| format!("forwards to `{target}`: {e}"));
        }
    }
    let is_effect = effect.unwrap_or(false);
    let symbol = attrs
        .iter()
        .find(|a| a.name.as_str() == "intrinsic")
        .and_then(|a| a.args.first())
        .and_then(|a| match &a.value {
            AttrValue::String { value } => Some(value.to_string()),
            _ => None,
        })
        .ok_or("no @intrinsic (an Almide body)")?;
    if crate::host_ops::is_uncounted_host_call(call) {
        return Err("capability-bearing intrinsic has no admitted host operation".into());
    }
    let type_params: BTreeSet<String> = generics
        .iter()
        .flatten()
        .map(|g| g.name.as_str().to_string())
        .collect();
    // A `Map[K, V]` (or `List[(K, V)]`) param binds K and V: its key, value,
    // and the returns spelled over them, take the map's native kinds.
    let map_kv = params.iter().find_map(|p| map_params(&p.ty, &type_params)).or_else(|| map_params(ret, &type_params));
    // A `Set[A]` param or return binds A the same way.
    let set_a = params.iter().find_map(|p| set_param_name(&p.ty, &type_params)).or_else(|| set_param_name(ret, &type_params));
    let fparams: Vec<FloorParam> = params
        .iter()
        .map(|p| {
            map_kv
                .as_ref()
                .and_then(|kv| map_param(&p.ty, kv))
                .or_else(|| set_a.as_ref().and_then(|a| set_param(&p.ty, a)))
                .or_else(|| (p.is_mut && simple(&p.ty) == Some("Bytes")).then_some(FloorParam::BytesMut))
                .or_else(|| floor_param(&p.ty, &type_params))
                .ok_or_else(|| format!("param type {:?} outside the per-type map", p.ty))
        })
        .collect::<Result<_, _>>()?;
    let fret = map_kv
        .as_ref()
        .and_then(|kv| map_ret(ret, kv))
        .or_else(|| set_a.as_ref().and_then(|a| set_ret(ret, a)))
        .or_else(|| floor_ret(ret, &type_params))
        .ok_or_else(|| format!("return type {ret:?} outside the per-type map"))?;
    // A pair-list return alone does not bind its two generics from two
    // independent lists. That signature belongs to the general typed runtime
    // adapter; this floor's map conversion needs an actual map/pairs source.
    if fret == FloorRet::MapEntries
        && !fparams.iter().any(|p| matches!(p, FloorParam::Map(_) | FloorParam::MapPairs)) {
        return Err("pair-list return has no map or pair-list parameter to bind its types".into());
    }
    if fret == FloorRet::ResFileStat && (module != "fs" || !file_stat_decl_agrees(&program.decls)) {
        return Err("runtime FileStat layout disagrees with its bundled declaration".into());
    }
    if fret == FloorRet::ResProcessStatus && (module != "process" || !process_status_decl_agrees(&program.decls)) {
        return Err("runtime ProcessStatus layout disagrees with its bundled declaration".into());
    }
    // An effect fn's error reaches its caller through its declared Result,
    // the carrier the call site's `!` reads; any other effect return walls.
    // A HOST OP is the other admitted effect: its effect is a capability, not
    // an error channel, and the certificate counts that capability at the
    // call site (`certificate::cap_witness`, the ids the wasm leg's op_caps
    // uses). It joins the floor when its row agrees with this declaration:
    // the same intrinsic fn, the same runtime symbol, at least one capability.
    let counted_host_op = crate::host_ops::host_op(module, func)
        .is_some_and(|o| o.intrinsic_fn == func && o.symbol == symbol && !o.caps.is_empty());
    if is_effect
        && !matches!(fret, FloorRet::RuntimeResult(..))
        && !matches!(fret, FloorRet::Res(_) | FloorRet::ResStr | FloorRet::ResValue | FloorRet::ResListStr | FloorRet::ResListValue | FloorRet::ResStrValuePairs | FloorRet::ResBytes | FloorRet::ResOptStr | FloorRet::ResOptBytes | FloorRet::ResOptListStr | FloorRet::ResFileStat | FloorRet::ResProcessStatus | FloorRet::ResBytesStrInt)
        && !counted_host_op
    {
        return Err("an effect fn whose return is not a Result carrier".into());
    }
    let rt = runtime_index();
    let body = signature_text(rt, &symbol).ok_or_else(|| format!("`{symbol}` is not a takeable runtime item"))?;
    let (rust_params, rust_ret) = rust_signature(body).ok_or("the runtime signature does not parse")?;
    if rust_params.len() != fparams.len() || !fparams.iter().zip(&rust_params).all(|(p, r)| param_agrees(*p, r)) {
        return Err(format!("runtime params {rust_params:?} disagree with the declaration"));
    }
    if !ret_agrees(fret, &rust_ret) {
        return Err(format!("runtime return `{rust_ret}` disagrees with the declaration"));
    }
    // An element-typed param or return needs the list that decides it, and
    // no equality or ordering capability on its generic (the declared
    // `capability_bounds`, read off the runtime's bounds): a native
    // `Vec<i64>` may carry Float bits, which `i64` comparison would misjudge.
    let elem_typed = matches!(fret, FloorRet::Elem(_)) || fparams.iter().any(|p| matches!(p, FloorParam::Elem(_)));
    if elem_typed {
        elem_typed_decided(&fparams, (module, func), str_elems)?;
    }
    let fparams: Vec<FloorParam> = fparams
        .into_iter()
        .zip(&rust_params)
        .map(|(p, r)| with_runtime_mode(p, r))
        .collect();
    let items = closure(&symbol, rt)?;
    let twin = almide_lang::reuse::of_intrinsic(&symbol).and_then(|r| {
        let t = r.owned_twin?;
        if !matches!(fparams.get(r.param), Some(FloorParam::List(_) | FloorParam::Set(_) | FloorParam::Map(_) | FloorParam::Bytes | FloorParam::Value(_))) {
            return None;
        }
        let (tp, tr) = rust_signature(signature_text(rt, t)?)?;
        // The twin differs only in taking the source by value.
        let same = tp.len() == rust_params.len()
            && tp.iter().zip(&rust_params).enumerate().all(|(k, (a, b))| k == r.param || squash(a) == squash(b))
            && squash(&tr) == squash(&rust_ret);
        same.then(|| Some((r.param, t.to_string(), closure(t, rt).ok()?)))?
    });
    let str_only = elem_typed && almide_lang::capability_bounds::of(module, func).next().is_some();
    let ordered = list_elem_ordered((module, func), generics.iter().flatten().map(|g| g.name.as_str()).collect(), params);
    let borrow_twin = borrow_twin_of(fret, &symbol, &rust_params, &rust_ret, rt);
    let hinted_twin = borrow_twin.as_ref().and_then(|(t, _)| hinted_twin_of(t, rt));
    Ok(with_by_ref_twin(FloorFn { symbol, params: fparams, ret: fret, items, twin, str_only, ordered, borrow_twin, hinted_twin, by_ref: None }, &rust_params, &rust_ret, rt))
}

/// A declared param `p` with the borrow mode its runtime spelling `r` takes
/// (`&…` = by reference), for the kinds whose mode the runtime decides.
fn with_runtime_mode(p: FloorParam, r: &str) -> FloorParam {
    match p {
        FloorParam::List(elem) if squash(r).starts_with("Vec<") => FloorParam::ListOwned(elem),
        FloorParam::Elem(_) => FloorParam::Elem(squash(r).starts_with('&')),
        FloorParam::Value(_) => FloorParam::Value(squash(r).starts_with('&')),
        FloorParam::Runtime(t, _) => FloorParam::Runtime(t, squash(r).starts_with('&')),
        FloorParam::Matrix(_) => FloorParam::Matrix(squash(r).starts_with('&')),
        FloorParam::Set(_) => FloorParam::Set(squash(r).starts_with('&')),
        FloorParam::SetElem(_) => FloorParam::SetElem(squash(r).starts_with('&')),
        FloorParam::Map(_) => FloorParam::Map(squash(r).starts_with('&')),
        FloorParam::MapKey(_) => FloorParam::MapKey(squash(r).starts_with('&')),
        FloorParam::MapVal(_) => FloorParam::MapVal(squash(r).starts_with('&')),
        other => other,
    }
}

/// [`derive_for`]'s element-typed declaration: a `List[A]` param decides
/// the element, and its generic needs no equality or ordering capability.
fn elem_typed_decided(fparams: &[FloorParam], (module, func): (&str, &str), str_elems: bool) -> Result<(), String> {
    if !fparams.iter().any(|p| matches!(p, FloorParam::List(ListElem::Any))) {
        return Err("an element-typed param or return with no `List[A]` param to decide it".into());
    }
    if almide_lang::capability_bounds::of(module, func).next().is_some() && !str_elems {
        return Err("an element-typed generic that needs equality or ordering".into());
    }
    Ok(())
}

/// Whether `module.func` bounds the element of one of its `List[A]` params by
/// ordering (`capability_bounds`, read off the runtime's generic bounds). A
/// bound on another generic (`sort_by`'s key) leaves the element unordered.
fn list_elem_ordered((module, func): (&str, &str), generics: Vec<&str>, params: &[almide_lang::ast::Param]) -> bool {
    almide_lang::capability_bounds::of(module, func).any(|b| {
        b.cap == almide_lang::capability_bounds::Cap::Ord
            && generics.get(b.generic).is_some_and(|g| {
                params.iter().any(|p| {
                    matches!(&p.ty, TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 && simple(&args[0]) == Some(*g))
                })
            })
    })
}
