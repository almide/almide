// Compiler-builtin fan.map ABI, shared with frontend constraints. Unlike a
// bundled @intrinsic this call has no source module declaration. Only the
// shared product selector's registered sites may use its parallel runtime.
pub(crate) fn parallel_selected(callee: &str) -> bool {
    STATE.with(|s| s.borrow().at.as_ref().is_some_and(|(f, _, _)| crate::native_parallel_maps::selected(f, callee)))
}

/// A host's threaded body may invoke only a callback whose reachable MIR
/// has no observable effects. Unknown callback values use the generated
/// Send-free twin as well; they must not acquire a new scheduling behavior.
fn host_callback_route(
    hof: &'static Hof,
    args: &[CallArg],
    origins: &BTreeMap<ValueId, (String, Vec<ValueId>)>,
    functions: &BTreeMap<&str, &MirFunction>,
) -> Option<&'static Hof> {
    let Some(seq) = almide_codegen::generated::runtime_fn_modes::send_free_twin(&hof.symbol) else { return Some(hof) };
    let quiet = hof.params.iter().zip(args).all(|((d, _), a)| {
        if !matches!(d, DTy::Fn(..)) { return true; }
        let (CallArg::Handle(v) | CallArg::Scalar(v)) = a else { return false };
        origins.get(v).is_some_and(|(name, _)| callback_thread_quiet(name, functions, &mut BTreeSet::new()))
    });
    if quiet { return Some(hof); }
    static ROUTES: OnceLock<Mutex<HashMap<String, Option<&'static Hof>>>> = OnceLock::new();
    let mut routes = ROUTES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    *routes.entry(hof.symbol.clone()).or_insert_with(|| {
        let (params, ret, items) = crate::native_floor::send_free_runtime_body(&hof.symbol)?;
        if params.len() != hof.params.len() || rust_agrees(&hof.ret, &ret) != Some(false) { return None; }
        for ((d, borrowed), p) in hof.params.iter().zip(&params) {
            if declared_param_mode(d, false, p) != Some(*borrowed) { return None; }
        }
        let (twin, items) = static_runtime_closure(seq, items)?;
        let mut route = hof.clone();
        route.twin = twin;
        route.items = items;
        route.owned = None;
        Some(&*Box::leak(Box::new(route)))
    })
}

fn callback_thread_quiet(name: &str, functions: &BTreeMap<&str, &MirFunction>, seen: &mut BTreeSet<String>) -> bool {
    if !seen.insert(name.to_string()) { return true; }
    let Some(f) = functions.get(name) else { return false };
    if !crate::certificate::cap_witness(f).used.is_empty() { return false; }
    f.ops.iter().all(|op| match op {
        Op::CallIndirect { .. } | Op::CallImport { .. } => false,
        Op::CallFn { name, .. } | Op::FuncRef { name, .. } => callback_thread_quiet(name, functions, seen),
        _ => true,
    })
}

fn parallel_hof() -> Option<&'static Hof> {
    static HOFS: OnceLock<Mutex<HashMap<(Option<(String, String, String)>, usize), Option<&'static Hof>>>> = OnceLock::new();
    let function = current_fn()?;
    let flags = crate::native_parallel_maps::float_lanes(&function)?;
    let declaration = crate::native_parallel_maps::declaration(&function);
    let key = (declaration.clone(), flags);
    let mut cache = HOFS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    *cache.entry(key).or_insert_with(|| {
        let mut hof = match declaration {
            Some((module, func, symbol)) => parallel_declared_hof(&module, &func, &symbol),
            None => parallel_map_hof(),
        }.ok()?;
        let floats = [(1, "A"), (2, "B")].into_iter()
            .filter(|(bit, _)| flags & bit != 0).map(|(_, name)| name.to_string()).collect();
        for (d, _) in &mut hof.params { substitute_float(d, &floats); }
        substitute_float(&mut hof.ret, &floats);
        Some(&*Box::leak(Box::new(hof)))
    })
}

fn parallel_map_hof() -> Result<Hof, String> {
    use almide_lang::types::{fan, Ty};
    let a = Ty::TypeVar(almide_base::intern::sym("A"));
    let b = Ty::TypeVar(almide_base::intern::sym("B"));
    let args = [Ty::list(a.clone()), fan::map_callback(a, b.clone())];
    let ret = fan::map_result(b);
    let dparams = args.iter().map(fan_dty).collect::<Option<Vec<_>>>().ok_or("fan builtin parameter schema changed")?;
    let ret = fan_dty(&ret).ok_or("fan builtin result schema changed")?;
    let symbol = "almide_rt_fan_map_par";
    let (params, rust_ret, items) = crate::native_floor::parallel_runtime_body(symbol)?;
    let callback = fan_rust_type(&dparams[1]).ok_or("fan callback schema has no Rust ABI")?;
    // Check the whole callback bound, not only the generic parameter name F.
    // The native runtime remains responsible for enforcing Send/Sync/Clone.
    let head: String = items.first().ok_or("no parallel runtime body")?.split('{').next().unwrap_or("")
        .chars().filter(|c| !c.is_whitespace()).collect();
    if params != [fan_rust_type(&dparams[0]).ok_or("fan input ABI")?, "F".into()]
        || Some(rust_ret) != fan_rust_type(&ret)
        || !head.contains(&format!("<A:Send+Sync+Clone,B:Send,F:{callback}+Send+Sync>")) {
        return Err("parallel runtime signature disagrees with the compiler builtin".into());
    }
    Ok(Hof { symbol: symbol.into(), twin: symbol.into(),
        params: dparams.into_iter().map(|d| (d, false)).collect(),
        mutable: vec![false; 2], matrix_ref_lists: vec![false; 2], ret, items, bounds: Vec::new(), owned: None })
}

fn fan_dty(ty: &almide_lang::types::Ty) -> Option<DTy> {
    use almide_lang::types::{constructor::TypeConstructorId as TC, Ty};
    Some(match ty {
        Ty::TypeVar(v) if matches!(v.as_str(), "A" | "B") => DTy::Var(v.to_string()),
        Ty::String => DTy::Str,
        Ty::Applied(TC::List, ts) if ts.len() == 1 => DTy::List(Box::new(fan_dty(&ts[0])?)),
        Ty::Applied(TC::Result, ts) if ts.len() == 2 => DTy::Result(Box::new(fan_dty(&ts[0])?), Box::new(fan_dty(&ts[1])?)),
        Ty::Fn { params, ret, .. } => DTy::Fn(params.iter().map(fan_dty).collect::<Option<_>>()?, Box::new(fan_dty(ret)?)),
        _ => return None,
    })
}

fn fan_rust_type(ty: &DTy) -> Option<String> {
    Some(match ty {
        DTy::Var(v) => v.clone(),
        DTy::Str => "AlmideString".into(),
        DTy::Bool => "bool".into(),
        DTy::List(e) => format!("Vec<{}>", fan_rust_type(e)?),
        DTy::Result(a, b) => format!("Result<{},{}>", fan_rust_type(a)?, fan_rust_type(b)?),
        DTy::Fn(ps, r) => format!("Fn({})->{}", ps.iter().map(fan_rust_type).collect::<Option<Vec<_>>>()?.join(","), fan_rust_type(r)?),
        _ => return None,
    })
}

/// Derive the early selected list call from its actual stdlib declaration.
/// The parallel symbol is selected by codegen's existing runtime pair table.
fn parallel_declared_hof(module: &str, func: &str, symbol: &str) -> Result<Hof, String> {
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached).ok_or("no parallel declaration module")?;
    let mut decls = program.decls.iter().filter(|d| matches!(d, Decl::Fn { name, .. } if name.as_str() == func));
    let Some(Decl::Fn { effect, generics, params, return_type, .. }) = decls.next() else { return Err("no parallel declaration".into()) };
    if decls.next().is_some() || effect.unwrap_or(false) || params.iter().any(|p| p.is_mut) { return Err("ambiguous/effectful/mutable parallel declaration".into()); }
    let names: Vec<_> = generics.iter().flatten().map(|g| g.name.to_string()).collect();
    let tps = names.iter().cloned().collect();
    let dparams = params.iter().map(|p| dty_of(&p.ty, &tps)).collect::<Option<Vec<_>>>().ok_or("parallel parameter schema")?;
    let ret = dty_of(return_type, &tps).ok_or("parallel return schema")?;
    let [DTy::List(input), callback @ DTy::Fn(_, _)] = dparams.as_slice() else { return Err("parallel declaration is not a list callback".into()) };
    let DTy::Var(input_name) = &**input else { return Err("parallel input is not generic".into()) };
    let (rust_params, rust_ret, items) = crate::native_floor::parallel_runtime_body(symbol)?;
    let input_ty = fan_rust_type(&dparams[0]).ok_or("parallel input ABI")?;
    let slice_ty = format!("&[{}]", fan_rust_type(input).ok_or("parallel element ABI")?);
    let borrowed = rust_params.first() == Some(&slice_ty);
    if rust_params != [if borrowed { slice_ty } else { input_ty }, "F".into()]
        || Some(rust_ret) != fan_rust_type(&ret) { return Err("parallel runtime signature disagrees".into()); }
    let mut bounds: Vec<_> = names.iter().map(|n| format!("{n}:{}", if n == input_name { "Send+Sync+Clone" } else { "Send" })).collect();
    bounds.push(format!("F:{}+Send+Sync", fan_rust_type(callback).ok_or("parallel callback ABI")?));
    let head: String = items.first().ok_or("no parallel body")?.split('{').next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
    if !head.contains(&format!("<{}>", bounds.join(","))) { return Err("parallel runtime callback bounds disagree".into()); }
    Ok(Hof { symbol: symbol.into(), twin: symbol.into(),
        params: dparams.into_iter().enumerate().map(|(i, d)| (d, i == 0 && borrowed)).collect(),
        mutable: vec![false; 2], matrix_ref_lists: vec![false; 2], ret, items, bounds: Vec::new(), owned: None })
}

#[cfg(test)]
mod parallel_abi_tests {
    #[test]
    fn callback_effects_and_unknown_calls_require_the_send_free_route() {
        use crate::{MirFunction, Op, RtFn};
        use std::collections::{BTreeMap, BTreeSet};
        let pure = MirFunction { name: "pure".into(), ..Default::default() };
        let printing = MirFunction { name: "printing".into(), ops: vec![
            Op::Call { dst: None, func: RtFn::PrintStr, args: vec![], result: None },
        ], ..Default::default() };
        let relay = MirFunction { name: "relay".into(), ops: vec![
            Op::CallFn { dst: None, name: "printing".into(), args: vec![], result: None },
        ], ..Default::default() };
        let functions = BTreeMap::from([("pure", &pure), ("printing", &printing), ("relay", &relay)]);
        assert!(super::callback_thread_quiet("pure", &functions, &mut BTreeSet::new()));
        for name in ["printing", "relay", "unknown"] {
            assert!(!super::callback_thread_quiet(name, &functions, &mut BTreeSet::new()));
        }
    }

    #[test]
    fn shared_compiler_builtin_agrees_with_parallel_runtime() {
        let hof = super::parallel_map_hof().expect("shared builtin and actual parallel ABI agree");
        assert_eq!(hof.symbol, "almide_rt_fan_map_par");
        let rows = almide_codegen::pass_rust_lowering_fan::parallel_declarations();
        assert_eq!(rows.len(), 4, "the existing parallel family is complete");
        for (module, func, _, symbol) in rows {
            super::parallel_declared_hof(module, func, symbol).unwrap_or_else(|why| panic!("{module}.{func}: {why}"));
        }
    }
}
