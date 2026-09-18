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
//! The signature comes from the registry source itself — the same
//! `source_to_ir` the linker runs on the same text — so the check cannot
//! drift from what is linked. A name the registry does not serve is left
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
    crate::render_wasm::self_host_runtime().iter().enumerate().find_map(|(idx, (_, entries))| {
        entries.iter().find(|(_, call)| *call == call_name).map(|(impl_fn, _)| (idx, *impl_fn))
    })
}

fn lower_source_sigs(idx: usize) -> Option<SourceSigs> {
    let (source, _) = crate::render_wasm::self_host_runtime()[idx];
    let ir = crate::pipeline::source_to_ir(source).ok()?;
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
/// when the registry does not serve that name (or its source does not lower).
pub fn registry_signature(call_name: &str) -> Option<RegistrySig> {
    let (idx, impl_fn) = registry_entry(call_name)?;
    let sigs = {
        let mut cache = source_cache().lock().unwrap_or_else(|e| e.into_inner());
        cache.entry(idx).or_insert_with(|| lower_source_sigs(idx)).clone()
    }?;
    sigs.get(impl_fn).cloned()
}

/// Every registered call name the registry serves (the gate's enumeration).
pub fn registered_call_names() -> Vec<&'static str> {
    crate::render_wasm::self_host_runtime().iter().flat_map(|(_, es)| es.iter().map(|(_, c)| *c)).collect()
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
