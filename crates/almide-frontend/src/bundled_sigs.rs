//! Bundled-stdlib signature resolution.
//!
//! When a stdlib module is migrated from `stdlib/defs/*.toml` to
//! `stdlib/<m>.almd` (Stdlib Declarative Unification Stage 2+), the
//! generated `stdlib_sigs.rs` lookup table loses its entries for that
//! module. The type checker still needs signatures to resolve
//! `module.fn(...)` calls — this module fills the gap.
//!
//! The approach is deliberately narrow: we embed the bundled source
//! via `stdlib::get_bundled_source`, parse it once per module on first
//! use, extract each `Decl::Fn`'s signature, and cache the result in
//! a process-wide map. Lookup is O(1) after the first call.
//!
//! # Why runtime parsing rather than build-time generation?
//!
//! The TOML pipeline runs a build script that reads TOML, converts
//! every entry into a literal Rust match arm, and writes the result
//! to `crates/almide-frontend/src/generated/stdlib_sigs.rs`. Doing
//! the same for bundled `.almd` would require invoking the Almide
//! parser from a build script, which would drag `almide-syntax` into
//! this crate's build dependency graph (it's currently only a normal
//! dependency).
//!
//! Per-call cost is negligible — each bundled module parses once, and
//! the cached sig map is a plain `HashMap`. The tradeoff (small
//! runtime parse, no build-graph churn) is the right one for the
//! arc's incremental migration cadence. A future build-time extractor
//! can replace this module without changing any call site.
//!
//! The actual parse is delegated to `almide_syntax::parse_cached`, a
//! shared process-wide AST cache. The codegen `pass_stdlib_lowering`
//! consumes the same cache to extract `@inline_rust` templates, so
//! both views of every bundled module derive from a single parse.

use std::collections::HashMap;
use std::sync::OnceLock;

use almide_base::intern::{sym, Sym};
use almide_lang::ast;

use crate::types::FnSig;

/// Per-module cache of (module, fn-name) → FnSig. `OnceLock` gives us
/// lazy, thread-safe initialization without a heavier `Mutex`. The
/// value is fully populated on first access and never mutated again.
fn cache() -> &'static std::sync::RwLock<HashMap<&'static str, HashMap<Sym, FnSig>>> {
    static CELL: OnceLock<std::sync::RwLock<HashMap<&'static str, HashMap<Sym, FnSig>>>> =
        OnceLock::new();
    CELL.get_or_init(|| std::sync::RwLock::new(HashMap::new()))
}

/// Look up a bundled-stdlib fn signature. Returns `None` if `module`
/// is not a bundled stdlib module or `func` is absent from its
/// bundled source.
pub fn lookup(module: &str, func: &str) -> Option<FnSig> {
    with_module(module, |sigs| sigs.get(&sym(func)).cloned()).flatten()
}

/// Return every fn name declared in a bundled stdlib module. Used by
/// reflection paths (outline, docs-gen, ...) that need the full
/// declared surface regardless of whether it originated in TOML or
/// bundled source. Names are interned `&'static str` so downstream
/// callers that expect the TOML contract (static lifetime) keep
/// working; the source string itself is `&'static` via `include_str!`.
pub fn module_fn_names(module: &str) -> Vec<&'static str> {
    with_module(module, |sigs| {
        sigs.keys()
            .map(|k| intern_static(k.as_str()))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

/// Lazily populate the per-module cache and apply a read-only
/// projection.
fn with_module<T>(module: &str, f: impl FnOnce(&HashMap<Sym, FnSig>) -> T) -> Option<T> {
    if let Some(map) = cached_module(module) {
        return Some(f(&map));
    }
    let sigs = build_module_sigs(module)?;
    let result = f(&sigs);
    store_module(module, sigs);
    Some(result)
}

/// Intern a sym's string as a process-lifetime leak. Used so the
/// `module_fn_names` API can return `&'static str` to match the
/// TOML-generated `module_functions` signature without forcing the
/// caller to hold on to the cache.
fn intern_static(s: &str) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static INTERNED: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let set = INTERNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let mut guard = set.lock().expect("bundled_sigs interner poisoned");
    if let Some(existing) = guard.get(s) {
        return *existing;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    guard.insert(leaked);
    leaked
}

/// Read-locked cache hit. Returned map is cloned to keep the read
/// lock critical section short.
fn cached_module(module: &str) -> Option<HashMap<Sym, FnSig>> {
    let guard = cache().read().ok()?;
    guard.get(module).cloned()
}

fn store_module(module: &str, sigs: HashMap<Sym, FnSig>) {
    if let Ok(mut guard) = cache().write() {
        // The `module` string is always a compile-time literal coming
        // from `BUNDLED_MODULES`, but to keep the cache key
        // `'static`, intern via `Box::leak`. Each bundled module is
        // inserted at most once for the process lifetime, so the
        // leak is bounded by the number of bundled modules (≤ 25).
        let leaked: &'static str = Box::leak(module.to_string().into_boxed_str());
        guard.insert(leaked, sigs);
    }
}

/// Parse the bundled source for `module` and extract every fn
/// declaration's signature. Returns `None` if the module is not
/// bundled or parsing fails outright.
fn build_module_sigs(module: &str) -> Option<HashMap<Sym, FnSig>> {
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return None;
    }
    let source = super::stdlib::get_bundled_source(module)?;
    let program = almide_lang::parse_cached(source)?;

    let mut out = HashMap::new();
    for decl in &program.decls {
        if let ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } = decl {
            let sig = build_fn_sig(params, return_type, effect, r#async, generics);
            out.insert(*name, sig);
        }
    }
    Some(out)
}

fn build_fn_sig(
    params: &[ast::Param],
    return_type: &ast::TypeExpr,
    effect: &Option<bool>,
    r#async: &Option<bool>,
    generics: &Option<Vec<ast::GenericParam>>,
) -> FnSig {
    // Build a `known_types` map that resolves each generic param name
    // (`[A, B]`) to `Ty::TypeVar(name)`. Without this, the shared
    // resolver would see bare `A` inside `List[A]` and return
    // `Ty::Named("A", [])` — the checker's unifier then treats `A` as
    // a nominal type and rejects callers like `list.len(xs: List[Int])`
    // with "expected List[A] but got List[Int]". The TOML-generated
    // `stdlib_sigs.rs` emits `Ty::TypeVar(s("A"))` directly; we match
    // that convention here.
    let gnames: Vec<Sym> = generics
        .as_ref()
        .map(|gs| gs.iter().map(|g| g.name).collect())
        .unwrap_or_default();
    let mut known_types: HashMap<Sym, almide_lang::types::Ty> = HashMap::new();
    for g in &gnames {
        known_types.insert(*g, almide_lang::types::Ty::TypeVar(*g));
    }
    let resolver_ctx = if gnames.is_empty() { None } else { Some(&known_types) };
    let ptys: Vec<(Sym, almide_lang::types::Ty)> = params
        .iter()
        .map(|p| (p.name, crate::canonicalize::resolve::resolve_type_expr(&p.ty, resolver_ctx)))
        .collect();
    let ret = crate::canonicalize::resolve::resolve_type_expr(return_type, resolver_ctx);
    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
    FnSig {
        generics: gnames,
        params: ptys,
        ret,
        is_effect,
        structural_bounds: HashMap::new(),
        protocol_bounds: HashMap::new(),
    }
}
