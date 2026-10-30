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
    /// A bare type parameter `A` that is the element of a `List[A]` param:
    /// the runtime takes the element by value (`T`, false) or by reference
    /// (`&T`, true), and the call site's list decides its kind.
    Elem(bool),
}

/// A declared list's element type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListElem {
    Int,
    Str,
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
    /// `Result[Int | Bool, String]` — the native Result carrier.
    Res,
    /// `List[Int]` — an owned `Vec<i64>`.
    ListInt,
    /// `List[String]` — a runtime `Vec<String>`, rebound as `Vec<AlmideString>`.
    ListStr,
    /// `Bytes` — the runtime's own `Vec<u8>`.
    Bytes,
    /// `Int? | Bool? | Float?` — a runtime `Option` of the scalar, carried as
    /// the native `Option<i64>` (`NTy::Opt`; a Float as its bits).
    Opt(Scalar),
    /// A return typed by the element `A` of the call's `List[A]` param: `A`,
    /// `A?` or `List[A]` (the call site's list decides the kind).
    Elem(ElemRet),
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
        Decl::Fn { name, effect, attrs, generics, params, return_type, .. } if name.as_str() == func => {
            Some((effect, attrs, generics, params, return_type))
        }
        _ => None,
    });
    let (effect, attrs, generics, params, ret) = decls.next().ok_or("no declaration")?;
    if decls.next().is_some() {
        return Err("declared more than once".into());
    }
    if effect.unwrap_or(false) {
        return Err("an effect fn".into());
    }
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
    let fparams: Vec<FloorParam> = params
        .iter()
        .map(|p| floor_param(&p.ty, &type_params).ok_or_else(|| format!("param type {:?} outside the per-type map", p.ty)))
        .collect::<Result<_, _>>()?;
    let fret = floor_ret(ret, &type_params).ok_or_else(|| format!("return type {ret:?} outside the per-type map"))?;
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
            other => other,
        })
        .collect();
    let items = closure(&symbol, rt)?;
    Ok(FloorFn { symbol, params: fparams, ret: fret, items })
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
            // Whether it is a list's element is checked against the params,
            // and its by-value/by-reference mode against the runtime, below.
            a if type_params.contains(a) => Some(FloorParam::Elem(false)),
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "List" && args.len() == 1 => {
            let elem = match simple(&args[0])? {
                "Int" => ListElem::Int,
                "String" => ListElem::Str,
                a if type_params.contains(a) => ListElem::Any,
                _ => return None,
            };
            Some(FloorParam::List(elem))
        }
        _ => None,
    }
}

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
            _ => None,
        },
        TypeExpr::Generic { name, args } if name.as_str() == "Result" && args.len() == 2 => {
            matches!(simple(&args[0])?, "Int" | "Bool")
                .then_some(())
                .filter(|_| simple(&args[1]) == Some("String"))
                .map(|_| FloorRet::Res)
        }
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
        FloorParam::Elem(_) => is_type_param(&r) || r.strip_prefix('&').is_some_and(is_type_param),
        FloorParam::List(elem) => {
            let inner = r
                .strip_prefix("&[")
                .and_then(|x| x.strip_suffix(']'))
                .or_else(|| r.strip_prefix("&Vec<").and_then(|x| x.strip_suffix('>')));
            match (elem, inner) {
                (ListElem::Int, Some(i)) => i == "i64" || is_type_param(i),
                (ListElem::Str, Some(i)) => i == "AlmideString" || is_type_param(i),
                (ListElem::Any, Some(i)) => is_type_param(i),
                _ => false,
            }
        }
    }
}

fn is_type_param(s: &str) -> bool {
    s.len() == 1 && s.chars().all(|c| c.is_ascii_uppercase())
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
        FloorRet::Res => t
            .strip_prefix("Result<")
            .and_then(|x| x.strip_suffix('>'))
            .and_then(|x| x.split_once(','))
            .is_some_and(|(ok, err)| (ok == "i64" || ok == "bool") && string(err)),
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
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            for (kind, name, text, attributed) in top_level_items(src) {
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
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            defined.extend(defined_names(src));
            trait_methods.extend(trait_method_names(src));
        }
        RuntimeIndex { items, defined, trait_methods }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Fn,
    Const,
    Other,
}

/// The top-level items of a runtime module: (kind, name, whole text, whether
/// a `#[cfg…]` attribute precedes it).
fn top_level_items<'a>(src: &'a str) -> Vec<(ItemKind, &'a str, &'a str, bool)> {
    let toks = lex(src);
    let mut out = Vec::new();
    let mut i = 0;
    let mut cfg_attr = false;
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
            "struct" | "enum" | "type" | "trait" | "mod" | "union" | "macro_rules" => (ItemKind::Other, j + 1),
            "impl" | "use" => {
                // No name of its own: skip to its end.
                let end = item_end(&toks, j);
                i = end + 1;
                cfg_attr = false;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        };
        let name_at = if kw.text == "macro_rules" { name_at + 1 } else { name_at };
        let Some(name) = toks.get(name_at).map(|t| t.text) else { break };
        let end = item_end(&toks, name_at);
        let end_off = toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(src.len());
        out.push((kind, name, &src[start_off..end_off], cfg_attr));
        cfg_attr = false;
        i = end + 1;
    }
    out
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
    let close = (open + 1..toks.len()).find(|&k| toks[k].text == ")" && toks[k].depth == toks[open].depth)?;
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
    let toks = lex(text);
    let rt = runtime_index();
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
        if matches!(prev, Some("." | "::" | "'")) {
            continue; // a method, a field, a path tail, a lifetime
        }
        if prev == Some("fn") {
            continue; // an item's own name
        }
        if matches!(prev, Some("let" | "mut" | "|" | "ref")) || (next == Some(":") && prev != Some("::")) {
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
        let used_as_item = matches!(next, Some("(" | "::" | "!")) || s.chars().next().is_some_and(|c| c.is_uppercase());
        if rt.defined.contains(s) && used_as_item {
            return Err(s.to_string());
        }
    }
    Ok(out)
}

/// The names the native header defines for a floor body.
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
        out.insert(name);
        let toks = lex(text);
        let is_mod = kind == ItemKind::Other && toks.iter().take_while(|t| t.text != name).any(|t| t.text == "mod");
        if let (true, Some(open), Some(close)) = (is_mod, text.find('{'), text.rfind('}')) {
            out.extend(defined_names(&text[open + 1..close]));
        }
    }
    out
}

/// The fn names the top-level `trait` items of `src` declare.
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
fn list_arg(elem: ListElem, code: &str, got: NTy) -> Option<String> {
    let int_list = matches!(elem, ListElem::Int | ListElem::Any);
    let str_list = matches!(elem, ListElem::Str | ListElem::Any);
    match got {
        NTy::Vec if int_list => Some(format!("&{code}")),
        NTy::VecRef if int_list => Some(code.to_string()),
        NTy::VecStr if str_list => Some(format!("&{code}")),
        NTy::VecStrRef if str_list => Some(code.to_string()),
        _ => None,
    }
}

/// The Rust expression calling `f`'s runtime body with `args` (each rendered
/// with the NTy it carries), and the NTy its result binds as (`None`: Unit).
pub(crate) fn render_call(f: &FloorFn, name: &str, args: &[(String, NTy)]) -> Result<(String, Option<NTy>), LowerError> {
    if args.len() != f.params.len() {
        return Err(wall(format!("native: floor call `{name}` arity mismatch")));
    }
    let mismatch = || wall(format!("native: floor call `{name}` arg type mismatch"));
    // The element kind of the call's `List[A]` arg, for an element-typed
    // param or return: an Int list's `i64`, a String list's `AlmideString`.
    let elem_str = f
        .params
        .iter()
        .zip(args)
        .find(|(p, _)| matches!(p, FloorParam::List(ListElem::Any)))
        .map(|(_, (_, got))| matches!(got, NTy::VecStr | NTy::VecStrRef));
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
                let v = match (elem_str, got) {
                    (Some(false), NTy::I64) => code.clone(),
                    (Some(true), NTy::Str) => format!("{code}.clone()"),
                    (Some(true), NTy::StrRef) => format!("AlmideString::from({code})"),
                    _ => return Err(mismatch()),
                };
                if *by_ref { format!("&{v}") } else { v }
            }
            FloorParam::Bytes => match got {
                NTy::Rec(_) if crate::render_native_rec::is_bytes(*got) => format!("&{code}"),
                NTy::RecRef(_) if crate::render_native_rec::is_bytes(*got) => code.clone(),
                _ => return Err(mismatch()),
            },
        });
    }
    let call = format!("{}({})", f.symbol, rendered.join(", "));
    Ok(match f.ret {
        FloorRet::Unit => (call, None),
        FloorRet::Int => (call, Some(NTy::I64)),
        FloorRet::Bool => (format!("({call}) as i64"), Some(NTy::I64)),
        FloorRet::Float => (call, Some(NTy::F64)),
        FloorRet::Str => (format!("AlmideString::from({call})"), Some(NTy::Str)),
        FloorRet::Res => (
            format!("({call}).map(|v| v as i64).map_err(AlmideString::from)"),
            Some(NTy::Res),
        ),
        FloorRet::ListInt => (call, Some(NTy::Vec)),
        FloorRet::Bytes => (call, Some(NTy::Rec(crate::render_native_rec::bytes_index()))),
        FloorRet::ListStr => (
            format!("({call}).into_iter().map(AlmideString::from).collect::<Vec<AlmideString>>()"),
            Some(NTy::VecStr),
        ),
        FloorRet::Opt(Scalar::Int) => (call, Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Bool) => (format!("({call}).map(|b| b as i64)"), Some(NTy::Opt)),
        FloorRet::Opt(Scalar::Float) => (format!("({call}).map(|x| x.to_bits() as i64)"), Some(NTy::Opt)),
        FloorRet::Elem(shape) => match (elem_str, shape) {
            (Some(false), ElemRet::Value) => (call, Some(NTy::I64)),
            (Some(false), ElemRet::Opt) => (call, Some(NTy::Opt)),
            (Some(false), ElemRet::List) => (call, Some(NTy::Vec)),
            (Some(true), ElemRet::Value) => (call, Some(NTy::Str)),
            (Some(true), ElemRet::List) => (call, Some(NTy::VecStr)),
            // An Option of a String is a heap payload, outside this carrier.
            _ => return Err(wall(format!("native: floor call `{name}` element-typed result outside the native carriers"))),
        },
    })
}
