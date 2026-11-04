//! The native runtime floor, derived from declarations (#4006, family D).
//!
//! A stdlib call joins the verified native render's floor when its
//! DECLARATION says how native code runs it: the bundled `.almd` source
//! declares the fn `@intrinsic("almide_rt_X")`, and `almide_rt_X` is the v0
//! native runtime's own body (runtime/rs, embedded by almide-codegen). The
//! render calls that body verbatim, with the items it reaches inside the
//! runtime, so the native output is v0's by construction, and v0 is the leg
//! the cross-target corpus already holds byte-equal to wasm. Nothing here
//! names a stdlib function: membership is read off the declaration, the
//! argument and result conversions off the declared Almide types (one rule
//! per TYPE), and the body off the runtime source.
//!
//! What stays out walls honestly, as before:
//! - a fn with an Almide body (no `@intrinsic`), an `effect fn`, or one whose
//!   declared types fall outside the per-type map below;
//! - a runtime body whose Rust signature disagrees with that map;
//! - a runtime body that reaches an item the native header does not carry: an
//!   name the runtime or the v0 prelude defines (a type, a macro, a module, a
//!   prelude fn) that is not an item it can take along (a top-level `fn` /
//!   `const` / `static` defined once, with no `#[cfg]`). A name neither
//!   defines is Rust's own. The resolver is lexical and conservative: what it
//!   cannot place, it refuses. `the_declared_native_floor_compiles`
//!   (tests/native_floor_compile_test.rs) compiles every admitted body
//!   together under the native header, so a body the resolver let through
//!   that rustc would reject fails a test, not a user's build.

use almide_lang::ast::{AttrValue, Decl, TypeExpr};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

/// The runtime items the native header itself defines for a floor body: the
/// §13 abort (`Error: <msg>` on stderr, exit 1), the v0 prelude's
/// `almide_abort` / `almide_abort_oom` with the render's own stdout (std's
/// line-buffered handle, nothing of ours to flush first).
pub(crate) const FLOOR_HEADER: &str = "fn almide_abort(msg: impl std::fmt::Display) -> ! { eprintln!(\"Error: {}\", msg); std::process::exit(1) }\nfn almide_abort_oom() -> ! { almide_abort(\"out of memory\") }";

/// A declared parameter's native kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloorParam {
    /// `Int` — the runtime takes `i64`.
    Int,
    /// `Bool` — the runtime takes `bool`; the render's i64 tests `!= 0`.
    Bool,
    /// `Float` — the runtime takes `f64`.
    Float,
    /// `String` — the runtime borrows `&str`.
    Str,
    /// `List[E]` — the runtime borrows a slice (`&[T]` / `&Vec<T>`).
    List(ListElem),
    /// `Bytes` — the runtime borrows its `Vec<u8>` (`&Vec<u8>` / `&[u8]`).
    Bytes,
    /// The stdlib `Value` — v0's runtime `AlmideValue`, borrowed (`true`)
    /// or by value (`false`).
    Value(bool),
    /// A bare type parameter `A` that is the element of a `List[A]` param:
    /// the runtime takes the element by value (`T`, false) or by reference
    /// (`&T`, true), and the call site's list decides its kind.
    Elem(bool),
    /// `Map[K, V]` — v0's runtime `AlmideMap`, borrowed (`true`) or by value
    /// (`false`; the caller's map is moved in when the call is its last read).
    Map(bool),
    /// The key `K` / the value `V` of the call's `Map[K, V]` param (by
    /// reference when `true`); the map's kinds decide the native kind.
    MapKey(bool),
    MapVal(bool),
    /// `List[(K, V)]` — the runtime takes a `Vec<(K, V)>` of Rust pairs.
    MapPairs,
}

/// A declared list's element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListElem {
    Int,
    Str,
    /// `List[Value]` — a slice of the runtime `AlmideValue`.
    Value,
    /// A type parameter of the declaration: any list the render carries.
    Any,
}

/// A declared result's native kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloorRet {
    Unit,
    Int,
    /// `Bool` from a runtime `bool` (`as i64`) or `i64`.
    Bool,
    Float,
    Str,
    /// `Result[Int | Bool | Float | Unit, String]` — the native Result
    /// carrier (`None`: a Unit Ok, carried as 0).
    Res(Option<Scalar>),
    /// `Result[String, String]` — the String-Ok carrier.
    ResStr,
    /// `List[Int]` — an owned `Vec<i64>`.
    ListInt,
    /// `List[String]` — a runtime `Vec<String>`, rebound as `Vec<AlmideString>`.
    ListStr,
    /// `Bytes` — the runtime's own `Vec<u8>`.
    Bytes,
    /// The stdlib `Value` — the runtime's own `AlmideValue`.
    Value,
    /// `Result[Value, String]` — the generated `Result<AlmideValue,
    /// AlmideString>` carrier.
    ResValue,
    /// `String?` — a runtime `Option<String>`, carried as the generated
    /// `Option<AlmideString>`.
    OptStr,
    /// `Result[List[String], String]` — the generated carrier of a
    /// `Vec<AlmideString>`.
    ResListStr,
    /// `Result[Bytes, String]` — the generated carrier of the runtime `Vec<u8>`.
    ResBytes,
    /// `Result[String?, String]` — the generated carrier of an
    /// `Option<AlmideString>`.
    ResOptStr,
    /// `Int? | Bool? | Float?` — a runtime `Option` of the scalar, carried as
    /// the native `Option<i64>` (`NTy::Opt`; a Float as its bits).
    Opt(Scalar),
    /// A return typed by the element `A` of the call's `List[A]` param: `A`,
    /// `A?` or `List[A]` (the call site's list decides the kind).
    Elem(ElemRet),
    /// `Map[K, V]` — the generated map alias of the call's (key, value) kinds.
    Map,
    /// `V?` / `List[K]` / `List[V]` of the call's map.
    MapOptVal,
    MapKeys,
    MapVals,
}

/// A scalar kind a runtime value converts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scalar {
    Int,
    Bool,
    Float,
}

/// The shape of an element-typed return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElemRet {
    /// `A`.
    Value,
    /// `A?`.
    Opt,
    /// `List[A]`.
    List,
}

/// One declared floor function.
#[derive(Debug)]
pub(crate) struct FloorFn {
    /// The runtime symbol the declaration names.
    pub symbol: String,
    pub params: Vec<FloorParam>,
    pub ret: FloorRet,
    /// The runtime body and every runtime item it reaches, each a whole item.
    pub items: Vec<&'static str>,
    /// The `_owned` twin the stdlib's `@reuse` declaration gives (the param
    /// it takes by value, its symbol, its items): a call whose list source
    /// is not read again moves it in, as v0 does (crates/almide-codegen
    /// owned_source.rs).
    pub twin: Option<(usize, String, Vec<&'static str>)>,
}

/// The floor entry the declaration of stdlib call `call` (`module.fn`) gives,
/// or `None` when it gives none (see the module doc for what walls).
pub(crate) fn declared(call: &str) -> Option<&'static FloorFn> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<&'static FloorFn>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(call) {
        return *hit;
    }
    let made = derive(call).map(|f| &*Box::leak(Box::new(f)));
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(call.to_string(), made);
    made
}

/// Whether a call to `name` returns the scalar-Ok Result carrier (`Some(false)`)
/// or the String-Ok one (`Some(true)`) through the declared floor: the result
/// rewrites seed their carrier values from it, as from a declared user fn.
pub(crate) fn declared_result_carrier(name: &str) -> Option<bool> {
    match declared(name)?.ret {
        FloorRet::Res(_) => Some(false),
        FloorRet::ResStr => Some(true),
        _ => None,
    }
}

/// Whether a floor call returns the scalar `Option<i64>` carrier: a declared
/// `Int? | Bool? | Float?`, or a map's `V?` whose specialized body declares a
/// scalar `V` (`map_get_skv(…) -> Int?`). The option rewrite seeds its
/// carrier values from it, as from a declared user fn.
pub(crate) fn declared_scalar_option(name: &str) -> bool {
    match declared(name).map(|f| f.ret) {
        Some(FloorRet::Opt(_)) => true,
        Some(FloorRet::MapOptVal) => specialized_ret(name).is_some_and(|t| match &t {
            TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 => {
                simple(&args[0]).and_then(scalar_of).is_some()
            }
            _ => false,
        }),
        _ => false,
    }
}

/// The declared return of the self-hosted body registered under `call`.
fn specialized_ret(call: &str) -> Option<TypeExpr> {
    crate::mir_ops::self_host_runtime().iter().find_map(|(src, pairs)| {
        let (impl_fn, _) = pairs.iter().find(|(_, c)| *c == call)?;
        almide_lang::parse_cached(src)?.decls.iter().find_map(|d| match d {
            Decl::Fn { name, return_type, .. } if name.as_str() == *impl_fn => Some(return_type.clone()),
            _ => None,
        })
    })
}

/// Every `module.fn` whose declaration gives a floor entry, over the bundled
/// stdlib: the enumeration the compile gate walks.
pub fn declared_floor_calls() -> Vec<String> {
    let mut out = Vec::new();
    for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(program) = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached) else {
            continue;
        };
        for d in &program.decls {
            if let Decl::Fn { name, .. } = d {
                let call = format!("{module}.{}", name.as_str());
                if declared(&call).is_some() {
                    out.push(call);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The Rust source of every declared floor body and the items it reaches,
/// each once, under the native header — what the compile gate hands rustc.
/// rustc checks an unused item, a generic one included, all the same.
pub fn declared_floor_source() -> String {
    let mut items: BTreeSet<&'static str> = BTreeSet::new();
    for call in declared_floor_calls() {
        items.extend(declared(&call).expect("enumerated from declared()").items.iter().copied());
    }
    let mut out = String::from("#![allow(dead_code, unused_imports, unused_variables, unused_mut, unused_parens)]\n");
    out.push_str(&crate::render_native_str::almide_string_alias());
    out.push_str(FLOOR_HEADER);
    out.push('\n');
    for it in items {
        out.push_str(it);
        out.push('\n');
    }
    out.push_str("fn main() {}\n");
    out
}

/// The stdlib fn a self-hosted body registered under `call` declares it
/// specializes (`@specializes("m.f")` on its declaration).
fn specialized_target(call: &str) -> Option<String> {
    crate::mir_ops::self_host_runtime().iter().find_map(|(src, pairs)| {
        let (impl_fn, _) = pairs.iter().find(|(_, c)| *c == call)?;
        let program = almide_lang::parse_cached(src)?;
        program.decls.iter().find_map(|d| match d {
            Decl::Fn { name, attrs, .. } if name.as_str() == *impl_fn => attrs
                .iter()
                .find(|a| a.name.as_str() == "specializes")
                .and_then(|a| a.args.first())
                .and_then(|a| match &a.value {
                    AttrValue::String { value } => Some(value.to_string()),
                    _ => None,
                }),
            _ => None,
        })
    })
}

/// `m.f` when `body` is exactly `m.f(p1, …, pn)` over `params`.
fn forwarded_call(body: &almide_lang::ast::Expr, params: &[almide_lang::ast::Param]) -> Option<String> {
    use almide_lang::ast::ExprKind;
    let ExprKind::Call { callee, args, named_args, .. } = &body.kind else { return None };
    let ExprKind::Member { object, field } = &callee.kind else { return None };
    let ExprKind::Ident { name: module } = &object.kind else { return None };
    let passes_params = named_args.is_empty()
        && args.len() == params.len()
        && args.iter().zip(params).all(|(a, p)| matches!(&a.kind, ExprKind::Ident { name } if *name == p.name));
    (passes_params && almide_lang::stdlib_info::is_bundled_module(module.as_str())).then(|| format!("{module}.{field}"))
}

/// The conformer call a protocol-generic stdlib fn answers with at a call
/// whose args carry `args`: `list.join[T: Text](xs: List[T], sep: T) =
/// T.__join(xs, sep)` over a `Vec<AlmideString>` is `string.__join`. The
/// type parameter is bound from the first param that spells it (`T` or
/// `List[T]`), and the conformer's fn is named the way the checker names
/// it (`requirement_fn_name`) in the conformer's own module.
pub(crate) fn protocol_instance(call: &str, args: &[NTy]) -> Option<String> {
    use almide_lang::ast::ExprKind;
    let (module, func) = call.split_once('.')?;
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached)?;
    let (generics, params, body) = program.decls.iter().find_map(|d| match d {
        Decl::Fn { name, generics, params, body: Some(body), attrs, .. }
            if name.as_str() == func && !attrs.iter().any(|a| a.name.as_str() == "intrinsic") =>
        {
            Some((generics.as_ref()?, params, body))
        }
        _ => None,
    })?;
    let ExprKind::Call { callee, args: cargs, .. } = &body.kind else { return None };
    let ExprKind::Member { object, field } = &callee.kind else { return None };
    let (ExprKind::Ident { name: tp } | ExprKind::TypeName { name: tp }) = &object.kind else { return None };
    if !generics.iter().any(|g| g.name == *tp) {
        return None;
    }
    let passes_params = cargs.len() == params.len()
        && cargs.iter().zip(params).all(|(a, p)| matches!(&a.kind, ExprKind::Ident { name } if *name == p.name));
    if !passes_params || params.len() != args.len() {
        return None;
    }
    // The conformer type the arg kinds bind the type parameter to.
    let bound = params.iter().zip(args).find_map(|(p, k)| match &p.ty {
        TypeExpr::Simple { name } if name == tp => conformer_of(*k),
        TypeExpr::Generic { name, args: ta } if name.as_str() == "List" && ta.len() == 1 && simple(&ta[0]) == Some(tp.as_str()) => {
            conformer_of_list(*k)
        }
        _ => None,
    })?;
    let conformer_module = bound.to_ascii_lowercase();
    almide_lang::stdlib_info::is_bundled_module(&conformer_module)
        .then(|| format!("{conformer_module}.{}", almide_lang::stdlib_info::requirement_fn_name(bound, field.as_str())))
}

/// The builtin type a native value of kind `k` is.
fn conformer_of(k: NTy) -> Option<&'static str> {
    match k {
        NTy::Str | NTy::StrRef => Some("String"),
        k if crate::render_native_rec::is_bytes(k) => Some("Bytes"),
        _ => None,
    }
}

/// The builtin element type of a native list of kind `k`.
fn conformer_of_list(k: NTy) -> Option<&'static str> {
    match k {
        NTy::VecStr | NTy::VecStrRef => Some("String"),
        NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(i).and_then(conformer_of),
        _ => None,
    }
}

fn derive(call: &str) -> Option<FloorFn> {
    derive_explained(call).ok()
}

/// Why `call` is not on the declared floor: the first step of the
/// derivation that refused it (a to-do list for widening the floor).
pub fn refusal(call: &str) -> Option<String> {
    derive_explained(call).err()
}

fn derive_explained(call: &str) -> Result<FloorFn, String> {
    let (module, func) = call.split_once('.').ok_or("not a module.fn call")?;
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
            Some(target) if target != call => derive_explained(&target).map_err(|e| format!("specializes `{target}`: {e}")),
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
    let type_params: BTreeSet<String> = generics
        .iter()
        .flatten()
        .map(|g| g.name.as_str().to_string())
        .collect();
    // A `Map[K, V]` (or `List[(K, V)]`) param binds K and V: its key, value,
    // and the returns spelled over them, take the map's native kinds.
    let map_kv = params.iter().find_map(|p| map_params(&p.ty, &type_params)).or_else(|| map_params(ret, &type_params));
    let fparams: Vec<FloorParam> = params
        .iter()
        .map(|p| {
            map_kv
                .as_ref()
                .and_then(|kv| map_param(&p.ty, kv))
                .or_else(|| floor_param(&p.ty, &type_params))
                .ok_or_else(|| format!("param type {:?} outside the per-type map", p.ty))
        })
        .collect::<Result<_, _>>()?;
    let fret = map_kv
        .as_ref()
        .and_then(|kv| map_ret(ret, kv))
        .or_else(|| floor_ret(ret, &type_params))
        .ok_or_else(|| format!("return type {ret:?} outside the per-type map"))?;
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
        && !matches!(fret, FloorRet::Res(_) | FloorRet::ResStr | FloorRet::ResValue | FloorRet::ResListStr | FloorRet::ResBytes | FloorRet::ResOptStr)
        && !counted_host_op
    {
        return Err("an effect fn whose return is not a Result carrier".into());
    }
    let rt = runtime_index();
    let body = rt.items.get(symbol.as_str()).ok_or_else(|| format!("`{symbol}` is not a takeable runtime item"))?;
    let (rust_params, rust_ret) = rust_signature(body.text).ok_or("the runtime signature does not parse")?;
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
        if !fparams.iter().any(|p| matches!(p, FloorParam::List(ListElem::Any))) {
            return Err("an element-typed param or return with no `List[A]` param to decide it".into());
        }
        if almide_lang::capability_bounds::of(module, func).next().is_some() {
            return Err("an element-typed generic that needs equality or ordering".into());
        }
    }
    let fparams: Vec<FloorParam> = fparams
        .into_iter()
        .zip(&rust_params)
        .map(|(p, r)| match p {
            FloorParam::Elem(_) => FloorParam::Elem(squash(r).starts_with('&')),
            FloorParam::Value(_) => FloorParam::Value(squash(r).starts_with('&')),
            FloorParam::Map(_) => FloorParam::Map(squash(r).starts_with('&')),
            FloorParam::MapKey(_) => FloorParam::MapKey(squash(r).starts_with('&')),
            FloorParam::MapVal(_) => FloorParam::MapVal(squash(r).starts_with('&')),
            other => other,
        })
        .collect();
    let items = closure(&symbol, rt)?;
    let twin = almide_lang::reuse::of_intrinsic(&symbol).and_then(|r| {
        let t = r.owned_twin?;
        if !matches!(fparams.get(r.param), Some(FloorParam::List(_))) {
            return None;
        }
        let (tp, tr) = rust_signature(rt.items.get(t)?.text)?;
        // The twin differs only in taking the source by value.
        let same = tp.len() == rust_params.len()
            && tp.iter().zip(&rust_params).enumerate().all(|(k, (a, b))| k == r.param || squash(a) == squash(b))
            && squash(&tr) == squash(&rust_ret);
        same.then(|| Some((r.param, t.to_string(), closure(t, rt).ok()?)))?
    });
    Ok(FloorFn { symbol, params: fparams, ret: fret, items, twin })
}

fn simple(t: &TypeExpr) -> Option<&str> {
    match t {
        TypeExpr::Simple { name } => Some(name.as_str()),
        _ => None,
    }
}

fn floor_param(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorParam> {
    match t {
        TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Some(FloorParam::Int),
            "Bool" => Some(FloorParam::Bool),
            "Float" => Some(FloorParam::Float),
            "String" => Some(FloorParam::Str),
            "Bytes" => Some(FloorParam::Bytes),
            n if n == VALUE_TYPE => Some(FloorParam::Value(false)),
            // Whether it is a list's element is checked against the params,
            // and its by-value/by-reference mode against the runtime, below.
            a if type_params.contains(a) => Some(FloorParam::Elem(false)),
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            let elem = match simple(&args[0])? {
                "Int" => ListElem::Int,
                "String" => ListElem::Str,
                n if n == VALUE_TYPE => ListElem::Value,
                a if type_params.contains(a) => ListElem::Any,
                _ => return None,
            };
            Some(FloorParam::List(elem))
        }
        _ => None,
    }
}

/// `(K, V)` when `t` is `Map[K, V]` or `List[(K, V)]` over two type params.
fn map_params(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<(String, String)> {
    let kv = |args: &[TypeExpr]| -> Option<(String, String)> {
        let (k, v) = (simple(args.first()?)?, simple(args.get(1)?)?);
        (args.len() == 2 && type_params.contains(k) && type_params.contains(v)).then(|| (k.to_string(), v.to_string()))
    };
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Map" => kv(args),
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => match &args[0] {
            TypeExpr::Tuple { elements } => kv(elements),
            _ => None,
        },
        _ => None,
    }
}

fn map_param(t: &TypeExpr, (k, v): &(String, String)) -> Option<FloorParam> {
    match t {
        TypeExpr::Simple { name } if name.as_str() == k => Some(FloorParam::MapKey(false)),
        TypeExpr::Simple { name } if name.as_str() == v => Some(FloorParam::MapVal(false)),
        TypeExpr::Generic { name, .. } if name.as_str() == "Map" => {
            map_params(t, &[k.clone(), v.clone()].into_iter().collect()).filter(|kv| kv == &(k.clone(), v.clone())).map(|_| FloorParam::Map(false))
        }
        TypeExpr::Generic { name, .. } if name.as_str() == "List" => {
            map_params(t, &[k.clone(), v.clone()].into_iter().collect()).filter(|kv| kv == &(k.clone(), v.clone())).map(|_| FloorParam::MapPairs)
        }
        _ => None,
    }
}

fn map_ret(t: &TypeExpr, (k, v): &(String, String)) -> Option<FloorRet> {
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Map" => {
            (args.len() == 2 && simple(&args[0]) == Some(k.as_str()) && simple(&args[1]) == Some(v.as_str())).then_some(FloorRet::Map)
        }
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 && simple(&args[0]) == Some(v.as_str()) => {
            Some(FloorRet::MapOptVal)
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => match simple(&args[0]) {
            Some(x) if x == k => Some(FloorRet::MapKeys),
            Some(x) if x == v => Some(FloorRet::MapVals),
            _ => None,
        },
        _ => None,
    }
}

/// `AlmideMap<A, B>` over two type params (squashed).
fn is_generic_map(t: &str) -> bool {
    t.strip_prefix("AlmideMap<")
        .and_then(|x| x.strip_suffix('>'))
        .and_then(|x| x.split_once(','))
        .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b))
}

/// The stdlib's dynamic data model, as its declarations spell it (the one
/// type `crate::lower::is_value_ty` names).
const VALUE_TYPE: &str = "Value";

fn scalar_of(name: &str) -> Option<Scalar> {
    match name {
        "Int" => Some(Scalar::Int),
        "Bool" => Some(Scalar::Bool),
        "Float" => Some(Scalar::Float),
        _ => None,
    }
}

fn floor_ret(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorRet> {
    match t {
        TypeExpr::Simple { name } if type_params.contains(name.as_str()) => Some(FloorRet::Elem(ElemRet::Value)),
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 => {
            floor_ret_option(&args[0], type_params)
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 && is_param_expr(&args[0], type_params) => {
            Some(FloorRet::Elem(ElemRet::List))
        }
        TypeExpr::Simple { name } => floor_ret_simple(name.as_str()),
        TypeExpr::Generic { name, args } if name.as_str() == "Result" && args.len() == 2 => floor_ret_result(&args[0], &args[1]),
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            match simple(&args[0])? {
                "Int" => Some(FloorRet::ListInt),
                "String" => Some(FloorRet::ListStr),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Is `e` one of the fn's type params, spelled as a simple type?
fn is_param_expr(e: &TypeExpr, type_params: &BTreeSet<String>) -> bool {
    simple(e).is_some_and(|a| type_params.contains(a))
}

/// The return kind of an Option of `arg`.
fn floor_ret_option(arg: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorRet> {
    if is_param_expr(arg, type_params) {
        return Some(FloorRet::Elem(ElemRet::Opt));
    }
    if simple(arg) == Some("String") {
        return Some(FloorRet::OptStr);
    }
    scalar_of(simple(arg)?).map(FloorRet::Opt)
}

/// The return kind of a simple (non-generic, non-type-param) type `name`.
fn floor_ret_simple(name: &str) -> Option<FloorRet> {
    match name {
        "Unit" => Some(FloorRet::Unit),
        "Int" => Some(FloorRet::Int),
        "Bool" => Some(FloorRet::Bool),
        "Float" => Some(FloorRet::Float),
        "String" => Some(FloorRet::Str),
        "Bytes" => Some(FloorRet::Bytes),
        n if n == VALUE_TYPE => Some(FloorRet::Value),
        _ => None,
    }
}

/// The return kind of a `Result[ok, err]`.
fn floor_ret_result(ok: &TypeExpr, err: &TypeExpr) -> Option<FloorRet> {
    if simple(err) != Some("String") {
        return None;
    }
    if let Some(r) = floor_ret_result_str_container(ok) {
        return Some(r);
    }
    match simple(ok)? {
        "Unit" => Some(FloorRet::Res(None)),
        "Bytes" => Some(FloorRet::ResBytes),
        "String" => Some(FloorRet::ResStr),
        n if n == VALUE_TYPE => Some(FloorRet::ResValue),
        ok => scalar_of(ok).map(|k| FloorRet::Res(Some(k))),
    }
}

/// A Result's `List[String]` / `Option[String]` ok type.
fn floor_ret_result_str_container(ok: &TypeExpr) -> Option<FloorRet> {
    match ok {
        TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1 && simple(&a[0]) == Some("String") => {
            Some(FloorRet::ResListStr)
        }
        TypeExpr::Generic { name, args: a } if is_option_name(name.as_str()) && a.len() == 1 && simple(&a[0]) == Some("String") => {
            Some(FloorRet::ResOptStr)
        }
        _ => None,
    }
}

/// `T?` parses as the generic `?`; `Option[T]` spells it out.
fn is_option_name(n: &str) -> bool {
    n == "?" || n == "Option"
}

include!("native_floor_sig.rs");
include!("native_floor_index.rs");
include!("native_floor_tail.rs");
