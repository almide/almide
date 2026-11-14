//! The registry-signature fit check behind every stdlib call name the routers
//! emit (#2184).
//!
//! The `*_call_name` routers pick a typed twin (`list.sort_by_str_key_rc`) or
//! decline, and a decline used to fall through to the PLAIN name — the scalar
//! impl — whatever the call's types were. The self-host impls are all declared
//! `Int`-typed, and the wasm ABI has exactly two value classes (an i64 scalar
//! slot, an i32 heap handle), so a plain impl reached with a heap where it
//! declares a scalar is a repr mismatch: for a direct argument the validator
//! rejects the module (loud); for a CLOSURE's return the `call_indirect` type
//! is only checked at run time — the #2154 trap, out of a build labelled
//! verified. This module closes the class at the one place every router's
//! output passes through: a name whose registered signature does not accept
//! the call's repr classes is rewritten to the module's `_x` refusal twin
//! (unlinked by construction → an honest render wall, the C-147 spelling).
//!
//! The signature comes from the registry source itself — its registered
//! declarations, gated equal to what `source_to_ir` (the linker's route on
//! the same text) lowers — so the check cannot drift from what is linked. A name the registry does not serve is left
//! alone: the render's unlinked-call wall already refuses it. The gate that
//! drives every router over a type lattice is `tests/router_signature_gate.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use almide_lang::types::Ty;

use crate::lower::is_heap_ty;

/// The wasm value class of a type at a call boundary. Every scalar rides the
/// uniform i64 slot, every heap block an i32 handle; a closure argument is a
/// table index whose `call_indirect` type is fixed by its arity and RETURN
/// class only (the closure ABI widens every parameter to i64).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReprClass {
    Scalar,
    Heap,
    Unit,
    Fn { arity: usize, ret: Box<ReprClass> },
    /// A checker-internal type (`Unknown`, a free `TypeVar`) — no class to
    /// compare; the fit check abstains on it.
    Unknown,
}

pub fn repr_class(ty: &Ty) -> ReprClass {
    match ty {
        Ty::Fn { params, ret, .. } => ReprClass::Fn { arity: params.len(), ret: Box::new(repr_class(ret)) },
        Ty::Unit | Ty::Never => ReprClass::Unit,
        Ty::Unknown | Ty::TypeVar(_) => ReprClass::Unknown,
        t if is_heap_ty(t) => ReprClass::Heap,
        _ => ReprClass::Scalar,
    }
}

/// Does a value of class `actual` fit a parameter declared as `declared`?
/// Unknown on either side abstains (fits).
fn class_fits(declared: &ReprClass, actual: &ReprClass) -> bool {
    match (declared, actual) {
        (ReprClass::Unknown, _) | (_, ReprClass::Unknown) => true,
        (ReprClass::Fn { arity: da, ret: dr }, ReprClass::Fn { arity: aa, ret: ar }) => da == aa && class_fits(dr, ar),
        _ => declared == actual,
    }
}

/// The declared signature of a registered stdlib impl, as the linker sees it.
#[derive(Clone, Debug)]
pub struct RegistrySig {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// The verdict of [`signature_fit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Every argument's class matches the registered impl's declared class.
    Fits,
    /// The registered impl declares a different class at the named position.
    Mismatch(String),
    /// No registry entry serves this call name (the render walls it as unlinked).
    Unregistered,
}

type SourceSigs = Arc<HashMap<String, RegistrySig>>;

/// Per-registry-source signature tables, lowered once per process.
fn source_cache() -> &'static Mutex<HashMap<usize, Option<SourceSigs>>> {
    static CACHE: OnceLock<Mutex<HashMap<usize, Option<SourceSigs>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The registry index of the source whose entry table maps `call_name`, and
/// the impl fn name it maps to.
fn registry_entry(call_name: &str) -> Option<(usize, &'static str)> {
    crate::mir_ops::self_host_runtime().iter().enumerate().find_map(|(idx, (_, entries))| {
        entries.iter().find(|(_, call)| *call == call_name).map(|(impl_fn, _)| (idx, *impl_fn))
    })
}

/// The source's signature table read from its declarations alone (#4682):
/// parse and register, as the checker does before it infers any body, then
/// name and type each fn as lowering would
/// (`almide_frontend::lower::entry_fn_ir_signatures`). That agrees with
/// [`lower_source_sigs`] without inferring and lowering the whole source
/// (value_core is 66 KB, about 26 ms of every build that reaches it). The
/// agreement is gated entry by entry over the whole registry
/// (`declared_sigs_gate` below), so the "cannot drift from what is linked"
/// promise in the module doc still holds.
///
/// The table is keyed by IR name, as the lowered one is, and named as the
/// linker names it (#4685): the source is canonicalized as its owning module
/// and its `__` fns keep their names (`stdlib_fn_ir_signatures`, the naming
/// `lower_stdlib_program` uses), so an entry whose impl is spelled `__x`
/// finds its row.
fn declared_source_sigs(idx: usize) -> Option<SourceSigs> {
    let (source, entries) = crate::mir_ops::self_host_runtime()[idx];
    let prog = almide_lang::parse_cached(source)?;
    let owner = crate::pipeline::self_host_owner(entries);
    let canon = almide_frontend::canonicalize::canonicalize_program_check_in(prog, std::iter::empty(), owner);
    let sigs = almide_frontend::lower::stdlib_fn_ir_signatures(prog, &canon.env)
        .into_iter()
        .map(|(name, (params, ret))| (name.as_str().to_string(), RegistrySig { params, ret }))
        .collect();
    Some(Arc::new(sigs))
}

/// The table as the linker sees it: the whole source inferred and lowered.
/// The reference [`declared_source_sigs`] is gated against.
#[cfg(test)]
fn lower_source_sigs(idx: usize) -> Option<SourceSigs> {
    let (source, entries) = crate::mir_ops::self_host_runtime()[idx];
    // Lowering this source installs its own ABI facts; the caller's stay (#3058).
    let _facts = crate::lower::AbiFactsScope::save();
    // #4685: lowered as the linker lowers it (stdlib mode). The entry-program
    // route escapes every `__` name (`almide_fn___list_concat`), so the impl
    // names the registry maps to were never found and those entries read
    // as unregistered.
    let ir = crate::pipeline::self_host_source_to_ir(source, entries).ok()?;
    let sigs = ir
        .functions
        .iter()
        .map(|f| {
            let params = f.params.iter().map(|p| p.ty.clone()).collect();
            (f.name.as_str().to_string(), RegistrySig { params, ret: f.ret_ty.clone() })
        })
        .collect();
    Some(Arc::new(sigs))
}

/// The declared signature of the impl registered under `call_name`, or `None`
/// when the registry does not serve that name (or its source does not parse).
pub fn registry_signature(call_name: &str) -> Option<RegistrySig> {
    let (idx, impl_fn) = registry_entry(call_name)?;
    let sigs = {
        let mut cache = source_cache().lock().unwrap_or_else(|e| e.into_inner());
        cache.entry(idx).or_insert_with(|| declared_source_sigs(idx)).clone()
    }?;
    sigs.get(impl_fn).cloned()
}

/// Every registered call name the registry serves (the gate's enumeration).
pub fn registered_call_names() -> Vec<&'static str> {
    crate::mir_ops::self_host_runtime().iter().flat_map(|(_, es)| es.iter().map(|(_, c)| *c)).collect()
}

/// Do the call's argument classes fit the registered impl's declared
/// parameter classes? The RESULT class is deliberately not compared here: a
/// direct call's result repr is the `CallFn`'s own `result` slot and the
/// validator pins it; the classes the validator cannot see are the argument
/// side — a closure's return class in particular, checked only at
/// `call_indirect` time.
pub fn signature_fit(call_name: &str, arg_tys: &[Ty]) -> Fit {
    let Some(sig) = registry_signature(call_name) else { return Fit::Unregistered };
    if sig.params.len() != arg_tys.len() {
        return Fit::Mismatch(format!(
            "{call_name} declares {} parameter(s), the call passes {}",
            sig.params.len(),
            arg_tys.len()
        ));
    }
    for (i, (declared, actual)) in sig.params.iter().zip(arg_tys).enumerate() {
        let (d, a) = (repr_class(declared), repr_class(actual));
        if !class_fits(&d, &a) {
            return Fit::Mismatch(format!("{call_name} parameter {i}: declared {d:?} ({declared:?}), call passes {a:?} ({actual:?})"));
        }
    }
    Fit::Fits
}

/// The module's refusal twin for a call the registered impl cannot take:
/// `list.sort_by` → `list.sort_by_x` (the C-147 spelling). A name already
/// spelled as a refusal is returned unchanged.
pub fn refusal_twin(module: &str, func: &str) -> String {
    if func.ends_with("_x") {
        format!("{module}.{func}")
    } else {
        format!("{module}.{func}_x")
    }
}

/// The inversion (#2184): keep `name` only when its registered signature
/// accepts the call's argument classes; otherwise route to the module's `_x`
/// refusal twin. An unregistered name passes through — the render walls it.
pub fn refuse_unless_fits(module: &str, func: &str, name: String, arg_tys: &[Ty]) -> String {
    match signature_fit(&name, arg_tys) {
        Fit::Fits | Fit::Unregistered => name,
        Fit::Mismatch(why) => {
            crate::trace::trace("ALMIDE_DBG_ROUTER", || format!("[router] {name} refused: {why}; args {arg_tys:?}"));
            refusal_twin(module, func)
        }
    }
}

// A caller may offer a declaration-backed route for a representation refusal.
// The shared lowerer knows no runtime symbols: the native pipeline supplies
// its checked declaration resolver, and wasm keeps the registry refusal.
type DeclaredRouteResolver = fn(&str) -> bool;
thread_local! {
    static DECLARED_ROUTE: std::cell::Cell<Option<DeclaredRouteResolver>> = const { std::cell::Cell::new(None) };
}
pub(crate) struct DeclaredRouteGuard(Option<DeclaredRouteResolver>);
impl DeclaredRouteGuard {
    pub(crate) fn set(resolve: DeclaredRouteResolver) -> Self {
        Self(DECLARED_ROUTE.with(|r| r.replace(Some(resolve))))
    }
}
impl Drop for DeclaredRouteGuard {
    fn drop(&mut self) { DECLARED_ROUTE.with(|r| r.set(self.0)); }
}
pub(super) fn declared_route_or(module: &str, func: &str, routed: String) -> String {
    // Some typed routers name a not-yet-registered implementation instead
    // of the canonical refusal twin. Registry absence is the refusal fact;
    // the target must still validate its declared route at each call site.
    if routed != refusal_twin(module, func) && registry_entry(&routed).is_some() { return routed; }
    declared_refusal_or(module, func, routed)
}
/// A router explicitly classified this shape as unsupported on its ABI.
/// A target can offer a checked declaration route without changing wasm's
/// refusal spelling or guessing refusal status from a function name.
pub(super) fn declared_refusal_or(module: &str, func: &str, routed: String) -> String {
    let base = format!("{module}.{func}");
    if DECLARED_ROUTE.with(|r| r.get().is_some_and(|resolve| resolve(&base))) { base } else { routed }
}

/// Retain checked native collection types before wasm key normalization.
/// The installed table already checked every key/member's source equality;
/// the target resolver still requires a declared runtime implementation.
pub(super) fn declared_collection_route(module: &str, func: &str, args: &[Ty], result: &Ty) -> Option<String> {
    if !crate::lower::owned_tco_slots() { return None; }
    let base = format!("{module}.{func}");
    let known = match module {
        "list" => crate::native_hof::declared_comparisons_agree(&base, args, result),
        "map" => args.first().into_iter().chain([result]).any(|ty| {
            crate::render_native_rec::rec_index(&crate::render_native_rec::block_key(ty))
                .is_some_and(|i| crate::render_native_rec::map_kinds(i).is_some())
        }),
        "set" => args.first().into_iter().chain([result]).any(|ty| {
            crate::render_native_rec::rec_index(&crate::render_native_rec::block_key(ty))
                .is_some_and(|i| crate::render_native_rec::set_elem(i).is_some())
        }),
        _ => false,
    };
    (known && DECLARED_ROUTE.with(|r| r.get().is_some_and(|resolve| resolve(&base)))).then_some(base)
}

#[cfg(test)]
mod declared_route_tests {
    use super::*;
    #[test]
    fn declaration_routes_are_scoped_and_preserve_other_registered_names() {
        let wall = "set.contains_x".to_string();
        assert_eq!(declared_route_or("set", "contains", wall.clone()), wall);
        {
            let _guard = DeclaredRouteGuard::set(|name| name == "set.contains");
            assert_eq!(declared_route_or("set", "contains", wall.clone()), "set.contains");
            assert_eq!(declared_route_or("set", "contains", "set.contains_str".into()), "set.contains_str");
            assert_eq!(declared_route_or("set", "contains", "set.contains_unregistered".into()), "set.contains");
            assert_eq!(declared_route_or("user", "contains", "user.contains_x".into()), "user.contains_x");
            { let _nested = DeclaredRouteGuard::set(|_| false);
              assert_eq!(declared_route_or("set", "contains", wall.clone()), wall); }
            assert_eq!(declared_route_or("set", "contains", wall.clone()), "set.contains");
        }
        assert_eq!(declared_route_or("set", "contains", wall.clone()), wall);
    }

    /// #4685: every registry entry has a declared signature. The table used
    /// to be keyed by the entry-program route's escaped names, so 87 entries
    /// whose impls are spelled in the `__` space (`__list_concat`,
    /// `net.__os_tcp_*`, …) read as unregistered and skipped the fit check.
    #[test]
    fn every_registry_entry_has_a_signature() {
        let missing: Vec<&str> = registered_call_names().into_iter().filter(|c| registry_signature(c).is_none()).collect();
        assert!(missing.is_empty(), "{} registry entr(ies) without a signature: {missing:?}", missing.len());
    }
}

#[cfg(test)]
mod declared_sigs_gate {
    use super::*;

    /// One entry's disagreement, or `None` when both routes give the same sig.
    fn disagreement(what: &str, declared: Option<&RegistrySig>, lowered: Option<&RegistrySig>) -> Option<String> {
        match (declared, lowered) {
            (Some(d), Some(l)) if d.params == l.params && d.ret == l.ret => None,
            (None, None) => None,
            _ => Some(format!("{what}: declared {declared:?}, lowered {lowered:?}")),
        }
    }

    /// #4682: the declaration-only table must equal the lowered one for every
    /// registered entry. A source that does not lower at all is a mismatch
    /// too: the lowered route served nothing for it.
    #[test]
    fn declared_signatures_equal_lowered_ones_for_every_registry_entry() {
        let mut compared = 0usize;
        let mut bad = Vec::new();
        for (idx, (_, entries)) in crate::mir_ops::self_host_runtime().iter().enumerate() {
            let declared = declared_source_sigs(idx);
            let lowered = lower_source_sigs(idx);
            for (impl_fn, call) in entries.iter() {
                compared += 1;
                let d = declared.as_ref().and_then(|s| s.get(*impl_fn));
                let l = lowered.as_ref().and_then(|s| s.get(*impl_fn));
                bad.extend(disagreement(&format!("{call} ({impl_fn})"), d, l));
            }
        }
        assert!(compared >= 200, "the sweep compared only {compared} entries; the registry enumeration broke");
        assert!(bad.is_empty(), "{} of {compared} entries disagree:\n{}", bad.len(), bad.join("\n"));
    }

    /// Negative control: the comparison is not vacuous.
    #[test]
    fn a_different_return_or_a_missing_side_is_a_disagreement() {
        let a = RegistrySig { params: vec![Ty::Int], ret: Ty::Int };
        let b = RegistrySig { params: vec![Ty::Int], ret: Ty::String };
        assert!(disagreement("x", Some(&a), Some(&a)).is_none());
        assert!(disagreement("x", Some(&a), Some(&b)).is_some());
        assert!(disagreement("x", Some(&a), None).is_some());
        assert!(disagreement("x", None, Some(&a)).is_some());
    }
}
