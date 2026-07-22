
fn list_call_name_modifiers(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // `list.set` over a List[String] → `list.set_str` (the val is a String HANDLE, not an i64
    // Int — the generic list.set's i64 val param mismatches; set_str rc-copies + co-owns). The
    // yaml `list.set(lines, dp, …)` shape.
    // The heap-element list MODIFIERS (set/insert/remove_at/swap/update) over a List[String]/
    // List[Value]: the generic i64 impls copy slots WITHOUT rc_inc (→ double-free when
    // result+source both drop) and i32/i64-mismatch on a typed element (set/insert's `x`; for
    // update the CLOSURE — a (String/Value)->same lambda renders an i32 handle RESULT while the
    // generic f: (Int)->Int call_indirect expects i64 → runtime type-mismatch trap, #736). The
    // _str/_value variants rc-copy each element to co-own and recursively free any replaced
    // element. The element type is the first arg's List parameter.
    if matches!(func, "set" | "insert" | "remove_at" | "swap" | "update") {
        if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
            if s.len() == 1 && matches!(s[0], Ty::String) {
                return Some(format!("list.{func}_str"));
            }
            if s.len() == 1 && is_value_ty(&s[0]) {
                return Some(format!("list.{func}_value"));
            }
        }
    }
    // The element-PRESERVING List[heap]-returning combinators (source elem == result elem).
    // slice joined for the same reason take/drop did: the Int twin's raw i64 element copy
    // aliased String handles un-owned — a scope-end double free (fuzz seed-20260718 index 680).
    if matches!(func, "filter" | "reverse" | "take" | "drop" | "slice" | "unique" | "dedup" | "intersperse") {
        if let Ty::Applied(TypeConstructorId::List, args) = result_ty {
            // A List[List[String]] result element is itself a heap list — the `_str` deep-copy
            // (string.repeat) would read its length word as a byte count. take/drop SHARE the inner
            // lists by handle via the `_liststr` variant; the other combinators are a later brick.
            if args.len() == 1
                && matches!(func, "take" | "drop")
                && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && matches!(e[0], Ty::String))
            {
                return Some(format!("list.{func}_liststr"));
            }
            if args.len() == 1 && matches!(args[0], Ty::String) {
                return Some(format!("list.{func}_str"));
            }
            // A NON-String heap element (a custom variant / record / Value handle):
            // the `_str` deep-copy would read the block's length word as a byte
            // count (garbage handles — the closures_and_variants UAF). `filter`
            // routes to the rc-sharing `_rc` variant; the other combinators have
            // no handle-sharing self-host yet, so route them to an UNREGISTERED
            // `_hshare` name — the render walls it cleanly (the fold_hacc
            // precedent), never a miscompile.
            if args.len() == 1 && is_heap_ty(&args[0]) {
                if func == "filter" {
                    return Some("list.filter_rc".to_string());
                }
                // unique/dedup COMPARE elements — the `_hshare` slot-wise eq is exact
                // ONLY for a flat scalar block (all-scalar tuple / List[scalar]). A
                // String-BEARING element (record/tuple with a String slot) would
                // compare the String HANDLE — two fresh identical strings mis-compare
                // as distinct (CONFIRMED: unique over an inferred record list returned
                // 3, native 2). The String-field record case routes to the generated
                // `__krec_*` twin BEFORE this router; everything else walls.
                if matches!(func, "unique" | "dedup")
                    && !is_flat_scalar_block_ty(&args[0])
                {
                    return Some(format!("list.{func}_x"));
                }
                return Some(format!("list.{func}_hshare"));
            }
        }
    }
    None
}

fn list_call_name_accessors(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1 name-router (codopsy7 complexity sweep): each group below is a SELF-CONTAINED,
    // independent classification with no shared mutable state — a pure text-move split of the
    // original `if COND { .. }` sequence into named helpers, called in the SAME order via early
    // return. No logic change: the first helper to return `Some` wins, exactly like the original
    // top-to-bottom `if` chain.
    if let Some(name) = list_call_name_accessors_option_result(func, result_ty) {
        return Some(name);
    }
    if let Some(name) = list_call_name_partition(func, arg_tys) {
        return Some(name);
    }
    if let Some(name) = list_call_name_get_or(func, result_ty) {
        return Some(name);
    }
    if let Some(name) = list_call_name_search(func, arg_tys) {
        return Some(name);
    }
    list_call_name_closure_passthrough(func, arg_tys)
}

/// Extracted from `list_call_name_accessors` (codopsy7 complexity sweep, pattern-1
/// group 1 of 5): element-RETURNING accessors / search over a List[heap] (the result is an
/// Option[heap]) — get/first/last (positional) + find (predicate higher-order). Verbatim.
fn list_call_name_accessors_option_result(func: &str, result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "get" | "first" | "last" | "find") {
        return None;
    }
    let Ty::Applied(TypeConstructorId::Option, args) = result_ty else { return None };
    // A List[Value] element is a dynamic Value, NOT a String — the `_str` variant DEEP-
    // COPIES via `string.repeat` (corrupting an Object to {}). Route get/first/last to the
    // Value accessor, which SHARES the element (rc_inc, like value.get's Ok). (find's
    // closure-keyed Value form is a later brick — only the positional accessors here.)
    if args.len() == 1 && is_value_ty(&args[0]) && matches!(func, "get" | "first" | "last") {
        return Some(format!("list.{func}_value"));
    }
    // `find` over a `List[(Int, String)]` (the `enumerate |> find(closure)` shape): the
    // element is a tuple, NOT a String — route to `find_int_str`, which loads each element
    // as a TUPLE HANDLE and hands it to the predicate closure (reading e.g. `e.1`). The
    // `_str` fallback would load it as a String handle (garbage).
    if func == "find"
        && args.len() == 1
        && matches!(&args[0],
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String))
    {
        return Some("list.find_int_str".to_string());
    }
    // A `List[List[String]]` element is itself a heap list, NOT a String — the `_str`
    // variant would DEEP-COPY it via `string.repeat`, reading the inner list's length word
    // as a byte count (garbage). Route to the handle-SHARE `_liststr` accessor (the
    // `List[String]` analogue of `_value`); the inner list is co-owned, dropped DropListStr.
    if args.len() == 1
        && matches!(func, "get" | "first" | "last")
        && matches!(&args[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && matches!(e[0], Ty::String))
    {
        return Some(format!("list.{func}_liststr"));
    }
    // A RECORD/aggregate element (`list.get(vars, idx)` over `List[EnvVar]`
    // — porta dedup_env): SHARE the element handle exactly like the
    // Value/liststr accessors (the record stays owned by the list; the
    // `_str` deep copy would read its block as string bytes). The `_value`
    // impl is layout-identical (load_handle + Some), so reuse it. Keyed by
    // elimination (this is a free fn, no layout registry): a heap element
    // that is NOT a String/List/Value is a nominal record/tuple/variant
    // block — all of which are single-handle share-safe.
    if args.len() == 1
        && matches!(func, "get" | "first" | "last")
        && is_heap_ty(&args[0])
        && !matches!(args[0], Ty::String)
        && !is_value_ty(&args[0])
        && !matches!(&args[0], Ty::Applied(TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set, _))
    {
        return Some(format!("list.{func}_value"));
    }
    // `find`'s Some() result is a DEEP COPY of the found element (`string.repeat`,
    // list_find_str's `__lfs_some`) — correct only for an actual String element; any
    // other heap type here (a flat scalar tuple, a record) falls through to a WALL
    // rather than corrupting the copy (the same class of bug fixed above for
    // contains/index_of — `find` just hasn't grown a `_hshare` copy variant yet).
    if args.len() == 1 && matches!(args[0], Ty::String) {
        return Some(format!("list.{func}_str"));
    }
    // Any remaining heap-element shape (find over Value/List/Map/Set; get/first/last
    // over a non-String heap List element) has no covering arm above — route to a
    // deliberately UNREGISTERED `_x` name. The bare `list.{func}` name is NOT a safe
    // fallback here: it links against the Int-typed generic self-host (list_search.almd
    // et al), which silently "succeeds" via raw i64-slot / pointer-identity comparison
    // instead of refusing to link (the fallthrough danger confirmed by probe elsewhere
    // in this dispatch — see the contains/index_of comment above).
    // get/first/last over a FLAT scalar-slot block element (List[scalar] /
    // all-scalar tuple): the handle-SHARING variant (Some payload rc_inc'd
    // by the Some-ctor Dup) — the deep-copy `_str` corrupts these shapes.
    if args.len() == 1
        && matches!(func, "get" | "first" | "last")
        && is_flat_scalar_block_ty(&args[0])
    {
        return Some(format!("list.{func}_hshare"));
    }
    if args.len() == 1 && is_heap_ty(&args[0]) {
        return Some(format!("list.{func}_x"));
    }
    None
}

/// Extracted from `list_call_name_accessors` (codopsy7 complexity sweep, pattern-1
/// group 2 of 5): `list.partition` keys on its element — scalar → the raw-copy self-host; a
/// FLAT heap element (String / List[scalar] / scalar tuple) → the co-owning `_rc` variant;
/// any richer element walls (`_x` — no recursive-co-own variant yet). Verbatim.
fn list_call_name_partition(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "partition" {
        return None;
    }
    let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() else { return None };
    if a.len() != 1 {
        return None;
    }
    if !is_heap_ty(&a[0]) {
        return Some("list.partition".to_string());
    }
    if matches!(a[0], Ty::String) || is_flat_scalar_block_ty(&a[0]) {
        return Some("list.partition_rc".to_string());
    }
    Some("list.partition_x".to_string())
}

/// Extracted from `list_call_name_accessors` (codopsy7 complexity sweep, pattern-1
/// group 3 of 5): `get_or` returns the ELEMENT directly (not an Option). Over a List[heap] it
/// must return an i32 handle (a deep copy), so it is keyed on the heap RESULT being the
/// element type. Verbatim.
fn list_call_name_get_or(func: &str, result_ty: &Ty) -> Option<String> {
    if !(func == "get_or" && is_heap_ty(result_ty)) {
        return None;
    }
    // A Value element SHARES (the _str deep copy corrupts a Value block —
    // json_gltf_walk's `list.get_or(meshes, 0, json.null())` returned "?").
    if is_value_ty(result_ty) {
        return Some("list.get_or_value".to_string());
    }
    if matches!(result_ty, Ty::String) {
        return Some("list.get_or_str".to_string());
    }
    // No handle-sharing self-host for other heap elements yet — an
    // UNREGISTERED name walls cleanly (the fold_hacc precedent).
    Some("list.get_or_hshare".to_string())
}

/// Extracted from `list_call_name_accessors` (codopsy7 complexity sweep, pattern-1
/// group 4 of 5): SUBJECT-keyed (arg 0) over a List[heap], where the result is scalar
/// (Bool/Int/Option[Int]) so it can't be keyed on the result type: search
/// (contains/index_of) does an ELEMENT-EQUALITY comparison — String routes to the byte-eq
/// `_str` family (correct only for actual String elements: __str_eq reads the length FIELD
/// as a byte count); a flat scalar-slot block (all-scalar tuple / List[scalar]) routes to
/// the slot-wise `_hshare` family (B32's `__uh_eq`, which reads length as an ELEMENT count
/// and compares raw i64 slots — exact for this shape). Any OTHER heap element (record,
/// Value, nested heap list, String-bearing tuple) has no correct comparison variant yet —
/// falls through to a WALL (never routed to `_str`, which would silently produce WRONG
/// results: a tuple/list `len` misread as a byte count truncates the compare to its first
/// ~2 bytes, a confirmed false-positive collision). NOTE: falling through to the bare
/// `list.contains`/`list.index_of` name here is NOT a safe wall — it links against the
/// Int-typed generic (list_search.almd), which silently "succeeds" (raw i64-slot compare,
/// i.e. POINTER-IDENTITY on a heap handle — the exact OLD C-015 bug) rather than refusing
/// to link (CONFIRMED by probe: a record-element `list.contains` produced invalid wasm via
/// this path, and a tuple-element `set.from_list` sharing the same generic-fallthrough
/// shape silently mis-deduped). Any excluded heap element must route to a deliberately
/// UNREGISTERED name (`_x`, the established wall-suffix convention) so the render step's
/// unlinked-call check catches it. Verbatim.
fn list_call_name_search(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "contains" | "index_of") {
        return None;
    }
    let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() else { return None };
    if a.len() != 1 || !is_heap_ty(&a[0]) {
        return None;
    }
    if matches!(a[0], Ty::String) {
        return Some(format!("list.{func}_str"));
    }
    if is_flat_scalar_block_ty(&a[0]) {
        return Some(format!("list.{func}_hshare"));
    }
    Some(format!("list.{func}_x"))
}

/// Extracted from `list_call_name_accessors` (codopsy7 complexity sweep, pattern-1
/// group 5 of 5): all/any/count/fold are pure closure-passthrough (each element handle is
/// loaded and handed to the predicate/accumulator closure — no internal equality compare or
/// deep copy), so the byte-eq `_str` family's ITERATION shape is safe for ANY heap element
/// type, not just String. Verbatim.
fn list_call_name_closure_passthrough(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "all" | "any" | "count" | "fold") {
        return None;
    }
    let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() else { return None };
    if a.len() == 1 && is_heap_ty(&a[0]) {
        return Some(format!("list.{func}_str"));
    }
    None
}


/// Every `set.*` typed-variant route (see the block comments).
fn set_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // `Set[heap]`-RETURNING constructors key on the RESULT element type; `set.to_list` over a
    // `Set[heap]` returns a `List[heap]`; the predicate `set.contains` keys on its SUBJECT
    // (arg 0) element type (its result is Bool). Every one of these funcs relies on `__str_eq`
    // (byte-level String-layout equality, for membership/dedup) AND/OR `string.repeat` (a
    // String-specific deep copy) internally (set_str.almd) — BOTH are unsound for a non-String
    // heap element: `__str_eq` misreads a block's slot-count `len` as a byte count (a confirmed
    // false-positive collision past the first ~2 bytes), and `string.repeat` would corrupt a
    // tuple/record/list block's bytes. Restrict the `_str` route to an ACTUAL String element;
    // any other heap element (tuple, record, nested list, Value) falls through to a WALL — no
    // correct Set variant exists yet for those (the flat-scalar `_hshare` family list.contains
    // just grew does not cover Set's dedup-on-build/algebra ops).
    let result_elem_is_string = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::String)
    );
    // RESULT-keyed: constructors / Set-returning algebra over heap elements. The bare
    // `set.{func}` fallback is NOT a safe wall for a non-String heap element here — it links
    // against the Int-typed generic (set_core.almd), which silently "succeeds" via raw i64-slot
    // / pointer-identity comparison (the OLD C-015 bug) instead of refusing to link (CONFIRMED
    // by probe: `set.from_list` over a `List[(Int,Int)]` silently mis-deduped, len 3 instead of
    // 2, then trapped later). Route explicitly to the UNREGISTERED `_x` wall name instead.
    let result_elem_is_heap = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && is_heap_ty(&a[0])
    );
    // FLAT all-scalar TUPLE elements (`Set[(Int, Int)]` — the C-015 compound-eq
    // class): each element normalizes through `__srec_key` (map_vkey.almd — the
    // tuple block is physically the all-scalar record) into a backing `_str` set,
    // so dedup/lookup are CONTENT equality. Build/lookup fns only — an
    // iteration-returning fn (to_list) would surface the normalized strings and
    // keeps the wall below.
    {
        let tup_set = |t: &Ty| {
            matches!(t, Ty::Applied(TypeConstructorId::Set, a) if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts)
                    if !ts.is_empty() && ts.iter().all(|c| matches!(c, Ty::Int | Ty::Bool))))
        };
        if matches!(func, "from_list" | "insert" | "contains")
            && (arg_tys.first().is_some_and(tup_set) || tup_set(result_ty))
        {
            return Some(format!("set.{func}_srec"));
        }
    }
    if matches!(
        func,
        "from_list" | "to_list" | "union" | "intersection" | "difference"
            | "new" | "insert" | "remove" | "symmetric_difference" | "filter"
    ) && result_elem_is_heap
    {
        return Some(if result_elem_is_string {
            format!("set.{func}_str")
        } else {
            format!("set.{func}_x")
        });
    }
    // TYPE-CHANGING `set.map` to a HEAP element result (C-039's `set.map(si, (x) =>
    // "n" + …)`): the generic's i64-result closure table type traps on the i32-result
    // closure. A scalar subject → String result composes through the insert_str twin;
    // any other heap result walls (`_x`). (A scalar RESULT over any subject keeps the
    // generic — its dedup and closure table are i64-correct there.)
    if func == "map" && result_elem_is_heap {
        let subj_scalar = matches!(arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && !is_heap_ty(&a[0]));
        return Some(if result_elem_is_string && subj_scalar {
            "set.map_i2s".to_string()
        } else {
            "set.map_x".to_string()
        });
    }
    // `all`/`any`/`fold` are pure closure-passthrough (no internal eq compare or deep copy) —
    // safe for any heap element type, same as the list-module analogue above.
    let arg0_elem_is_heap = matches!(
        arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
    );
    if matches!(func, "all" | "any" | "fold") && arg0_elem_is_heap {
        return Some(format!("set.{func}_str"));
    }
    // ARG-keyed eq/membership: a Bool-returning fn over a `Set[heap]` subject (arg 0) — String
    // element only (the `__str_eq`/`__set_has_str` unsoundness above); any other heap element
    // routes to the `_x` wall (the same fallthrough danger as the RESULT-keyed family).
    let arg0_elem_is_string = matches!(
        arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && matches!(a[0], Ty::String)
    );
    if matches!(func, "contains" | "is_subset" | "is_disjoint" | "eq") && arg0_elem_is_heap {
        return Some(if arg0_elem_is_string {
            format!("set.{func}_str")
        } else {
            format!("set.{func}_x")
        });
    }
    None
}

/// Every `map.*` typed-variant route: the skv entries/from_list fast paths,
/// then the (key, value) repr-family table (see the block comments).
fn map_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty, map_key_nullary: bool, map_key_scalar_rec: bool) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // `map.entries` / `map.from_list` over the skv repr (`Map[String, scalar]` — the
    // tokenizer vocab): route to the skv self-hosts; the all-String repr keeps its
    // existing `map.entries_str`. Other reprs wall (unregistered).
    if func == "entries" {
        if let Some(Ty::Applied(TypeConstructorId::Map, a)) = arg_tys.first() {
            if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]) {
                return Some("map.entries_skv".to_string());
            }
        }
    }
    if func == "from_list" {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && !is_heap_ty(&ts[1]))
            {
                return Some("map.from_list_skv".to_string());
            }
        }
    }
    // A map's REPR is set by its (key, value) heap-ness, read from whichever Map type the call
    // exposes: arg 0 (the SUBJECT of set/get/fold/filter/…) takes priority, else the RESULT
    // (map.new() has no args). The two repr families:
    //   key heap, value heap  → `_str` (map_str: interleaved all-String entries)
    //   key heap, value scalar → `_skv` (map_skv: String keys + i64 values, serves
    //                            Map[String,Int] AND Map[String,Float] — the value is one i64)
    //   key scalar             → the plain map_core (Map[Int,Int]); a scalar-key heap-value map
    //                            has no variant yet, so it falls through (walled by repr).
    // The element-returning forms (get → Option[V], keys/values → List[elem]) read the same
    // key/value reprs off the subject map (arg 0).
    let map_kv = |ty: &Ty| match ty {
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => {
            Some((is_heap_ty(&a[0]), is_heap_ty(&a[1])))
        }
        _ => None,
    };
    let kv = arg_tys
        .first()
        .and_then(&map_kv)
        .or_else(|| map_kv(result_ty));
    if let Some((key_heap, val_heap)) = kv {
        // Each repr family routes ONLY the funcs its self-hosted variant file actually defines
        // (an unlisted func keeps the plain name — never a dangling `_str`/`_skv` reference).
        // Whether the VALUE type is exactly String — the `_str` variant stores
        // interleaved all-String entries, so a heap-but-not-String value
        // (`Map[String, List[Int]]`) must NOT route there (its deep-copy would
        // read the list block as string bytes). It routes to an UNREGISTERED
        // `_hval` name instead — a clean render wall, never invalid wasm.
        let val_is_string = matches!(
            arg_tys.first().or(Some(result_ty)),
            Some(Ty::Applied(TypeConstructorId::Map, a)) if a.len() == 2 && matches!(a[1], Ty::String)
        );
        // Both the `_str` (all-String interleaved entries) and `_skv` (String key + i64 value)
        // families do KEY EQUALITY via `__str_eq`/`__skv_eq` — a byte-level compare that
        // misreads a non-String heap block's slot-count `len` as a byte count (the same
        // confirmed false-positive-collision class fixed above for list/set). A non-String heap
        // KEY (a tuple, record, nested list) must NOT route to either family — CONFIRMED via
        // probe to currently produce INVALID WASM (an i32/i64 ABI-width mismatch on `map.set`
        // with a tuple key), not silently wrong bytes, but still not the honest wall this repr
        // gate is meant to guarantee. Gate both families on an ACTUAL String key.
        let key_is_string = matches!(
            arg_tys.first().or(Some(result_ty)),
            Some(Ty::Applied(TypeConstructorId::Map, a)) if a.len() == 2 && matches!(a[0], Ty::String)
        );
        // The FLAT-heap-value class map_hval actually implements (List[scalar]).
        let val_is_flat_list = matches!(
            arg_tys.first().or(Some(result_ty)),
            Some(Ty::Applied(TypeConstructorId::Map, a))
                if a.len() == 2 && matches!(&a[1],
                    Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && !is_heap_ty(&e[0]))
        );
        // The CLOSURE-value class (`Map[String, () -> Unit]` — the mclo family): the
        // hval twins are handle-level (set stores + rc-shares the value handle, get/
        // get_or share it back), so a closure block rides them unchanged; only the
        // DROP routes differently (`$__drop_map_mclo` — the bind/arg registration
        // sites key on `is_map_fn_ty`). `eq` is EXCLUDED (hval eq compares values
        // STRUCTURALLY as List — closure identity has no such compare; it stays an
        // honest wall).
        let val_is_fn = matches!(
            arg_tys.first().or(Some(result_ty)),
            Some(Ty::Applied(TypeConstructorId::Map, a))
                if a.len() == 2 && matches!(a[1], Ty::Fn { .. })
        );
        let variant = match (key_heap, val_heap) {
            // `Map[String, List[scalar]]` — the implemented subset of the heap-value
            // family (new/set/eq/len/contains/get/get_or; other funcs keep the
            // unregistered wall name). `get`/`get_or` SHARE the stored value (the
            // hshare discipline).
            (true, true)
                if val_is_flat_list
                    && matches!(
                        func,
                        "new" | "set" | "eq" | "len" | "contains" | "get" | "get_or" | "keys"
                    ) =>
            {
                Some("_hval")
            }
            (true, true)
                if val_is_fn
                    && matches!(func, "new" | "set" | "len" | "contains" | "get" | "get_or") =>
            {
                Some("_hval")
            }
            // `Map[String, <Fn>]` from_list (keyed on the RESULT — the arg is the pairs
            // List): the hval pair-walk is handle-level (each value rc-shared in via
            // `map_set_hval`), so a closure block rides it unchanged; the RESULT's
            // type-driven drop routing (`is_map_fn_ty` → map_mclo) frees the values via
            // `__drop_closure`.
            (true, true)
                if func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(a[1], Ty::Fn { .. })) =>
            {
                Some("_hval")
            }
            // `Map[String, List[Int]]` from_list / display (the map-of-lists literal):
            // keyed on the RESULT/first-arg map; to_string_hval passes through
            // verbatim (the B22 suffix guard).
            // `Map[String, String]` from_list (the String-valued map literal): keyed on
            // the RESULT type (from_list's first arg is the pairs List, not a Map).
            (true, true)
                if func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2
                            && matches!(a[0], Ty::String)
                            && matches!(a[1], Ty::String)) =>
            {
                Some("_str")
            }
            (true, true)
                if !val_is_string
                    && func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2
                            && matches!(a[0], Ty::String)
                            && matches!(&a[1], Ty::Applied(TypeConstructorId::List, b)
                                if b.len() == 1 && matches!(b[0], Ty::Int))) =>
            {
                Some("_hval")
            }
            // `Map[String, Map[String, String]]` get_or / from_list — the msv family
            // (map_fold_heap_acc's nested-map literal + get_or default).
            (true, true)
                if func == "get_or"
                    && matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::Map, a))
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(&a[1], Ty::Applied(TypeConstructorId::Map, b)
                                if b.len() == 2
                                    && matches!(b[0], Ty::String)
                                    && matches!(b[1], Ty::String))) =>
            {
                Some("_msv")
            }
            (true, true)
                if func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(&a[1], Ty::Applied(TypeConstructorId::Map, b)
                                if b.len() == 2
                                    && matches!(b[0], Ty::String)
                                    && matches!(b[1], Ty::String))) =>
            {
                Some("_msv")
            }
            // `Map[String, List[Option[Int]]]` from_list — the mlo family
            // (compound_repr_interp's `deep` inner-map literal).
            (true, true)
                if func == "from_list"
                    && crate::lower::is_map_mlo_ty(result_ty) =>
            {
                Some("_mlo")
            }
            (true, true) if func == "to_string_hval" => Some(""),
            // An ALREADY-SUFFIXED mlo display (`map.to_string_mlo` from the interp
            // leaf) — pass through verbatim (re-suffixing would fabricate
            // `to_string_mlo_hval_wall`).
            (true, true) if func == "to_string_mlo" => Some(""),
            // `map.from_list` over a NAMED-value map (`["o": Point{..}]` / `["a":
            // Circle(3.0)]` — the desugared map literal): construction is handle-level
            // (the `_str` family's pair copy + co-own rc_inc works for ANY heap value
            // slot); the RESULT's type-driven drop routing (`map_named_value_drop`)
            // decides the correct sweep, and an unadmitted value type walls THERE —
            // never a leaky flat link here.
            (true, true)
                if func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(a[1], Ty::Named(..))) =>
            {
                Some("_hobj")
            }
            // An ALL-SCALAR record key with a String value (`Map[Color, String]` —
            // the hash_protocol deriving-Hash shape): the key normalizes to the
            // comma-joined decimal string of its slots (map_vkey.almd's _srec
            // family — content identity ⇔ string identity for Int/Bool fields).
            (true, true)
                if map_key_scalar_rec
                    && matches!(func, "from_list" | "get")
                    && {
                        let str_val = |t: &Ty| {
                            matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                                if a.len() == 2 && matches!(a[1], Ty::String))
                        };
                        arg_tys.first().is_some_and(str_val) || str_val(result_ty)
                    } =>
            {
                Some("_srec")
            }
            // `map.entries` over the hval-TUPLE flavor (`Map[String, (Int, Int)]` —
            // the C-039 tuple-valued map.map result): the typechange twin.
            (true, true)
                if func == "entries"
                    && matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::Map, a))
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(&a[1], Ty::Tuple(ts)
                                if !ts.is_empty() && ts.iter().all(|c| !is_heap_ty(c)))) =>
            {
                Some("_hvalt")
            }
            // TYPE-CHANGING `map.map` str → skv (`map.map(ms, (v) => string.len(v))`
            // — C-039): the result value narrows to a scalar.
            (true, true)
                if key_is_string
                    && func == "map"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1])) =>
            {
                Some("_str2skv")
            }
            (true, true) if !val_is_string => Some("_hval_wall"),
            // A fn OUTSIDE the family's implemented list must fall to an UNREGISTERED
            // wall suffix, NEVER to `None`: a `None` here returns the BARE `map.{func}`
            // name, which links the scalar-key map_core generic against the string-key
            // 16-byte-stride layout — raw i64 slot copies of STRING handles with no
            // rc_inc, so the result map aliases its inputs' keys unowned and scope-end
            // double-frees them (`map.merge` on `Map[String, Int]` trapped the rc_dec
            // sentinel on the verified default — the #790 map-merge row).
            // `to_string` (the `${map}` interp display) links the BARE self-host
            // `map.to_string` — the pre-wall behavior this family's wall must not
            // mangle (map_interp_self_hosts_via_keys_values pins it).
            (true, true) | (true, false) if key_is_string && func == "to_string" => Some(""),
            (true, true) if key_is_string => Some(
                if matches!(
                    func,
                    "new" | "set" | "remove" | "merge" | "update" | "filter" | "get" | "keys"
                        | "values" | "len" | "is_empty" | "contains" | "all" | "any" | "count"
                        | "fold" | "entries"
                ) {
                    "_str"
                } else {
                    "_str_wall"
                },
            ),
            // TYPE-CHANGING `map.map` skv → str (`map.map(mi, (v) => int.to_string(v)
            // + "!")` — C-039): String values in the result → the typechange twin.
            (true, false)
                if key_is_string
                    && func == "map"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(a[1], Ty::String)) =>
            {
                Some("_skv2str")
            }
            // TYPE-CHANGING `map.map` skv → hval-TUPLE (`map.map(mi, (v) => (v, v*v))`
            // — C-039): all-scalar tuple values → the raw skv-split build twin.
            (true, false)
                if key_is_string
                    && func == "map"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String)
                            && matches!(&a[1], Ty::Tuple(ts)
                                if !ts.is_empty() && ts.iter().all(|c| !is_heap_ty(c)))) =>
            {
                Some("_skv2hvalt")
            }
            // `map.map` transforms the VALUES — the skv impl is scalar-value in AND out, so
            // it also needs the RESULT map's value scalar (a `(v) => int.to_string(v)` maps
            // into the `_str` repr — no skv form; wall it rather than mislink).
            (true, false)
                if key_is_string
                    && func == "map"
                    && !matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 && !is_heap_ty(&a[1])) =>
            {
                Some("_skv_wall")
            }
            // An ALREADY-SUFFIXED skv-repr display (`map.to_string_sb` from the interp
            // leaf — `${Map[String, Bool]}`) — pass through verbatim (re-suffixing
            // would fabricate `to_string_sb_skv_wall`, the to_string_hval lesson).
            (true, false) if func == "to_string_sb" => Some(""),
            (true, false) if key_is_string => Some(
                if matches!(
                    func,
                    "new" | "set" | "remove" | "filter" | "get" | "get_or" | "keys" | "values"
                        | "len" | "is_empty" | "contains" | "all" | "any" | "count" | "fold"
                        | "eq" | "find" | "update" | "merge" | "map"
                ) {
                    "_skv"
                } else {
                    "_skv_wall"
                },
            ),
            // A NULLARY-ONLY variant key with a SCALAR value (`Map[Direction, Int]` —
            // the hash_protocol deriving-Hash shape): the key's identity IS its tag,
            // so normalize to a raw i64 key (map_vkey.almd's from_list/get). Other
            // funcs and heap values keep the honest wall below.
            (true, false)
                if map_key_nullary
                    && matches!(func, "from_list" | "get")
                    && {
                        // from_list's FIRST arg is the pairs List — probe the
                        // RESULT type too (the is_ivh OR discipline).
                        let scalar_val = |t: &Ty| {
                            matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                                if a.len() == 2 && !is_heap_ty(&a[1]))
                        };
                        arg_tys.first().is_some_and(scalar_val) || scalar_val(result_ty)
                    } =>
            {
                Some("_vtag")
            }
            // FLAT all-scalar TUPLE key with a String value (`Map[(Int, Int), String]`
            // — C-015): the key normalizes through `__srec_key` (the tuple block IS the
            // all-scalar record physically) into the backing `_str` map — content
            // identity ⇔ string identity. `len` reads the backing str-flavor header
            // directly; the lookup/build set routes `_srec`.
            (true, true) | (true, false)
                if {
                    let tup_key_str_val = |t: &Ty| {
                        matches!(t, Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2
                            && matches!(&a[0], Ty::Tuple(ts)
                                if !ts.is_empty()
                                    && ts.iter().all(|c| matches!(c, Ty::Int | Ty::Bool)))
                            && matches!(a[1], Ty::String))
                    };
                    (arg_tys.first().is_some_and(tup_key_str_val)
                        || tup_key_str_val(result_ty))
                        && matches!(func, "from_list" | "get" | "set" | "contains" | "len")
                } =>
            {
                if func == "len" {
                    return Some("map.len_str".to_string());
                }
                Some("_srec")
            }
            // A non-String heap KEY (tuple/record/nested list) reaching here has no correct
            // variant — route to an explicit UNREGISTERED wall name rather than falling through
            // to the bare `map.{func}` name, which links against the scalar-key map_core generic
            // and produces INVALID WASM (an i32/i64 ABI-width mismatch, CONFIRMED by probe) —
            // a crash, not the honest compile-time wall this repr gate exists to guarantee.
            (true, true) | (true, false) => Some("_key_wall"),
            // `Map[Int, String]` — the implemented scalar-key/heap-value variant
            // (new/set/eq). Other funcs, and other heap value types, keep an
            // UNREGISTERED wall name (never the plain Map[Int,Int] i64-slot link
            // that emitted invalid wasm — map_set_eq's original failure).
            // An ALREADY-SUFFIXED synthesized display call (`map.to_string_ivh` from
            // the interp leaf table) — pass through verbatim (re-suffixing would
            // fabricate `to_string_ivh_ivh_wall`).
            (false, true) if func == "to_string_ivh" => Some(""),
            (false, true)
                if {
                    // `from_list`'s FIRST arg is the pairs List, not the Map — key
                    // its admission on the RESULT type instead (either probe works
                    // for the Map-first fns).
                    // A Bool key is the SAME raw i64 slot (0/1) — the ivh find's
                    // i64 compare serves it verbatim, so admit it alongside Int.
                    let is_ivh = |t: &Ty| {
                        matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                            if a.len() == 2
                                && matches!(a[0], Ty::Int | Ty::Bool)
                                && matches!(a[1], Ty::String))
                    };
                    (arg_tys.first().is_some_and(is_ivh) || is_ivh(result_ty))
                        && matches!(func, "new" | "set" | "eq" | "from_list" | "len" | "get")
                } =>
            {
                Some("_ivh")
            }
            // TYPE-CHANGING `map.map` ivh → core (`map.map(mik, (v) => string.len(v))`
            // — C-039): the String values narrow to scalars.
            (false, true)
                if func == "map"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2
                            && matches!(a[0], Ty::Int | Ty::Bool)
                            && !is_heap_ty(&a[1])) =>
            {
                Some("_ivh2core")
            }
            (false, true) => Some("_ivh_wall"),
            // `Map[Int, Float]` from_list (the float_map literal): ONE scalar-KV impl
            // (map_if.almd) — the paired i64 slots carry f64 bits verbatim. Display
            // routes separately (the interp table's to_string_if). Other scalar-scalar
            // maps keep the plain (unlinked) name — walls cleanly.
            (false, false)
                if func == "from_list"
                    && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2
                            && matches!(a[0], Ty::Int)
                            && matches!(a[1], Ty::Float)) =>
            {
                Some("_if")
            }
            (false, false) if func == "to_string_if" => Some(""),
            _ => None,
        };
        if let Some(suffix) = variant {
            return Some(format!("map.{func}{suffix}"));
        }
    }
    None
}


