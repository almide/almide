fn value_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    match ty {
        // A nested record/tuple expands INLINE (recursive `__str_concat` + field formatters).
        Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..) if resolve_aggregate(ty, registry).is_some() => {
            aggregate_synthetic_names(ty, registry, out);
        }
        // Every OTHER value type routes to exactly ONE `to_string`-family call — the SAME single
        // wrapper [`display_value`] / [`interp_part_leaf`] emit (Int → int.to_string, Float →
        // float.to_string_compound, String → string.quote, List → list.to_string*, Map/Set/Option/
        // Result → the unlinked `<module>.to_string` that walls). Keyed off `display_leaf_call` so
        // the gate's count is BY CONSTRUCTION the lowering's emitted call set.
        _ => {
            if let Some((m, f)) = display_leaf_call(ty) {
                out.push(format!("{m}.{f}"));
            }
        }
    }
}

/// The SYNTHETIC call names the recursive Display ([`display_aggregate`]) introduces for an
/// aggregate of type `ty`: one `__str_concat` per `ConcatStr` fold the expansion builds
/// (= the number of `concat_all` parts at this level) plus the field formatters recursively.
/// MIRRORS `display_aggregate`'s structure EXACTLY so the gate credits precisely the
/// synthetic CallFns the lowering emits (count == lower for the aggregate, by construction).
fn aggregate_synthetic_names(ty: &Ty, registry: &RecordLayouts, out: &mut Vec<String>) {
    // A non-resolvable aggregate (structural record, unregistered) yields no Display tree —
    // the part declines and the whole interp credits 0 (matched by `interp_synthetic_call_names`).
    let Some((type_name, is_tuple, fields)) = resolve_aggregate(ty, registry) else {
        return;
    };
    if !is_tuple && type_name.is_none() {
        return; // structural record has no Display → walls, credits 0
    }
    // `concat_all` parts at this level: opening + (per field: a leading ", " for idx>0,
    // a "field: " label for a record, the field formatter) + closing.
    //   record: 1 (open) + Σ_i [ (i>0 → 1) + 1 (label) + 1 (formatter) ] + 1 (close)
    //   tuple:  1 (open) + Σ_i [ (i>0 → 1) +            1 (formatter) ] + 1 (close)
    let mut concat_parts = 2; // open + close
    for (idx, _) in fields.iter().enumerate() {
        if idx > 0 {
            concat_parts += 1; // ", "
        }
        if !is_tuple {
            concat_parts += 1; // "field: "
        }
        concat_parts += 1; // the field formatter expression
    }
    for _ in 0..concat_parts {
        out.push("__str_concat".to_string());
    }
    for (_, fty) in &fields {
        value_synthetic_names(fty, registry, out);
    }
}

/// Count the synthetic `CallFn`s [`desugar_string_interp`] yields for `parts` — the
/// `ConcatStr` and `module.to_string`-family call NODES of the desugared tree. The corpus
/// gate adds exactly this to its IR call count for each interp (it counts the same tree),
/// so the MIR calls the lowering emits are 1:1 backed. `None` (a part with no admitted
/// Display) ⇒ 0 (the interp stays Opaque, lowering emits no synthetic call).
pub fn interp_str_synthetic_call_count(parts: &[IrStringPart], registry: &RecordLayouts) -> usize {
    interp_synthetic_call_names(parts, registry).len()
}

/// The SYNTHETIC call names [`desugar_string_interp`] introduces for `parts`: one
/// `__str_concat` per TOP-LEVEL fold step (= `parts.len()`: K parts over the `""` seed ⇒ K
/// concats) and, per non-passthrough part, the Display wrappers it adds — a scalar part one
/// `<module>.to_string`, a RECORD/TUPLE part the full recursive `__str_concat` + field-
/// formatter set ([`aggregate_synthetic_names`]). It DOES NOT include the operands' OWN
/// inner calls (a `${g(x)}` callee) — those live in the original part exprs and are reached
/// separately by `count_ir_calls`'s descent, so no double count. Empty (a `None` desugar —
/// a part with no admitted Display) ⇒ the interp stays Opaque, crediting none.
pub fn interp_synthetic_call_names(parts: &[IrStringPart], registry: &RecordLayouts) -> Vec<String> {
    // A part with no admitted Display ⇒ the whole interp is non-desugarable (the lowering
    // returns `None` and defers to Opaque), so it credits zero synthetic calls.
    if desugar_string_interp(parts, registry).is_none() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(parts.len() * 2);
    // The TOP-LEVEL fold: K parts over the `""` seed ⇒ K `__str_concat` (the interp's own
    // outer concatenation — a record/tuple part is ONE top-level part here, its INNER
    // `__str_concat`s are added by `value_synthetic_names` below).
    for _ in 0..parts.len() {
        names.push("__str_concat".to_string());
    }
    // Per-part accumulation: each `p` writes only its own additions to the shared
    // `names` accumulator (a fold, not a router with cross-iteration state) — the
    // established safe pattern for extracting a loop body into a helper.
    for p in parts {
        push_synthetic_call_names_for_part(p, registry, &mut names);
    }
    names
}

/// One part's contribution to [`interp_synthetic_call_names`] — extracted loop body.
fn push_synthetic_call_names_for_part(p: &IrStringPart, registry: &RecordLayouts, names: &mut Vec<String>) {
    let IrStringPart::Expr { expr } = p else {
        return;
    };
    if matches!(expr.ty, Ty::String) {
        return; // a String part is a no-call passthrough
    }
    // A TOP-LEVEL record/tuple part mirrors `interp_part_leaf`'s decision tree
    // EXACTLY (the mir == ir contract): an ANON record is ALWAYS one generated
    // `__repr_anonrec_<hash>` call; an expand-foldable named/tuple part credits the
    // full recursive tree; a non-expandable NAMED record one `__repr_rec_<R>`; any
    // other non-expandable aggregate one `compound.to_string` (the wall).
    if matches!(expr.ty, Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..))
        && resolve_aggregate(&expr.ty, registry).is_some()
    {
        push_synthetic_call_names_for_aggregate_part(expr, registry, names);
    } else if let Some(n) = container_repr_name(&expr.ty, registry) {
        // Mirrors `interp_part_leaf`'s container-repr arm: ONE generated call node.
        names.push(n);
    } else {
        value_synthetic_names(&expr.ty, registry, names);
    }
}

fn push_synthetic_call_names_for_aggregate_part(expr: &IrExpr, registry: &RecordLayouts, names: &mut Vec<String>) {
    if let Ty::Record { fields } = &expr.ty {
        names.push(format!(
            "__repr_{}",
            crate::lower::anon_record_drop_name(fields)
        ));
    } else if aggregate_part_expandable(expr, registry) {
        aggregate_synthetic_names(&expr.ty, registry, names);
    } else if let Ty::Named(name, _) = &expr.ty {
        names.push(format!(
            "__repr_rec_{}",
            crate::lower::drop_fn_ident(name.as_str())
        ));
    } else {
        names.push("compound.to_string".to_string());
    }
}

/// Is a WHOLE interpolation DESUGARABLE (every part has an admitted Display)? When true, the
/// lowering folds it to a `ConcatStr` chain; when false, it stays the deferred Opaque.
/// (Desugarable does NOT imply LINKABLE — a Float part desugars but float.to_string is
/// unlinked, so the function walls at render. Use the registry to split proven-vs-walled;
/// this predicate only answers "does the lowering fold it".)
pub fn interp_str_desugarable(parts: &[IrStringPart], registry: &RecordLayouts) -> bool {
    desugar_string_interp(parts, registry).is_some()
}

/// Does `module.func` return a real MATERIALIZED `Result[Int, String]` (the DynListStr len-as-tag
/// layout)? Its result may be tracked in `materialized_results` so an `Ok`/`Err` `match` over it
/// EXECUTES. NARROW to fns actually self-hosted — any other Result is a deferred `Opaque` (len 0,
/// would misread as `Ok`). `int.parse` is the canonical for string.to_int/to_integer/parse_int.
/// The CallFn name for a stdlib `module.func` call, routing the REPR-POLYMORPHIC list combinators
/// to their `_str` variant when the RESULT is a `List[heap]` (e.g. `list.map` over a `List[String]`
/// → `list.map_str`, a DynListStr-result impl). The element repr (i64 vs i32 handle) demands a
/// separate variant; the variant reads/writes via the heap-aware prim ops. Scalar-result lists keep
/// the plain name. `module.func` is unchanged for everything else.
pub(crate) fn list_heap_call_name(
    module: &str,
    func: &str,
    arg_tys: &[Ty],
    result_ty: &Ty,
    // Is the Map KEY type (of the first-arg/result Map) a NULLARY-ONLY variant?
    // Computed by the caller (LowerCtx has the variant_layouts; this router is a
    // free fn) — gates the `_vtag` tag-normalized map family. `map_key_scalar_rec`
    // is the all-Int/Bool-field record-key twin, gating `_srec`.
    map_key_nullary: bool,
    map_key_scalar_rec: bool,
) -> String {
    // A MONO-SPECIALIZED stdlib call name (`result.or_else__Int_String_String` —
    // the optimizer suffixes a generic intrinsic's instantiation) must route by
    // its BASE name: the registry links base names only, so the suffixed form
    // fell through every router arm to an UNLINKED dotted name and walled the fn
    // (fuzz B-198's or_else). The instantiation's types are already in
    // `arg_tys`/`result_ty` — the suffix carries no information the router needs.
    let func = func.split_once("__").map_or(func, |(base, _)| base);
    // #781/codopsy8: the monolithic 780-line dispatch (cog 324) is decomposed into
    // a special-case pre-router (fold/random/fan) then a per-module router — a
    // pure text-move split of the original two-phase structure (the ORIGINAL code
    // already ran the 3 special-case `if`s BEFORE the per-module `match`; this
    // just names that boundary). Routing ORDER is load-bearing and preserved: the
    // heap-accumulator `fold` guard fires BEFORE the per-module tables (a
    // scalar-acc fold over heap elements falls through to `list.fold_str`).
    let routed = list_heap_call_name_special_cases(module, func, arg_tys, result_ty).or_else(
        || list_heap_call_name_module_routed(module, func, arg_tys, result_ty, map_key_nullary, map_key_scalar_rec),
    );
    routed.unwrap_or_else(|| format!("{module}.{func}"))
}

/// Extracted from `list_heap_call_name` (codopsy8 complexity sweep, phase 1 of 2): the 3
/// special-case guards that fire BEFORE the per-module router (`random.choice`/`shuffle`
/// hval sharing, `fan.map`, and the heap-accumulator `fold` intercept). Verbatim.
fn list_heap_call_name_special_cases(
    module: &str,
    func: &str,
    arg_tys: &[Ty],
    result_ty: &Ty,
) -> Option<String> {
    if module == "random" && matches!(func, "choice" | "shuffle") {
        return Some(random_call_name(func, arg_tys));
    }
    if module == "fan" && func == "map" {
        return Some(fan_map_call_name(arg_tys, result_ty));
    }
    if func == "fold" && matches!(module, "list" | "map" | "set") && is_heap_ty(result_ty) {
        return Some(heap_fold_call_name(module, arg_tys, result_ty));
    }
    None
}

/// Extracted from `list_heap_call_name` (codopsy8 complexity sweep, phase 2 of 2): the
/// per-module table, tried after the special cases above have declined. Verbatim.
fn list_heap_call_name_module_routed(
    module: &str,
    func: &str,
    arg_tys: &[Ty],
    result_ty: &Ty,
    map_key_nullary: bool,
    map_key_scalar_rec: bool,
) -> Option<String> {
    match module {
        "list" => list_call_name(func, arg_tys, result_ty),
        "set" => set_call_name(func, arg_tys, result_ty),
        "map" => map_call_name(func, arg_tys, result_ty, map_key_nullary, map_key_scalar_rec),
        "result" | "option" if func == "unwrap_or" => unwrap_or_call_name(module, arg_tys),
        "option" => option_call_name(func, arg_tys, result_ty),
        "result" => result_call_name(func, arg_tys, result_ty),
        // `value.keys` IS `json.keys` (one impl, two stdlib names) — remap to the
        // registered self-host; every other value.* rides its own dotted name.
        "value" if func == "keys" => Some("json.keys".to_string()),
        _ => None,
    }
}

/// Route the payload-polymorphic `option` combinators by PAYLOAD/RESULT repr. The
/// self-host impls (`option_map.almd`) are `Option[Int]`-typed — SCALAR payloads
/// only (Int/Bool/Float ride the i64 slot identically). A HEAP payload or result
/// (`option.map(some("hi"), (s) => s + "!")`) invoked the scalar impl anyway: the
/// closure declares the `$closure_fnN_h` (i32-result) type while `__opt_map_some`
/// calls through `$closure_fnN` (i64) — the "indirect call type mismatch" TRAP on
/// the verified default (the #790 option.map row, main-reachable). Route those to
/// an UNREGISTERED wall suffix instead — the fn walls, v0 runs the shape correctly
/// (the same honest-wall pattern as the map `_skv_wall` family).
fn option_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1/2 split (codopsy8 complexity sweep): the `to_list` payload check and the
    // closure-result-repr match are two independent phases of the original top-to-bottom
    // `if` + `match` — a pure text-move split, no logic change.
    option_call_name_to_list(func, arg_tys)
        .or_else(|| option_call_name_closure_result_repr(func, arg_tys, result_ty))
}

/// Extracted from `option_call_name` (codopsy8 complexity sweep, phase 1 of 2):
/// `option.to_list` keys on the PAYLOAD: a flat heap payload (String /
/// List[scalar] / scalar tuple) rides the co-owning `_rc` variant (the raw slot
/// copy aliased the payload un-owned — double free); a richer payload walls. Verbatim.
fn option_call_name_to_list(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    if func != "to_list" {
        return None;
    }
    if let Some(Ty::Applied(TC::Option, a)) = arg_tys.first() {
        if a.len() == 1 && is_heap_ty(&a[0]) {
            if matches!(a[0], Ty::String) || is_flat_scalar_block_ty(&a[0]) {
                return Some("option.to_list_rc".to_string());
            }
            return Some("option.to_list_x".to_string());
        }
    }
    None
}

/// Extracted from `option_call_name` (codopsy8 complexity sweep, phase 2 of 2): ONE mismatch
/// axis is the CLOSURE's RESULT repr: params always ride the
/// widened i64 slots, and an Option-returning closure uses the same `_h` table
/// type the impl declares (flat_map / or_else match by construction; filter's
/// pred is scalar-result; flatten / zip take no closure at all). The two shapes
/// whose USER closure result repr can diverge from the scalar-typed impl:
///   - `option.map` with a HEAP mapped payload (impl `f: (Int) -> Int` = i64
///     result; a `(s) => s + "!"` closure declares the i32 `_h` type)
///   - `option.unwrap_or_else` with a HEAP payload (impl `f: () -> Int`)
/// Verbatim.
fn option_call_name_closure_result_repr(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let heap_option =
        |t: &Ty| matches!(t, Ty::Applied(TC::Option, a) if a.len() == 1 && is_heap_ty(&a[0]));
    match func {
        // The heap twins declare the closure heap-typed, so the `_h` CallIndirect
        // table type matches by construction (option_map.almd's `_h` family).
        "map" if heap_option(result_ty) => Some("option.map_h".to_string()),
        // filter/flatten/or_else heap twins: the OTHER axis is OWNERSHIP — the
        // kept payload must SHARE (Dup) into the rebuilt some(); the scalar
        // rewrap raw-copied the handle un-owned (or_else: fuzz seed-20260718
        // index 622, correct output then an __rc_dec trap at scope end).
        "filter" if heap_option(result_ty) => Some("option.filter_h".to_string()),
        "flatten" if heap_option(result_ty) => Some("option.flatten_h".to_string()),
        "or_else" if heap_option(result_ty) => Some("option.or_else_h".to_string()),
        // `to_result` over ANY heap payload: the heap twin builds the CAP-AS-TAG
        // Result the consumers read (the scalar impl's len-as-tag misread). The twin's
        // internals are payload-type-independent (one handle slot, Dup-shared into
        // ok()), so a NESTED heap payload (`Option[Result[Int, String]]` —
        // to_result_nested_share) rides the same routine as the String payload; the
        // co-own (+1) discipline is exactly the fix for the v0 double-free that
        // fixture pinned.
        "to_result"
            if matches!(arg_tys.first(), Some(Ty::Applied(TC::Option, a))
                if a.len() == 1 && is_heap_ty(&a[0])) =>
        {
            Some("option.to_result_h".to_string())
        }
        "unwrap_or_else" if is_heap_ty(result_ty) => {
            Some("option.unwrap_or_else_h".to_string())
        }
        _ => None,
    }
}
