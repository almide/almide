//! BorrowInferencePass: Roc-style "borrowed by default, own when needed" analysis.
//!
//! For each user function parameter of heap type (String, Vec, Record, etc.):
//! 1. Start as Borrowed
//! 2. Walk the function body to find ownership-requiring uses
//! 3. If none found → mark param as Ref/RefStr/RefSlice
//! 4. Insert Borrow nodes at call sites for borrowed params
//!
//! This eliminates unnecessary .clone() at call sites when the callee only reads the value.

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use almide_base::intern::{sym, Sym};
use super::use_kind::{Chain, Ctor, Site, SlotMode, SlotOracle, Use, UseSites};

/// The predicate `infer_program_fn_borrows` / `infer_program_module_borrows`
/// apply: a fn whose borrows this pass infers. Tests and the generic
/// TEMPLATES (erased after monomorphisation) are left out; a monomorphised
/// INSTANCE is a concrete fn with a concrete body and is analysed like any
/// other — until #2231 every instance was skipped wholesale and so owned
/// every param (`first__Int(xs: Vec<i64>)` borrowing `&xs` once: 30 of the
/// certifier's C4 lines). Its `mut` params keep the by-ref convention
/// through `param_borrow`, which reads `is_mut` before the body policy.
fn is_analysed_fn(func: &IrFunction) -> bool {
    let derived = is_derive_fn(func);
    !func.is_test
        && (derived || !func.generics.as_ref().map_or(false, |g| !g.is_empty()))
}

/// Every key the rounds WILL publish a signature under — the canonical
/// names and every mirror spelling (`mirror_keys`). A call that resolves to
/// one of these before it is published is a forward or mutually recursive
/// reference and is read optimistically (`Scope::call_slot`); a key outside
/// this set is never going to be known and reads pessimistically. The set
/// must name the mirrors too: a root fn's round runs BEFORE the module fns'
/// mirrors exist, and a cross-module derive call that read `Own` from the
/// miss in round 0 and `Ref` from the mirror in round 1 was the descent
/// (#1713) the monotone ascent must never take.
fn seed_pending_user_fns(program: &IrProgram) -> HashSet<String> {
    let mut set = HashSet::new();
    for func in &program.functions {
        if is_analysed_fn(func) {
            set.insert(func.name.to_string());
        }
    }
    for module in &program.modules {
        let mod_name = module.name.to_string();
        for func in &module.functions {
            if is_analysed_fn(func) {
                set.insert(format!("{}::{}", mod_name, func.name));
                set.extend(mirror_keys(&mod_name, func).into_iter().map(|(k, _)| k));
            }
        }
    }
    set
}

/// Pre-bake the owned-param signature a TCO-bound function will end up with.
///
/// `TailCallOptPass` (which runs after this) rewrites a tail-recursive function
/// into a loop whose params are the mutable loop state, forcing to owned every
/// slot the loop cannot keep a reference in (`pass_tco::loop_keeps_borrow`).
/// Borrow inference runs before TCO, so without this it would infer those params
/// as `Ref` and a caller forwarding a value into one would get a `Ref` param that
/// clashes with the post-TCO owned signature → E0308. Bake the owned-ness in now
/// so the whole call chain stays consistent.
///
/// The predicate is READ from `pass_tco`, not restated here: the two used to be
/// two copies of one `Bytes | Fn` type test, which is the second place a rule
/// has to be remembered.
fn tco_owned_params(func: &IrFunction, mut borrows: Vec<ParamBorrow>) -> Vec<ParamBorrow> {
    if crate::pass_tco::is_tco_candidate(func) {
        let identity = crate::pass_tco::tco_identity_carried(func);
        for (i, b) in borrows.iter_mut().enumerate() {
            if matches!(b, ParamBorrow::Own) { continue; }
            let keep = func.params.get(i).is_some_and(|p| {
                crate::pass_tco::loop_keeps_borrow(p, identity.get(i).copied().unwrap_or(false))
            });
            if !keep {
                *b = ParamBorrow::Own;
            }
        }
    }
    borrows
}

/// Record the names of every user-declared RECORD type so a `t: Tok` param
/// (`Ty::Named`) is borrow-inferred like a structural record instead of being
/// deep-cloned at every read (#647).
pub(crate) fn seed_record_names(program: &IrProgram) -> HashSet<String> {
    seed_named_types(program, |k| matches!(k, IrTypeDeclKind::Record { .. }))
}

/// The names of every user-declared VARIANT type. A `s: Shape` param is
/// borrow-eligible like a record; whether its `match` reads it by reference
/// is decided per body by [`scrutinee_binders_borrow_only`].
pub(crate) fn seed_variant_names(program: &IrProgram) -> HashSet<String> {
    seed_named_types(program, |k| matches!(k, IrTypeDeclKind::Variant { .. }))
}

fn seed_named_types(program: &IrProgram, keep: impl Fn(&IrTypeDeclKind) -> bool) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut collect = |decls: &[IrTypeDecl]| {
        for td in decls {
            if keep(&td.kind) {
                set.insert(td.name.to_string());
            }
        }
    };
    collect(&program.type_decls);
    for m in &program.modules { collect(&m.type_decls); }
    set
}

/// Seed `sigs` with `@intrinsic` fns from every bundled stdlib module —
/// including ones that weren't lowered into `program.modules` (non
/// auto-imported modules like `bytes`, `regex`, `fs`). The key is the
/// mangled runtime symbol so `rewrite_calls` can look it up from
/// `RuntimeCall.symbol` verbatim. Extracted verbatim from
/// `infer_borrow_signatures` (cog>100 decomposition): only ever writes to
/// `sigs` (`.insert`), never reads it back, so it's a safe write-only
/// accumulator to thread out.
/// Apply the `@mutating` / implicit-mut / explicit `mut` param override:
/// promote a heap container's inferred Ref-family borrow to `RefMut`.
/// Extracted from `seed_intrinsic_sigs` (cog>100 decomposition, second
/// round): only ever writes into `borrows` via index, never reads `sigs` —
/// a safe write-only accumulator to thread out.
fn apply_intrinsic_mut_overrides(
    symbol: &str,
    params: &[almide_lang::ast::Param],
    attrs: &[almide_lang::ast::Attribute],
    return_type: &almide_lang::ast::TypeExpr,
    borrow_ref_names: &[&str],
    borrows: &mut [ParamBorrow],
) {
    // The native runtime's own signature is the ground truth when the symbol
    // has a Rust body: promote exactly the params it declares `&mut`. The
    // `mut`/`@mutating`/implicit-Unit shapes below are only a FALLBACK for
    // symbols with no native body (wasm-only prims, self-hosted intrinsics).
    if let Some(native) = crate::generated::runtime_fn_modes::runtime_param_mutability(symbol) {
        for (idx, b) in borrows.iter_mut().enumerate() {
            let is_borrow_ref = params.get(idx)
                .map(|p| borrow_ref_names.iter().any(|r| r == &p.name.as_str()))
                .unwrap_or(false);
            if !is_borrow_ref
                && native.get(idx).copied().unwrap_or(false)
                && matches!(b, ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr)
            {
                *b = ParamBorrow::RefMut;
            }
        }
        return;
    }
    // Mutated parameters: explicit `mut` keyword, `@mutating`,
    // or implicit (returns Unit with Ref-mode container first arg).
    // `@borrow_ref(param)` always wins over mutation inference.
    let has_mutating = attrs.iter().any(|a| a.name.as_str() == "mutating");
    let implicit_mut = is_unit_type_expr(return_type);
    // Collect all mut param indices
    let mut mut_indices: Vec<usize> = params.iter().enumerate()
        .filter(|(_, p)| p.is_mut)
        .map(|(i, _)| i)
        .collect();
    if (has_mutating || implicit_mut) && !mut_indices.contains(&0) {
        mut_indices.push(0);
    }
    for idx in mut_indices {
        let param_name = params.get(idx).map(|p| p.name.as_str());
        let is_borrow_ref = param_name
            .map(|n| borrow_ref_names.iter().any(|r| r == &n))
            .unwrap_or(false);
        if !is_borrow_ref {
            if let Some(b) = borrows.get_mut(idx) {
                // A heap container an intrinsic mutates in place (Unit
                // return) is taken by `&mut`. Every heap container
                // reaches here already Ref-family:
                // `intrinsic_borrow_mode_from_type_expr` seeds
                // `List`→RefSlice, `String`→RefStr, and
                // `Bytes`/`Map`/`Set`/records→Ref — so promoting the
                // Ref family to RefMut covers them all. Primitives are
                // seeded `Own` and stay `Own` (never mutated in place
                // through a reference).
                if matches!(b, ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr) {
                    *b = ParamBorrow::RefMut;
                }
            }
        }
    }
}

/// Seed `sigs` with one `@intrinsic` fn declaration's borrow signature.
/// Extracted from `seed_intrinsic_sigs` (cog>100 decomposition, second
/// round): a no-op for anything that isn't an `@intrinsic` fn — mirrors the
/// original per-decl `let ... else { continue }` guard chain, just with
/// `continue` becoming `return` from this standalone function.
fn seed_intrinsic_sig_for_fn(
    params: &[almide_lang::ast::Param],
    attrs: &[almide_lang::ast::Attribute],
    return_type: &almide_lang::ast::TypeExpr,
    sigs: &mut HashMap<String, Vec<ParamBorrow>>,
) {
    use almide_lang::ast::AttrValue;
    let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "intrinsic") else { return };
    let Some(first) = attr.args.first() else { return };
    let AttrValue::String { value: symbol } = &first.value else { return };
    // Params in AST are `TypeExpr`, not resolved `Ty`. Convert
    // the simple cases into a `Ty` so `intrinsic_borrow_mode`
    // can reuse the same logic as the IR-side path.
    let mut borrows: Vec<ParamBorrow> = params.iter()
        .map(|p| intrinsic_borrow_mode_from_type_expr(&p.ty))
        .collect();
    // `@consume(p1, p2, ...)` overrides the inferred borrow for
    // the named params to `Own`. Required when the runtime fn
    // consumes a container (e.g. `xs: Vec<T>` on
    // `almide_rt_list_map`) rather than borrowing it.
    let consume_names: Vec<&str> = attrs.iter()
        .filter(|a| a.name.as_str() == "consume")
        .flat_map(|a| a.args.iter().filter_map(|arg| match &arg.value {
            AttrValue::Ident { name } => Some(name.as_str()),
            AttrValue::String { value } => Some(value.as_str()),
            _ => None,
        }))
        .collect();
    for (idx, p) in params.iter().enumerate() {
        if consume_names.iter().any(|n| n == &p.name.as_str()) {
            borrows[idx] = ParamBorrow::Own;
        }
    }
    // `@borrow_ref(p1, p2, ...)` — opposite override: force
    // `Ref` on params the default heuristic would pass by
    // value (e.g. user-defined named types whose runtime fn
    // takes `&T`, like `JsonPath`).
    let borrow_ref_names: Vec<&str> = attrs.iter()
        .filter(|a| a.name.as_str() == "borrow_ref")
        .flat_map(|a| a.args.iter().filter_map(|arg| match &arg.value {
            AttrValue::Ident { name } => Some(name.as_str()),
            AttrValue::String { value } => Some(value.as_str()),
            _ => None,
        }))
        .collect();
    for (idx, p) in params.iter().enumerate() {
        if borrow_ref_names.iter().any(|n| n == &p.name.as_str()) {
            borrows[idx] = ParamBorrow::Ref;
        }
    }
    apply_intrinsic_mut_overrides(symbol, params, attrs, return_type, &borrow_ref_names, &mut borrows);
    sigs.insert(symbol.clone(), borrows);
}

/// The generated primitive codec helpers a derived decode calls by bare name
/// — `__decode_option_<prim>(v, key)` and `__decode_default_<prim|list_prim>(v,
/// key, default)` — reach this pass with no declaration behind them (the
/// native runtime twins are prelude fns, not bundled `@intrinsic` decls), so
/// the unknown-callee fallback forced every derived decode that used one to
/// OWN its `Value` (#2052): the whole document was moved or cloned per field,
/// and an outer decode that borrowed (`alt: Addr?`) handed its `&Value` to the
/// by-value option driver — rustc E0308 on a program `check` had accepted.
/// The runtime twins take `&AlmideValue` (runtime/rs/src/value.rs), so slot 0
/// borrows here; the key and the default are consumed.
fn seed_codec_helper_sigs(sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    for prim in ["string", "int", "float", "bool"] {
        sigs.insert(format!("__decode_option_{prim}"), vec![ParamBorrow::Ref, ParamBorrow::Own]);
        sigs.insert(format!("__decode_default_{prim}"), vec![ParamBorrow::Ref, ParamBorrow::Own, ParamBorrow::Own]);
        sigs.insert(format!("__decode_default_list_{prim}"), vec![ParamBorrow::Ref, ParamBorrow::Own, ParamBorrow::Own]);
    }
}

/// The built-in output fns (`println(x)` and kin) are free calls with no
/// declaration in any bundled module, so the oracle saw an UNKNOWN callee
/// and consumed their argument — a `fn say(name: String) = println(name)`
/// owned `name` for a value the `println!` arm only formats by reference
/// (#2231, the certifier's C4 on `say` / `show` / `report` / `flag`). They
/// borrow.
fn seed_builtin_output_sigs(sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    for name in ["println", "print", "eprintln", "eprint"] {
        sigs.entry(name.to_string()).or_insert_with(|| vec![ParamBorrow::Ref]);
    }
}

pub(crate) fn seed_intrinsic_sigs(sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    use almide_lang::ast::Decl;
    for &mod_name in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(source) = almide_lang::stdlib_info::bundled_source(mod_name) else { continue };
        let Some(parsed) = almide_lang::parse_cached(source) else { continue };
        for decl in &parsed.decls {
            let Decl::Fn { params, attrs, return_type, .. } = decl else { continue };
            seed_intrinsic_sig_for_fn(params, attrs, return_type, sigs);
        }
    }
}

/// Float ordering variants (IntrinsicLoweringPass swaps `..._sort` →
/// `..._sort_float` etc. for `List[Float]`; C-055). They have the SAME
/// borrow shape as their base symbol — `sort`/`min`/`max` borrow the slice,
/// `sort_by` consumes the Vec — so alias the base signature rather than
/// re-deriving it (the float variants are runtime-only and carry no
/// `@intrinsic` attr to seed from).
fn alias_float_variant_sigs(sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    for (base, float_var) in [
        ("almide_rt_list_sort", "almide_rt_list_sort_float"),
        ("almide_rt_list_min", "almide_rt_list_min_float"),
        ("almide_rt_list_max", "almide_rt_list_max_float"),
        ("almide_rt_list_sort_by", "almide_rt_list_sort_by_float"),
    ] {
        if let Some(b) = sigs.get(base).cloned() {
            sigs.insert(float_var.to_string(), b);
        }
    }
}

/// One function's signature for this round: the derive-restricted or the
/// full inference, then the TCO bake.
fn round_borrows(func: &IrFunction, round: &Round, module: Option<&str>) -> Vec<ParamBorrow> {
    let name = func.name.to_string();
    let scope = Scope { round, module, current_fn: &name };
    let borrows = if is_derive_fn(func) { derived_value_borrows(func, &scope) } else { infer_function_borrows(func, &scope) };
    tco_owned_params(func, borrows)
}

/// One fixed-point iteration's pass over top-level functions: writes into
/// `sigs` and into each function's own `param.borrow`, reading callee
/// signatures only through the round's frozen snapshot.
fn infer_program_fn_borrows(program: &mut IrProgram, sigs: &mut HashMap<String, Vec<ParamBorrow>>, round: &Round) {
    for func in &mut program.functions {
        if !is_analysed_fn(func) { continue; }
        let borrows = round_borrows(func, round, None);
        // Always record the signature (including all-Own) so that the
        // fixed-point iteration can distinguish "known to be Own" from
        // "not yet analysed". Without this, self-recursive functions
        // whose first-pass inference produced all-Own would be looked
        // up as None forever → conservative fallback → Own sticks.
        sigs.insert(func.name.to_string(), borrows.clone());
        for (param, borrow) in func.params.iter_mut().zip(borrows) {
            param.borrow = borrow;
        }
    }
}

/// Upsert one MIRROR key (`owner` is the claimant's `mod::name` identity —
/// unique across modules, so same-named fns in two modules stay two
/// claimants). Mirrors are shared namespace, so the first claimant
/// keeps the key (`or_insert`'s arbitration against same-named fns and against
/// canonical entries) — but the claimant itself must keep its mirror CURRENT
/// across fixed-point iterations. Frozen `or_insert` mirrors were #1713: a
/// derived decode whose record has an Option-of-heap field infers `Ref` only
/// in round 1 (round 0 runs before the generated option workers' signatures
/// exist), so the bare `Type.decode` key a cross-module call site carries kept
/// round 0's `Own` while the definition emitted `&Value` — E0308 on a program
/// `check` had passed.
fn upsert_mirror(
    sigs: &mut HashMap<String, Vec<ParamBorrow>>,
    owners: &mut HashMap<String, String>,
    key: String,
    owner: &str,
    borrows: &[ParamBorrow],
) {
    use std::collections::hash_map::Entry;
    match sigs.entry(key) {
        Entry::Occupied(mut e) => {
            if owners.get(e.key()).is_some_and(|o| o == owner) {
                *e.get_mut() = borrows.to_vec();
            }
        }
        Entry::Vacant(e) => {
            owners.insert(e.key().clone(), owner.to_string());
            e.insert(borrows.to_vec());
        }
    }
}

/// The keys a module fn's signature is published under besides its
/// canonical `mod::name`, in the order they are written. Every spelling a
/// call site can carry gets one (#1087 / #1549 / #433 × #411-B):
///
/// - the mangled runtime symbol `almide_rt_<mod>_<name>` — `ResolveCallsPass`
///   rewrites bundled-Almide calls to that `Named` target;
/// - the dotted `mod.name` a cross-module convention-method call site carries;
/// - for a convention method (`Box.twice`), the bare name the frontend's
///   `convention_emit_key` resolves it to;
/// - for a NAMESPACED derive (`varlib.Pigment.decode` — the fn is named
///   `mod.Type.method` once its type is `mod.Type`), the trailing
///   `Type.method` under both the module scope and bare, and the symbol the
///   definition emits with the leading `{origin}_` stripped — the two
///   spellings `BuiltinLowering` resolves (`collect_module_method_fns`),
///   which the plain mirrors miss because they prefix the origin twice.
///
/// `@inline_rust` / `@wasm_intrinsic` / `@intrinsic` fns publish nothing
/// here: they are seeded under the mangled runtime symbol up front and must
/// not be overwritten by bundled-body inference. The mirrors past the first
/// two are SHARED namespace (a same-named method in another module claims the
/// same key), so they go through [`upsert_mirror`]; the `shared` flag says
/// which.
fn mirror_keys(mod_name: &str, func: &IrFunction) -> Vec<(String, bool)> {
    let is_dispatch_only = func.attrs.iter().any(|a|
        matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic" | "intrinsic"));
    if is_dispatch_only {
        return Vec::new();
    }
    let origin = mod_name.replace('.', "_");
    let flat = func.name.as_str().replace('.', "_");
    let mut keys = vec![
        (format!("almide_rt_{}_{}", origin, flat), false),
        (format!("{}.{}", mod_name, func.name), false),
    ];
    if func.name.as_str().contains('.') {
        keys.push((func.name.to_string(), true));
    }
    let segs: Vec<&str> = func.name.as_str().split('.').collect();
    if segs.len() > 2 {
        let tail = format!("{}.{}", segs[segs.len() - 2], segs[segs.len() - 1]);
        let base = flat.strip_prefix(&format!("{}_", origin)).unwrap_or(&flat).to_string();
        keys.push((format!("{}::{}", mod_name, tail), true));
        keys.push((tail, true));
        keys.push((format!("almide_rt_{}_{}", origin, base), true));
    }
    keys
}

/// One fixed-point iteration's pass over module functions. Same shape and
/// same safety rationale as `infer_program_fn_borrows`.
fn infer_program_module_borrows(program: &mut IrProgram, sigs: &mut HashMap<String, Vec<ParamBorrow>>, mirror_owners: &mut HashMap<String, String>, round: &Round) {
    for module in &mut program.modules {
        let mod_name = module.name.to_string();
        for func in &mut module.functions {
            if !is_analysed_fn(func) { continue; }
            let borrows = round_borrows(func, round, Some(&mod_name));
            let owner = format!("{}::{}", mod_name, func.name);
            sigs.insert(owner.clone(), borrows.clone());
            for (key, shared) in mirror_keys(&mod_name, func) {
                if shared {
                    upsert_mirror(sigs, mirror_owners, key, &owner, &borrows);
                } else {
                    sigs.insert(key, borrows.clone());
                }
            }
            for (param, borrow) in func.params.iter_mut().zip(borrows) {
                param.borrow = borrow;
            }
        }
    }
}

/// Phase 1: infer borrow signatures for all functions by fixed-point
/// iteration. One pass is not enough because a caller's ownership needs
/// depend on the borrow signatures of its callees: round 1 handles leaf
/// functions, later rounds propagate those borrows up through their callers.
///
/// The iteration is a monotone ascent on a finite lattice — a slot only ever
/// moves `Ref` → `RefMut` → `Own` (a callee that consumes more makes its
/// callers consume more, never less), so it converges by itself and there is
/// no round cap to tune (#2186: the old cap of 6, then 64, was load-bearing
/// under the optimistic first round). What IS checked is the monotonicity
/// the argument rests on: a slot moving DOWN between rounds is an ICE, and
/// so is running past the lattice height, which is the most rounds a
/// monotone ascent can take.
pub fn infer_borrow_signatures(program: &mut IrProgram) -> HashMap<String, Vec<ParamBorrow>> {
    let mut sigs: HashMap<String, Vec<ParamBorrow>> = HashMap::new();
    // Which fn (`mod::name`) first claimed each MIRROR key — see `upsert_mirror`.
    let mut mirror_owners: HashMap<String, String> = HashMap::new();

    let records = seed_record_names(program);
    let variants = seed_variant_names(program);
    seed_intrinsic_sigs(&mut sigs);
    seed_builtin_output_sigs(&mut sigs);
    seed_codec_helper_sigs(&mut sigs);
    alias_float_variant_sigs(&mut sigs);
    let pending = seed_pending_user_fns(program);

    let mut iter = 0usize;
    loop {
        let snapshot = sigs.clone();
        let round = Round { snapshot: &snapshot, pending: &pending, records: &records, variants: &variants };
        infer_program_fn_borrows(program, &mut sigs, &round);
        infer_program_module_borrows(program, &mut sigs, &mut mirror_owners, &round);

        // ALMIDE_DBG_BORROW=<substr>: dump every matching sig key per
        // fixed-point iteration (the probe that caught #1713's frozen mirrors).
        if let Some(filter) = almide_base::env::var("ALMIDE_DBG_BORROW") {
            for (k, v) in &sigs {
                if k.contains(&filter) { eprintln!("[borrow iter {iter}] {k} -> {v:?}"); }
            }
        }
        if sigs == snapshot {
            break;
        }
        assert_monotone_round(&snapshot, &sigs, iter);
        iter += 1;
    }
    sigs
}

/// Write each fused chain's source mode into its node (#2287): a chain whose
/// receiving lambdas only READ the source element — the verdict every round's
/// use walk gave `Iterable::consumed` through `chain_elements_consumed` —
/// iterates from a borrow, so `consume` becomes `false` and every later pass
/// (the clone pass, `ChainSourceBorrow`, the renderer) sees the same mode the
/// param verdict was built on. Read with the FINAL signatures: the fixed
/// point's last round saw exactly these, so the verdict cannot move.
pub fn commit_chain_source_modes(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct Commit<'a> { scope: Scope<'a> }
    impl IrMutVisitor for Commit<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            // Only a PLACE — a variable or a projection of one — has an owner
            // a borrow can spare: a temporary (`list.range(0, n)`, a call's
            // result, a literal) is consumed as it always was; borrowing it
            // would iterate a `&Vec` that dies at the same point and, at the
            // default opt-level, cost the inner loop of spectralnorm's indexed
            // spelling 2x (the spelling-ratio gate, #2098).
            if let IrExprKind::IterChain { source, consume, steps, collector } = &mut expr.kind
                && *consume
                && is_place(source)
                && !crate::use_kind::chain_elements_consumed(&source.ty, steps, collector, &self.scope)
            {
                *consume = false;
            }
        }
    }
    fn is_place(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Var { .. } => true,
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => is_place(object),
            IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. } | IrExprKind::Clone { expr } => is_place(expr),
            _ => false,
        }
    }
    let records = seed_record_names(program);
    let variants = seed_variant_names(program);
    let pending = HashSet::new();
    let round = Round { snapshot: sigs, pending: &pending, records: &records, variants: &variants };
    let commit_fn = |body: &mut IrExpr, module: Option<&str>, name: &str| {
        let mut c = Commit { scope: Scope { round: &round, module, current_fn: name } };
        c.visit_expr_mut(body);
    };
    for f in &mut program.functions { commit_fn(&mut f.body, None, f.name.as_str()); }
    for tl in &mut program.top_lets { commit_fn(&mut tl.value, None, ""); }
    for m in &mut program.modules {
        let module = m.name.to_string();
        for f in &mut m.functions { commit_fn(&mut f.body, Some(&module), f.name.as_str()); }
        for tl in &mut m.top_lets { commit_fn(&mut tl.value, Some(&module), ""); }
    }
}

/// The rank of a mode on the borrow lattice: a slot may only climb.
fn borrow_rank(b: ParamBorrow) -> u8 {
    match b {
        ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr => 0,
        ParamBorrow::RefMut => 1,
        ParamBorrow::Own => 2,
    }
}

/// The monotonicity the convergence argument rests on, checked after every
/// round that changed something: no slot moved down, and the round count
/// has not passed the lattice height (every slot climbing one rank per
/// round, plus the round that observes the fixed point).
fn assert_monotone_round(before: &HashMap<String, Vec<ParamBorrow>>, after: &HashMap<String, Vec<ParamBorrow>>, iter: usize) {
    let mut height = 0usize;
    for (key, now) in after {
        height += now.len() * 2;
        let Some(was) = before.get(key) else { continue };
        for (i, (w, n)) in was.iter().zip(now).enumerate() {
            assert!(
                borrow_rank(*n) >= borrow_rank(*w),
                "[ICE] borrow inference is not monotone: {key} slot {i} moved {w:?} -> {n:?} in round {iter}",
            );
        }
    }
    assert!(
        iter <= height,
        "[ICE] borrow inference did not converge within the lattice height ({height} rounds)",
    );
}

fn is_derive_fn(func: &IrFunction) -> bool {
    // Auto-derived convention methods get the restricted inference of
    // `derived_value_borrows` — they are a generated API surface whose call
    // sites (often cross-module, where the borrow signature can't be looked
    // up) pass owned values, so a Ref param would mismatch (E0308). With record
    // borrow inference enabled (#647), a record-typed derived `encode(p:
    // Pigment)` would otherwise become `&Pigment` and break those owned-arg
    // call sites; only `Value` params are inferred (#1679).
    //
    // Identification is structural, NOT name-based: `lower/mod.rs` stamps every
    // generated convention fn with a synthetic `@derived` attribute at the single
    // point it produces them. The generator is the source of truth — we never
    // guess from the method name (`encode`/`eq`/...), which a user could also use.
    func.attrs.iter().any(|a| a.name.as_str() == "derived")
}


/// AST-side variant of `intrinsic_borrow_mode` — derives the borrow
/// mode directly from an `ast::TypeExpr` (no resolve pass needed).
/// Used to seed the signature table from bundled stdlib source before
/// the IR-level fns are visited.
fn intrinsic_borrow_mode_from_type_expr(ty: &almide_lang::ast::TypeExpr) -> ParamBorrow {
    use almide_lang::ast::TypeExpr;
    match ty {
        TypeExpr::Simple { name } => {
            let n = name.as_str();
            match n {
                "Int" | "Int8" | "Int16" | "Int32" | "Int64"
                | "UInt8" | "UInt16" | "UInt32" | "UInt64"
                | "Float" | "Float32" | "Float64"
                | "Bool" | "Unit"
                    => ParamBorrow::Own,
                "String" => ParamBorrow::RefStr,
                "Bytes" => ParamBorrow::Ref,
                // Known stdlib struct types whose runtime fns uniformly
                // take `&T`. `Value` is the codec universal model,
                // `Matrix` / `AlmideMatrix` is the numeric tensor, both
                // heavy enough that pass-by-ref is the default.
                "Value" | "Matrix" | "AlmideMatrix" => ParamBorrow::Ref,
                // Named types, possibly type parameters (`A`, `B`) —
                // treat as Own so the caller keeps ownership. When the
                // concrete type is a heap value, the Borrow/Clone IR
                // nodes travel through unchanged.
                _ => ParamBorrow::Own,
            }
        }
        TypeExpr::Generic { name, .. } => {
            let n = name.as_str();
            match n {
                "List" => ParamBorrow::RefSlice,
                "Map" | "Set" => ParamBorrow::Ref,
                // Option / Result: consume by value (see doc on the
                // IR-side `intrinsic_borrow_mode`).
                "Option" | "Result" => ParamBorrow::Own,
                // `Matrix[T]` parametric form — same borrow surface as
                // bare `Matrix` (both map to `&AlmideMatrix` at the
                // runtime boundary).
                "Matrix" => ParamBorrow::Ref,
                _ => ParamBorrow::Own,
            }
        }
        TypeExpr::Record { .. } | TypeExpr::Variant { .. } => ParamBorrow::Ref,
        TypeExpr::Tuple { .. } => ParamBorrow::Ref,
        // Fn / OpenRecord / Union — pass owned.
        _ => ParamBorrow::Own,
    }
}

fn is_unit_type_expr(ty: &almide_lang::ast::TypeExpr) -> bool {
    use almide_lang::ast::TypeExpr;
    matches!(ty, TypeExpr::Simple { name } if name.as_str() == "Unit")
}

include!("pass_borrow_inference_ownership.rs");
include!("pass_borrow_inference_call_sites.rs");
