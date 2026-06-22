
/// Resolve a Call's return type. Order:
/// 1. User-defined functions (top-level or module) — read from SymbolTable
/// 2. Generated stdlib signatures (from TOML) with TypeVar substitution
/// 3. Stdlib `list.*` polymorphic ops — compute from lambda return types
///
/// Returning `None` is fine; the emit layer still has its fallbacks.
fn resolve_call_ret_ty(
    target: &CallTarget,
    args: &[IrExpr],
    _vt: &VarTable,
    symbols: &SymbolTable,
) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TCI;

    // 1. User-defined function lookup
    match target {
        CallTarget::Module { module, func, .. } => {
            if let Some(ret) = symbols.lookup_module(module.as_str(), func.as_str()) {
                if !ret.has_unresolved_deep() {
                    return Some(ret.clone());
                }
            }
        }
        CallTarget::Named { name } => {
            if let Some(ret) = symbols.lookup_named(name.as_str()) {
                if !ret.has_unresolved_deep() {
                    return Some(ret.clone());
                }
            }
        }
        // Calling a closure VALUE (`f(x)` where `f` is a Fn-typed var/expr — e.g.
        // a HOF lambda parameter): the call's type is the callee's RETURN type, not
        // its whole Fn type. Without this the node keeps the `fn(..) -> T` type and
        // a later `acc + f(x)` trips the IR verifier (AddInt on a function value).
        CallTarget::Computed { callee } => {
            if let Ty::Fn { ret, .. } = &callee.ty {
                if !ret.has_unresolved_deep() {
                    return Some((**ret).clone());
                }
            }
        }
        _ => {}
    }

    // Decode (module, func) from every stdlib call-target shape:
    //   - `Module { list, map }`                 — pre-lowering
    //   - `Named { "almide_rt_list_map" }`       — post-ResolveCalls or
    //                                              frontend mangling
    let (module_owned, func_owned): (String, String) = match target {
        CallTarget::Module { module, func, .. } => (module.as_str().to_string(), func.as_str().to_string()),
        CallTarget::Named { name } => {
            let s = name.as_str();
            if let Some(rest) = s.strip_prefix("almide_rt_") {
                if let Some(under) = rest.find('_') {
                    (rest[..under].to_string(), rest[under+1..].to_string())
                } else { return None }
            } else { return None }
        }
        _ => return None,
    };
    let module = module_owned.as_str();
    let func = func_owned.as_str();

    // 2. Stdlib polymorphic list operations with lambda return types.
    //    These need the lambda argument's Fn::ret, which isn't expressible
    //    in the TOML template.
    if module != "list" { return None; }

    // Helper: get the element type of List[T] argument at given index.
    let list_elem = |idx: usize| -> Option<Ty> {
        let arg = args.get(idx)?;
        if let Ty::Applied(_, a) = &arg.ty {
            a.first().cloned().filter(|t| !t.has_unresolved_deep())
        } else { None }
    };
    // Helper: get a lambda argument's return type (if it's a concrete Fn).
    let lambda_ret = |idx: usize| -> Option<Ty> {
        let arg = args.get(idx)?;
        if let Ty::Fn { ret, .. } = &arg.ty {
            if !ret.has_unresolved_deep() { Some((**ret).clone()) } else { None }
        } else { None }
    };
    // Helper: wrap in List
    let list_of = |t: Ty| Ty::Applied(TCI::List, vec![t]);

    match func {
        "map" | "filter_map" => {
            // map(list, f) -> List[ret of f]
            lambda_ret(1).map(list_of)
        }
        "filter" | "take_while" | "drop_while" | "unique_by" | "dedup_by" => {
            // filter(list, pred) -> List[elem]
            list_elem(0).map(list_of)
        }
        "flat_map" => {
            // flat_map(list, f) -> List[inner_elem of f's return]
            if let Some(inner) = lambda_ret(1) {
                if let Ty::Applied(_, a) = &inner {
                    a.first().cloned().filter(|t| !t.has_unresolved_deep()).map(list_of)
                } else { None }
            } else { None }
        }
        "zip" => {
            // zip(xs, ys) -> List[(A, B)]
            let a = list_elem(0)?;
            let b = list_elem(1)?;
            Some(list_of(Ty::Tuple(vec![a, b])))
        }
        "fold" => {
            // fold(list, init, f) -> type of init
            let init = args.get(1)?;
            if !init.ty.has_unresolved_deep() { Some(init.ty.clone()) } else { None }
        }
        "reduce" | "min_by" | "max_by" => {
            // Option[elem]
            let elem = list_elem(0)?;
            Some(Ty::Applied(TCI::Option, vec![elem]))
        }
        "any" | "all" => Some(Ty::Bool),
        "count" => Some(Ty::Int),
        "len" => Some(Ty::Int),
        "first" | "last" | "find" => {
            // Option[elem]
            let elem = list_elem(0)?;
            Some(Ty::Applied(TCI::Option, vec![elem]))
        }
        "reverse" | "sort" | "sort_by" | "dedup" => list_elem(0).map(list_of),
        "concat" | "append" | "prepend" => list_elem(0).map(list_of),
        "slice" | "take" | "drop" | "chunks" => list_elem(0).map(list_of),
        "flatten" => {
            // flatten(List[List[T]]) -> List[T]
            list_elem(0).and_then(|inner| {
                if let Ty::Applied(_, a) = &inner {
                    a.first().cloned().filter(|t| !t.has_unresolved_deep()).map(list_of)
                } else { None }
            })
        }
        "partition" => {
            // (List[elem], List[elem])
            let elem = list_elem(0)?;
            let l = list_of(elem);
            Some(Ty::Tuple(vec![l.clone(), l]))
        }
        "enumerate" => {
            // List[(Int, elem)]
            let elem = list_elem(0)?;
            Some(list_of(Ty::Tuple(vec![Ty::Int, elem])))
        }
        _ => None,
    }
}

/// Get the effective type of an expression, preferring VarTable for Var/EnvLoad
/// over the potentially-stale expr.ty.
fn effective_ty(expr: &IrExpr, vt: &VarTable) -> Ty {
    match &expr.kind {
        IrExprKind::Var { id } => {
            let vt_ty = &vt.get(*id).ty;
            if !vt_ty.has_unresolved_deep() { vt_ty.clone() }
            else { expr.ty.clone() }
        }
        IrExprKind::EnvLoad { env_var, .. } => {
            let vt_ty = &vt.get(*env_var).ty;
            if !vt_ty.has_unresolved_deep() { vt_ty.clone() }
            else { expr.ty.clone() }
        }
        _ => expr.ty.clone(),
    }
}

// ── Canonical "is this type unresolved?" check ──────────────────────
//
// Replaces the three-way confusion between:
//   - `Ty::is_unresolved()`            — Unknown | TypeVar
//   - `Ty::is_unresolved_structural()` — Unknown | TypeVar | OpenRecord
//   - `has_deep_unresolved()`          — recursive into Tuple/Applied/Fn
// This pass uses the recursive form because `Tuple([Unknown, Float])`
// must count as unresolved even though `Tuple` itself isn't.

/// Reconcile a BinOp's variant with its operand types.
/// Returns Some(new_op) when we should rewrite. Only fixes Int↔Float
/// confusion; leaves other ops alone.
fn reconcile_binop(op: BinOp, lt: &Ty, rt: &Ty) -> Option<BinOp> {
    let operand_is_float = matches!(lt, Ty::Float) || matches!(rt, Ty::Float);
    let operand_is_int = matches!(lt, Ty::Int) && matches!(rt, Ty::Int);

    match op {
        BinOp::AddInt if operand_is_float => Some(BinOp::AddFloat),
        BinOp::SubInt if operand_is_float => Some(BinOp::SubFloat),
        BinOp::MulInt if operand_is_float => Some(BinOp::MulFloat),
        BinOp::DivInt if operand_is_float => Some(BinOp::DivFloat),
        BinOp::ModInt if operand_is_float => Some(BinOp::ModFloat),
        BinOp::PowInt if operand_is_float => Some(BinOp::PowFloat),

        BinOp::AddFloat if operand_is_int => Some(BinOp::AddInt),
        BinOp::SubFloat if operand_is_int => Some(BinOp::SubInt),
        BinOp::MulFloat if operand_is_int => Some(BinOp::MulInt),
        BinOp::DivFloat if operand_is_int => Some(BinOp::DivInt),
        BinOp::ModFloat if operand_is_int => Some(BinOp::ModInt),
        BinOp::PowFloat if operand_is_int => Some(BinOp::PowInt),

        _ => None,
    }
}


// ── Audit / hard gate: residual unresolved (or value-Never) types ───
//
// Two consumers share one collector ([`collect_unresolved_sites`]):
//
//   1. The `ConcretizeTypes` postcondition ([`audit_remaining_unresolved`]),
//      verified mid-pipeline in debug / `ALMIDE_VERIFY_IR` builds.
//   2. The HARD codegen-entry gate ([`assert_types_concretized`]), run
//      unconditionally on EVERY build (debug AND release, Rust AND WASM)
//      right before emit. A surviving `Ty::Unknown` (or a value-position
//      `Ty::Never`) here is the root of the `Unknown→i32` WASM fallback that
//      silently miscompiled `fan.map` and friends: this gate turns that whole
//      class from a runtime trap into a clean compile-time error.
//
// Both read the same skip predicate so "what is a legitimate residual" is
// defined in exactly one place.

/// One residual-unresolved expression, with enough context for a diagnostic
/// that names the function and source span. `span` is `None` when the IR node
/// lost its provenance (synthetic nodes inserted by passes).
#[derive(Debug, Clone)]
pub struct UnresolvedSite {
    /// Enclosing function, e.g. `fn main` or `list::map`.
    pub location: String,
    /// IR node kind name (`Var`, `Member`, `Call`, …).
    pub kind: &'static str,
    /// `{:?}` of the offending `Ty` (e.g. `Unknown`, `Tuple([Unknown, Int])`).
    pub ty: String,
    /// Source span of the node, if it carries one.
    pub span: Option<almide_base::span::Span>,
    /// Extra context (var name + stored ty, member field, …).
    pub detail: String,
    /// True when the violation is a value-position `Ty::Never` rather than an
    /// `Unknown`/`TypeVar`. Distinguished so the diagnostic can say which.
    pub value_never: bool,
}

/// A node whose `ty` is unresolved but which legitimately has no concrete
/// runtime type to fill in — these are NOT violations. The list is small and
/// every entry is justified; it is the single source of truth shared by the
/// soft audit and the hard gate.
fn is_legit_unresolved(expr: &IrExpr) -> bool {
    // Nodes that have no runtime representation at all.
    matches!(&expr.kind,
        IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::OptionNone
        | IrExprKind::EmptyMap
    )
    // Empty list literal `[]` whose element type could not be pinned down by
    // either upstream inference or `propagate_expected_ty`. The stored element
    // count is zero, so every emit path — `Vec::<T>::new()` on Rust, the 4-byte
    // `[len=0]` header on WASM — produces the same bytes regardless of `T`.
    // Treating it as a violation would force the gate to stay soft just to
    // cover `for _ in []` / `fan.map([], f)` style uses that have no bearing on
    // runtime behavior.
    || matches!(&expr.kind,
        IrExprKind::List { elements } if elements.is_empty())
    // `ResultErr(...)` or `Unwrap { ResultErr(...) }` in guard-else: the Ok
    // slot may remain Unknown because the checker can't determine it from
    // `err()` alone. The ok-path is unreachable at runtime so the Unknown is
    // harmless.
    || matches!(&expr.kind, IrExprKind::ResultErr { .. })
    || matches!(&expr.kind,
        IrExprKind::Unwrap { expr: inner }
            if matches!(inner.kind, IrExprKind::ResultErr { .. }))
    // `Block` whose sole tail is the same skipped `Unwrap` pattern — the block
    // is just the desugared `else { err(...)! }` wrapper that lowering emits for
    // `guard` statements. `Block.ty` mirrors `tail.ty`, so marking only the
    // Unwrap would leave the outer Block as a spurious violation.
    || matches!(&expr.kind,
        IrExprKind::Block { stmts, expr: Some(tail) }
            if stmts.is_empty()
                && matches!(&tail.kind,
                    IrExprKind::Unwrap { expr: inner }
                        if matches!(inner.kind, IrExprKind::ResultErr { .. })))
    // OpenRecord-typed expressions: an open-record bound
    // (`fn f(x: { name: String, .. })`) is a structural constraint, not an
    // inference failure. The Var node for such a param trivially carries its
    // declared OpenRecord ty through monomorphization's `__Unknown` fallback
    // path. Emit handles OpenRecord via its structural dispatch — no Unknown
    // slot to fill.
    || matches!(&expr.ty, Ty::OpenRecord { .. })
    // The node's type is unresolved ONLY inside empty-container payload slots
    // (`Option[Unknown]`, `List[Unknown]`, `Set[Unknown]`, `Map[_, Unknown]`,
    // possibly nested in a `Record`/`Tuple`). This generalizes the two leaf
    // entries above (bare `OptionNone`, empty `[]`) one level up: an unannotated
    // `let leaf = { value: 1, left: none, right: none }` gives the *record* —
    // and any `Var`/`Member` reading it — a type whose only Unknowns sit in the
    // `Option` payloads of fields that are only ever `none`. A `some(x)` /
    // non-empty literal would have pinned the payload during inference, so an
    // Unknown payload that survived here is NEVER materialized; the container is
    // empty/None at runtime and its payload type is unobservable on both targets
    // (the very property that makes the bare-`OptionNone`/empty-`[]` entries
    // sound — emit already handles those exact slots). A bare `Unknown`, or one
    // inside a Tuple/Result-Ok/Fn position (which DOES carry a value), is not
    // covered and still fails the gate.
    || unresolved_only_in_empty_payloads(&expr.ty)
}

/// True when every `Unknown`/`TypeVar` in `ty` sits in an *empty-container
/// payload* position — the element slot of `Option`/`List`/`Set`, or the value
/// slot of `Map` — possibly nested through `Record`/`Tuple` fields. Such a slot
/// holds no bytes unless the container is populated, and a populated container
/// would have pinned the payload during inference; so an Unknown that reaches
/// here marks an empty/None container whose payload type is unobservable.
///
/// Returns `false` for a fully-concrete `ty` (so it never masks a real value),
/// for a bare `Unknown`/`TypeVar`, and for an Unknown in any value-bearing
/// position (`Tuple` element, `Result` Ok, `Map` KEY, `Fn` param/ret) — those
/// stay hard violations.
fn unresolved_only_in_empty_payloads(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    // Nothing unresolved ⇒ not "unresolved only in payloads" (the caller already
    // gates on `has_unresolved_deep`, but be explicit so the helper is total).
    if !ty.has_unresolved_deep() { return false; }
    // A bare residual `Unknown`/`TypeVar` is legit ONLY in an `Option` element
    // slot. That is the one undecidable-empty-payload class the frontend E018
    // check does NOT own: an unannotated `none` that is only ever `none`
    // (a recursive record field — `let leaf = { value: 1, left: none }`), whose
    // `Option` payload is never materialized. Every OTHER undecidable empty
    // collection — an empty `[]` / `[:]` / `set.new()` / `map.new()` /
    // `list.with_capacity` whose element the program never pins — is now a
    // user-facing compile error raised in the frontend BEFORE codegen (E018),
    // so a bare-`Unknown` `List`/`Set` element or `Map` value can no longer
    // reach this gate from user code. We therefore no longer whitelist it: the
    // gate is back to "an Unknown here is a COMPILER bug". The collection slots
    // still RECURSE (so a `List[Option[Unknown]]` of only-`none` elements stays
    // legit through the `Option`), but a bare `Unknown` directly in them is a
    // violation again.
    fn ok(ty: &Ty) -> bool {
        // A concrete subtree is always fine.
        if !ty.has_unresolved_deep() { return true; }
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        match ty {
            // Option element: a bare `Unknown`/`TypeVar` here is the never-
            // materialized `none` payload — the one whitelisted leaf.
            Ty::Applied(TCI::Option, args) if args.len() == 1 => {
                matches!(args[0], Ty::Unknown | Ty::TypeVar(_)) || ok(&args[0])
            }
            // List/Set element: only a DEEPER empty-payload shape (e.g. an
            // `Option` of `none`) is legit; a bare `Unknown` here was an
            // undecidable empty collection and is now an E018 the frontend
            // rejects first, so it is a gate violation again.
            Ty::Applied(TCI::List, args)
            | Ty::Applied(TCI::Set, args) if args.len() == 1 => ok(&args[0]),
            // Map[K, V]: the KEY is load-bearing (hashed/compared). The VALUE,
            // like a List element, is legit only via a deeper empty payload —
            // a bare `Unknown` value is an undecidable empty map (E018).
            Ty::Applied(TCI::Map, args) if args.len() == 2 => {
                !args[0].has_unresolved_deep() && ok(&args[1])
            }
            // Records/tuples are transparent: qualify iff EVERY unresolved field
            // is itself an empty-payload slot.
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().all(|(_, t)| ok(t))
            }
            Ty::Tuple(elems) => elems.iter().all(ok),
            // Anything else carrying an Unknown (bare Unknown/TypeVar, Result,
            // Fn, …) is load-bearing — not covered.
            _ => false,
        }
    }
    let _ = TCI::Option; // keep the import used on all cfgs
    ok(ty)
}
