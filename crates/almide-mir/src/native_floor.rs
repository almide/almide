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
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};
use crate::native_runtime_types::RuntimeType;

include!("native_floor_header.rs");

/// A declared parameter's native kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FloorParam {
    /// `Int` — the runtime takes `i64`.
    Int,
    /// `Bool` — the runtime takes `bool`; the render's i64 tests `!= 0`.
    Bool,
    /// `Float` — the runtime takes `f64`.
    Float,
    /// `Float32` — decode the lane's low binary32 bits as an `f32`.
    Float32,
    /// `String` — the runtime borrows `&str`.
    Str,
    /// `List[E]` — the runtime borrows a slice (`&[T]` / `&Vec<T>`).
    List(ListElem),
    /// `List[E]` passed to an owned runtime `Vec<T>` parameter.
    ListOwned(ListElem),
    /// `Bytes` — the runtime borrows its `Vec<u8>` (`&Vec<u8>` / `&[u8]`).
    Bytes,
    /// `mut b: Bytes` — the runtime writes the caller's own `Vec<u8>` in
    /// place (`&mut Vec<u8>`): the MIR call with no result that edits its
    /// argument's block, on a render where every binding owns its value.
    BytesMut,
    /// The stdlib `Value` — v0's runtime `AlmideValue`, borrowed (`true`)
    /// or by value (`false`).
    Value(bool),
    /// An opaque stdlib value, borrowing or cloning the runtime's own type.
    Runtime(RuntimeType, bool),
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
    /// `List[Bool]` — a slice of `bool` (the render's list holds 0 / 1).
    Bool,
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
    Runtime(RuntimeType),
    /// Result<T, String>, optionally enclosed in Option (HTTP poll).
    RuntimeResult(RuntimeType, bool),
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
    ResStrValuePairs,
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
    /// `Result[Bytes?, String]` owns the optional raw byte buffer.
    ResOptBytes,
    /// An optional owned list of strings returned by a host read.
    ResOptListStr,
    /// The fs module's checked four-field metadata record.
    ResFileStat,
    /// The process module's checked status record.
    ResProcessStatus,
    /// A socket datagram: owned bytes, sender text, sender port.
    ResBytesStrInt,
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
    /// `List[(K, V)]` of the call's map: the runtime's `Vec<(K, V)>`, rebound
    /// as the generated list of the program's `(K, V)` tuple.
    MapEntries,
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
    /// An equality- or ordering-bound generic served for String elements
    /// only (owner ruling ○, 2026-11-04): the call's list must be a String
    /// list, which carries no Float bits.
    pub str_only: bool,
    /// The element of the `List[A]` param is ordering-bound (`list.sort`,
    /// `list.min`, …): a generated struct or enum derives no ordering, so a
    /// call over a list of declared records or variants walls.
    pub ordered: bool,
    /// The runtime's borrowing `_ref` twin of a `Result[Value, String]` body
    /// (its symbol, its items): the Ok side a reference into the Value arg.
    pub borrow_twin: Option<(String, Vec<&'static str>)>,
    /// The runtime's `_ref` twin taking each by-value Value param by reference.
    pub by_ref: Option<Box<FloorFn>>,
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

/// The scalar a stdlib fn declared `@scalar_text` turns into its canonical
/// text: `Some(ScalarText::Int)` for an `Int` param (decimal digits),
/// `Some(ScalarText::Bool)` for a `Bool` one (`true` / `false`),
/// `Some(ScalarText::Float)` for a `Float` one (its `{}` Display), `None` for a
/// fn without the declaration. The certified render folds such a call into
/// the format string of the text it builds, reading only this declaration.
pub(crate) fn declared_scalar_text(call: &str) -> Option<ScalarText> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<ScalarText>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(call) {
        return *hit;
    }
    let made = scalar_text_of(call);
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(call.to_string(), made);
    made
}

/// What a `@scalar_text` fn formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScalarText {
    Int,
    Bool,
    Float,
}

fn scalar_text_of(call: &str) -> Option<ScalarText> {
    let (module, func) = call.split_once('.')?;
    let program = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached)?;
    // A self-hosted body that declares the fn it specializes
    // (`@specializes("float.__to_string_compound")`) has that fn's text, as
    // the floor serves it as that fn ([`derive_for`]).
    let declared_here = program.decls.iter().any(|d| matches!(d, Decl::Fn { name, .. } if name.as_str() == func));
    if !declared_here {
        return specialized_target(call).filter(|t| t != call).and_then(|t| scalar_text_of(&t));
    }
    program.decls.iter().find_map(|d| match d {
        Decl::Fn { name, attrs, params, .. }
            if name.as_str() == func && attrs.iter().any(|a| a.name.as_str() == "scalar_text") =>
        {
            match params.as_slice() {
                [p] => match simple(&p.ty) {
                    Some("Int") => Some(ScalarText::Int),
                    Some("Bool") => Some(ScalarText::Bool),
                    Some("Float") => Some(ScalarText::Float),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    })
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
        FloorParam::List(ListElem::Any) | FloorParam::ListOwned(ListElem::Any) => match e {
            NTy::I64 | NTy::F32 => matches!(got, NTy::Vec | NTy::VecRef),
            NTy::Str => matches!(got, NTy::VecStr | NTy::VecStrRef),
            e => matches!(got, NTy::Rec(i) | NTy::RecRef(i) if crate::render_native_rec::list_elem(*i).is_some_and(|got| crate::native_value_repr::same_owned(got, e))),
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
/// Whether the declared floor fn of `call` takes its param `k` by value (the
/// runtime body consumes it, or its `@reuse` twin does at a last use, where
/// the borrowing body would copy that container into its result — an update
/// that returns the same kind of container): a heap param handed there at
/// its last use is the body's to give away. A twin whose borrowing body only
/// reads the container (`map.values`) gains nothing from taking it over, and
/// a caller that reads its argument again would copy it whole for the call.
pub(crate) fn takes_by_value(call: &str, k: usize) -> bool {
    declared(call).is_some_and(|f| {
        // The render calls the `_ref` twin where there is one: it lends.
        let f = f.by_ref.as_deref().unwrap_or(f);
        matches!(f.params.get(k), Some(FloorParam::Map(false) | FloorParam::Set(false) | FloorParam::Value(false) | FloorParam::Matrix(false) | FloorParam::ListOwned(_)))
            || f.twin.as_ref().is_some_and(|t| {
                t.0 == k
                    && matches!(
                        (f.params.get(k), f.ret),
                        (Some(FloorParam::List(_)), FloorRet::ListInt | FloorRet::ListStr | FloorRet::Elem(ElemRet::List))
                            | (Some(FloorParam::Set(_)), FloorRet::Set)
                            | (Some(FloorParam::Map(_)), FloorRet::Map)
                            | (Some(FloorParam::Bytes), FloorRet::Bytes)
                    )
            })
    })
}

/// Whether the declared floor fn of `call` borrows a list at param `k` (a
/// slice: it reads the list and keeps no part of it).
pub(crate) fn borrows_list(call: &str, k: usize) -> bool {
    declared(call).is_some_and(|f| matches!(f.params.get(k), Some(FloorParam::List(_))) && f.twin.as_ref().is_none_or(|t| t.0 != k))
}

/// The generated type of param `k` of the self-hosted body a floor call
/// specializes, when that body spells a concrete one (`map.len_skv`'s
/// `Map[String, V]` over a program's `Map[String, Int]` is not one).
pub(crate) fn specialized_param_rec(call: &str, k: usize) -> Option<u16> {
    declared(call)?;
    let t = crate::mir_ops::self_host_runtime().iter().find_map(|(src, pairs)| {
        let (impl_fn, _) = pairs.iter().find(|(_, c)| *c == call)?;
        almide_lang::parse_cached(src)?.decls.iter().find_map(|d| match d {
            Decl::Fn { name, params, .. } if name.as_str() == *impl_fn => params.get(k).map(|p| p.ty.clone()),
            _ => None,
        })
    })?;
    crate::render_native_rec::rec_index(&crate::render_native_rec::tuple_key(&ty_of(&t)?))
}

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
    out.push_str(floor_header());
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
    derive_for(call, false)
}

/// The element type of the first list param the self-hosted body `call`
/// names declares (`list.contains_str`'s `List[String]` → `String`).
fn specialized_list_elem(call: &str) -> Option<String> {
    crate::mir_ops::self_host_runtime().iter().find_map(|(src, pairs)| {
        let (impl_fn, _) = pairs.iter().find(|(_, c)| *c == call)?;
        almide_lang::parse_cached(src)?.decls.iter().find_map(|d| match d {
            Decl::Fn { name, params, .. } if name.as_str() == *impl_fn => params.iter().find_map(|p| match &p.ty {
                TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => simple(&args[0]).map(str::to_string),
                _ => None,
            }),
            _ => None,
        })
    })
}

/// Whether the element type of the first list param the self-hosted body
/// `call` names declares holds a Float at any depth (`List[List[Float]]`'s
/// `List[Float]`, `List[Float?]`'s `Float?`): the native render carries every
/// such Float as its bits.
pub(crate) fn specialized_list_elem_holds_float(call: &str) -> bool {
    fn holds_float(t: &TypeExpr) -> bool {
        match t {
            TypeExpr::Simple { name } => name.as_str() == "Float",
            TypeExpr::Generic { args, .. } => args.iter().any(holds_float),
            TypeExpr::Tuple { elements } => elements.iter().any(holds_float),
            TypeExpr::Union { members } => members.iter().any(holds_float),
            TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => fields.iter().any(|f| holds_float(&f.ty)),
            _ => false,
        }
    }
    crate::mir_ops::self_host_runtime().iter().any(|(src, pairs)| {
        pairs.iter().filter(|(_, c)| *c == call).any(|(impl_fn, _)| {
            almide_lang::parse_cached(src).is_some_and(|program| program.decls.iter().any(|d| match d {
                Decl::Fn { name, params, .. } if name.as_str() == *impl_fn => params.iter().find_map(|p| match &p.ty {
                    TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => Some(holds_float(&args[0])),
                    _ => None,
                }) == Some(true),
                _ => false,
            }))
        })
    })
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
            "Float32" => Some(FloorParam::Float32),
            "String" => Some(FloorParam::Str),
            "Bytes" => Some(FloorParam::Bytes),
            "Matrix" => Some(FloorParam::Matrix(false)),
            n if n == VALUE_TYPE => Some(FloorParam::Value(false)),
            n if RuntimeType::of(n).is_some() => Some(FloorParam::Runtime(RuntimeType::of(n)?, false)),
            // Whether it is a list's element is checked against the params,
            // and its by-value/by-reference mode against the runtime, below.
            a if type_params.contains(a) => Some(FloorParam::Elem(false)),
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => floor_list_param(&args[0], type_params),
        _ => None,
    }
}

/// The param kind of a `List[elem]`.
fn floor_list_param(elem: &TypeExpr, type_params: &BTreeSet<String>) -> Option<FloorParam> {
    if let TypeExpr::Tuple { elements } = elem {
        let pair: Vec<Option<&str>> = elements.iter().map(simple).collect();
        return (pair == [Some("String"), Some(VALUE_TYPE)]).then_some(FloorParam::List(ListElem::StrValuePairs));
    }
    if let TypeExpr::Generic { name, args: inner } = elem {
        let rows = name.as_str() == "List" && inner.len() == 1 && simple(&inner[0]) == Some("Float");
        return rows.then_some(FloorParam::List(ListElem::FloatRows));
    }
    let elem = match simple(elem)? {
        "Int" => ListElem::Int,
        "Float" => ListElem::Float,
        "Bool" => ListElem::Bool,
        "Matrix" => ListElem::Matrix,
        "String" => ListElem::Str,
        n if n == VALUE_TYPE => ListElem::Value,
        a if type_params.contains(a) => ListElem::Any,
        _ => return None,
    };
    Some(FloorParam::List(elem))
}

/// The stdlib's dynamic data model, as its declarations spell it (the one
/// type `crate::lower::is_value_ty` names).
const VALUE_TYPE: &str = "Value";

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
        "FileStat" => Some(FloorRet::ResFileStat),
        "ProcessStatus" => Some(FloorRet::ResProcessStatus),
        "String" => Some(FloorRet::ResStr),
        n if n == VALUE_TYPE => Some(FloorRet::ResValue),
        n if RuntimeType::of(n).is_some() => Some(FloorRet::RuntimeResult(RuntimeType::of(n)?, false)),
        ok => scalar_of(ok).map(|k| FloorRet::Res(Some(k))),
    }
}

/// `T?` parses as the generic `?`; `Option[T]` spells it out.
fn is_option_name(n: &str) -> bool {
    n == "?" || n == "Option"
}

include!("native_floor_sig.rs");
include!("native_floor_index.rs");
include!("native_floor_host.rs");
include!("native_floor_scan.rs");
include!("native_floor_resolve.rs");
include!("native_floor_generated.rs");
include!("native_floor_tail.rs");
include!("native_floor_ret.rs");
include!("native_floor_kinds.rs");
include!("native_floor_parallel.rs");
include!("native_floor_derive.rs");
