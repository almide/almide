//! A stdlib call served by its declared runtime
//! body when the linked self-hosted body cannot take the call (#4006, C1).
//! Calls without a function argument use the same declaration checks and
//! call witness; they invoke the runtime body without a closure adapter.
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
//! Declaration-backed routes are fallbacks of calls the linked body refuses.
//! Registered parallel fan.map sites instead require their selected runtime
//! before any sequential rendering (native_hof_parallel).
//!
//! The runtime body takes its closure as `Rc<dyn Fn>`; the render calls its
//! static-dispatch twin (`static_twin`: the same body text with that param
//! taken as `impl Fn`), so the closure costs no allocation and LLVM can
//! inline the call, as the standard codegen's fused iterator chain does.
//! The closure passed borrows a stack copy of the closure block's slots
//! (scalar captures only: the env is a plain slot list) and calls the lifted
//! lambda the block was built from, lending each element the way a
//! `CallIndirect` lends a heap argument. The function-typed argument must be
//! a closure block this function built from a lifted lambda, so the lambda
//! and its signature are known; any other walls as before. A list argument
//! the call is its last reader of moves into the runtime body; any other is
//! copied, so the caller keeps its value either way.
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
    Unit,
    Int,
    Bool,
    Float,
    Str,
    Matrix,
    Value,
    Bytes,
    Runtime(crate::native_runtime_types::RuntimeType),
    List(Box<DTy>),
    Tuple(Vec<DTy>),
    Opt(Box<DTy>),
    Result(Box<DTy>, Box<DTy>),
    /// `Map[K, V]`: v0's runtime `AlmideMap<K, V>`, the generated map alias.
    Map(Box<DTy>, Box<DTy>),
    Set(Box<DTy>),
    Fn(Vec<DTy>, Box<DTy>),
}

/// The runtime route of one declared combinator.
#[derive(Clone, Debug)]
pub(crate) struct Hof {
    /// The runtime body the declaration names.
    pub symbol: String,
    /// Each param's declared type and whether the runtime borrows it.
    params: Vec<(DTy, bool)>,
    /// Mutable loans come from the source declaration and must agree with
    /// the runtime's `&mut` signature. They never pass a copied receiver.
    mutable: Vec<bool>,
    /// Runtime slices whose elements dereference to the declared Matrix.
    matrix_ref_lists: Vec<bool>,
    ret: DTy,
    /// The runtime body's static-dispatch twin (`static_twin`) and every
    /// other runtime item the body reaches.
    items: Vec<&'static str>,
    /// The twin's name.
    pub twin: String,
    bounds: Vec<(String, almide_lang::capability_bounds::Cap)>,
    /// The `@reuse` declaration's route (#3707): the param its owned twin
    /// takes by value and that twin's static-dispatch name. A call that is
    /// the last reader of that argument hands it over instead of the
    /// borrowing body copying it into its result.
    owned: Option<(usize, String)>,
}

include!("native_hof_decl_types.rs");

/// The runtime's Rust spelling of a declared type agrees with it (squashed);
/// `Some(by_ref)` when it does.
fn rust_agrees(d: &DTy, r: &str) -> Option<bool> {
    let by_ref = r.starts_with('&');
    let r = r.strip_prefix('&').unwrap_or(r);
    let ok = scalar_agrees(d, r, by_ref).unwrap_or_else(|| composite_agrees(d, r, by_ref));
    ok.then_some(by_ref)
}

/// [`rust_agrees`] for the leaf types; `None` for a composite.
fn scalar_agrees(d: &DTy, r: &str, by_ref: bool) -> Option<bool> {
    Some(match d {
        DTy::Unit => (r == "()" || r.is_empty()) && !by_ref,
        DTy::Int => r == "i64",
        DTy::Bool => r == "bool",
        DTy::Float => r == "f64",
        DTy::Str => (by_ref && r == "str") || (!by_ref && r == "AlmideString"),
        DTy::Matrix => r == "AlmideMatrix",
        DTy::Value => r == "AlmideValue",
        DTy::Bytes => r == "Vec<u8>" || (by_ref && r == "[u8]"),
        DTy::Runtime(runtime) => r == runtime.rust(),
        DTy::Var(_) => r.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !by_ref,
        _ => return None,
    })
}

/// [`rust_agrees`] for the composite types (every leaf is [`scalar_agrees`]'s).
fn composite_agrees(d: &DTy, r: &str, by_ref: bool) -> bool {
    match d {
        DTy::Tuple(_) => r.starts_with('(') && r.ends_with(')') && !by_ref,
        // A runtime generic standing for a concrete declared element
        // (`fn f<M: Deref<Target = AlmideMatrix>>(xs: &[M])`) carries a bound
        // the native element kind need not meet: not this route's.
        DTy::List(e) => {
            let inner = r.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).or_else(|| r.strip_prefix('[').and_then(|x| x.strip_suffix(']')));
            let bare_param = inner.is_some_and(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_uppercase()));
            (r.starts_with("Vec<") || (by_ref && r.starts_with('['))) && !(bare_param && !matches!(**e, DTy::Var(_)))
        }
        DTy::Opt(_) => r.starts_with("Option<"),
        DTy::Map(..) => r.starts_with("AlmideMap<"),
        DTy::Set(_) => r.starts_with("AlmideSet<"),
        DTy::Result(..) => r.starts_with("Result<"),
        DTy::Fn(..) => r.starts_with("std::rc::Rc<dynFn(") && !by_ref,
        _ => false,
    }
}

/// The registry call a linked impl fn serves.
fn registry_call(impl_fn: &str) -> Option<String> {
    crate::mir_ops::self_host_runtime()
        .iter()
        .find_map(|(_, pairs)| pairs.iter().find(|(i, _)| *i == impl_fn).map(|(_, c)| c.to_string()))
        .or_else(|| crate::host_ops::host_op_for_call_name(impl_fn)
            .map(|h| format!("{}.{}", h.module, h.func)))
}

fn derive(impl_fn: &str) -> Option<Hof> {
    derive_explained(impl_fn).ok()
}

// [`derive`], with the step that refused.
include!("native_hof_derive.rs");

include!("native_hof_static.rs");
include!("native_hof_generic.rs");
include!("native_hof_parallel.rs");
include!("native_hof_specialize.rs");
include!("native_hof_matrix.rs");

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

/// Whether every runtime body a declared route of `call` may serve takes its
/// param `k` by value, plainly (not a tuple list it converts, not beside a
/// mutable loan it snapshots for): an argument handed there at its last use
/// moves in (`runtime_arg`'s dying arm) where a borrowed one is copied. A
/// site the render serves through the linked body instead lends it.
pub(crate) fn takes_by_value(call: &str, k: usize) -> bool {
    // The call names the linked body (`list_enumerate`) or its source call.
    let hofs: Vec<&Hof> = crate::mir_ops::self_host_runtime()
        .iter()
        .flat_map(|(_, pairs)| pairs.iter())
        .filter(|(implementation, source)| *source == call || *implementation == call)
        .filter_map(|(implementation, _)| of(implementation))
        .collect();
    // A body with a callback is served only when the callback's types fit,
    // which the render decides; one the linked body serves instead lends,
    // and taken over there the caller would copy for nothing.
    !hofs.is_empty()
        && hofs.iter().all(|h| {
            !h.mutable.iter().any(|m| *m)
                && !h.params.iter().any(|(d, _)| matches!(d, DTy::Fn(..)))
                && matches!(h.params.get(k), Some((d, false)) if !matches!(d, DTy::Fn(..)) && !has_runtime_tuple(d))
        })
}

/// A declaration-backed generic route for a source call. The linker will
/// still resolve its registered body; every actual call is type-checked by
/// `plan` before that body can be replaced by the runtime implementation.
pub(crate) fn declared_call(call: &str) -> bool {
    crate::mir_ops::self_host_runtime().iter().any(|(_, pairs)| {
        pairs.iter().any(|(implementation, source)| *source == call && of(implementation).is_some())
    }) || (crate::host_ops::host_op_for_call_name(call).is_some() && of(call).is_some())
}

// ── per-render state ──

#[derive(Default)]
struct State {
    /// The function and op being rendered, and whether the caller hands an
    /// argument over at that op (a call-site move follows it).
    at: Option<(String, usize, bool)>,
    /// The lifted lambda each closure block of the current function was built from.
    origins: BTreeMap<ValueId, (String, Vec<ValueId>)>,
    /// The sites served here: (function, op index) → the runtime symbol.
    sites: BTreeMap<(String, usize), String>,
    /// The linked bodies the served sites left unreached, dropped for the
    /// final render: a call to one is served here or walls.
    pruned: BTreeSet<String>,
    /// The closure blocks only served sites read (`elidable`): their slots
    /// reach the call as its stack env, so the block itself is not built.
    elided: BTreeSet<(String, ValueId)>,
    /// The lambdas a served `list.flat_map` calls through their array twin
    /// (`array_twins`): each one's literal arity and result value.
    array_twins: BTreeMap<String, (usize, ValueId)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Clear the per-render state (`pruned` names the linked bodies the final
/// render no longer carries).
pub(crate) fn reset(pruned: BTreeSet<String>, elided: BTreeSet<(String, ValueId)>) {
    STATE.with(|s| *s.borrow_mut() = State { pruned, elided, ..State::default() });
}

/// Capability graph projection of the exact selected runtime sites. Host
/// calls use their declared capability row even after their linked body is
/// pruned. FuncRef edges are unchanged, so callback effects remain reachable.
pub(crate) fn capability_program(functions: Vec<MirFunction>) -> BTreeMap<String, MirFunction> {
    let sites = sites();
    functions.into_iter().map(|mut f| {
        for (at, op) in f.ops.iter_mut().enumerate() {
            let Some(symbol) = sites.get(&(f.name.clone(), at)) else { continue };
            let Some(host) = crate::host_ops::HOST_OPS.iter().find(|o| o.symbol == symbol) else { continue };
            if let Op::CallFn { name, .. } = op { *name = format!("{}.{}", host.module, host.func); }
        }
        (f.name.clone(), f)
    }).collect()
}

/// `op` of `func` builds a closure block the final render does not build.
pub(crate) fn elided(func: &MirFunction, op: &Op) -> bool {
    let Op::ListLit { dst, .. } = op else { return false };
    STATE.with(|s| s.borrow().elided.contains(&(func.name.clone(), *dst)))
}

/// The closure blocks of `functions` that nothing but the served `sites`
/// reads (its releases aside, which erase to scope end on this render).
pub(crate) fn elidable(functions: &[MirFunction], sites: &BTreeMap<(String, usize), String>) -> BTreeSet<(String, ValueId)> {
    let mut out = BTreeSet::new();
    for f in functions {
        if !sites.keys().any(|(n, _)| *n == f.name) {
            continue;
        }
        for blk in closure_origins(f).keys() {
            if f.ret == Some(*blk) || f.params.iter().any(|p| p.value == *blk) {
                continue;
            }
            let only_served = f.ops.iter().enumerate().all(|(i, op)| {
                let mut reads = Vec::new();
                crate::mir_ops::op_reads(op, &mut reads);
                !reads.contains(blk)
                    || crate::render_native_move::bookkeeping(op)
                    || (matches!(op, Op::CallFn { .. }) && sites.contains_key(&(f.name.clone(), i)))
            });
            if only_served {
                out.insert((f.name.clone(), *blk));
            }
        }
    }
    out
}

/// The sites served so far.
pub(crate) fn sites() -> BTreeMap<(String, usize), String> {
    STATE.with(|s| s.borrow().sites.clone())
}

/// `name` is a linked body the final render dropped.
pub(crate) fn pruned(name: &str) -> bool {
    STATE.with(|s| s.borrow().pruned.contains(name))
}

/// The lifted lambda the closure block `v` of the function being rendered
/// was built from.
pub(crate) fn lambda_of(v: ValueId) -> Option<String> {
    STATE.with(|s| s.borrow().origins.get(&v).map(|(name, _)| name.clone()))
}

/// The function being rendered.
pub(crate) fn current_fn() -> Option<String> {
    STATE.with(|s| s.borrow().at.as_ref().map(|(f, _, _)| f.clone()))
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
fn closure_origins(func: &MirFunction) -> BTreeMap<ValueId, (String, Vec<ValueId>)> {
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
    let mut out: BTreeMap<_, _> = func.ops
        .iter()
        .filter_map(|op| match op {
            Op::ListLit { dst, elems } if once(*dst) => {
                let name = refs.get(elems.first()?)?;
                once(elems[0]).then(|| (*dst, (name.to_string(), elems.clone())))
            }
            _ => None,
        })
        .collect();
    let (_, _, bases) = crate::render_native_addr::block_bases(func);
    for op in &func.ops {
        let Op::Prim { kind: crate::PrimKind::Store { width: 8 }, args, .. } = op else { continue };
        let [addr, rhs] = args.as_slice() else { continue };
        let Some((block, Some(off))) = bases.get(addr) else { continue };
        let Some(name) = refs.get(rhs) else { continue };
        let stores = func.ops.iter().filter(|op| matches!(op, Op::Prim { kind: crate::PrimKind::Store { .. }, args, .. }
            if args.first().and_then(|a| bases.get(a)) == Some(&(*block, Some(*off))))).count();
        if *off == i64::from(crate::lower::layout::BLOCK_HEADER) && once(*block) && once(*rhs) && stores == 1 {
            out.insert(*block, (name.to_string(), Vec::new()));
        }
    }
    out
}

// ── rendering a served call ──

include!("native_hof_types.rs");
include!("native_hof_mut.rs");

/// A declared `Float` result read off a lambda that may return it as a real
/// `f64` or as its i64 bits (the lane the MIR carries a Float in): both are
/// the same Float.
const LANE_F64: &str = "trait AlmdLaneF64 { fn lane_f64(self) -> f64; }\nimpl AlmdLaneF64 for f64 { #[inline] fn lane_f64(self) -> f64 { self } }\nimpl AlmdLaneF64 for i64 { #[inline] fn lane_f64(self) -> f64 { f64::from_bits(self as u64) } }";

/// The adapter: a closure, borrowing the env array `env` (a copy of the
/// closure block's scalar slots), that calls the lifted lambda `lambda`
/// (params of kinds `ps`) on each element it is lent. `bits[i]`: how that
/// param crosses into the lambda's lane ([`Lane`]). `ret_lane`: a declared
/// `Float` result is read through [`LANE_F64`], a declared `Bool` one is
/// tested `!= 0`; else the lambda's result kind `ret` is the runtime's.
fn adapter(lambda: Option<&str>, env: &str, (ps, moved): (&[NativeSigKind], &[bool]), bits: &[Lane], (ret, ret_lane): (NativeSigKind, Lane)) -> Option<String> {
    if ps.iter().chain([&ret]).any(|k| matches!(k, NativeSigKind::Narrow { .. })) {
        return None;
    }
    let spell = |k: NativeSigKind, b: Lane| match b {
        Lane::F64 => "f64".to_string(),
        Lane::Bool => "bool".to_string(),
        Lane::Same => rec::owned_ty(clo::owned_nty(k)),
    };
    let params: Vec<String> = ps.iter().zip(bits).enumerate().map(|(i, (k, b))| format!("__p{i}: {}", spell(*k, *b))).collect();
    let args: String = ps
        .iter()
        .zip(bits)
        .enumerate()
        .map(|(i, (k, b))| match (k, b) {
            (_, Lane::F64) => format!(", __p{i}.to_bits() as i64"),
            (_, Lane::Bool) => format!(", __p{i} as i64"),
            (NativeSigKind::Str | NativeSigKind::ListI64 | NativeSigKind::ListStr | NativeSigKind::Rec(_), _)
                if !moved.get(i).copied().unwrap_or(false) => format!(", &__p{i}"),
            _ => format!(", __p{i}"),
        })
        .collect();
    let call = format!("{}{args})", callback_head(lambda, env, ps, Some(ret)));
    let (call, ret_ty) = match ret_lane {
        Lane::F64 => (format!("AlmdLaneF64::lane_f64({call})"), "f64".to_string()),
        Lane::Bool => (format!("({call}) != 0"), "bool".to_string()),
        Lane::Same => (call, spell(ret, Lane::Same)),
    };
    Some(format!("|{}| -> {ret_ty} {{ {call} }}", params.join(", ")))
}

/// How a closure param or result the runtime spells crosses into the
/// lambda's i64 lane: as is, a runtime `f64` taken as its bits, or a runtime
/// `bool` taken as 0/1.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Same,
    F64,
    Bool,
}

/// One non-function argument in the runtime's mode: borrowed as is, or owned
/// — moved when the call is its last reader, else copied.
fn runtime_arg(code: &str, got: NTy, by_ref: bool, dying: bool) -> String {
    let owned = matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_) | NTy::Res | NTy::ResStr);
    match (by_ref, got) {
        (true, _) if owned || matches!(got, NTy::I64 | NTy::F64 | NTy::Opt) => format!("&{code}"),
        (true, _) => code.to_string(),
        (false, NTy::I64 | NTy::F64 | NTy::Opt) => code.to_string(),
        (false, _) if owned && dying => crate::render_native_move::take_expr(code),
        (false, _) if owned => format!("{code}.clone()"),
        (false, NTy::StrRef) => format!("AlmideString::from({code})"),
        (false, NTy::VecRef | NTy::VecStrRef) => format!("{code}.to_vec()"),
        (false, _) => format!("{code}.clone()"),
    }
}

/// The call's argument codes and kinds; a closure block this render does
/// not build stands as a block (its slots are the env). `None` = an argument
/// the render cannot spell.
fn call_args(call: &crate::render_native::NativeCall<'_>, fname: &str, tys: &BTreeMap<ValueId, NTy>) -> Option<Vec<(String, NTy)>> {
    call.args
        .iter()
        .map(|a| match a {
            CallArg::Handle(v) if STATE.with(|st| st.borrow().elided.contains(&(fname.to_string(), *v))) => Some((var(*v), NTy::Vec)),
            a => crate::render_native::call_arg(a, tys).ok(),
        })
        .collect()
}

include!("native_hof_callbacks.rs");

/// Render `call` (to the linked body `call.name`) through its runtime body.
/// `Ok(false)` = this route does not serve it (the caller's wall stands).
pub(crate) fn try_render(
    call: crate::render_native::NativeCall<'_>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    s: crate::render_native::OpSink<'_>,
) -> Result<bool, LowerError> {
    let crate::render_native::OpSink { tys, out, indent, used_shims } = s;
    let Some((hof, at, served)) = plan(&call, user_fns, sigs, tys) else { return Ok(false) };
    let (expr, ret, prelude, shims, reads) = served;
    for slot in reads {
        crate::render_native_bounds::check(slot);
        crate::render_native_bounds::access(slot);
    }
    if almide_codegen::generated::runtime_fn_modes::crosses_threads(&hof.symbol) { crate::render_native::require_fan_runtime(); }
    used_shims.push(crate::native_floor::floor_header());
    used_shims.extend(hof.items.iter().copied());
    used_shims.extend(shims);
    let pad = "    ".repeat(indent);
    for line in &prelude {
        out.push_str(&format!("{pad}{line}\n"));
    }
    match call.dst {
        Some(d) => {
            tys.insert(*d, ret);
            out.push_str(&format!("{pad}let mut {}: {} = {expr};\n", var(*d), rec::owned_ty(ret)));
        }
        None => out.push_str(&format!("{pad}{expr};\n")),
    }
    STATE.with(|st| st.borrow_mut().sites.insert(at, hof.symbol.clone()));
    Ok(true)
}

/// A served call: the expression, its result kind, the env bindings before
/// it, and the shims it needs.
type Served = (String, NTy, Vec<String>, Vec<&'static str>, Vec<crate::render_native_bounds::Slot>);

/// The served rendering of `call`, or `None` when this route does not serve it.
fn plan(
    call: &crate::render_native::NativeCall<'_>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    tys: &BTreeMap<ValueId, NTy>,
) -> Option<(&'static Hof, (String, usize), Served)> {
    let hof = if parallel_selected(call.name) { parallel_hof()? } else { of(call.name)? };
    let (at, origins) = STATE.with(|st| {
        let st = st.borrow();
        st.at.clone().filter(|a| !a.2).map(|a| (a, st.origins.clone()))
    })?;
    let hof = host_callback_route(hof, call.args, &origins, user_fns)?;
    if call.args.len() != hof.params.len() {
        return None;
    }
    let got = call_args(call, &at.0, tys)?;
    crate::trace::trace_for("ALMIDE_DBG_LOWER_FN", &at.0, || format!("HOF {}:{} {} got={got:?} result={:?} params={:?}", at.0, at.1, call.name, declared_result(call, user_fns), hof.params));
    let mut env = Env::new();
    // Containers establish their element representation before a standalone
    // generic value: a Float list stores bits while its scalar argument is f64.
    for vars in [false, true] {
      for ((d, _), (_, t)) in hof.params.iter().zip(&got) {
        if !matches!(d, DTy::Fn(..)) && matches!(d, DTy::Var(_)) == vars {
            if generic_scalar_lane(&env, d, *t, &hof.bounds).is_some() { continue; }
            if has_value_conversion(d) { bind_callback_shape(&mut env, d, *t)?; }
            else { bind_param(&mut env, d, *t)?; }
        }
      }
    }
    // The checked result constrains callback-only generics before choosing
    // an adapter. Map Float values use an i64 bit carrier: without the
    // result constraint a callback's f64 lane selects the wrong conversion.
    let declared_return = declared_result(call, user_fns);
    if let Some(want) = declared_return {
        if has_value_conversion(&hof.ret) { bind_callback_shape(&mut env, &hof.ret, want)?; }
        else { bind_param(&mut env, &hof.ret, want)?; }
    }
    // Tuple slots erase scalar source types to lanes. A runtime comparison
    // bound must not silently compare Float bits as Int keys. Until source
    // key types accompany this adapter, only String-bound keys are admitted.
    let tuple_input = hof.params.iter().any(|(d, _)| !matches!(d, DTy::Fn(..)) && has_runtime_tuple(d));
    if tuple_input && hof.bounds.iter().any(|(v, _)| env.get(v) != Some(&NTy::Str)) { return None; }
    // Admit ordering only when both the source declaration and generated
    // representation preserve it, including recursively declared variants.
    if hof.bounds.iter().any(|(v, cap)| *cap == almide_lang::capability_bounds::Cap::Ord && env.get(v).and_then(|k| k.rec()).is_some_and(|i| !rec::native_order_safe(i))) {
        return None;
    }
    let cx = FnArgCx { user_fns, sigs, tys, origins: &origins, at: at.1, bounds: &hof.bounds };
    let (mut rendered, mut side) = (Vec::new(), FnArgOut::default());
    let mut twin = hof.twin.as_str();
    for (k, (((d, by_ref), (code, t)), a)) in hof.params.iter().zip(&got).zip(call.args.iter()).enumerate() {
        if hof.matrix_ref_lists[k] {
            rendered.push(format!("&{code}.iter().collect::<Vec<_>>()"));
            continue;
        }
        if hof.mutable[k] {
            if has_value_conversion(d) || generic_scalar_lane(&env, d, *t, &hof.bounds).is_some() { return None; }
            rendered.push(mutable_place_arg(a, code, *t, tys)?);
            continue;
        }
        if let Some(owned) = handed_over(hof, k, a, *t) {
            twin = owned;
            rendered.push(runtime_arg(code, *t, false, true));
            continue;
        } else if let Some(want) = generic_scalar_lane(&env, d, *t, &hof.bounds) {
            let value = crate::native_value_repr::scalar_recode(code, *t, want)?;
            rendered.push(if *by_ref { format!("&({value})") } else { value });
            continue;
        }
        rendered.push(match d {
            DTy::Fn(dps, dret) => fn_arg(&cx, &mut env, (dps, dret), (a, *t), &mut side)?,
            DTy::Unit => "()".to_string(),
            // The render carries a Bool in its i64 lane; the runtime takes `bool`.
            DTy::Bool => format!("({code} != 0)"),
            d if has_value_conversion(d) => {
                let owned = runtime_arg(code, *t, false, false);
                let value = convert_callback_value(&env, d, crate::render_native::owned_nty(*t), &owned, false)?;
                if *by_ref {
                    let snapshot = format!("__tuple_arg_{}_{}", at.1, k);
                    side.prelude.push(format!("let {snapshot} = {value};"));
                    format!("&{snapshot}")
                } else { value }
            }
            _ => value_arg(hof, (at.1, k), (a, code.as_str(), *t), &mut side),
        });
    }
    if declared_return.is_none() && kind_of(&env, &hof.ret).is_none() {
        let ret = sigs.get(call.name)?.1?;
        bind_param(&mut env, &hof.ret, clo::owned_nty(ret))?;
    }
    let ret = declared_return.or_else(|| kind_of(&env, &hof.ret))?;
    if almide_codegen::generated::runtime_fn_modes::crosses_threads(&hof.symbol) { side.prelude.append(&mut side.share); }
    if hof.symbol == "almide_rt_list_flat_map" && twin == hof.twin {
        twin = flat_map_array_route(call, (&origins, user_fns), &mut rendered, &mut side).unwrap_or(twin);
    }
    let expr = format!("{}({})", twin, rendered.join(", "));
    let expr = runtime_result_as(&env, &hof.ret, &expr, ret)?;
    Some((hof, (at.0, at.1), (expr, ret, side.prelude, side.shims, side.reads)))
}

include!("native_hof_owned.rs");

/// [`plan`]'s rendering of the plain value argument `k` of the call at op
/// `at` (`a`, rendered as `code`, of kind `t`).
fn value_arg(hof: &Hof, (at, k): (usize, usize), (a, code, t): (&CallArg, &str, NTy), side: &mut FnArgOut) -> String {
    let by_ref = hof.params[k].1;
    if hof.mutable.iter().any(|m| *m) {
        // Evaluate the other values before lending the receiver
        // exclusively, including an argument that reads its alias.
        let snapshot = format!("__arg_{}_{}", at, k);
        let owned = crate::render_native::owned_nty(t);
        let value = runtime_arg(code, t, false, false);
        side.prelude.push(format!("let {snapshot} = {value};"));
        return if by_ref { runtime_arg(&snapshot, owned, true, false) } else { snapshot };
    }
    // A borrowed payload whose carrier dies at the call: the
    // payload itself, moved out, where the param takes it by value.
    let taken = match a {
        CallArg::Handle(v) if !by_ref => crate::render_native_move::taken_arg(*v).and_then(|take| {
            let owned = crate::render_native::owned_nty(t);
            (owned != t).then(|| (format!("{{ {take} }}"), owned))
        }),
        _ => None,
    };
    match taken {
        Some((take, owned)) => runtime_arg(&take, owned, false, true),
        None => {
            let dying = matches!(a, CallArg::Handle(v) if crate::render_native_move::moving_arg(*v));
            runtime_arg(code, t, by_ref, dying)
        }
    }
}

/// A call the linked body `call.name` refused with `refused`: served here,
/// or that refusal stands.
pub(crate) fn serve_or(
    refused: LowerError,
    call: crate::render_native::NativeCall<'_>,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    s: crate::render_native::OpSink<'_>,
) -> Result<(), LowerError> {
    if try_render(call, user_fns, sigs, s)? {
        Ok(())
    } else {
        Err(refused)
    }
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
            ("list_flat_map_h", "almide_rt_list_flat_map"),
            ("option_map", "almide_rt_option_map"),
            ("matrix_map", "almide_rt_matrix_map"),
            ("set_filter", "almide_rt_set_filter"),
            ("set_filter_str", "almide_rt_set_filter"),
            ("set_map", "almide_rt_set_map"),
            ("set_fold", "almide_rt_set_fold"),
            ("set_fold_str", "almide_rt_set_fold"),
            ("set_any", "almide_rt_set_any"),
            ("set_all", "almide_rt_set_all"),
            ("bytes_split", "almide_rt_bytes_split"),
            ("float_to_string", "almide_rt_float_to_string"),
        ] {
            let derived = super::derive_explained(impl_fn);
            assert_eq!(derived.as_ref().map(|h| h.symbol.as_str()), Ok(symbol), "{impl_fn}");
        }
    }

    /// A linked body with no function-typed param has the route its
    /// declaration names, called as it is (no static twin).
    #[test]
    fn a_body_without_a_function_param_calls_its_runtime_body_as_it_is() {
        let derived = super::derive_explained("list_contains_str");
        assert_eq!(derived.as_ref().map(|h| (h.symbol.as_str(), h.twin.as_str())), Ok(("almide_rt_list_contains", "almide_rt_list_contains")));
    }
}


include!("native_hof_result.rs");

include!("native_hof_bounds.rs");
include!("native_hof_tuple.rs");

fn declared_result(call: &crate::render_native::NativeCall<'_>, user_fns: &BTreeMap<&str, &MirFunction>) -> Option<NTy> {
    let fname = STATE.with(|s| s.borrow().at.as_ref().map(|a| a.0.clone()))?;
    let key = user_fns.get(fname.as_str())?.aggregate_blocks.get(&(*call.dst)?)?;
    rec::declared_kind(key)
}

/// Refuse a representation-specialized library body whose declared result
/// disagrees with the source call. Its declaration-backed runtime adapter
/// can then instantiate the real result type, including empty constructors.
pub(crate) fn check_result(call: &crate::render_native::NativeCall<'_>, user_fns: &BTreeMap<&str, &MirFunction>, sigs: &NativeSigs) -> Result<(), LowerError> {
    if of(call.name).is_none() { return Ok(()); }
    if let (Some(want), Some((_, Some(got)))) = (declared_result(call, user_fns), sigs.get(call.name)) {
        if !crate::native_value_repr::same_owned(want, clo::owned_nty(*got)) {
            return Err(wall(format!("native: linked result of `{}` differs from its checked call type", call.name)));
        }
    }
    Ok(())
}
