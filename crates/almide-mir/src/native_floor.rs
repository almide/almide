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
    /// The builtin `Matrix` — v0's runtime `AlmideMatrix`, borrowed (`true`)
    /// or by value (`false`).
    Matrix(bool),
    /// `Set[A]` — v0's runtime `AlmideSet`, borrowed (`true`) or by value
    /// (`false`; the caller's set is moved in when the call is its last read).
    Set(bool),
    /// The element `A` of the call's `Set[A]` param (by reference when
    /// `true`); the set's kind decides the native kind.
    SetElem(bool),
}

/// A declared list's element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListElem {
    Int,
    Str,
    /// `List[Value]` — a slice of the runtime `AlmideValue`.
    Value,
    /// `List[(String, Value)]` — a slice of the runtime's
    /// `(AlmideString, AlmideValue)` pairs (an object's entries).
    StrValuePairs,
    /// `List[Float]` — a slice of `f64` (the render's list holds the bits).
    Float,
    /// `List[List[Float]]` — a slice of `Vec<f64>` rows.
    FloatRows,
    /// `List[Matrix]` — a slice of the runtime `AlmideMatrix`.
    Matrix,
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
    /// `Result[List[Value], String]` — the generated carrier of the runtime's
    /// `Vec<AlmideValue>`.
    ResListValue,
    /// The builtin `Matrix` — the runtime's own `AlmideMatrix`.
    Matrix,
    /// `Matrix?` — the generated carrier of an `Option<AlmideMatrix>`.
    OptMatrix,
    /// `List[Matrix]` — the generated list of the runtime's `AlmideMatrix`.
    ListMatrix,
    /// `List[List[Float]]` — the runtime's `Vec<Vec<f64>>`, rebound as the
    /// generated list of the render's float lists (each cell its bits).
    FloatRows,
    /// `Set[A]` — the generated set alias of the call's set kind (its `Set[A]`
    /// arg's, else the declared type of the value it binds).
    Set,
    /// `List[A]` of the call's set.
    SetElems,
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

/// The native kinds of the type arguments of a declared `ctor[…]` type whose
/// arguments the native render carries as is: `Int`, `String`, a list of
/// either.
fn declared_kinds(t: &TypeExpr, ctor: &str) -> Option<Vec<NTy>> {
    let TypeExpr::Generic { name, args } = t else { return None };
    if name.as_str() != ctor {
        return None;
    }
    args.iter()
        .map(|a| match a {
            TypeExpr::Simple { name } => match name.as_str() {
                "Int" => Some(NTy::I64),
                "String" => Some(NTy::Str),
                _ => None,
            },
            TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => match simple(&args[0])? {
                "Int" => Some(NTy::Vec),
                "String" => Some(NTy::VecStr),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// A set built from a list arg holds that list's elements.
fn set_list_agrees(params: &[FloorParam], args: &[(String, NTy)], e: NTy) -> bool {
    params.iter().zip(args).all(|(p, (_, got))| match p {
        FloorParam::List(ListElem::Any) => match e {
            NTy::I64 => matches!(got, NTy::Vec | NTy::VecRef),
            NTy::Str => matches!(got, NTy::VecStr | NTy::VecStrRef),
            _ => false,
        },
        _ => true,
    })
}

/// Whether a floor call returning its list's element Option (`A?`) returns
/// the scalar `Option<i64>` carrier: unless the self-hosted body registered
/// under `call` declares another element (`list.get_str`'s `String?`).
pub(crate) fn elem_option_is_scalar(call: &str) -> bool {
    specialized_ret(call).is_none_or(|t| match &t {
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 => {
            simple(&args[0]).and_then(scalar_of).is_some()
        }
        _ => false,
    })
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
    // A `Set[A]` param or return binds A the same way.
    let set_a = params.iter().find_map(|p| set_param_name(&p.ty, &type_params)).or_else(|| set_param_name(ret, &type_params));
    let fparams: Vec<FloorParam> = params
        .iter()
        .map(|p| {
            map_kv
                .as_ref()
                .and_then(|kv| map_param(&p.ty, kv))
                .or_else(|| set_a.as_ref().and_then(|a| set_param(&p.ty, a)))
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
        && !matches!(fret, FloorRet::Res(_) | FloorRet::ResStr | FloorRet::ResValue | FloorRet::ResListStr | FloorRet::ResListValue | FloorRet::ResBytes | FloorRet::ResOptStr)
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
            FloorParam::Matrix(_) => FloorParam::Matrix(squash(r).starts_with('&')),
            FloorParam::Set(_) => FloorParam::Set(squash(r).starts_with('&')),
            FloorParam::SetElem(_) => FloorParam::SetElem(squash(r).starts_with('&')),
            FloorParam::Map(_) => FloorParam::Map(squash(r).starts_with('&')),
            FloorParam::MapKey(_) => FloorParam::MapKey(squash(r).starts_with('&')),
            FloorParam::MapVal(_) => FloorParam::MapVal(squash(r).starts_with('&')),
            other => other,
        })
        .collect();
    let items = closure(&symbol, rt)?;
    let twin = almide_lang::reuse::of_intrinsic(&symbol).and_then(|r| {
        let t = r.owned_twin?;
        if !matches!(fparams.get(r.param), Some(FloorParam::List(_) | FloorParam::Set(_))) {
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
            "Matrix" => Some(FloorParam::Matrix(false)),
            n if n == VALUE_TYPE => Some(FloorParam::Value(false)),
            // Whether it is a list's element is checked against the params,
            // and its by-value/by-reference mode against the runtime, below.
            a if type_params.contains(a) => Some(FloorParam::Elem(false)),
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            if let TypeExpr::Tuple { elements } = &args[0] {
                let pair: Vec<Option<&str>> = elements.iter().map(simple).collect();
                return (pair == [Some("String"), Some(VALUE_TYPE)]).then_some(FloorParam::List(ListElem::StrValuePairs));
            }
            if let TypeExpr::Generic { name, args: inner } = &args[0] {
                let rows = name.as_str() == "List" && inner.len() == 1 && simple(&inner[0]) == Some("Float");
                return rows.then_some(FloorParam::List(ListElem::FloatRows));
            }
            let elem = match simple(&args[0])? {
                "Int" => ListElem::Int,
                "Float" => ListElem::Float,
                "Matrix" => ListElem::Matrix,
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

/// `A` when `t` is `Set[A]` over a type param.
fn set_param_name(t: &TypeExpr, type_params: &BTreeSet<String>) -> Option<String> {
    match t {
        TypeExpr::Generic { name, args } if name.as_str() == "Set" && args.len() == 1 => {
            simple(&args[0]).filter(|a| type_params.contains(*a)).map(str::to_string)
        }
        _ => None,
    }
}

fn set_param(t: &TypeExpr, a: &str) -> Option<FloorParam> {
    match t {
        TypeExpr::Simple { name } if name.as_str() == a => Some(FloorParam::SetElem(false)),
        TypeExpr::Generic { name, args } if name.as_str() == "Set" && args.len() == 1 && simple(&args[0]) == Some(a) => {
            Some(FloorParam::Set(false))
        }
        _ => None,
    }
}

fn set_ret(t: &TypeExpr, a: &str) -> Option<FloorRet> {
    match t {
        TypeExpr::Generic { name, args } if args.len() == 1 && simple(&args[0]) == Some(a) => match name.as_str() {
            "Set" => Some(FloorRet::Set),
            "List" => Some(FloorRet::SetElems),
            _ => None,
        },
        _ => None,
    }
}

/// `AlmideSet<A>` over a type param (squashed).
fn is_generic_set(t: &str) -> bool {
    t.strip_prefix("AlmideSet<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param)
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
    let is_param = |e: &TypeExpr| simple(e).is_some_and(|a| type_params.contains(a));
    match t {
        TypeExpr::Simple { name } if type_params.contains(name.as_str()) => Some(FloorRet::Elem(ElemRet::Value)),
        TypeExpr::Generic { name, args } if is_option_name(name.as_str()) && args.len() == 1 => {
            if is_param(&args[0]) {
                return Some(FloorRet::Elem(ElemRet::Opt));
            }
            if simple(&args[0]) == Some("String") {
                return Some(FloorRet::OptStr);
            }
            if simple(&args[0]) == Some("Matrix") {
                return Some(FloorRet::OptMatrix);
            }
            scalar_of(simple(&args[0])?).map(FloorRet::Opt)
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 && is_param(&args[0]) => {
            Some(FloorRet::Elem(ElemRet::List))
        }
        TypeExpr::Simple { name } => match name.as_str() {
            "Unit" => Some(FloorRet::Unit),
            "Int" => Some(FloorRet::Int),
            "Bool" => Some(FloorRet::Bool),
            "Float" => Some(FloorRet::Float),
            "String" => Some(FloorRet::Str),
            "Bytes" => Some(FloorRet::Bytes),
            "Matrix" => Some(FloorRet::Matrix),
            n if n == VALUE_TYPE => Some(FloorRet::Value),
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "Result" && args.len() == 2 => {
            if simple(&args[1]) != Some("String") {
                return None;
            }
            match &args[0] {
                TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1 && simple(&a[0]) == Some("String") => {
                    return Some(FloorRet::ResListStr)
                }
                TypeExpr::Generic { name, args: a } if name.as_str() == "List" && a.len() == 1 && simple(&a[0]) == Some(VALUE_TYPE) => {
                    return Some(FloorRet::ResListValue)
                }
                TypeExpr::Generic { name, args: a } if is_option_name(name.as_str()) && a.len() == 1 && simple(&a[0]) == Some("String") => {
                    return Some(FloorRet::ResOptStr)
                }
                _ => {}
            }
            match simple(&args[0])? {
                "Unit" => Some(FloorRet::Res(None)),
                "Bytes" => Some(FloorRet::ResBytes),
                "String" => Some(FloorRet::ResStr),
                n if n == VALUE_TYPE => Some(FloorRet::ResValue),
                ok => scalar_of(ok).map(|k| FloorRet::Res(Some(k))),
            }
        }
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            if let TypeExpr::Generic { name, args: inner } = &args[0] {
                let rows = name.as_str() == "List" && inner.len() == 1 && simple(&inner[0]) == Some("Float");
                return rows.then_some(FloorRet::FloatRows);
            }
            match simple(&args[0])? {
                "Int" => Some(FloorRet::ListInt),
                "String" => Some(FloorRet::ListStr),
                "Matrix" => Some(FloorRet::ListMatrix),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `T?` parses as the generic `?`; `Option[T]` spells it out.
fn is_option_name(n: &str) -> bool {
    n == "?" || n == "Option"
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn param_agrees(p: FloorParam, rust: &str) -> bool {
    let r = squash(rust);
    match p {
        FloorParam::Int => r == "i64",
        FloorParam::Bool => r == "bool",
        FloorParam::Float => r == "f64",
        FloorParam::Str => r == "&str",
        FloorParam::Bytes => r == "&Vec<u8>" || r == "&[u8]",
        FloorParam::Value(_) => r == "&AlmideValue" || r == "AlmideValue",
        FloorParam::Matrix(_) => r == "&AlmideMatrix" || r == "AlmideMatrix",
        FloorParam::Set(_) => is_generic_set(r.strip_prefix('&').unwrap_or(&r)),
        FloorParam::SetElem(_) => is_type_param(r.strip_prefix('&').unwrap_or(&r)),
        FloorParam::Map(_) => is_generic_map(r.strip_prefix('&').unwrap_or(&r)),
        FloorParam::MapKey(_) | FloorParam::MapVal(_) => is_type_param(r.strip_prefix('&').unwrap_or(&r)),
        FloorParam::MapPairs => r
            .strip_prefix("Vec<(")
            .and_then(|x| x.strip_suffix(")>"))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(a, b)| is_type_param(a) && is_type_param(b)),
        FloorParam::Elem(_) => is_type_param(&r) || r.strip_prefix('&').is_some_and(is_type_param),
        FloorParam::List(elem) => {
            let inner = r
                .strip_prefix("&[")
                .and_then(|x| x.strip_suffix(']'))
                .or_else(|| r.strip_prefix("&Vec<").and_then(|x| x.strip_suffix('>')));
            match (elem, inner) {
                (ListElem::Int, Some(i)) => i == "i64" || is_type_param(i),
                (ListElem::Str, Some(i)) => i == "AlmideString" || is_type_param(i),
                (ListElem::Value, Some(i)) => i == "AlmideValue",
                (ListElem::StrValuePairs, Some(i)) => i == "(AlmideString,AlmideValue)",
                (ListElem::Float, Some(i)) => i == "f64",
                (ListElem::FloatRows, Some(i)) => i == "Vec<f64>",
                (ListElem::Matrix, Some(i)) => i == "AlmideMatrix",
                (ListElem::Any, Some(i)) => is_type_param(i),
                _ => false,
            }
        }
    }
}

fn is_type_param(s: &str) -> bool {
    s.len() == 1 && s.chars().all(|c| c.is_ascii_uppercase())
}

/// `(ok, err)` of a squashed `Result<ok,err>`, split at its top-level comma.
fn result_parts(t: &str) -> Option<(&str, &str)> {
    let inner = t.strip_prefix("Result<")?.strip_suffix('>')?;
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => return Some((&inner[..i], &inner[i + 1..])),
            _ => {}
        }
    }
    None
}

fn ret_agrees(r: FloorRet, rust: &str) -> bool {
    let t = squash(rust);
    let string = |x: &str| x == "AlmideString" || x == "String";
    match r {
        FloorRet::Unit => t.is_empty() || t == "()",
        FloorRet::Int => t == "i64",
        FloorRet::Bool => t == "bool" || t == "i64",
        FloorRet::Float => t == "f64",
        FloorRet::Str => string(&t),
        FloorRet::ListInt => t == "Vec<i64>",
        FloorRet::ListStr => t == "Vec<String>" || t == "Vec<AlmideString>",
        FloorRet::Opt(Scalar::Int) => t == "Option<i64>",
        FloorRet::Opt(Scalar::Bool) => t == "Option<bool>",
        FloorRet::Opt(Scalar::Float) => t == "Option<f64>",
        FloorRet::Elem(ElemRet::Value) => is_type_param(&t),
        FloorRet::Elem(ElemRet::Opt) => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::Elem(ElemRet::List) => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::Bytes => t == "Vec<u8>",
        FloorRet::Value => t == "AlmideValue",
        FloorRet::Matrix => t == "AlmideMatrix",
        FloorRet::OptMatrix => t == "Option<AlmideMatrix>",
        FloorRet::ListMatrix => t == "Vec<AlmideMatrix>",
        FloorRet::FloatRows => t == "Vec<Vec<f64>>",
        FloorRet::Set => is_generic_set(&t),
        FloorRet::SetElems => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::Map => is_generic_map(&t),
        FloorRet::MapOptVal => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::MapKeys | FloorRet::MapVals => t.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(is_type_param),
        FloorRet::ResValue => result_parts(&t).is_some_and(|(ok, err)| ok == "AlmideValue" && string(err)),
        FloorRet::OptStr => t.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(string),
        FloorRet::ResListValue => result_parts(&t).is_some_and(|(ok, err)| ok == "Vec<AlmideValue>" && string(err)),
        FloorRet::ResListStr => result_parts(&t).is_some_and(|(ok, err)| {
            ok.strip_prefix("Vec<").and_then(|x| x.strip_suffix('>')).is_some_and(string) && string(err)
        }),
        FloorRet::ResBytes => result_parts(&t).is_some_and(|(ok, err)| ok == "Vec<u8>" && string(err)),
        FloorRet::ResOptStr => result_parts(&t).is_some_and(|(ok, err)| {
            ok.strip_prefix("Option<").and_then(|x| x.strip_suffix('>')).is_some_and(string) && string(err)
        }),
        FloorRet::Res(k) => t
            .strip_prefix("Result<")
            .and_then(|x| x.strip_suffix('>'))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(ok, err)| {
                let ok_agrees = match k {
                    None => ok == "()",
                    Some(Scalar::Int) => ok == "i64",
                    Some(Scalar::Bool) => ok == "bool",
                    Some(Scalar::Float) => ok == "f64",
                };
                ok_agrees && string(err)
            }),
        FloorRet::ResStr => t
            .strip_prefix("Result<")
            .and_then(|x| x.strip_suffix('>'))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(ok, err)| string(ok) && string(err)),
    }
}

// ── The runtime index: every top-level item of the embedded runtime ──

struct RtItem {
    text: &'static str,
}

struct RuntimeIndex {
    /// Items defined exactly once at the top level of the runtime modules.
    items: HashMap<&'static str, RtItem>,
    /// Every name the runtime and its prelude define (a type, a macro, a
    /// module, a twice-defined fn, …): reaching one that is not in `items` walls.
    defined: BTreeSet<&'static str>,
    /// The methods the runtime's and the prelude's own traits declare
    /// (`IntoAlmide::into_almide`): a body calling one needs the trait and
    /// its impls, which the header does not carry.
    trait_methods: BTreeSet<&'static str>,
    /// Every fn name the runtime defines at its top level (taken or not).
    fns: BTreeSet<&'static str>,
}

fn runtime_index() -> &'static RuntimeIndex {
    static IDX: OnceLock<RuntimeIndex> = OnceLock::new();
    IDX.get_or_init(|| {
        let mut items: HashMap<&'static str, RtItem> = HashMap::new();
        let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
        // A module the runtime glob-imports (`mod almide_libm { … } pub use
        // almide_libm::*;`) puts its names in scope unqualified: each of them
        // takes the whole module and its glob along.
        let mut glob_mods: Vec<(&'static str, &'static str)> = Vec::new();
        // Runtime types and their impl blocks: a type travels with its
        // inherent impls and its impls of std traits.
        let mut types: Vec<(&'static str, &'static str)> = Vec::new();
        let mut impls: Vec<(&'static str, &'static str)> = Vec::new();
        let mut fns: BTreeSet<&'static str> = BTreeSet::new();
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            for (kind, name, text, attributed) in top_level_items(src) {
                if kind == ItemKind::Impl {
                    if !attributed {
                        impls.push((name, text));
                    }
                    continue;
                }
                if kind == ItemKind::Fn {
                    fns.insert(name);
                }
                if kind == ItemKind::Type && !attributed {
                    types.push((name, text));
                }
                *seen.entry(name).or_default() += 1;
                // A `#[cfg]`-gated item may have a sibling for another target:
                // never take one along.
                if matches!(kind, ItemKind::Fn | ItemKind::Const) && !attributed {
                    items.insert(name, RtItem { text });
                }
                let is_mod = kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != name).any(|t| t.text == "mod");
                if is_mod && !attributed && src.contains(&format!("\npub use {name}::*;")) {
                    let whole: &'static str = Box::leak(format!("{text}\npub use {name}::*;").into_boxed_str());
                    glob_mods.push((name, whole));
                }
            }
        }
        items.retain(|n, _| seen.get(n) == Some(&1));
        for (name, whole) in glob_mods {
            if seen.get(name) != Some(&1) {
                continue;
            }
            let body = &whole[whole.find('{').unwrap_or(0) + 1..whole.rfind("} //").or_else(|| whole.rfind('}')).unwrap_or(whole.len())];
            items.insert(name, RtItem { text: whole });
            for inner in defined_names(body) {
                if !items.contains_key(inner) && seen.get(inner).is_none() {
                    items.insert(inner, RtItem { text: whole });
                }
            }
        }
        // Every name the runtime or the v0 prelude defines, at any depth: what
        // a body may reach only as a takeable item. The prelude rides in the
        // runtime crate's source (`emit_runtime_crate`).
        let prelude: &'static str = Box::leak(almide_codegen::emit_runtime_crate().into_boxed_str());
        let mut defined: BTreeSet<&'static str> = defined_names(prelude);
        let mut trait_methods = trait_method_names(prelude);
        let mut runtime_traits: BTreeSet<&'static str> = BTreeSet::new();
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            defined.extend(defined_names(src));
            defined.extend(thread_local_names(src));
            trait_methods.extend(trait_method_names(src));
            runtime_traits.extend(trait_names(src));
        }
        // A trait the prelude alone defines travels whole: its declaration and
        // every prelude impl of it, taken by any of its methods or its name.
        // (An impl a runtime module adds is not taken, so a body that reaches
        // the method through such a type fails the compile gate, never a
        // user's build.)
        let traits: Vec<_> = prelude_traits(prelude)
            .into_iter()
            .filter(|(name, ..)| !runtime_traits.contains(name) && !items.contains_key(name))
            .collect();
        for (name, decl, _, methods) in &traits {
            items.insert(name, RtItem { text: decl });
            for m in methods {
                trait_methods.remove(m);
                items.entry(m).or_insert(RtItem { text: decl });
            }
        }
        // An impl for a type the header cannot carry (it reaches such a
        // name) stays behind: no carried body holds a value of that type.
        let provisional = RuntimeIndex { items, defined, trait_methods, fns };
        let mut taken: Vec<(&'static str, &'static str, Vec<&'static str>)> = Vec::new();
        for (name, decl, impls, methods) in traits {
            let mut whole = String::from(decl);
            for imp in impls.into_iter().filter(|imp| references_in(&provisional, imp).is_ok()) {
                whole.push('\n');
                whole.push_str(imp);
            }
            taken.push((name, &*Box::leak(whole.into_boxed_str()), methods));
        }
        let RuntimeIndex { mut items, defined, mut trait_methods, fns } = provisional;
        for (name, whole, methods) in taken {
            items.insert(name, RtItem { text: whole });
            for m in methods {
                items.insert(m, RtItem { text: whole });
            }
        }
        // A runtime type travels with its inherent impls and its impls of std
        // traits (a trait the runtime or the prelude defines stays behind, as
        // a runtime module's impl of a prelude trait does above). Added last,
        // so the prelude traits' impl filter above never takes an impl for one.
        for (name, decl) in types {
            if seen.get(name) != Some(&1) || items.contains_key(name) {
                continue;
            }
            let mut whole = String::from(decl);
            for (_, imp) in impls.iter().filter(|(n, _)| *n == name) {
                let trait_of = impl_parts(&lex(imp)).and_then(|(t, _)| t);
                if trait_of.is_some_and(|t| defined.contains(t)) {
                    continue;
                }
                whole.push('\n');
                whole.push_str(imp);
            }
            items.insert(name, RtItem { text: Box::leak(whole.into_boxed_str()) });
        }
        // A runtime trait defined once travels whole too: its declaration and
        // every runtime impl of it (`AlmideMapKey` and its blanket impls), taken
        // by its name or any of its methods.
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            for (name, decl, _, methods) in prelude_traits(src) {
                if seen.get(name) != Some(&1) || items.contains_key(name) {
                    continue;
                }
                let mut whole = String::from(decl);
                for (_, imp) in &impls {
                    if impl_parts(&lex(imp)).and_then(|(t, _)| t) == Some(name) {
                        whole.push('\n');
                        whole.push_str(imp);
                    }
                }
                let whole: &'static str = Box::leak(whole.into_boxed_str());
                items.insert(name, RtItem { text: whole });
                for m in methods {
                    trait_methods.remove(m);
                    items.entry(m).or_insert(RtItem { text: whole });
                }
            }
        }
        RuntimeIndex { items, defined, trait_methods, fns }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Fn,
    Const,
    /// A `struct` / `enum` / `type` / `union`: its text includes its
    /// attributes (a `#[derive]` travels with it).
    Type,
    /// An `impl` block, named by its self type.
    Impl,
    Other,
}

/// The top-level items of a runtime module: (kind, name, whole text, whether
/// a `#[cfg…]` attribute precedes it).
fn top_level_items<'a>(src: &'a str) -> Vec<(ItemKind, &'a str, &'a str, bool)> {
    let toks = lex(src);
    let mut out = Vec::new();
    let mut i = 0;
    let mut cfg_attr = false;
    let mut attr_start: Option<usize> = None;
    while i < toks.len() {
        let t = &toks[i];
        if t.depth != 0 {
            i += 1;
            continue;
        }
        if t.text == "#" {
            // An attribute: `#[...]` (skip to its closing bracket).
            let start = i;
            let mut j = i + 1;
            while j < toks.len() && !(toks[j].text == "]" && toks[j].depth == 0) {
                j += 1;
            }
            cfg_attr |= toks[start..j].iter().any(|t| t.text == "cfg" || t.text == "cfg_attr");
            attr_start.get_or_insert(toks[start].off);
            i = j + 1;
            continue;
        }
        // Visibility and qualifiers.
        let start_off = t.off;
        let mut j = i;
        if toks[j].text == "pub" {
            j += 1;
            if toks.get(j).is_some_and(|t| t.text == "(") {
                while j < toks.len() && toks[j].text != ")" {
                    j += 1;
                }
                j += 1;
            }
        }
        while toks.get(j).is_some_and(|t| matches!(t.text, "const" | "unsafe" | "extern" | "async") && toks.get(j + 1).is_some_and(|n| matches!(n.text, "fn" | "unsafe" | "extern" | "\"C\""))) {
            j += 1;
        }
        let Some(kw) = toks.get(j) else { break };
        let (kind, name_at) = match kw.text {
            "fn" => (ItemKind::Fn, j + 1),
            "const" | "static" => (ItemKind::Const, j + 1),
            "struct" | "enum" | "type" | "union" => (ItemKind::Type, j + 1),
            "trait" | "mod" | "macro_rules" => (ItemKind::Other, j + 1),
            "impl" => {
                let end = item_end(&toks, j);
                let end_off = toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(src.len());
                if let Some(name) = impl_self_type(&toks[j..=end.min(toks.len() - 1)]) {
                    out.push((ItemKind::Impl, name, &src[start_off..end_off], cfg_attr));
                }
                i = end + 1;
                cfg_attr = false;
                attr_start = None;
                continue;
            }
            "use" => {
                // No name of its own: skip to its end.
                let end = item_end(&toks, j);
                i = end + 1;
                cfg_attr = false;
                attr_start = None;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        };
        let name_at = if kw.text == "macro_rules" { name_at + 1 } else { name_at };
        let Some(name) = toks.get(name_at).map(|t| t.text) else { break };
        // A `const`/`static` ends at its `;`: its initializer may hold a block
        // (`X: T = T { … };`).
        let end = if kind == ItemKind::Const { const_end(&toks, name_at) } else { item_end(&toks, name_at) };
        let end_off = toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(src.len());
        let from = if kind == ItemKind::Type { attr_start.unwrap_or(start_off) } else { start_off };
        out.push((kind, name, &src[from..end_off], cfg_attr));
        cfg_attr = false;
        attr_start = None;
        i = end + 1;
    }
    out
}

/// The self type of an `impl` block (`impl<T> Trait for Name<T> {` → `Name`),
/// and its trait, if any, as the first token of `impl … for`.
fn impl_self_type<'a>(toks: &[Tok<'a>]) -> Option<&'a str> {
    impl_parts(toks).map(|(_, ty)| ty)
}

fn impl_parts<'a>(toks: &[Tok<'a>]) -> Option<(Option<&'a str>, &'a str)> {
    let base = toks.first()?.depth;
    let mut k = 1;
    // Skip the impl's own generics.
    if toks.get(k).is_some_and(|t| t.text == "<") {
        let mut n = 0i32;
        while let Some(t) = toks.get(k) {
            match t.text {
                "<" => n += 1,
                ">" => n -= 1,
                _ => {}
            }
            k += 1;
            if n == 0 {
                break;
            }
        }
    }
    let head = &toks[k..];
    let open = head.iter().position(|t| t.text == "{" && t.depth == base)?;
    let first_ident = |ts: &[Tok<'a>]| ts.iter().map(|t| t.text).find(|s| is_ident(s) && !matches!(*s, "dyn" | "mut"));
    match head[..open].iter().position(|t| t.text == "for") {
        Some(f) => Some((first_ident(&head[..f]), first_ident(&head[f + 1..open])?)),
        None => Some((None, first_ident(&head[..open])?)),
    }
}

/// The `;` ending a `const`/`static` item, outside any nesting.
fn const_end(toks: &[Tok], from: usize) -> usize {
    let base = toks[from].depth;
    let mut nest = 0i32;
    for (j, t) in toks.iter().enumerate().skip(from) {
        match t.text {
            "(" | "[" => nest += 1,
            ")" | "]" => nest -= 1,
            ";" if t.depth == base && nest == 0 => return j,
            _ => {}
        }
    }
    toks.len().saturating_sub(1)
}

/// The index of the token that ends the item starting at `from`: the `}`
/// closing its first block, or a `;` met before any block, both outside any
/// `()` / `[]` / `<>` nesting (`[u8; 4]` is not an end).
fn item_end(toks: &[Tok], from: usize) -> usize {
    let base = toks[from].depth;
    let mut nest = 0i32;
    let mut j = from;
    while j < toks.len() {
        let t = &toks[j];
        match t.text {
            "(" | "[" => nest += 1,
            ")" | "]" => nest -= 1,
            _ => {}
        }
        if t.depth == base && nest == 0 && t.text == ";" {
            return j;
        }
        if t.text == "{" && t.depth == base && nest == 0 {
            let mut k = j + 1;
            while k < toks.len() && !(toks[k].text == "}" && toks[k].depth == base) {
                k += 1;
            }
            return k;
        }
        j += 1;
    }
    toks.len().saturating_sub(1)
}

/// The Rust param types and return type of a runtime fn item.
fn rust_signature(text: &str) -> Option<(Vec<String>, String)> {
    let toks = lex(text);
    let fn_at = toks.iter().position(|t| t.text == "fn")?;
    let open = toks[fn_at..].iter().position(|t| t.text == "(")? + fn_at;
    // The `)` matching the param list's `(` (a param type may hold parens:
    // `Vec<(K, V)>`).
    let close = {
        let mut n = 0i32;
        (open..toks.len()).find(|&k| {
            match toks[k].text {
                "(" => n += 1,
                ")" => n -= 1,
                _ => {}
            }
            n == 0
        })?
    };
    let inner = &text[toks[open].off + 1..toks[close].off];
    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in inner.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            params.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        params.push(cur);
    }
    let params = params
        .into_iter()
        .map(|p| p.split_once(':').map(|(_, t)| t.trim().to_string()))
        .collect::<Option<Vec<_>>>()?;
    let body_open = (close + 1..toks.len()).find(|&k| toks[k].text == "{" || toks[k].text == "where")?;
    let after = &text[toks[close].off + 1..toks[body_open].off];
    let ret = after.trim().strip_prefix("->").map(|r| r.trim().to_string()).unwrap_or_default();
    Some((params, ret))
}

/// The runtime body of `symbol` and every item it reaches, or `None` when it
/// reaches something the native header cannot carry.
fn closure(symbol: &str, rt: &'static RuntimeIndex) -> Result<Vec<&'static str>, String> {
    let mut taken: BTreeSet<&'static str> = BTreeSet::new();
    let mut texts: BTreeSet<&'static str> = BTreeSet::new();
    let mut todo: Vec<String> = vec![symbol.to_string()];
    let mut out = Vec::new();
    while let Some(name) = todo.pop() {
        let (&key, item) = rt
            .items
            .get_key_value(name.as_str())
            .ok_or_else(|| format!("`{name}` is not a takeable runtime item"))?;
        if header_items().contains(key) || !taken.insert(key) || !texts.insert(item.text) {
            continue;
        }
        // A runtime module that needs a crate (`almide_codegen::runtime_crate_deps`)
        // cannot ride the native program, which links no dependency.
        if let Some((dep, _)) = almide_codegen::runtime_crate_deps(item.text).first() {
            return Err(format!("`{key}` needs the `{dep}` crate, which a native program does not link"));
        }
        out.push(item.text);
        for r in references(item.text).map_err(|n| format!("`{key}` reaches `{n}`, which the native header cannot carry"))? {
            if !taken.contains(r.as_str()) {
                todo.push(r);
            }
        }
    }
    Ok(out)
}

/// The runtime item names `text` reaches that need a definition beside it,
/// or `Err(name)` when it reaches a name the runtime or its prelude defines that
/// is not an item we can take along (a type, a macro, a module, a prelude
/// fn, a `#[cfg]` twin). A name neither of them defines is Rust's own.
fn references(text: &str) -> Result<Vec<String>, String> {
    references_in(runtime_index(), text)
}

fn references_in(rt: &RuntimeIndex, text: &str) -> Result<Vec<String>, String> {
    let toks = lex(text);
    let header = header_items();
    let mut out = Vec::new();
    let mut locals: BTreeSet<&str> = BTreeSet::new();
    for (i, t) in toks.iter().enumerate() {
        let s = t.text;
        if !is_ident(s) {
            continue;
        }
        let prev = i.checked_sub(1).map(|k| toks[k].text);
        let next = toks.get(i + 1).map(|t| t.text);
        if prev == Some(".") && rt.trait_methods.contains(s) {
            return Err(s.to_string()); // a method of the runtime's own traits
        }
        if prev == Some(".") && rt.items.contains_key(s) && !rt.defined.contains(s) {
            out.push(s.to_string()); // a method of a prelude trait that travels whole
            continue;
        }
        if matches!(prev, Some("." | "::" | "'")) {
            continue; // a method, a field, a path tail, a lifetime
        }
        if prev == Some("fn") {
            continue; // an item's own name
        }
        // A closure param sits between bars (`|e|`, `|a, b|`); the name after
        // the closing bar is the closure's body.
        let closure_param = prev == Some("|") && matches!(next, Some("|" | "," | ":"));
        if matches!(prev, Some("let" | "mut" | "ref")) || closure_param || (next == Some(":") && prev != Some("::")) {
            locals.insert(s);
            continue; // a binding, a param, a field or label name
        }
        if header.contains(s) {
            continue;
        }
        if rt.items.contains_key(s) {
            out.push(s.to_string());
            continue;
        }
        if locals.contains(s) {
            continue;
        }
        // A lowercase name used as a value (no call, path or macro after it)
        // is a binding of this body: a pattern or closure param the token
        // scan did not see bind it. An item it could name would be a `const`
        // or `static`, which are UPPERCASE.
        // (`c != x` lexes as `c`, `!`, `=`: only `name!(` / `![` / `!{` is a macro.)
        let macro_call = next == Some("!") && toks.get(i + 2).is_some_and(|t| matches!(t.text, "(" | "[" | "{"));
        let used_as_item = matches!(next, Some("(" | "::")) || macro_call || s.chars().next().is_some_and(|c| c.is_uppercase());
        if rt.defined.contains(s) && used_as_item {
            return Err(s.to_string());
        }
        // A runtime fn that is not takeable (a `#[cfg]` twin) passed as a value.
        if rt.defined.contains(s) && !rt.items.contains_key(s) && !locals.contains(s) && rt.fns.contains(s) {
            return Err(s.to_string());
        }
    }
    Ok(out)
}

/// The names the native header defines for a floor body.
/// Is `text` a runtime item (the floor header or an item of the embedded
/// runtime the floor takes along)? The render places these in the program's
/// runtime part (`render_native::RT_MOD`), which the build compiles as its
/// own crate with the runtime's profile (owner ruling 2026-11-02).
pub(crate) fn is_runtime_text(text: &str) -> bool {
    static TEXTS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    text == FLOOR_HEADER || TEXTS.get_or_init(|| runtime_index().items.values().map(|i| i.text).collect()).contains(text)
}

/// `text` with every top-level item it opens made `pub` (a fn, const,
/// static, type, trait or module that is not already): the runtime part is a
/// module, or a crate of its own, that the program reaches through a glob.
pub(crate) fn pub_items(text: &str) -> String {
    let toks = lex(text);
    // (offset, length replaced, replacement)
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    let mut i = 0;
    let mut expect_item = true;
    let mut nested = false;
    while i < toks.len() {
        let t = &toks[i];
        if t.depth != 0 {
            nested = true;
            i += 1;
            continue;
        }
        if nested {
            // Back at depth 0 after an item's braces: the next item.
            nested = false;
            expect_item = true;
        }
        if t.text == "#" {
            // An attribute: skip to its closing bracket.
            let mut j = i + 1;
            let mut brackets = 0i32;
            while j < toks.len() {
                match toks[j].text {
                    "[" => brackets += 1,
                    "]" => {
                        brackets -= 1;
                        if brackets == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        if expect_item {
            expect_item = false;
            if t.text == "pub" && toks.get(i + 1).is_some_and(|n| n.text == "(") {
                // `pub(crate)` / `pub(super)`: visible from the program too.
                if let Some(close) = toks[i + 1..].iter().find(|n| n.text == ")") {
                    edits.push((t.off, close.off + 1 - t.off, "pub"));
                }
            } else if matches!(t.text, "fn" | "const" | "static" | "struct" | "enum" | "type" | "trait" | "mod" | "union" | "unsafe" | "use")
                && !(t.text == "unsafe" && toks.get(i + 1).is_some_and(|n| n.text == "impl"))
            {
                edits.push((t.off, 0, "pub "));
            }
        }
        if t.text == ";" {
            expect_item = true;
        }
        i += 1;
    }
    let mut out = text.to_string();
    for (off, len, rep) in edits.into_iter().rev() {
        out.replace_range(off..off + len, rep);
    }
    out
}

fn header_items() -> &'static BTreeSet<&'static str> {
    static H: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    H.get_or_init(|| {
        let alias: &'static str = Box::leak(crate::render_native_str::almide_string_alias().into_boxed_str());
        let mut out = defined_names(FLOOR_HEADER);
        // The alias line defines the string's two names; its spliced module body
        // is the string's own, never a runtime item.
        out.extend(lex(alias).windows(2).filter(|w| w[0].text == "type" || w[0].text == "mod").map(|w| w[1].text));
        out.extend(lex(alias).windows(4).filter(|w| w[0].text == "use").map(|w| w[3].text));
        out
    })
}

/// Every name `src` defines at its top level, and inside the `mod` blocks it
/// defines there (a runtime module re-exports one with a glob,
/// `pub use almide_libm::*`, so its names are in scope unqualified).
fn defined_names(src: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    for (kind, name, text, _) in top_level_items(src) {
        // An impl defines no name: it is named by the type it is for.
        if kind == ItemKind::Impl {
            continue;
        }
        out.insert(name);
        let toks = lex(text);
        let is_mod = kind == ItemKind::Other && toks.iter().take_while(|t| t.text != name).any(|t| t.text == "mod");
        if let (true, Some(open), Some(close)) = (is_mod, text.find('{'), text.rfind('}')) {
            out.extend(defined_names(&text[open + 1..close]));
        }
    }
    out
}

/// The statics a top-level `thread_local! { static NAME: … }` defines: the
/// item scan does not see inside a macro, and none of them can be taken along.
fn thread_local_names(src: &str) -> BTreeSet<&str> {
    let toks = lex(src);
    let mut out = BTreeSet::new();
    for (i, t) in toks.iter().enumerate() {
        if t.text == "thread_local" && toks.get(i + 1).is_some_and(|n| n.text == "!") {
            let base = t.depth;
            let mut j = i + 3;
            while j < toks.len() && toks[j].depth > base {
                if toks[j].text == "static" {
                    if let Some(n) = toks.get(j + 1) {
                        out.insert(n.text);
                    }
                }
                j += 1;
            }
        }
    }
    out
}

/// The fn names the top-level `trait` items of `src` declare.
/// The traits `src` defines at its top level.
fn trait_names(src: &str) -> BTreeSet<&str> {
    top_level_items(src)
        .into_iter()
        .filter(|(kind, name, text, _)| *kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != *name).any(|t| t.text == "trait"))
        .map(|(_, name, _, _)| name)
        .collect()
}

/// Each trait `src` defines at its top level (with no `#[cfg]`), as (name,
/// its declaration, every top-level impl of it in `src`, its method names).
#[allow(clippy::type_complexity)]
fn prelude_traits(src: &'static str) -> Vec<(&'static str, &'static str, Vec<&'static str>, Vec<&'static str>)> {
    let toks = lex(src);
    // Top-level impls: `impl … Trait for …` → the trait name before `for`.
    let mut impls: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i].depth == 0 && toks[i].text == "impl" {
            let end = item_end(&toks, i);
            let for_at = (i..end).find(|&k| toks[k].text == "for" && toks[k].depth == 0);
            if let Some(f) = for_at.filter(|f| *f > i + 1) {
                let tr = toks[f - 1].text;
                let end_off = toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(src.len());
                impls.entry(tr).or_default().push(&src[toks[i].off..end_off]);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    let mut out = Vec::new();
    for (kind, name, text, attributed) in top_level_items(src) {
        let ltoks = lex(text);
        if kind != ItemKind::Other || attributed || !ltoks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            continue;
        }
        let methods: Vec<&'static str> = ltoks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text).collect();
        out.push((name, text, impls.get(name).cloned().unwrap_or_default(), methods));
    }
    out
}

fn trait_method_names(src: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    for (kind, name, text, _) in top_level_items(src) {
        let toks = lex(text);
        if kind == ItemKind::Other && toks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            out.extend(toks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text));
        }
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_alphabetic() || f == '_') && s != "_" && c.all(|x| x.is_alphanumeric() || x == '_')
}

// ── A small Rust lexer: identifiers and punctuation, comments and literals skipped ──

struct Tok<'a> {
    text: &'a str,
    off: usize,
    /// Brace depth (`{}` only) BEFORE this token.
    depth: u32,
}

fn lex(src: &str) -> Vec<Tok<'_>> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth: u32 = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            let mut d = 0;
            while i < b.len() {
                if b[i..].starts_with(b"/*") {
                    d += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    d -= 1;
                    i += 2;
                    if d == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == b'r' && matches!(b.get(i + 1), Some(b'"' | b'#')) && raw_string_end(b, i + 1).is_some() {
            let end = raw_string_end(b, i + 1).unwrap_or(b.len());
            out.push(Tok { text: &src[i..end], off: i, depth });
            i = end;
        } else if c == b'b' && b.get(i + 1) == Some(&b'"') {
            let end = string_end(b, i + 1);
            out.push(Tok { text: &src[i..end], off: i, depth });
            i = end;
        } else if c == b'"' {
            let end = string_end(b, i);
            out.push(Tok { text: &src[i..end], off: i, depth });
            i = end;
        } else if c == b'\'' || (c == b'b' && b.get(i + 1) == Some(&b'\'')) {
            let q = if c == b'b' { i + 1 } else { i };
            // A char literal ('x', '\n', '\u{..}') or a lifetime ('a).
            if let Some(end) = char_end(b, q) {
                out.push(Tok { text: &src[i..end], off: i, depth });
                i = end;
            } else {
                out.push(Tok { text: "'", off: i, depth });
                i = q + 1;
            }
        } else if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80) {
                i += 1;
            }
            out.push(Tok { text: &src[s..i], off: s, depth });
        } else if c.is_ascii_digit() {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || (b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))) {
                i += 1;
            }
            out.push(Tok { text: &src[s..i], off: s, depth });
        } else if b[i..].starts_with(b"::") {
            out.push(Tok { text: "::", off: i, depth });
            i += 2;
        } else {
            if c == b'}' {
                depth = depth.saturating_sub(1);
            }
            out.push(Tok { text: &src[i..i + 1], off: i, depth });
            if c == b'{' {
                depth += 1;
            }
            i += 1;
        }
    }
    out
}

fn string_end(b: &[u8], open: usize) -> usize {
    let mut i = open + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

fn raw_string_end(b: &[u8], at: usize) -> Option<usize> {
    let mut i = at;
    let mut hashes = 0;
    while b.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    if b.get(i) != Some(&b'"') {
        return None;
    }
    i += 1;
    while i < b.len() {
        if b[i] == b'"' && b[i + 1..].iter().take(hashes).filter(|&&x| x == b'#').count() == hashes {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(b.len())
}

fn char_end(b: &[u8], q: usize) -> Option<usize> {
    let mut i = q + 1;
    if b.get(i) == Some(&b'\\') {
        i += 2;
        while i < b.len() && b[i] != b'\'' && i - q < 12 {
            i += 1;
        }
        return (b.get(i) == Some(&b'\'')).then_some(i + 1);
    }
    // One scalar (possibly multi-byte) then a closing quote.
    let s = std::str::from_utf8(&b[i..(i + 4).min(b.len())]).ok().or_else(|| {
        (1..4).rev().find_map(|n| std::str::from_utf8(&b[i..(i + n).min(b.len())]).ok())
    })?;
    let ch = s.chars().next()?;
    let after = i + ch.len_utf8();
    (b.get(after) == Some(&b'\'')).then_some(after + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_skips_literals_and_tracks_depth() {
        let t = lex("fn a() { let s = \"}\"; let c = '{'; x(s) }\nfn b<'a>(x: &'a str) {}");
        let fns: Vec<_> = t.iter().filter(|t| t.text == "fn").map(|t| t.depth).collect();
        assert_eq!(fns, vec![0, 0]);
    }

    #[test]
    fn rust_signature_reads_params_and_ret() {
        let (p, r) = rust_signature("pub fn almide_rt_x<T: Clone>(xs: &[T], n: i64) -> Result<i64, AlmideString> { x }").unwrap();
        assert_eq!(p, vec!["&[T]", "i64"]);
        assert_eq!(r, "Result<i64, AlmideString>");
    }
}

// ── Rendering a declared floor call ──

use crate::lower::LowerError;
use crate::render_native::{as_f64_arg, as_str_arg, wall, NTy};

/// A declared list param's argument: a slice borrow of the list the render
/// carries, when its element kind is the declared one.
/// The owned value of kind `want` that a call arg of kind `got` gives.
fn owned_arg(want: NTy, code: &str, got: NTy) -> Option<String> {
    Some(match (want, got) {
        (NTy::I64, NTy::I64) => code.to_string(),
        (NTy::I64, NTy::F64) => format!("{code}.to_bits() as i64"),
        (NTy::Str, NTy::Str) => format!("{code}.clone()"),
        (NTy::Str, NTy::StrRef) => format!("AlmideString::from({code})"),
        (NTy::Vec, NTy::Vec | NTy::VecRef) => format!("{code}.to_vec()"),
        (NTy::VecStr, NTy::VecStr | NTy::VecStrRef) => format!("{code}.to_vec()"),
        (NTy::Rec(i), NTy::Rec(j) | NTy::RecRef(j)) if i == j => format!("{code}.clone()"),
        _ => return None,
    })
}

/// The (key, value) kinds of a generated list of pairs.
fn pair_kinds(list: u16) -> Option<(NTy, NTy)> {
    match crate::render_native_rec::list_elem(list)? {
        NTy::Rec(t) => Some((crate::render_native_rec::rec_field(t, 0)?, crate::render_native_rec::rec_field(t, 1)?)),
        _ => None,
    }
}

/// A generated list of `(String, Value)` tuples: the program's spelling of a
/// `List[(String, Value)]` (a struct per tuple, fields `f0` / `f1`).
fn is_str_value_pairs(list: u16) -> bool {
    match crate::render_native_rec::list_elem(list) {
        Some(NTy::Rec(t)) => {
            crate::render_native_rec::field_count(t) == 2
                && crate::render_native_rec::rec_field(t, 0) == Some(NTy::Str)
                && crate::render_native_rec::rec_field(t, 1).is_some_and(crate::render_native_rec::is_value)
        }
        _ => false,
    }
}

/// The runtime's owned `Vec<(AlmideString, AlmideValue)>` of a `(String,
/// Value)` list the render carries: its own tuples moved out when the caller
/// does not read it again (v0's move into the owned twin), else each copied.
fn owned_pairs(code: &str, got: NTy, dying: bool) -> Option<String> {
    match got {
        NTy::Rec(i) if is_str_value_pairs(i) && dying => Some(format!(
            "{}.into_iter().map(|t| (t.f0, t.f1)).collect::<Vec<_>>()",
            crate::render_native_move::take_expr(code)
        )),
        NTy::Rec(i) | NTy::RecRef(i) if is_str_value_pairs(i) => {
            Some(format!("{code}.iter().map(|t| (t.f0.clone(), t.f1.clone())).collect::<Vec<_>>()"))
        }
        _ => None,
    }
}

fn list_arg(elem: ListElem, code: &str, got: NTy) -> Option<String> {
    if elem == ListElem::StrValuePairs {
        return owned_pairs(code, got, false).map(|v| format!("&{v}"));
    }
    let elem_of = |i: u16| crate::render_native_rec::list_elem(i);
    match (elem, got) {
        // A type parameter's list: a generated list too, borrowed whole.
        (ListElem::Any, NTy::Rec(i)) if elem_of(i).is_some() => return Some(format!("&{code}")),
        (ListElem::Any, NTy::RecRef(i)) if elem_of(i).is_some() => return Some(code.to_string()),
        // The render's float list holds each cell's bits.
        (ListElem::Float, NTy::Vec | NTy::VecRef) => {
            return Some(format!("&{code}.iter().map(|x| f64::from_bits(*x as u64)).collect::<Vec<f64>>()"))
        }
        (ListElem::FloatRows, NTy::Rec(i) | NTy::RecRef(i)) if elem_of(i) == Some(NTy::Vec) => {
            return Some(format!(
                "&{code}.iter().map(|r| r.iter().map(|x| f64::from_bits(*x as u64)).collect::<Vec<f64>>()).collect::<Vec<Vec<f64>>>()"
            ))
        }
        (ListElem::Matrix, NTy::Rec(i)) if elem_of(i).is_some_and(crate::render_native_rec::is_matrix) => return Some(format!("&{code}")),
        (ListElem::Matrix, NTy::RecRef(i)) if elem_of(i).is_some_and(crate::render_native_rec::is_matrix) => return Some(code.to_string()),
        (ListElem::Float | ListElem::FloatRows | ListElem::Matrix, _) => return None,
        _ => {}
    }
    let int_list = matches!(elem, ListElem::Int | ListElem::Any);
    let str_list = matches!(elem, ListElem::Str | ListElem::Any);
    match got {
        NTy::Vec if int_list => Some(format!("&{code}")),
        NTy::VecRef if int_list => Some(code.to_string()),
        NTy::VecStr if str_list => Some(format!("&{code}")),
        NTy::VecStrRef if str_list => Some(code.to_string()),
        NTy::Rec(i) if elem == ListElem::Value && crate::render_native_rec::list_elem(i).is_some_and(crate::render_native_rec::is_value) => {
            Some(format!("&{code}"))
        }
        NTy::RecRef(i) if elem == ListElem::Value && crate::render_native_rec::list_elem(i).is_some_and(crate::render_native_rec::is_value) => {
            Some(code.to_string())
        }
        _ => None,
    }
}

/// Whether the owned twin can take the list arg `(code, got)` of `param` as
/// is: a `Vec<i64>` / `Vec<AlmideString>`, or a generated list the param
/// borrows whole (a `List[Value]`'s `Vec<AlmideValue>`), never one the call
/// converts first.
fn twin_takes(param: Option<&FloorParam>, (code, got): &(String, NTy)) -> bool {
    match (param, got) {
        (_, NTy::Vec | NTy::VecStr) => true,
        (Some(FloorParam::List(elem)), NTy::Rec(_)) => list_arg(*elem, code, *got).is_some_and(|r| r == format!("&{code}")),
        _ => false,
    }
}

/// The Rust expression calling `f`'s runtime body with `args` (each rendered
/// with the NTy it carries), and the NTy its result binds as (`None`: Unit).
pub(crate) fn render_call(
    f: &FloorFn,
    name: &str,
    args: &[(String, NTy)],
    dying: &[bool],
    dst_map: Option<u16>,
) -> Result<(String, Option<NTy>), LowerError> {
    if args.len() != f.params.len() {
        return Err(wall(format!("native: floor call `{name}` arity mismatch")));
    }
    let mismatch = || wall(format!("native: floor call `{name}` arg type mismatch"));
    // The (key, value) kinds of the call's map, from its `Map[K, V]` arg or
    // its `List[(K, V)]` pairs.
    let map_kv: Option<(NTy, NTy)> = f.params.iter().zip(args).find_map(|(p, (_, got))| match (p, got) {
        (FloorParam::Map(_), NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::map_kinds(*i),
        (FloorParam::MapPairs, NTy::Rec(i) | NTy::RecRef(i)) => pair_kinds(*i),
        _ => None,
    });
    // The element kind of the call's set, from its `Set[A]` arg.
    let set_e: Option<NTy> = f.params.iter().zip(args).find_map(|(p, (_, got))| match (p, got) {
        (FloorParam::Set(_), NTy::Rec(i) | NTy::RecRef(i)) => crate::render_native_rec::set_elem(*i),
        _ => None,
    });
    // The element kind of the call's `List[A]` arg, for an element-typed
    // param or return: an Int list's `i64`, a String list's `AlmideString`.
    // A generated list's element is its declared kind (a record, a nested
    // list, a runtime `Value`, …).
    let elem_kind: Option<NTy> = f
        .params
        .iter()
        .zip(args)
        .find(|(p, _)| matches!(p, FloorParam::List(ListElem::Any)))
        .and_then(|(_, (_, got))| match got {
            NTy::Vec | NTy::VecRef => Some(NTy::I64),
            NTy::VecStr | NTy::VecStrRef => Some(NTy::Str),
            NTy::Rec(i) | NTy::RecRef(i) => crate::render_native_rec::list_elem(*i),
            _ => None,
        });
    let elem_str = elem_kind.map(|k| k == NTy::Str);
    let mut rendered = Vec::new();
    for ((code, got), p) in args.iter().zip(&f.params) {
        rendered.push(match p {
            FloorParam::Int => (*got == NTy::I64).then(|| code.clone()).ok_or_else(mismatch)?,
            FloorParam::Bool => (*got == NTy::I64).then(|| format!("({code} != 0)")).ok_or_else(mismatch)?,
            FloorParam::Float => as_f64_arg(code, *got)?,
            FloorParam::Str => matches!(got, NTy::Str | NTy::StrRef)
                .then(|| as_str_arg(code, *got))
                .ok_or_else(mismatch)?,
            FloorParam::List(elem) => list_arg(*elem, code, *got).ok_or_else(mismatch)?,
            FloorParam::Elem(by_ref) => {
                let v = match (elem_kind, got) {
                    (Some(NTy::I64), NTy::I64) => code.clone(),
                    (Some(NTy::Str), NTy::Str) => format!("{code}.clone()"),
                    (Some(NTy::Str), NTy::StrRef) => format!("AlmideString::from({code})"),
                    (Some(want @ (NTy::Rec(_) | NTy::Vec | NTy::VecStr)), _) => owned_arg(want, code, *got).ok_or_else(mismatch)?,
                    _ => return Err(mismatch()),
                };
                if *by_ref { format!("&{v}") } else { v }
            }
            FloorParam::Map(by_ref) => {
                let ok = matches!(got, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::map_kinds(*i).is_some());
                if !ok {
                    return Err(mismatch());
                }
                match (got, by_ref, dying.get(rendered.len()).copied().unwrap_or(false)) {
                    (NTy::Rec(_), true, _) => format!("&{code}"),
                    (NTy::RecRef(_), true, _) => code.clone(),
                    // The caller's map, not read again: v0's move.
                    (NTy::Rec(_), false, true) => format!("std::mem::take(&mut {code})"),
                    _ => format!("{code}.clone()"),
                }
            }
            FloorParam::MapKey(by_ref) | FloorParam::MapVal(by_ref) => {
                let (k, v) = map_kv.ok_or_else(mismatch)?;
                let want = if matches!(p, FloorParam::MapKey(_)) { k } else { v };
                let owned = owned_arg(want, code, *got).ok_or_else(mismatch)?;
                let own_value = matches!(got, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_));
                match (by_ref, own_value, dying.get(rendered.len()).copied().unwrap_or(false)) {
                    // Borrow the caller's own value; a borrowed one is copied once.
                    (true, true, _) => format!("&{code}"),
                    (true, false, _) if *got == NTy::I64 => format!("&{code}"),
                    (true, false, _) => format!("&{owned}"),
                    // The caller's value, not read again: moved in.
                    (false, true, true) => format!("std::mem::take(&mut {code})"),
                    (false, _, _) => owned,
                }
            }
            FloorParam::MapPairs => {
                let (NTy::Rec(i) | NTy::RecRef(i)) = got else { return Err(mismatch()) };
                let pair = crate::render_native_rec::list_elem(*i).ok_or_else(mismatch)?;
                let (a, b) = match pair {
                    NTy::Vec | NTy::VecRef => ("t[0]".to_string(), "t[1]".to_string()),
                    // A pair of Strings the slot typing read as a String list.
                    NTy::VecStr | NTy::VecStrRef => ("t[0].clone()".to_string(), "t[1].clone()".to_string()),
                    NTy::Rec(j) if crate::render_native_rec::field_count(j) == 2 => ("t.f0.clone()".to_string(), "t.f1.clone()".to_string()),
                    _ => return Err(mismatch()),
                };
                format!("{code}.iter().map(|t| ({a}, {b})).collect::<Vec<_>>()")
            }
            FloorParam::Value(by_ref) => match (got, by_ref) {
                (NTy::Rec(_), true) if crate::render_native_rec::is_value(*got) => format!("&{code}"),
                (NTy::RecRef(_), true) if crate::render_native_rec::is_value(*got) => code.clone(),
                (NTy::Rec(_) | NTy::RecRef(_), false) if crate::render_native_rec::is_value(*got) => format!("{code}.clone()"),
                _ => return Err(mismatch()),
            },
            FloorParam::Set(by_ref) => {
                let ok = matches!(got, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::set_elem(*i).is_some());
                if !ok {
                    return Err(mismatch());
                }
                match (got, by_ref) {
                    (NTy::Rec(_), true) => format!("&{code}"),
                    (NTy::RecRef(_), true) => code.clone(),
                    _ => format!("{code}.clone()"),
                }
            }
            FloorParam::SetElem(by_ref) => {
                let want = set_e.ok_or_else(mismatch)?;
                let owned = owned_arg(want, code, *got).ok_or_else(mismatch)?;
                let own_value = matches!(got, NTy::Str);
                match (by_ref, own_value, dying.get(rendered.len()).copied().unwrap_or(false)) {
                    (true, true, _) => format!("&{code}"),
                    (true, false, _) if *got == NTy::I64 => format!("&{code}"),
                    (true, false, _) => format!("&{owned}"),
                    (false, true, true) => format!("std::mem::take(&mut {code})"),
                    (false, _, _) => owned,
                }
            }
            FloorParam::Matrix(by_ref) => match (got, by_ref) {
                (NTy::Rec(_), true) if crate::render_native_rec::is_matrix(*got) => format!("&{code}"),
                (NTy::RecRef(_), true) if crate::render_native_rec::is_matrix(*got) => code.clone(),
                (NTy::Rec(_) | NTy::RecRef(_), false) if crate::render_native_rec::is_matrix(*got) => format!("{code}.clone()"),
                _ => return Err(mismatch()),
            },
            FloorParam::Bytes => match got {
                NTy::Rec(_) if crate::render_native_rec::is_bytes(*got) => format!("&{code}"),
                NTy::RecRef(_) if crate::render_native_rec::is_bytes(*got) => code.clone(),
                _ => return Err(mismatch()),
            },
        });
    }
    // The list source not read again moves into the owned twin.
    let mut symbol = f.symbol.as_str();
    if let Some((k, twin, _)) = &f.twin {
        // A `(String, Value)` list is converted into the runtime's pairs in
        // any case: the owned twin takes that copy (or the moved-out source)
        // instead of copying it once more.
        if matches!(f.params.get(*k), Some(FloorParam::Set(_))) {
            // The caller's set, not read again: moved into the owned twin.
            if dying.get(*k) == Some(&true) && matches!(args.get(*k).map(|a| a.1), Some(NTy::Rec(_))) {
                rendered[*k] = crate::render_native_move::take_expr(&args[*k].0);
                symbol = twin;
            }
        } else if f.params.get(*k) == Some(&FloorParam::List(ListElem::StrValuePairs)) {
            let (code, got) = &args[*k];
            rendered[*k] = owned_pairs(code, *got, dying.get(*k) == Some(&true)).ok_or_else(mismatch)?;
            symbol = twin;
        } else if dying.get(*k) == Some(&true) && twin_takes(f.params.get(*k), &args[*k]) {
            rendered[*k] = crate::render_native_move::take_expr(&args[*k].0);
            symbol = twin;
        }
    }
    let call = format!("{symbol}({})", rendered.join(", "));
    Ok(match f.ret {
        FloorRet::Unit => (call, None),
        FloorRet::Int => (call, Some(NTy::I64)),
        FloorRet::Bool => (format!("({call}) as i64"), Some(NTy::I64)),
        FloorRet::Float => (call, Some(NTy::F64)),
        FloorRet::Str => (format!("AlmideString::from({call})"), Some(NTy::Str)),
        FloorRet::Res(k) => {
            let ok = match k {
                None => "|_| 0i64",
                Some(Scalar::Int) => "|v| v",
                Some(Scalar::Bool) => "|v| v as i64",
                Some(Scalar::Float) => "|v| v.to_bits() as i64",
            };
            (format!("({call}).map({ok}).map_err(AlmideString::from)"), Some(NTy::Res))
        }
        FloorRet::ResStr => (
            format!("({call}).map(AlmideString::from).map_err(AlmideString::from)"),
            Some(NTy::ResStr),
        ),
        FloorRet::ListInt => (call, Some(NTy::Vec)),
        FloorRet::Bytes => (call, Some(NTy::Rec(crate::render_native_rec::bytes_index()))),
        FloorRet::Value => (call, Some(NTy::Rec(crate::render_native_rec::value_index()))),
        FloorRet::Matrix => (call, Some(NTy::Rec(crate::render_native_rec::matrix_index()))),
        FloorRet::Set => match (set_e, dst_map) {
            (Some(e), _) => (call, Some(NTy::Rec(crate::render_native_rec::set_type_of(e)))),
            // No set arg (`set.new()`, `set.from_list(xs)`): the result's
            // declared set type, whose elements the list arg must be.
            (None, Some(i)) if crate::render_native_rec::set_elem(i).is_some() => {
                let e = crate::render_native_rec::set_elem(i);
                let list_ok = f.params.iter().zip(args).all(|(p, (_, got))| match p {
                    FloorParam::List(ListElem::Any) => match e {
                        Some(NTy::I64) => matches!(got, NTy::Vec | NTy::VecRef),
                        Some(NTy::Str) => matches!(got, NTy::VecStr | NTy::VecStrRef),
                        _ => false,
                    },
                    _ => true,
                });
                if !list_ok {
                    return Err(mismatch());
                }
                (call, Some(NTy::Rec(i)))
            }
            // Or the type the self-hosted body this call specializes declares.
            _ => match specialized_ret(name).as_ref().and_then(|t| declared_kinds(t, "Set")) {
                Some(e) if e.len() == 1 && matches!(e[0], NTy::I64 | NTy::Str) && set_list_agrees(&f.params, args, e[0]) => {
                    (call, Some(NTy::Rec(crate::render_native_rec::set_type_of(e[0]))))
                }
                _ => return Err(mismatch()),
            },
        },
        FloorRet::SetElems => match set_e.ok_or_else(mismatch)? {
            NTy::I64 => (call, Some(NTy::Vec)),
            NTy::Str => (call, Some(NTy::VecStr)),
            _ => return Err(mismatch()),
        },
        FloorRet::OptMatrix => {
            let m = NTy::Rec(crate::render_native_rec::matrix_index());
            (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(m))))
        }
        FloorRet::ListMatrix => {
            let m = NTy::Rec(crate::render_native_rec::matrix_index());
            (call, Some(NTy::Rec(crate::render_native_rec::list_type_of(m))))
        }
        FloorRet::FloatRows => (
            format!("({call}).into_iter().map(|r| r.into_iter().map(|x| x.to_bits() as i64).collect::<Vec<i64>>()).collect::<Vec<Vec<i64>>>()"),
            Some(NTy::Rec(crate::render_native_rec::list_type_of(NTy::Vec))),
        ),
        FloorRet::ResValue => {
            let v = NTy::Rec(crate::render_native_rec::value_index());
            let r = crate::render_native_rec::result_type_of(v, NTy::Str);
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(r)))
        }
        FloorRet::OptStr => (
            format!("({call}).map(AlmideString::from)"),
            Some(NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str))),
        ),
        FloorRet::ResListStr => (
            format!("({call}).map(|v| v.into_iter().map(AlmideString::from).collect::<Vec<AlmideString>>()).map_err(AlmideString::from)"),
            Some(NTy::Rec(crate::render_native_rec::result_type_of(NTy::VecStr, NTy::Str))),
        ),
        FloorRet::ResListValue => {
            let l = NTy::Rec(crate::render_native_rec::list_type_of(NTy::Rec(crate::render_native_rec::value_index())));
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(crate::render_native_rec::result_type_of(l, NTy::Str))))
        }
        FloorRet::ResBytes => {
            let b = NTy::Rec(crate::render_native_rec::bytes_index());
            (format!("({call}).map_err(AlmideString::from)"), Some(NTy::Rec(crate::render_native_rec::result_type_of(b, NTy::Str))))
        }
        FloorRet::Map => match (map_kv, dst_map) {
            (Some((k, v)), _) => (call, Some(NTy::Rec(crate::render_native_rec::map_type_of(k, v)))),
            // No map arg (`map.new()`): the result's declared map type.
            (None, Some(i)) => (call, Some(NTy::Rec(i))),
            // Or the type the self-hosted body this call specializes declares
            // (`map_new_skv() -> Map[String, Int]`).
            (None, None) => match specialized_ret(name).as_ref().and_then(|t| declared_kinds(t, "Map")) {
                Some(kv) if kv.len() == 2 => (call, Some(NTy::Rec(crate::render_native_rec::map_type_of(kv[0], kv[1])))),
                _ => return Err(mismatch()),
            },
        },
        FloorRet::MapOptVal => match map_kv.ok_or_else(mismatch)?.1 {
            NTy::I64 => (call, Some(NTy::Opt)),
            v => (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(v)))),
        },
        FloorRet::MapKeys | FloorRet::MapVals => {
            let (k, v) = map_kv.ok_or_else(mismatch)?;
            let e = if f.ret == FloorRet::MapKeys { k } else { v };
            let t = match e {
                NTy::I64 => NTy::Vec,
                NTy::Str => NTy::VecStr,
                other => NTy::Rec(crate::render_native_rec::list_type_of(other)),
            };
            (call, Some(t))
        }
        FloorRet::ResOptStr => {
            let o = NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str));
            (
                format!("({call}).map(|o| o.map(AlmideString::from)).map_err(AlmideString::from)"),
                Some(NTy::Rec(crate::render_native_rec::result_type_of(o, NTy::Str))),
            )
        }
        FloorRet::ListStr => (
            format!("({call}).into_iter().map(AlmideString::from).collect::<Vec<AlmideString>>()"),
            Some(NTy::VecStr),
        ),
        FloorRet::Opt(Scalar::Int) => (call, Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Bool) => (format!("({call}).map(|b| b as i64)"), Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Float) => (format!("({call}).map(|x| x.to_bits() as i64)"), Some(NTy::Opt)),
        FloorRet::Elem(shape) => match (elem_str, shape) {
            // A generated list's element: itself, its generated `Option`, its list.
            (Some(false), _) if matches!(elem_kind, Some(NTy::Rec(_)) | Some(NTy::Vec) | Some(NTy::VecStr)) => {
                let e = elem_kind.expect("matched");
                let t = match shape {
                    ElemRet::Value => e,
                    ElemRet::Opt => NTy::Rec(crate::render_native_rec::option_type_of(e)),
                    ElemRet::List => NTy::Rec(crate::render_native_rec::list_type_of(e)),
                };
                (call, Some(t))
            }
            (Some(false), ElemRet::Value) => (call, Some(NTy::I64)),
            (Some(false), ElemRet::Opt) => (call, Some(NTy::Opt)),
            (Some(false), ElemRet::List) => (call, Some(NTy::Vec)),
            (Some(true), ElemRet::Value) => (call, Some(NTy::Str)),
            (Some(true), ElemRet::List) => (call, Some(NTy::VecStr)),
            // An Option of a String element: the generated `Option<AlmideString>` carrier.
            (Some(true), ElemRet::Opt) => (call, Some(NTy::Rec(crate::render_native_rec::option_type_of(NTy::Str)))),
            _ => return Err(wall(format!("native: floor call `{name}` element-typed result outside the native carriers"))),
        },
    })
}
