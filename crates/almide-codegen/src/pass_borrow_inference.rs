//! BorrowInferencePass: Roc-style "borrowed by default, own when needed" analysis.
//!
//! For each user function parameter of heap type (String, Vec, Record, etc.):
//! 1. Start as Borrowed
//! 2. Walk the function body to find ownership-requiring uses
//! 3. If none found → mark param as Ref/RefStr/RefSlice
//! 4. Insert Borrow nodes at call sites for borrowed params
//!
//! This eliminates unnecessary .clone() at call sites when the callee only reads the value.

use std::collections::HashMap;
use std::cell::RefCell;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// `true` if the bundled `module.func`'s `@inline_rust` template
/// borrows the param at position `pos` (`&{name}`, `&*{name}`,
/// `&mut {name}`, or `&mut *{name}`). Consumed ("owned") params have
/// no sigil and render via `{name}` alone.
fn bundled_borrow_at(module: &str, func: &str, pos: usize) -> bool {
    use almide_lang::ast::{AttrValue, Decl};
    let Some(source) = almide_lang::stdlib_info::bundled_source(module) else {
        return false;
    };
    let Some(program) = almide_lang::parse_cached(source) else { return false; };
    for decl in &program.decls {
        let Decl::Fn { name, attrs, params, .. } = decl else { continue };
        if name.as_str() != func { continue; }
        let Some(pname) = params.get(pos).map(|p| p.name) else { return false; };
        let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "inline_rust") else {
            return false;
        };
        let Some(first) = attr.args.first() else { return false; };
        let AttrValue::String { value } = &first.value else { return false; };
        let p = pname.as_str();
        return value.contains(&format!("&{{{}}}", p))
            || value.contains(&format!("&*{{{}}}", p))
            || value.contains(&format!("&mut {{{}}}", p))
            || value.contains(&format!("&mut *{{{}}}", p));
    }
    false
}

// Thread-local snapshot of currently-known borrow signatures, used during
// inference so that when we check `fn caller(data: Bytes) { other(data) }`
// we can consult `other`'s borrows and avoid pessimistically marking `data`
// as owned. Populated before each fixed-point iteration in
// `infer_borrow_signatures`.
thread_local! {
    static SIGS_SNAPSHOT: RefCell<HashMap<String, Vec<ParamBorrow>>> = RefCell::new(HashMap::new());
    static MOD_SCOPE: RefCell<Option<String>> = RefCell::new(None);
    // Name of the function currently being analysed. Self-recursive calls to
    // this function are treated optimistically (we don't scan their args for
    // ownership needs), which lets a TCO-loop body like `foo(data, next, ...)`
    // keep `data: &Vec<u8>` instead of collapsing to `Vec<u8>` on the first
    // pass and never recovering.
    static CURRENT_FN: RefCell<Option<String>> = RefCell::new(None);
    // Names of user-declared RECORD types (`type Tok = { … }`). A param of such a
    // type is `Ty::Named("Tok")` (not a structural `Ty::Record`), so without this
    // set `is_heap_type`/`intrinsic_borrow_mode` treat it as Own and every reader
    // deep-clones the whole record. Records get borrow inference like structural
    // records; user VARIANTs stay Own (conservative — variant borrowing is not
    // generalized here). #647
    static RECORD_NAMES: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
}

/// Is `name` a user-declared record type (eligible for record borrow inference)?
fn is_record_type_name(name: &str) -> bool {
    RECORD_NAMES.with(|r| r.borrow().contains(name))
}

fn lookup_user_borrows(callee: &str) -> Option<Vec<ParamBorrow>> {
    SIGS_SNAPSHOT.with(|s| {
        let s = s.borrow();
        MOD_SCOPE.with(|m| {
            let m = m.borrow();
            if let Some(mod_name) = m.as_deref() {
                if let Some(v) = s.get(&format!("{}::{}", mod_name, callee)) {
                    return Some(v.clone());
                }
            }
            s.get(callee).cloned()
        })
    })
}

/// Phase 1: Infer borrow signatures for all functions via fixed-point iteration.
///
/// One pass is not enough because a caller's ownership needs depend on the
/// borrow signatures of its callees. Round 1 handles leaf functions; later
/// rounds propagate those borrows up through their callers. Converges quickly
/// in practice — typical fix-points reach in 2-3 rounds; we cap at 6 for
/// safety.
/// Pre-bake the owned-param signature a TCO-bound function will end up with.
///
/// `TailCallOptPass` (which runs after this) rewrites a tail-recursive function
/// into a loop whose params are the mutable loop state, forcing them to owned —
/// EXCEPT a `Bytes` param kept borrowed to avoid cloning a large buffer each
/// iteration (same rule as `pass_tco::rewrite_to_loop`). Borrow inference runs
/// before TCO, so without this it would infer those params as `Ref` and a caller
/// forwarding a value into one would get a `Ref` param that clashes with the
/// post-TCO owned signature → E0308. Bake the owned-ness in now so the whole
/// call chain stays consistent.
fn tco_owned_params(func: &IrFunction, mut borrows: Vec<ParamBorrow>) -> Vec<ParamBorrow> {
    if crate::pass_tco::is_tco_candidate(func) {
        for (i, b) in borrows.iter_mut().enumerate() {
            let is_preserved_bytes = matches!(func.params.get(i).map(|p| &p.ty), Some(Ty::Bytes))
                && !matches!(b, ParamBorrow::Own);
            if !is_preserved_bytes {
                *b = ParamBorrow::Own;
            }
        }
    }
    borrows
}

/// Record the names of every user-declared RECORD type so a `t: Tok` param
/// (`Ty::Named`) is borrow-inferred like a structural record instead of being
/// deep-cloned at every read (#647).
fn seed_record_names(program: &IrProgram) {
    RECORD_NAMES.with(|r| {
        let mut set = r.borrow_mut();
        set.clear();
        let mut collect = |decls: &[IrTypeDecl]| {
            for td in decls {
                if matches!(td.kind, IrTypeDeclKind::Record { .. }) {
                    set.insert(td.name.to_string());
                }
            }
        };
        collect(&program.type_decls);
        for m in &program.modules { collect(&m.type_decls); }
    });
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
    params: &[almide_lang::ast::Param],
    attrs: &[almide_lang::ast::Attribute],
    return_type: &almide_lang::ast::TypeExpr,
    borrow_ref_names: &[&str],
    borrows: &mut [ParamBorrow],
) {
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
    apply_intrinsic_mut_overrides(params, attrs, return_type, &borrow_ref_names, &mut borrows);
    sigs.insert(symbol.clone(), borrows);
}

fn seed_intrinsic_sigs(sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
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

/// One fixed-point iteration's pass over top-level functions. Extracted
/// verbatim from `infer_borrow_signatures`: writes into `sigs` and into each
/// function's own `param.borrow`, never reads `sigs` back within the same
/// iteration (that read happens via the `SIGS_SNAPSHOT` thread-local frozen
/// by the caller before this runs) — a safe write-only accumulator.
fn infer_program_fn_borrows(program: &mut IrProgram, sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    for func in &mut program.functions {
        if func.is_test || is_derive_fn(func) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
        let borrows = tco_owned_params(func, infer_function_borrows(func));
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

/// One fixed-point iteration's pass over module functions. Same shape and
/// same safety rationale as `infer_program_fn_borrows`.
fn infer_program_module_borrows(program: &mut IrProgram, sigs: &mut HashMap<String, Vec<ParamBorrow>>) {
    for module in &mut program.modules {
        let mod_name = module.name.to_string();
        MOD_SCOPE.with(|m| *m.borrow_mut() = Some(mod_name.clone()));
        for func in &mut module.functions {
            if func.is_test || is_derive_fn(func) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
            let borrows = tco_owned_params(func, infer_function_borrows(func));
            sigs.insert(format!("{}::{}", mod_name, func.name), borrows.clone());
            // `ResolveCallsPass` rewrites bundled-Almide calls to
            // `CallTarget::Named { almide_rt_<m>_<f> }`. BorrowInsertion
            // looks up that Named key directly, so also mirror the
            // signature under the mangled symbol. Skip for
            // @inline_rust / @intrinsic fns — those are already seeded
            // under the mangled runtime symbol in the first loop and
            // shouldn't be overwritten by bundled-body inference.
            let is_dispatch_only = func.attrs.iter().any(|a|
                matches!(a.name.as_str(),
                    "inline_rust" | "wasm_intrinsic" | "intrinsic"));
            if !is_dispatch_only {
                let mangled = format!(
                    "almide_rt_{}_{}",
                    mod_name.replace('.', "_"),
                    func.name.as_str().replace('.', "_"),
                );
                sigs.insert(mangled, borrows.clone());
            }
            for (param, borrow) in func.params.iter_mut().zip(borrows) {
                param.borrow = borrow;
            }
        }
    }
}

pub fn infer_borrow_signatures(program: &mut IrProgram) -> HashMap<String, Vec<ParamBorrow>> {
    let mut sigs: HashMap<String, Vec<ParamBorrow>> = HashMap::new();

    seed_record_names(program);
    seed_intrinsic_sigs(&mut sigs);
    alias_float_variant_sigs(&mut sigs);

    for _iter in 0..6 {
        // Snapshot current sigs into thread-local so check_needs_ownership can see them.
        SIGS_SNAPSHOT.with(|s| *s.borrow_mut() = sigs.clone());
        let prev_sigs = sigs.clone();

        MOD_SCOPE.with(|m| *m.borrow_mut() = None);
        infer_program_fn_borrows(program, &mut sigs);
        infer_program_module_borrows(program, &mut sigs);

        if sigs == prev_sigs {
            break;
        }
    }

    // Clean up thread-locals so they don't leak across separate compilations.
    SIGS_SNAPSHOT.with(|s| s.borrow_mut().clear());
    MOD_SCOPE.with(|m| *m.borrow_mut() = None);

    sigs
}

fn infer_function_borrows(func: &IrFunction) -> Vec<ParamBorrow> {
    CURRENT_FN.with(|c| *c.borrow_mut() = Some(func.name.to_string()));

    // `@inline_rust` / `@wasm_intrinsic` fns (Stdlib Declarative
    // Unification Stage 2+) are dispatch-only declarations with a
    // Hole body. Their call sites route through a literal template
    // that is authoritative for borrow semantics — if the template
    // writes `&*{s}`, the underlying runtime takes `&str`; if it
    // writes `{s}`, the runtime consumes ownership. Running the
    // inference on a Hole body would spuriously mark every heap
    // param as `RefStr` / `RefSlice`, causing BorrowInsertionPass
    // to wrap the arg again and produce `&*&*` in the emitted Rust.
    // Default every param to Own here so the template is the sole
    // authority.
    // `@inline_rust` / `@wasm_intrinsic`: the template is authoritative
    // for borrow semantics (it spells out `&*{s}` / `&{m}` / `{n}`
    // explicitly), so every param is `Own` and the template controls
    // the arg decoration verbatim.
    let has_inline_template = func.attrs.iter().any(|a|
        matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic"));
    if has_inline_template {
        return func.params.iter().map(|_| ParamBorrow::Own).collect();
    }

    // `@intrinsic`: no template. Derive the borrow mode mechanically
    // from each param's Almide type so BorrowInsertion (not the walker)
    // decorates args at the call site:
    //   String                        → RefStr   (`&*{s}`)
    //   List / Bytes / Record / Option / Result / Map / Set
    //                                 → Ref      (`&{m}`)
    //   Int / Float / Bool / sized numerics
    //                                 → Own      (by value)
    //   Generic (TypeVar)             → Own      (caller decides)
    let has_intrinsic = func.attrs.iter().any(|a| a.name.as_str() == "intrinsic");
    if has_intrinsic {
        return func.params.iter().map(|param| {
            intrinsic_borrow_mode(&param.ty)
        }).collect();
    }

    func.params.iter().map(|param| {
        if !is_heap_type(&param.ty) {
            return ParamBorrow::Own;
        }

        // Explicit `mut` heap param → passed by mutable reference, and it is
        // authoritative: the checker (`validate_mut_args`) guarantees the caller
        // hands over a `var` binding, so the param IS a `&mut T` by construction
        // regardless of how the body uses it — it may mutate a *field* of it
        // (`list.push(b.xs, v)` on `mut b`, #703) or forward it to another `mut`
        // callee. Body-inference below tracks the param var alone, not member
        // chains, and would otherwise force a forwarded `mut` record back to Own.
        // Honor the keyword here, before those heuristics (mirrors the @intrinsic
        // mut path; a primitive `mut x: Int` is filtered by the heap guard above).
        if param.is_mut {
            return ParamBorrow::RefMut;
        }

        // If the function body directly returns this param, it needs ownership
        if is_var(&func.body, param.var) {
            return ParamBorrow::Own;
        }

        let mut needs_own = false;
        check_needs_ownership(&func.body, param.var, &mut needs_own);


        if needs_own {
            return ParamBorrow::Own;
        }

        // Implicit_mut for bundled bodies: when the body forwards this
        // param into a callee that expects `RefMut` (`bytes.set_u16_le`
        // et al), the caller's own param must also be `RefMut`. Without
        // this promotion the generated code writes `&mut b` against a
        // `b: &Vec<u8>` sig, which fails to borrow-check. Only applies
        // when `needs_own` was false — if the param was already owned
        // the `&mut` wrap would go through a local mutable binding.
        let mut needs_refmut = false;
        check_needs_refmut(&func.body, param.var, &mut needs_refmut);
        if needs_refmut {
            return ParamBorrow::RefMut;
        }

        if matches!(&param.ty, Ty::String) {
            ParamBorrow::RefStr
        } else if matches!(&param.ty, Ty::Applied(TypeConstructorId::List, _)) {
            ParamBorrow::RefSlice
        } else {
            ParamBorrow::Ref
        }
    }).collect()
}

fn is_derive_fn(func: &IrFunction) -> bool {
    // Auto-derived convention methods are excluded from borrow inference — they
    // are a generated API surface whose call sites (often cross-module, where the
    // borrow signature can't be looked up) pass owned values, so a Ref param would
    // mismatch (E0308). Once record borrow inference is enabled (#647), a
    // record-typed derived `encode(p: Pigment)` would otherwise become `&Pigment`
    // and break those owned-arg call sites.
    //
    // Identification is structural, NOT name-based: `lower/mod.rs` stamps every
    // generated convention fn with a synthetic `@derived` attribute at the single
    // point it produces them. The generator is the source of truth — we never
    // guess from the method name (`encode`/`eq`/...), which a user could also use.
    func.attrs.iter().any(|a| a.name.as_str() == "derived")
}

fn is_monomorphized(name: &str) -> bool {
    name.contains("__")
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

/// Borrow mode derived from an `@intrinsic` fn's Almide param type.
/// Used to populate the signature table so `BorrowInsertion` can
/// decorate call-site args uniformly without walker-side heuristics.
fn intrinsic_borrow_mode(ty: &Ty) -> ParamBorrow {
    match ty {
        // Owned scalars — pass by value.
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float | Ty::Float32 | Ty::Bool | Ty::Unit
            => ParamBorrow::Own,

        // String → &str.
        Ty::String => ParamBorrow::RefStr,

        // List → &Vec / &[T].
        Ty::Applied(TypeConstructorId::List, _) => ParamBorrow::RefSlice,

        // Bytes / Record / Variant / Map / Set → & reference.
        Ty::Bytes
        | Ty::Record { .. } | Ty::Variant { .. }
        | Ty::Applied(TypeConstructorId::Map, _)
        | Ty::Applied(TypeConstructorId::Set, _)
            => ParamBorrow::Ref,

        // A user-declared RECORD type (`t: Tok` → `Ty::Named("Tok")`) borrows like
        // a structural record (#647). Non-record Named types fall through to Own.
        Ty::Named(n, _) if is_record_type_name(n.as_str()) => ParamBorrow::Ref,

        // Option / Result → Own. `.unwrap_or` / `.map` consume the
        // container, and the walker renders `.is_some()` /
        // `.is_none()` via `Fn(Option<T>) -> bool` signatures that
        // accept the value by move and borrow internally. Passing a
        // `&Option<T>` would break the runtime-fn ergonomics for no
        // Almide-level gain.
        Ty::Applied(TypeConstructorId::Option, _)
        | Ty::Applied(TypeConstructorId::Result, _)
            => ParamBorrow::Own,

        // Generic TypeVar / user types / Fn / Tuple / etc. — pass owned.
        // The caller knows the concrete type; if it resolves to a borrow
        // type downstream, Clone/Borrow annotations travel through the
        // call unchanged.
        _ => ParamBorrow::Own,
    }
}

/// Eligible types for borrow inference. The Record case is the key
/// addition — without it, a `GGUFFile`-style record carried through a
/// layer loop gets `.clone()` inserted on every iteration (observed on
/// bonsai-almide at 72% inclusive time, cf.
/// memory/feedback_almide_bytes_clone.md).
fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty,
        Ty::String
        | Ty::Bytes
        | Ty::Applied(TypeConstructorId::List, _)
        // Map/Set are heap collections too — without them a `mut Map`/`mut Set`
        // parameter is forced to `Own` here (never reaching the borrow analysis),
        // so an in-place `map.insert(m, …)` emits `&mut m` against a non-`mut`
        // owned binding and fails to borrow-check (#436, E0596). With them the
        // param is inferred Ref/RefMut/Own like a List.
        | Ty::Applied(TypeConstructorId::Map, _)
        | Ty::Applied(TypeConstructorId::Set, _)
        | Ty::Record { .. }
        | Ty::OpenRecord { .. }
    ) || matches!(ty, Ty::Named(n, _) if is_record_type_name(n.as_str()))
}

include!("pass_borrow_inference_p2.rs");
include!("pass_borrow_inference_p3.rs");
