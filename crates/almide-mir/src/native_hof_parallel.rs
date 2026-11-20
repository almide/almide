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
        origins.get(v).is_some_and(|(name, _)| callback_thread_quiet(name, &[], functions, &mut BTreeSet::new()))
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

/// Whether `name`, entered with its params `known` (param k holds a closure
/// block whose lambda this walk follows from where it was built), reaches no
/// capability. Closures are accounted where they are built (`FuncRef`, as
/// `certificate::reachable_caps` does), so a closure call is quiet when its
/// block is one built here or a `known` param (#4741: the self-hosted
/// `map.upsert` a fold step calls, handed the step's own lambda). A block of
/// any other origin (a capture, a loaded slot, an unknown param) could be a
/// closure built outside the walk, so it keeps the call off the threads.
fn callback_thread_quiet(name: &str, known: &[bool], functions: &BTreeMap<&str, &MirFunction>, seen: &mut BTreeSet<(String, Vec<bool>)>) -> bool {
    if !seen.insert((name.to_string(), known.to_vec())) { return true; }
    let Some(f) = functions.get(name) else { return false };
    let local = closure_origins(f);
    let followed = |v: &ValueId| local.contains_key(v)
        || f.params.iter().zip(known).any(|(p, k)| *k && p.value == *v);
    let mut direct = (*f).clone();
    direct.ops.retain(|op| !matches!(op, Op::CallIndirect { .. }));
    if !crate::certificate::cap_witness(&direct).used.is_empty() { return false; }
    f.ops.iter().all(|op| match op {
        Op::CallIndirect { args, .. } => matches!(args.first(), Some(CallArg::Handle(b)) if followed(b)),
        Op::CallImport { .. } => false,
        Op::CallFn { name, args, .. } => {
            let passed: Vec<bool> = args.iter().map(|a| matches!(a, CallArg::Handle(v) | CallArg::Scalar(v) if followed(v))).collect();
            if functions.contains_key(name.as_str()) { callback_thread_quiet(name, &passed, functions, seen) }
            else { absent_callee_quiet(name, &passed) }
        }
        Op::FuncRef { name, .. } => callback_thread_quiet(name, &[], functions, seen),
        _ => true,
    })
}

/// A callee the render serves without a MIR body: a dotted stdlib call on the
/// declared floor, or a linked body pruned for its runtime route. It is quiet
/// when it is no host op and every function-typed param its declaration has
/// receives a closure the walk follows (its lambda is checked where built).
fn absent_callee_quiet(name: &str, passed: &[bool]) -> bool {
    let call = if name.contains('.') { name.to_string() } else if pruned(name) { match registry_call(name) { Some(c) => c, None => return false } } else { return false };
    if crate::host_ops::host_op_for_call_name(&call).is_some() { return false; }
    declared_fn_params(&call).is_some_and(|fns| fns.len() == passed.len()
        && fns.iter().zip(passed).all(|(is_fn, followed)| !is_fn || *followed))
}

/// For each param of the stdlib declaration `call` serves (or `@specializes`),
/// whether its type may hold a function; `None` when no single declaration
/// is found.
fn declared_fn_params(call: &str) -> Option<Vec<bool>> {
    let target = crate::native_floor::specializes(call).unwrap_or_else(|| call.to_string());
    let (module, func) = target.split_once('.')?;
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached)?;
    let mut decls = program.decls.iter().filter_map(|d| match d {
        Decl::Fn { name, params, .. } if name.as_str() == func => Some(params),
        _ => None,
    });
    let params = decls.next()?;
    if decls.next().is_some() { return None; }
    let declared: BTreeSet<String> = program.decls.iter().filter_map(|d| match d {
        Decl::Type { name, .. } => Some(name.to_string()),
        _ => None,
    }).collect();
    Some(params.iter().map(|p| may_hold_fn(&p.ty, &declared)).collect())
}

/// A function type anywhere in `t`; a type the module declares itself (an
/// alias may name a function type) counts as one.
fn may_hold_fn(t: &TypeExpr, declared: &BTreeSet<String>) -> bool {
    let any = |ts: &[TypeExpr]| ts.iter().any(|t| may_hold_fn(t, declared));
    match t {
        TypeExpr::Fn { .. } => true,
        TypeExpr::Simple { name } => declared.contains(name.as_str()),
        TypeExpr::Generic { name, args } => declared.contains(name.as_str()) || any(args),
        TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => fields.iter().any(|f| may_hold_fn(&f.ty, declared)),
        TypeExpr::Tuple { elements } => any(elements),
        TypeExpr::Union { members } => any(members),
        TypeExpr::Variant { .. } => true,
        TypeExpr::ConstLit { .. } => false,
    }
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

/// The compiler builtin's own schema: `fan.map(List[A], (A) -> Result[B,
/// String]) -> Result[List[B], String]`, as the frontend constrains it.
fn fan_map_schema() -> Result<(Vec<DTy>, DTy), String> {
    use almide_lang::types::{fan, Ty};
    let a = Ty::TypeVar(almide_base::intern::sym("A"));
    let b = Ty::TypeVar(almide_base::intern::sym("B"));
    let args = [Ty::list(a.clone()), fan::map_callback(a, b.clone())];
    let dparams = args.iter().map(fan_dty).collect::<Option<Vec<_>>>().ok_or("fan builtin parameter schema changed")?;
    let ret = fan_dty(&fan::map_result(b)).ok_or("fan builtin result schema changed")?;
    Ok((dparams, ret))
}

fn fan_map_route(symbol: &str, twin: String, schema: (Vec<DTy>, DTy), items: Vec<&'static str>) -> Hof {
    let (dparams, ret) = schema;
    Hof { symbol: symbol.into(), twin,
        params: dparams.into_iter().map(|d| (d, false)).collect(),
        mutable: vec![false; 2], matrix_ref_lists: vec![false; 2], ret, items, bounds: Vec::new(), owned: None }
}

fn parallel_map_hof() -> Result<Hof, String> {
    let (dparams, ret) = fan_map_schema()?;
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
    Ok(fan_map_route(symbol, symbol.into(), (dparams, ret), items))
}

/// The call a `fan.map` outside the scalar self-host matrix lowers to: no
/// body is linked for it, so only the builtin's sequential runtime body can
/// serve it (a site the product selector threads is a wrapper of its own).
pub(crate) fn sequential_map_call(call: &str) -> bool {
    call == crate::lower::fan_map_call_name(&[], &almide_lang::types::Ty::Unit)
}

/// The same route for a site the lowerer handed to the builtin's linked
/// scalar body (a render with a threaded site offers that link to every
/// pairing): the link refused the element kinds, the runtime body takes them.
fn refused_map_link(impl_fn: &str) -> Option<&'static Hof> {
    let source = registry_call(impl_fn)?;
    let (module, func) = source.split_once('.')?;
    let unlinked = crate::lower::registry_sig::refusal_twin(module, func);
    sequential_map_call(&unlinked).then(|| of(&unlinked)).flatten()
}

/// The sequential runtime body of the builtin (every element runs in list
/// order, the lowest-index `Err` is the map's — ADR-0024 D1), taking the
/// caller's closure statically. Any element pair the adapter can spell rides
/// it: nothing crosses a thread, so there is no `Send` bound to meet.
fn sequential_map_hof() -> Result<Hof, String> {
    let (dparams, ret) = fan_map_schema()?;
    let symbol = "almide_rt_fan_map";
    let (params, rust_ret, items) = crate::native_floor::runtime_body_explained(symbol)?;
    let callback = fan_rust_type(&dparams[1]).ok_or("fan callback schema has no Rust ABI")?;
    if params != [fan_rust_type(&dparams[0]).ok_or("fan input ABI")?, format!("std::rc::Rc<dyn{callback}>")]
        || Some(rust_ret) != fan_rust_type(&ret) {
        return Err("sequential runtime signature disagrees with the compiler builtin".into());
    }
    let (twin, items) = static_runtime_closure(symbol, items).ok_or("the sequential runtime body has no static callback route")?;
    Ok(fan_map_route(symbol, twin, (dparams, ret), items))
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
        DTy::List(e) => format!("AlmideList<{}>", fan_rust_type(e)?),
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
        assert!(super::callback_thread_quiet("pure", &[], &functions, &mut BTreeSet::new()));
        for name in ["printing", "relay", "unknown"] {
            assert!(!super::callback_thread_quiet(name, &[], &functions, &mut BTreeSet::new()));
        }
    }

    /// #4741: a chunked line fold whose step reaches no capability folds on
    /// the threaded runtime body, as v0.69.0 did, through the stdlib floor
    /// calls (`string.len`, `string.split_once`) and the self-hosted
    /// `map.upsert` the step hands its own lambda. A step that prints, or that
    /// calls a closure built outside the walk, keeps the Send-free twin.
    #[test]
    fn a_quiet_chunked_fold_step_runs_on_the_threaded_runtime() {
        let render = |step: &str, init: &str, acc: &str| crate::pipeline::try_render_rust_source(&format!(r#"
import fs
type Stats = {{ min: Int, count: Int }}
{step}
effect fn main() -> Unit = {{
  let parts: List[{acc}] = fs.fold_lines_chunked("in.txt", 12, {init}, step)!
  println(int.to_string(list.len(parts)))
}}
"#)).expect("the chunked fold renders");
        let threaded = |rust: &str| (rust.contains("almide_rt_fs_fold_lines_chunked(") || rust.contains("almide_rt_fs_fold_lines_chunked_str_lent("))
            && !rust.contains("almide_rt_fs_fold_lines_chunked_seq");
        let sum = render("fn step(acc: Int, line: String) -> Int = acc + string.len(line)", "0", "Int");
        assert!(threaded(&sum), "an Int step that only reads its line is threaded");
        assert!(sum.contains("almide_rt_fs_fold_lines_chunked_str_lent("), "a step that only reads its line takes it lent, not copied per line");
        let stats = render(r#"fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] = match string.split_once(line, ";") {
  some((k, v)) => {
    let t = string.len(v)
    map.upsert(acc, k, Stats { min: t, count: 1 }, (s) => Stats { min: if t < s.min then t else s.min, count: s.count + 1 })
  },
  none => acc,
}"#, "map.new()", "Map[String, Stats]");
        assert!(threaded(&stats), "the aggregation step (split_once + upsert with its own lambda) is threaded");
        assert!(stats.contains("almide_rt_map_upsert_str_static("), "the found key is borrowed, not copied per line");
        assert!(stats.contains("almide_rt_fs_fold_lines_chunked_str_lent("), "the aggregation step reads its line lent");
        let printing = render("fn step(acc: Int, line: String) -> Int = { println(line); acc + 1 }", "0", "Int");
        assert!(printing.contains("almide_rt_fs_fold_lines_chunked_seq"), "a printing step keeps the chunk order of its output");
    }

    /// #4741: every line fold that copies the line it hands its step derives
    /// a lent-string twin from its own text.
    #[test]
    fn line_folds_pair_their_lent_string_twins() {
        for symbol in ["almide_rt_fs_fold_lines", "almide_rt_fs_fold_lines_chunked", "almide_rt_fs_fold_lines_chunked_seq", "almide_rt_fs_fold_lines_range"] {
            let (twin, text, items) = crate::native_floor::str_lent_twin(symbol).unwrap_or_else(|| panic!("{symbol} derives a lent-string twin"));
            assert_eq!(twin, format!("{symbol}_str_lent"));
            assert!(text.contains(&format!("fn {twin}")) && text.contains("Fn(A, &str)") && !text.contains("AlmideString::from("), "{symbol}: {text}");
            assert!(items.iter().any(|i| i.contains(&format!("fn {symbol}"))), "{symbol}: the twin reaches the body's own items");
        }
        // A body that copies no string it hands its callback has no twin.
        assert!(crate::native_floor::str_lent_twin("almide_rt_list_map").is_none());
        let (sig, n) = crate::native_floor::callback_strs_lent("<A>(f:std::rc::Rc<dynFn(A,AlmideString)->A>)->Result<A,AlmideString>");
        assert_eq!((sig.as_str(), n), ("<A>(f:std::rc::Rc<dynFn(A,&str)->A>)->Result<A,AlmideString>", 1));
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
