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
/// The BASE name of a stdlib call: the monomorphizer suffixes a generic
/// intrinsic's instantiation (`option.collect__Int`, `result.or_else__Int_String_String`),
/// and every name-keyed stdlib decision — registry routing, materialized-variant
/// read-shape tracking — is about the BASE fn. The instantiation's types travel
/// separately in `arg_tys`/`result_ty`, so the suffix carries no information any
/// of them needs. Keying on the suffixed form silently loses the decision: the
/// C-145 registry miss walled every `or_else` instantiation, and the same miss in
/// `is_self_host_option_module_fn` left a `match` over a mono-specialized
/// `option.collect` result UNTRACKED, walling the whole function.
///
/// #1144: a carrier name may itself BEGIN with `__` (the ADR-0006
/// `__fallible_*` family — `fs.__fallible_fold_lines`). The mono suffix is a
/// `__` strictly INSIDE the name, so the split must start AFTER a leading
/// `__`; splitting at offset 0 returned the empty string, and every name-keyed
/// decision (registry routing, read-shape tracking) then missed silently —
/// walling the whole fn instead of routing it.
pub(crate) fn base_stdlib_fn_name(func: &str) -> &str {
    let lead = if func.starts_with("__") { 2 } else { 0 };
    match func[lead..].split_once("__") {
        Some((base, _)) => &func[..lead + base.len()],
        None => func,
    }
}

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
    // Is `list.enumerate`'s source element a RICH named variant the drop
    // generator covers? Computed by the caller (LowerCtx has the layouts;
    // this router is a free fn) — gates `list.enumerate_h` (#1496); any
    // other rich element keeps the honest `_x` wall.
    enum_rich_variant: bool,
) -> String {
    // A MONO-SPECIALIZED stdlib call name (`result.or_else__Int_String_String` —
    // the optimizer suffixes a generic intrinsic's instantiation) must route by
    // its BASE name: the registry links base names only, so the suffixed form
    // fell through every router arm to an UNLINKED dotted name and walled the fn
    // (fuzz B-198's or_else). The instantiation's types are already in
    // `arg_tys`/`result_ty` — the suffix carries no information the router needs.
    let func = base_stdlib_fn_name(func);
    // #781/codopsy8: the monolithic 780-line dispatch (cog 324) is decomposed into
    // a special-case pre-router (fold/random/fan) then a per-module router — a
    // pure text-move split of the original two-phase structure (the ORIGINAL code
    // already ran the 3 special-case `if`s BEFORE the per-module `match`; this
    // just names that boundary). Routing ORDER is load-bearing and preserved: the
    // heap-accumulator `fold` guard fires BEFORE the per-module tables (a
    // scalar-acc fold over heap elements falls through to `list.fold_str`).
    let routed = list_heap_call_name_special_cases(module, func, arg_tys, result_ty).or_else(
        || list_heap_call_name_module_routed(module, func, arg_tys, result_ty, map_key_nullary, map_key_scalar_rec, enum_rich_variant),
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
    if module == "fan" && func == "any_map" {
        return Some(fan_any_call_name(arg_tys, result_ty));
    }
    if func == "fold" && matches!(module, "list" | "map" | "set") && is_heap_ty(result_ty) {
        return Some(heap_fold_call_name(module, arg_tys, result_ty));
    }
    // `fs.fold_lines` / `fs.fold_lines_chunked` (#1134, the C-220 streaming trio):
    // the `Map[String, Int]` accumulator routes to the `_msi` self-host twin
    // (fs_fold_lines.almd); any other accumulator routes to an unregistered
    // `_x` name and walls cleanly at render (never a wrong-typed link).
    // The C-220 streaming trio (#1134): typed routing to the self-host twins in
    // fs_fold_lines.almd — `Map[String, Int]` accumulators to `_msi`
    // (fold_lines / fold_lines_chunked), the `List[String]` accumulator to
    // `_ls` (fold_lines_range, the collect_partition shape). Any other
    // accumulator type routes to an unregistered `_x` name and walls cleanly
    // at render — never a wrong-typed link.
    // #1144 (C-274): the ADR-0006 fallible carrier the checker rewrites
    // `fs.fold_lines(p, z, (a, l) => g(a, l)!)` into. It routes on the SAME
    // accumulator key as the total form — `Map[String, Int]` to the `_msi`
    // fallible walker (fs_fold_lines.almd), every other accumulator to an
    // unregistered `_x` name that walls at render rather than mislinking.
    if module == "fs" && func == "__fallible_fold_lines" {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let msi_acc = matches!(arg_tys.get(1),
            Some(Ty::Applied(TC::Map, a)) if a.len() == 2
                && matches!(a[0], Ty::String) && matches!(a[1], Ty::Int));
        return Some(if msi_acc {
            "fs.__fallible_fold_lines_msi".to_string()
        } else if matches!(arg_tys.get(1), Some(Ty::Int)) {
            // The Int accumulator (fs_streaming's traced_step cell): the `_i`
            // fallible walker — same first-err trace contract as `_msi`.
            "fs.__fallible_fold_lines_i".to_string()
        } else {
            "fs.__fallible_fold_lines_x".to_string()
        });
    }
    if module == "fs" && matches!(func, "fold_lines" | "fold_lines_chunked" | "fold_lines_range") {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let init_idx = match func { "fold_lines" => 1, "fold_lines_chunked" => 2, _ => 3 };
        let msi_acc = matches!(arg_tys.get(init_idx),
            Some(Ty::Applied(TC::Map, a)) if a.len() == 2
                && matches!(a[0], Ty::String) && matches!(a[1], Ty::Int));
        let ls_acc = matches!(arg_tys.get(init_idx),
            Some(Ty::Applied(TC::List, a)) if a.len() == 1 && matches!(a[0], Ty::String));
        // The Int accumulator (#1233 — the chunked error-path shape
        // `fold_lines_chunked(p, w, 0, (acc, line) => acc)`) routes to the
        // `_i` twin; its `Result[List[Int], String]` payload class rides the
        // fan.map precedent (cap-as-tag @16, flat DropListStr — a List[scalar]
        // block frees flat). The remaining family cells (fold_lines int acc,
        // range int acc, non-String-element lists, …) stay `_x`-walled until
        // their twins land — never a wrong-typed link.
        let int_acc = matches!(arg_tys.get(init_idx), Some(Ty::Int));
        // `fold_lines`'s own String accumulator (the concat-fold shape) — the
        // `_s` twin; chunked/range String accs have no twin and stay `_x`.
        let s_acc = matches!(arg_tys.get(init_idx), Some(Ty::String));
        return Some(if msi_acc && func != "fold_lines_range" {
            format!("fs.{func}_msi")
        } else if ls_acc {
            // All three family members carry an `_ls` twin now: `fold_lines_range`
            // (the original cell), `fold_lines_chunked` (#1233), and `fold_lines`
            // itself (the read_lines-equivalence shape, fs_streaming C-220).
            format!("fs.{func}_ls")
        } else if int_acc && matches!(func, "fold_lines" | "fold_lines_chunked") {
            format!("fs.{func}_i")
        } else if s_acc && func == "fold_lines" {
            format!("fs.{func}_s")
        } else {
            format!("fs.{func}_x")
        });
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
    enum_rich_variant: bool,
) -> Option<String> {
    match module {
        "list" => list_call_name(func, arg_tys, result_ty, enum_rich_variant),
        "set" => set_call_name(func, arg_tys, result_ty),
        "map" => map_call_name(func, arg_tys, result_ty, map_key_nullary, map_key_scalar_rec),
        "result" | "option" if func == "unwrap_or" => unwrap_or_call_name(module, arg_tys),
        "option" => option_call_name(func, arg_tys, result_ty),
        "result" => result_call_name(func, arg_tys, result_ty),
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
        // #1114: to_result is now E-GENERIC (a pure bundled body). The `_h`
        // twin's sig is the String-message form, so the rename is gated to
        // E=String instantiations — the only ones whose semantics the twin
        // implements. A custom-E heap instantiation monos the bundled body
        // (and walls honestly where the heap-result ctor zoo does not cover
        // its payload — the walled-real baseline names that frontier).
        "to_result"
            if matches!(arg_tys.first(), Some(Ty::Applied(TC::Option, a))
                if a.len() == 1 && is_heap_ty(&a[0]))
                && matches!(arg_tys.get(1), Some(Ty::String)) =>
        {
            Some("option.to_result_h".to_string())
        }
        // A CUSTOM-E instantiation (#1114's typed-error route): the type-blind
        // default routed EVERY scalar-A instantiation to the registry's
        // (Int?, String) body, whose `__copy_str` read a VARIANT error block as
        // string bytes — the match over the result dispatched on a corrupt copy
        // and printed NOTHING (silent wrong output vs native's payload; the
        // shim-v1 failure selfhost-link-v2.md names). A scalar A routes to the
        // `_ve` twin (err arm co-owns the block — payload-type-independent, the
        // `_h` discipline); a HEAP A with custom E routes to a name the registry
        // does NOT serve, so the unlinked-callee wall fires (honest refusal,
        // never the silent corruption).
        "to_result"
            if matches!(arg_tys.get(1),
                Some(Ty::Named(..) | Ty::Variant { .. }
                    | Ty::Applied(TC::UserDefined(_), _))) =>
        {
            let scalar_a = matches!(arg_tys.first(), Some(Ty::Applied(TC::Option, a))
                if a.len() == 1 && !is_heap_ty(&a[0]));
            if scalar_a {
                Some("option.to_result_ve".to_string())
            } else {
                Some("option.to_result__custom_e_heap_payload".to_string())
            }
        }
        // Everything the rows above did not claim falls through to the
        // REGISTRY's base body, which is the `(o: Int?, msg: String)` form:
        // its `e` param is a String HANDLE and its err arm copies string
        // bytes. That body is therefore correct for `E = String` and for
        // nothing else — but the fall-through was type-blind, so:
        //   * a SCALAR E (Int/Float/Bool, the sized ints) passed an i64 into
        //     an i32 handle param and the module failed wasm VALIDATION —
        //     `almide check` accepted, native built, and the artifact was
        //     invalid (#1431, the acceptance-gap class);
        //   * a heap NON-String E (List/Map/Bytes) matched the i32 repr but
        //     had its block read as string bytes — the same silent corruption
        //     the custom-E row above names for variants.
        // Route both to a name the registry does not serve so the unlinked-
        // callee wall fires: an honest refusal on the wasm leg instead of a
        // broken artifact. (`Result[<scalar>, <scalar>]` is not renderable on
        // this leg anyway — the hand-written `match o { some(v) => ok(v),
        // none => err(e) }` walls on the untracked-subject rule — so there is
        // no twin to write here yet; the walled-real baseline names it.)
        "to_result" if !matches!(arg_tys.get(1), Some(Ty::String)) => {
            Some("option.to_result__unsupported_e".to_string())
        }
        "unwrap_or_else" if is_heap_ty(result_ty) => {
            Some("option.unwrap_or_else_h".to_string())
        }
        _ => None,
    }
}
