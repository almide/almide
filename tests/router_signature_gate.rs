//! The router signature gate (#2184).
//!
//! Every stdlib call the incumbent wasm leg lowers passes through the
//! `*_call_name` routers (`crates/almide-mir/src/lower/mod_p4_*.rs`), which pick
//! a typed twin or decline; a decline falls through to the plain `module.func`
//! name. Nothing checked that the name they emitted was one the call's types
//! could actually reach: the self-host impls are all declared `Int`-typed, and a
//! closure that returns a handle through a slot declared `(Int) -> Int` is a
//! `call_indirect` type mismatch at run time (#2154) — invisible to the wasm
//! validator, so the artifact shipped as verified.
//!
//! This gate drives the routers over the whole declared stdlib surface
//! instantiated on a small type lattice and asserts, for every instantiation:
//!
//! 1. the FINAL name (after `registry_sig::refuse_unless_fits`, the #2184
//!    inversion at the routers' single caller) either is a `_x` refusal twin,
//!    is unregistered (the render walls it as unlinked), or resolves in
//!    `self_host_registry` to a signature whose repr classes accept the
//!    argument types;
//! 2. the `_x` refusal twin of every surface function is itself unlinked —
//!    a registered `_x` would turn a refusal into a mislink;
//! 3. the number of instantiations where the routers ALONE would have mislinked
//!    (the safety net fired) is ratcheted: it may only go down. Each is a
//!    router that leans on the net instead of choosing a typed twin or its own
//!    `_x`; the table this test prints names them.

use std::collections::{BTreeMap, HashMap};

use almide_frontend::stdlib::{lookup_sig, module_functions};
use almide_lang::intern::sym;
use almide_lang::stdlib_info::STDLIB_MODULES;
use almide_lang::types::constructor::TypeConstructorId as TC;
use almide_lang::types::{substitute, FnSig, Ty};
use almide_mir::lower::registry_sig::{refuse_unless_fits, registry_signature, signature_fit, Fit};
use almide_mir::lower::{routers_call_name, RouterHints};

/// Instantiations (over the surface below) where the routers' own verdict does
/// not fit the registered signature and the caller-side inversion is what
/// refuses it. Shrink-only, and already at its floor: the two holes the first
/// sweep found (`map.map`, `result.map_err`) were closed in their routers.
/// Raising it means a router regressed to leaning on the net.
const ROUTER_HOLE_CEILING: usize = 0;

fn list(t: Ty) -> Ty {
    Ty::Applied(TC::List, vec![t])
}
fn option(t: Ty) -> Ty {
    Ty::Applied(TC::Option, vec![t])
}
fn record() -> Ty {
    Ty::Record { fields: vec![(sym("x"), Ty::Int), (sym("y"), Ty::String)] }
}

/// The lattice from the issue: scalars, a tuple, a record, scalar- and
/// heap-element lists, scalar- and heap-payload options.
fn lattice() -> Vec<Ty> {
    vec![
        Ty::Int,
        Ty::Float,
        Ty::String,
        Ty::Bool,
        Ty::Tuple(vec![Ty::Int, Ty::String]),
        record(),
        list(Ty::Int),
        list(Ty::String),
        option(Ty::Int),
        option(Ty::String),
    ]
}

/// The reduced lattice for signatures with three or more generics (keeps the
/// product bounded while still crossing every repr class).
fn small_lattice() -> Vec<Ty> {
    vec![Ty::Int, Ty::String, Ty::Tuple(vec![Ty::Int, Ty::String]), list(Ty::Int), option(Ty::Int)]
}

/// Every instantiation of `sig`'s generics over the lattice, as
/// (arg types, result type).
fn instantiations(sig: &FnSig) -> Vec<(Vec<Ty>, Ty)> {
    let pool = if sig.generics.len() >= 3 { small_lattice() } else { lattice() };
    let mut bindings: Vec<HashMap<_, Ty>> = vec![HashMap::new()];
    for g in &sig.generics {
        bindings = bindings
            .iter()
            .flat_map(|b| {
                pool.iter().map(move |t| {
                    let mut b = b.clone();
                    b.insert(*g, t.clone());
                    b
                })
            })
            .collect();
    }
    bindings
        .iter()
        .map(|b| {
            let args = sig.params.iter().map(|(_, t)| substitute(t, b)).collect();
            (args, substitute(&sig.ret, b))
        })
        .collect()
}

struct Row {
    module: &'static str,
    func: &'static str,
    args: Vec<Ty>,
    raw: String,
    fin: String,
    raw_fit: Fit,
}

/// Route every surface function on every lattice instantiation.
fn sweep() -> Vec<Row> {
    let mut rows = Vec::new();
    for module in STDLIB_MODULES {
        for func in module_functions(module) {
            let Some(sig) = lookup_sig(module, func) else { continue };
            for (args, ret) in instantiations(&sig) {
                let raw = routers_call_name(module, func, &args, &ret, RouterHints::default());
                let raw_fit = signature_fit(&raw, &args);
                let fin = refuse_unless_fits(module, func, raw.clone(), &args);
                rows.push(Row { module, func, args, raw, fin, raw_fit });
            }
        }
    }
    rows
}

fn short(t: &Ty) -> String {
    match t {
        Ty::Applied(c, a) => format!("{c:?}[{}]", a.iter().map(short).collect::<Vec<_>>().join(",")),
        Ty::Tuple(ts) => format!("({})", ts.iter().map(short).collect::<Vec<_>>().join(",")),
        Ty::Record { .. } => "Record".into(),
        Ty::Fn { params, ret, .. } => {
            format!("({}) -> {}", params.iter().map(short).collect::<Vec<_>>().join(","), short(ret))
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn every_emitted_name_is_a_refusal_or_fits_its_registered_signature() {
    let rows = sweep();
    assert!(rows.len() > 1000, "the sweep went blind: {} instantiations", rows.len());
    let fits = rows.iter().filter(|r| r.raw_fit == Fit::Fits).count();
    let unregistered = rows.iter().filter(|r| r.raw_fit == Fit::Unregistered).count();
    eprintln!(
        "sweep: {} instantiations — routers' verdict fits {fits}, unregistered {unregistered}, mismatched {}",
        rows.len(),
        rows.len() - fits - unregistered
    );
    // The registry must actually resolve for the check to mean anything: a
    // table that lowered nothing would report every name unregistered.
    assert!(fits > 100, "the registry resolved only {fits} instantiation(s) — the signature table went blind");
    let escaped: Vec<_> = rows
        .iter()
        .filter(|r| matches!(signature_fit(&r.fin, &r.args), Fit::Mismatch(_)))
        .collect();
    for r in &escaped {
        eprintln!(
            "MISLINK {}.{} ({}) -> {}: {:?}",
            r.module,
            r.func,
            r.args.iter().map(short).collect::<Vec<_>>().join(", "),
            r.fin,
            signature_fit(&r.fin, &r.args)
        );
    }
    assert!(escaped.is_empty(), "{} instantiation(s) reach a registered impl whose signature cannot take them", escaped.len());
}

#[test]
fn every_refusal_twin_is_unlinked() {
    let mut linked = Vec::new();
    for module in STDLIB_MODULES {
        for func in module_functions(module) {
            let twin = format!("{module}.{func}_x");
            if registry_signature(&twin).is_some() {
                linked.push(twin);
            }
        }
    }
    assert!(linked.is_empty(), "registered `_x` names would link a refusal: {linked:?}");
}

#[test]
fn router_holes_are_ratcheted() {
    let rows = sweep();
    let holes: Vec<_> = rows.iter().filter(|r| matches!(r.raw_fit, Fit::Mismatch(_))).collect();
    // One line per (function, routed name, reason shape) so the table names
    // routers, not instantiations.
    let mut table: BTreeMap<String, usize> = BTreeMap::new();
    for r in &holes {
        let Fit::Mismatch(why) = &r.raw_fit else { unreachable!() };
        let key = format!(
            "{}.{} -> {} :: {}",
            r.module,
            r.func,
            r.raw,
            why.split(" (").next().unwrap_or(why)
        );
        *table.entry(key).or_default() += 1;
    }
    eprintln!("router holes (safety net fired): {} instantiations, {} distinct", holes.len(), table.len());
    for (k, n) in &table {
        eprintln!("  {n:4}  {k}");
    }
    // Pinned exactly: the ceiling is zero, so any hole is a rise, and a lower
    // number does not exist — the ratchet has already been driven to its end.
    assert_eq!(
        holes.len(),
        ROUTER_HOLE_CEILING,
        "router holes rose above the ceiling: a router emits a name its call's types cannot reach — choose the typed twin or the `_x` twin in the router"
    );
}

#[test]
fn the_2154_shape_is_refused_by_the_router_itself() {
    // `list.sort_by(entries, ((w, c)) => (0 - c, w))` — a tuple element, a tuple key.
    let elem = Ty::Tuple(vec![Ty::String, Ty::Int]);
    let key = Ty::Tuple(vec![Ty::Int, Ty::String]);
    let f = Ty::Fn { params: vec![elem.clone()], ret: Box::new(key), is_effect: false };
    let args = vec![list(elem.clone()), f];
    let raw = routers_call_name("list", "sort_by", &args, &list(elem), RouterHints::default());
    assert_eq!(raw, "list.sort_by_x");
}

#[test]
fn a_scalar_key_over_scalar_elements_keeps_the_plain_route() {
    let f = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int), is_effect: false };
    let args = vec![list(Ty::Int), f];
    let raw = routers_call_name("list", "sort_by", &args, &list(Ty::Int), RouterHints::default());
    assert_eq!(raw, "list.sort_by");
    assert_eq!(signature_fit(&raw, &args), Fit::Fits);
    assert_eq!(refuse_unless_fits("list", "sort_by", raw.clone(), &args), raw);
}
