//! A stdlib call with a function argument, served by its declared runtime
//! body when the linked self-hosted body cannot take the call (#4006, C1).
//!
//! A stdlib combinator (`list.fold`, `list.map`, `list.flat_map`,
//! `option.map`, `matrix.map`) reaches the native render through the
//! self-hosted body the registry links for it (`pipeline_native_selfhost.rs`).
//! That body is written for the wasm leg's representation: `list_fold_str`
//! serves a fold over any list of heap elements (all of them are one pointer
//! wide there) but declares `List[String]`, and `matrix_map` takes the
//! `List[List[Float]]` rows where the native matrix is v0's `AlmideMatrix`.
//! A call whose argument kinds the linked body does not declare walls.
//!
//! Such a call is served instead by the runtime body its stdlib DECLARATION
//! names (`@intrinsic("almide_rt_list_fold")`, the body v0 calls), the way
//! the declared floor serves every other intrinsic (`crate::native_floor`).
//! Nothing here names a stdlib fn: the route is read off the declaration of
//! the fn the linked body implements (its registry call, or the fn it
//! `@specializes`), its type params are bound from the call's argument kinds,
//! and its function-typed params from the lambda the call passes.
//!
//! This route is ONLY the fallback of a call the linked body refuses, so every
//! program the linked bodies already render keeps its render.
//!
//! The closure crosses as `Rc<dyn Fn>`, the runtime's own parameter type: an
//! adapter that owns a copy of the closure's env block (scalar captures only:
//! the env is a plain slot list) and calls the lifted lambda the block was
//! built from, lending each element the way a `CallIndirect` lends a heap
//! argument. The function-typed argument must be a closure block this
//! function built from a lifted lambda, so the lambda and its signature are
//! known; any other walls as before. A list argument the call is its last
//! reader of moves into the runtime body; any other is copied, so the caller
//! keeps its value either way.
//!
//! The site is recorded for the `native-calls` witness, which gives it a row
//! against a runtime block (`certificate::native_calls_witness_with`): every
//! heap argument is lent (mode 0), as the render hands it over.

use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy, NativeSigKind, NativeSigs};
use crate::render_native_closure as clo;
use crate::render_native_rec as rec;
use crate::{CallArg, MirFunction, Op, ValueId};
use almide_lang::ast::{AttrValue, Decl, TypeExpr};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

/// A declared type over the declaration's type params.
#[derive(Clone, Debug, PartialEq)]
enum DTy {
    Var(String),
    Int,
    Bool,
    Float,
    Str,
    Matrix,
    List(Box<DTy>),
    Opt(Box<DTy>),
    Fn(Vec<DTy>, Box<DTy>),
}

/// The runtime route of one declared combinator.
#[derive(Debug)]
pub(crate) struct Hof {
    /// The runtime body the declaration names.
    pub symbol: String,
    /// Each param's declared type and whether the runtime borrows it.
    params: Vec<(DTy, bool)>,
    ret: DTy,
    /// The runtime body and every runtime item it reaches.
    items: Vec<&'static str>,
}

fn dty_of(t: &TypeExpr, tps: &BTreeSet<String>) -> Option<DTy> {
    Some(match t {
        TypeExpr::Simple { name } => match name.as_str() {
            "Int" => DTy::Int,
            "Bool" => DTy::Bool,
            "Float" => DTy::Float,
            "String" => DTy::Str,
            "Matrix" => DTy::Matrix,
            a if tps.contains(a) => DTy::Var(a.to_string()),
            _ => return None,
        },
        TypeExpr::Generic { name, args } if args.len() == 1 => match name.as_str() {
            "List" => DTy::List(Box::new(dty_of(&args[0], tps)?)),
            "?" | "Option" => DTy::Opt(Box::new(dty_of(&args[0], tps)?)),
            _ => return None,
        },
        TypeExpr::Fn { params, ret, is_effect: false } => {
            DTy::Fn(params.iter().map(|p| dty_of(p, tps)).collect::<Option<_>>()?, Box::new(dty_of(ret, tps)?))
        }
        _ => return None,
    })
}

/// The runtime's Rust spelling of a declared type agrees with it (squashed);
/// `Some(by_ref)` when it does.
fn rust_agrees(d: &DTy, r: &str) -> Option<bool> {
    let by_ref = r.starts_with('&');
    let r = r.strip_prefix('&').unwrap_or(r);
    let ok = match d {
        DTy::Int => r == "i64",
        DTy::Bool => r == "bool",
        DTy::Float => r == "f64",
        DTy::Str => r == "str",
        DTy::Matrix => r == "AlmideMatrix",
        DTy::Var(_) => r.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !by_ref,
        DTy::List(_) => r.starts_with("Vec<") || (by_ref && r.starts_with('[')),
        DTy::Opt(_) => r.starts_with("Option<") && !by_ref,
        DTy::Fn(..) => r.starts_with("std::rc::Rc<dynFn(") && !by_ref,
    };
    ok.then_some(by_ref)
}

/// The registry call a linked impl fn serves.
fn registry_call(impl_fn: &str) -> Option<&'static str> {
    crate::mir_ops::self_host_runtime()
        .iter()
        .find_map(|(_, pairs)| pairs.iter().find(|(i, _)| *i == impl_fn).map(|(_, c)| *c))
}

fn derive(impl_fn: &str) -> Option<Hof> {
    derive_explained(impl_fn).ok()
}

/// [`derive`], with the step that refused.
fn derive_explained(impl_fn: &str) -> Result<Hof, String> {
    let call = registry_call(impl_fn).ok_or("no registry call")?;
    let target = crate::native_floor::specializes(call).unwrap_or_else(|| call.to_string());
    let (module, func) = target.split_once('.').ok_or("not a module.fn")?;
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached).ok_or("no module source")?;
    let mut decls = program.decls.iter().filter_map(|d| match d {
        Decl::Fn { name, effect, attrs, generics, params, return_type, .. } if name.as_str() == func => {
            Some((effect, attrs, generics, params, return_type))
        }
        _ => None,
    });
    let (effect, attrs, generics, params, ret) = decls.next().ok_or_else(|| format!("no declaration of `{target}`"))?;
    if decls.next().is_some() || effect.unwrap_or(false) {
        return Err("declared twice, or an effect fn".into());
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
    let tps: BTreeSet<String> = generics.iter().flatten().map(|g| g.name.as_str().to_string()).collect();
    let dparams: Vec<DTy> = params.iter().map(|p| dty_of(&p.ty, &tps)).collect::<Option<_>>().ok_or("a param type outside the map")?;
    if !dparams.iter().any(|p| matches!(p, DTy::Fn(..))) {
        return Err("no function-typed param".into());
    }
    let dret = dty_of(ret, &tps).ok_or("a return type outside the map")?;
    let (rust_params, rust_ret, items) = crate::native_floor::runtime_body(&symbol).ok_or_else(|| format!("`{symbol}` is no takeable runtime body"))?;
    if rust_params.len() != dparams.len() || rust_agrees(&dret, &rust_ret) != Some(false) {
        return Err(format!("runtime signature {rust_params:?} -> {rust_ret} disagrees"));
    }
    let params = dparams
        .into_iter()
        .zip(&rust_params)
        .map(|(d, r)| rust_agrees(&d, r).map(|b| (d, b)))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("runtime params {rust_params:?} disagree"))?;
    Ok(Hof { symbol, params, ret: dret, items })
}

/// The runtime route of the linked impl fn `impl_fn`, when the declaration
/// of the stdlib fn it implements gives one.
pub(crate) fn of(impl_fn: &str) -> Option<&'static Hof> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<&'static Hof>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(impl_fn) {
        return *hit;
    }
    let made = derive(impl_fn).map(|h| &*Box::leak(Box::new(h)));
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(impl_fn.to_string(), made);
    made
}

// ── per-render state ──

#[derive(Default)]
struct State {
    /// The function and op being rendered, and whether the caller hands an
    /// argument over at that op (a call-site move follows it).
    at: Option<(String, usize, bool)>,
    /// The lifted lambda each closure block of the current function was built from.
    origins: BTreeMap<ValueId, String>,
    /// The sites served here: (function, op index) → the runtime symbol.
    sites: BTreeMap<(String, usize), String>,
    /// The linked bodies the served sites left unreached, dropped for the
    /// final render: a call to one is served here or walls.
    pruned: BTreeSet<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Clear the per-render state (`pruned` names the linked bodies the final
/// render no longer carries).
pub(crate) fn reset(pruned: BTreeSet<String>) {
    STATE.with(|s| *s.borrow_mut() = State { pruned, ..State::default() });
}

/// The sites served so far.
pub(crate) fn sites() -> BTreeMap<(String, usize), String> {
    STATE.with(|s| s.borrow().sites.clone())
}

/// `name` is a linked body the final render dropped.
pub(crate) fn pruned(name: &str) -> bool {
    STATE.with(|s| s.borrow().pruned.contains(name))
}

/// The render is at op `i` of `func`.
pub(crate) fn at(func: &MirFunction, i: usize) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.at.as_ref().is_none_or(|(f, _, _)| *f != func.name) {
            s.origins = closure_origins(func);
        }
        let handed = func.call_moves.contains(&(i + 1));
        s.at = Some((func.name.clone(), i, handed));
    });
}

/// The closure blocks `func` builds from a lifted lambda: an all-scalar env
/// is the slot list `[FuncRef, 0, captures…]` (`materialize_closure_block`).
fn closure_origins(func: &MirFunction) -> BTreeMap<ValueId, String> {
    let refs: BTreeMap<ValueId, &str> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::FuncRef { dst, name } => Some((*dst, name.as_str())),
            _ => None,
        })
        .collect();
    if refs.is_empty() {
        return BTreeMap::new();
    }
    let mut count: BTreeMap<ValueId, usize> = BTreeMap::new();
    for op in &func.ops {
        let set = match op {
            Op::SetLocal { local, .. } => Some(*local),
            op => crate::mir_ops::defined_value(op),
        };
        if let Some(v) = set {
            *count.entry(v).or_default() += 1;
        }
    }
    let once = |v: ValueId| count.get(&v) == Some(&1);
    func.ops
        .iter()
        .filter_map(|op| match op {
            Op::ListLit { dst, elems } if once(*dst) => {
                let name = refs.get(elems.first()?)?;
                once(elems[0]).then(|| (*dst, name.to_string()))
            }
            _ => None,
        })
        .collect()
}

// ── rendering a served call ──

/// The call's type-param bindings, as owned native kinds.
type Env = BTreeMap<String, NTy>;

fn bind(env: &mut Env, v: &str, t: NTy) -> Option<()> {
    match env.get(v) {
        Some(b) if *b != t => None,
        Some(_) => Some(()),
        None => {
            env.insert(v.to_string(), t);
            Some(())
        }
    }
}

/// An element or payload of declared type `d` carried as `t`.
fn bind_inner(env: &mut Env, d: &DTy, t: NTy) -> Option<()> {
    match d {
        DTy::Var(v) => bind(env, v, t),
        DTy::Int | DTy::Bool => (t == NTy::I64).then_some(()),
        DTy::Str => (t == NTy::Str).then_some(()),
        _ => None,
    }
}

/// The element kind of a list the render carries.
fn list_elem(t: NTy) -> Option<NTy> {
    match t {
        NTy::Vec | NTy::VecRef => Some(NTy::I64),
        NTy::VecStr | NTy::VecStrRef => Some(NTy::Str),
        NTy::Rec(i) | NTy::RecRef(i) => rec::list_elem(i),
        _ => None,
    }
}

/// The payload kind of an Option the render carries.
fn opt_payload(t: NTy) -> Option<NTy> {
    match t {
        NTy::Opt => Some(NTy::I64),
        NTy::Rec(i) | NTy::RecRef(i) => rec::option_payload(i),
        _ => None,
    }
}

/// Bind the type params a non-function param of declared type `d` fixes.
fn bind_param(env: &mut Env, d: &DTy, t: NTy) -> Option<()> {
    match d {
        DTy::List(e) => bind_inner(env, e, list_elem(t)?),
        DTy::Opt(e) => bind_inner(env, e, opt_payload(t)?),
        DTy::Var(v) => bind(env, v, crate::render_native::owned_nty(t)),
        DTy::Matrix => rec::is_matrix(t).then_some(()),
        DTy::Int | DTy::Bool => (t == NTy::I64).then_some(()),
        DTy::Float => (t == NTy::F64).then_some(()),
        DTy::Str => matches!(t, NTy::Str | NTy::StrRef).then_some(()),
        DTy::Fn(..) => None,
    }
}

/// The native kind a declared type stands for under `env`.
fn kind_of(env: &Env, d: &DTy) -> Option<NTy> {
    Some(match d {
        DTy::Var(v) => *env.get(v)?,
        DTy::Int | DTy::Bool => NTy::I64,
        DTy::Float => NTy::F64,
        DTy::Str => NTy::Str,
        DTy::Matrix => NTy::Rec(rec::matrix_index()),
        // A list or Option of Float bits would leave the render's lanes
        // (`Vec<i64>`, `Option<i64>`) for the runtime's `f64`: refused.
        DTy::List(e) => match kind_of(env, e)? {
            NTy::I64 => NTy::Vec,
            NTy::Str => NTy::VecStr,
            NTy::F64 => return None,
            k => NTy::Rec(rec::list_type_of(k)),
        },
        DTy::Opt(e) => match kind_of(env, e)? {
            NTy::I64 => NTy::Opt,
            NTy::F64 => return None,
            k => NTy::Rec(rec::option_type_of(k)),
        },
        DTy::Fn(..) => return None,
    })
}

/// A declared closure param or result of declared type `d` and lambda kind
/// `k`: binds the type params it leaves unbound, else the kinds must agree.
/// `Some(true)` = a declared `Float` the lambda takes in its i64 lane (the
/// MIR carries a Float as its bits), so the adapter converts it.
fn bind_lambda(env: &mut Env, d: &DTy, k: NativeSigKind) -> Option<bool> {
    if *d == DTy::Float && k == NativeSigKind::I64 {
        return Some(true);
    }
    if let Some(t) = kind_of(env, d) {
        return (clo::arg_kind(t) == k).then_some(false);
    }
    let t = clo::owned_nty(k);
    match d {
        DTy::Var(v) => bind(env, v, t),
        DTy::List(e) => bind_inner(env, e, list_elem(t)?),
        DTy::Opt(e) => bind_inner(env, e, opt_payload(t)?),
        _ => None,
    }?;
    (kind_of(env, d).map(clo::arg_kind) == Some(k)).then_some(false)
}

/// A declared `Float` result read off a lambda that may return it as a real
/// `f64` or as its i64 bits (the lane the MIR carries a Float in): both are
/// the same Float.
const LANE_F64: &str = "trait AlmdLaneF64 { fn lane_f64(self) -> f64; }\nimpl AlmdLaneF64 for f64 { #[inline] fn lane_f64(self) -> f64 { self } }\nimpl AlmdLaneF64 for i64 { #[inline] fn lane_f64(self) -> f64 { f64::from_bits(self as u64) } }";

/// The adapter: an `Rc<dyn Fn>` that owns a copy of the env block and calls
/// the lifted lambda `lambda` (params of kinds `ps`) on each element it is
/// lent. `bits[i]`: that param is a runtime `f64` the lambda takes as its
/// bits. `float_ret`: the declared result is a `Float` (read through
/// [`LANE_F64`]); else the lambda's result kind `ret` is the runtime's.
fn adapter(lambda: &str, env_code: &str, ps: &[NativeSigKind], bits: &[bool], ret: NativeSigKind, float_ret: bool) -> Option<String> {
    if ps.iter().chain([&ret]).any(|k| matches!(k, NativeSigKind::Narrow { .. })) {
        return None;
    }
    let spell = |k: NativeSigKind, b: bool| if b { "f64".to_string() } else { rec::owned_ty(clo::owned_nty(k)) };
    let tys: Vec<String> = ps.iter().zip(bits).map(|(k, b)| spell(*k, *b)).collect();
    let params: Vec<String> = tys.iter().enumerate().map(|(i, t)| format!("__p{i}: {t}")).collect();
    let args: String = ps
        .iter()
        .zip(bits)
        .enumerate()
        .map(|(i, (k, b))| match k {
            _ if *b => format!(", __p{i}.to_bits() as i64"),
            NativeSigKind::Str | NativeSigKind::ListI64 | NativeSigKind::ListStr | NativeSigKind::Rec(_) => format!(", &__p{i}"),
            _ => format!(", __p{i}"),
        })
        .collect();
    let call = format!("{}(&__env{args})", crate::render_native::mangle(lambda));
    let (call, ret_ty) = if float_ret { (format!("AlmdLaneF64::lane_f64({call})"), "f64".to_string()) } else { (call, spell(ret, false)) };
    Some(format!(
        "{{ let __env: Vec<i64> = {env_code}.to_vec(); let __f: std::rc::Rc<dyn Fn({}) -> {ret_ty}> = std::rc::Rc::new(move |{}| {call}); __f }}",
        tys.join(", "),
        params.join(", ")
    ))
}

/// One non-function argument in the runtime's mode: borrowed as is, or owned
/// — moved when the call is its last reader, else copied.
fn runtime_arg(code: &str, got: NTy, by_ref: bool, dying: bool) -> String {
    let owned = matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) | NTy::Res | NTy::ResStr);
    match (by_ref, got) {
        (true, _) if owned => format!("&{code}"),
        (true, _) => code.to_string(),
        (false, NTy::I64 | NTy::F64 | NTy::Opt) => code.to_string(),
        (false, _) if owned && dying => crate::render_native_move::take_expr(code),
        (false, _) if owned => format!("{code}.clone()"),
        (false, NTy::StrRef) => format!("AlmideString::from({code})"),
        (false, NTy::VecRef | NTy::VecStrRef) => format!("{code}.to_vec()"),
        (false, _) => format!("{code}.clone()"),
    }
}

/// Render `call` (to the linked body `call.name`) through its runtime body.
/// `Ok(false)` = this route does not serve it (the caller's wall stands).
pub(crate) fn try_render(
    call: crate::render_native::NativeCall<'_>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    s: crate::render_native::OpSink<'_>,
) -> Result<bool, LowerError> {
    let Some(hof) = of(call.name) else { return Ok(false) };
    let Some((at, origins)) = STATE.with(|st| {
        let st = st.borrow();
        st.at.clone().filter(|a| !a.2).map(|a| (a, st.origins.clone()))
    }) else {
        return Ok(false);
    };
    let crate::render_native::OpSink { tys, out, indent, used_shims } = s;
    if call.args.len() != hof.params.len() {
        return Ok(false);
    }
    let mut got: Vec<(String, NTy)> = Vec::new();
    for a in call.args.iter() {
        match crate::render_native::call_arg(a, tys) {
            Ok(x) => got.push(x),
            Err(_) => return Ok(false),
        }
    }
    let mut env = Env::new();
    for ((d, _), (_, t)) in hof.params.iter().zip(&got) {
        if !matches!(d, DTy::Fn(..)) && bind_param(&mut env, d, *t).is_none() {
            return Ok(false);
        }
    }
    let mut rendered: Vec<String> = Vec::new();
    let mut shims: Vec<&'static str> = Vec::new();
    for (((d, by_ref), (code, t)), a) in hof.params.iter().zip(&got).zip(call.args.iter()) {
        let r = match d {
            DTy::Fn(dps, dret) => {
                let CallArg::Handle(blk) = a else { return Ok(false) };
                let Some(lambda) = origins.get(blk) else { return Ok(false) };
                // The lambda borrows every param, as its signature spells it.
                if !user_fns.get(lambda.as_str()).is_some_and(|l| l.owned_params.is_empty()) {
                    return Ok(false);
                }
                let Some((ps, Some(ret))) = clo::lambda_sig(sigs, lambda) else { return Ok(false) };
                if ps.len() != dps.len() || !matches!(t, NTy::Vec | NTy::VecRef) {
                    return Ok(false);
                }
                let mut bits = Vec::new();
                for (dp, k) in dps.iter().zip(&ps) {
                    match bind_lambda(&mut env, dp, *k) {
                        Some(b) => bits.push(b),
                        None => return Ok(false),
                    }
                }
                let float_ret = **dret == DTy::Float;
                if !float_ret && bind_lambda(&mut env, dret, ret) != Some(false) {
                    return Ok(false);
                }
                if float_ret {
                    shims.push(LANE_F64);
                }
                match adapter(lambda, code, &ps, &bits, ret, float_ret) {
                    Some(r) => r,
                    None => return Ok(false),
                }
            }
            _ => {
                let dying = matches!(a, CallArg::Handle(v) if crate::render_native_move::moving_arg(*v));
                runtime_arg(code, *t, *by_ref, dying)
            }
        };
        rendered.push(r);
    }
    let Some(ret) = kind_of(&env, &hof.ret) else { return Ok(false) };
    let expr = format!("{}({})", hof.symbol, rendered.join(", "));
    used_shims.push(crate::native_floor::FLOOR_HEADER);
    used_shims.extend(hof.items.iter().copied());
    used_shims.extend(shims);
    for _ in 0..indent {
        out.push_str("    ");
    }
    match call.dst {
        Some(d) => {
            tys.insert(*d, ret);
            out.push_str(&format!("let mut {}: {} = {expr};\n", var(*d), rec::owned_ty(ret)));
        }
        None => out.push_str(&format!("{expr};\n")),
    }
    STATE.with(|st| st.borrow_mut().sites.insert((at.0, at.1), hof.symbol.clone()));
    Ok(true)
}

/// A call to a linked body the final render dropped, served here or walled.
pub(crate) fn render_pruned(
    call: crate::render_native::NativeCall<'_>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    s: crate::render_native::OpSink<'_>,
) -> Result<(), LowerError> {
    if try_render(call, user_fns, sigs, s)? {
        Ok(())
    } else {
        Err(wall(format!("native: the runtime route of `{}` refused the call it served before", call.name)))
    }
}

/// Drop the linked bodies (`linked`) nothing reaches once the served `sites`
/// call the runtime instead: the roots are every other function, the edges
/// every call and fn reference but the served sites'. Returns the dropped names.
pub(crate) fn prune(
    functions: &mut Vec<MirFunction>,
    linked: &BTreeSet<String>,
    sites: &BTreeMap<(String, usize), String>,
) -> BTreeSet<String> {
    let by_name: BTreeMap<&str, &MirFunction> = functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let mut seen: BTreeSet<String> = functions.iter().filter(|f| !linked.contains(&f.name)).map(|f| f.name.clone()).collect();
    let mut todo: Vec<String> = seen.iter().cloned().collect();
    while let Some(n) = todo.pop() {
        let Some(f) = by_name.get(n.as_str()) else { continue };
        for (i, op) in f.ops.iter().enumerate() {
            let callee = match op {
                Op::CallFn { name, .. } if !sites.contains_key(&(n.clone(), i)) => name,
                Op::FuncRef { name, .. } => name,
                _ => continue,
            };
            if seen.insert(callee.clone()) {
                todo.push(callee.clone());
            }
        }
    }
    let dropped: BTreeSet<String> = functions.iter().filter(|f| !seen.contains(&f.name)).map(|f| f.name.clone()).collect();
    functions.retain(|f| seen.contains(&f.name));
    dropped
}

#[cfg(test)]
mod tests {
    /// Each linked combinator a native call can refuse has its runtime
    /// route, read off the declaration of the fn it implements.
    #[test]
    fn the_linked_combinators_have_their_runtime_route() {
        for (impl_fn, symbol) in [
            ("list_fold_str", "almide_rt_list_fold"),
            ("list_map_str", "almide_rt_list_map"),
            ("list_map", "almide_rt_list_map"),
            ("list_flat_map_str", "almide_rt_list_flat_map"),
            ("option_map", "almide_rt_option_map"),
            ("matrix_map", "almide_rt_matrix_map"),
        ] {
            let derived = super::derive_explained(impl_fn);
            assert_eq!(derived.as_ref().map(|h| h.symbol.as_str()), Ok(symbol), "{impl_fn}");
        }
    }

    /// A linked body with no function-typed param has none.
    #[test]
    fn a_body_without_a_function_param_has_no_route() {
        assert!(super::of("list_contains_str").is_none());
    }
}

