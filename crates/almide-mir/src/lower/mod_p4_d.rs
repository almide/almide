
fn list_call_name_modifiers(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1 name-router (codopsy7 complexity sweep): the two groups below are
    // independent, self-contained classifications — a pure text-move split, called in the
    // SAME order via early return. No logic change.
    if let Some(name) = list_call_name_element_modifiers(func, arg_tys) {
        return Some(name);
    }
    list_call_name_preserving_combinators(func, result_ty)
}

/// Extracted from `list_call_name_modifiers` (codopsy7 complexity sweep, group 1 of 2):
/// `list.set` over a List[String] → `list.set_str` (the val is a String HANDLE, not an i64
/// Int — the generic list.set's i64 val param mismatches; set_str rc-copies + co-owns). The
/// yaml `list.set(lines, dp, …)` shape. The heap-element list MODIFIERS
/// (set/insert/remove_at/swap/update) over a List[String]/List[Value]: the generic i64 impls
/// copy slots WITHOUT rc_inc (→ double-free when result+source both drop) and i32/i64-mismatch
/// on a typed element (set/insert's `x`; for update the CLOSURE — a (String/Value)->same
/// lambda renders an i32 handle RESULT while the generic f: (Int)->Int call_indirect expects
/// i64 → runtime type-mismatch trap, #736). The _str/_value variants rc-copy each element to
/// co-own and recursively free any replaced element. The element type is the first arg's List
/// parameter. Verbatim.
fn list_call_name_element_modifiers(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "set" | "insert" | "remove_at" | "swap" | "update") {
        return None;
    }
    let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() else { return None };
    if s.len() == 1 && matches!(s[0], Ty::String) {
        return Some(format!("list.{func}_str"));
    }
    if s.len() == 1 && is_value_ty(&s[0]) {
        return Some(format!("list.{func}_value"));
    }
    None
}

/// Extracted from `list_call_name_modifiers` (codopsy7 complexity sweep, group 2 of 2): the
/// element-PRESERVING List[heap]-returning combinators (source elem == result elem). slice
/// joined for the same reason take/drop did: the Int twin's raw i64 element copy aliased
/// String handles un-owned — a scope-end double free (fuzz seed-20260718 index 680). Verbatim.
fn list_call_name_preserving_combinators(func: &str, result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "filter" | "reverse" | "take" | "drop" | "slice" | "unique" | "dedup" | "intersperse") {
        return None;
    }
    let Ty::Applied(TypeConstructorId::List, args) = result_ty else { return None };
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
        if matches!(func, "unique" | "dedup") && !is_flat_scalar_block_ty(&args[0]) {
            return Some(format!("list.{func}_x"));
        }
        return Some(format!("list.{func}_hshare"));
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


/// Every `set.*` typed-variant route (see the block comments). `Set[heap]`-RETURNING
/// constructors key on the RESULT element type; `set.to_list` over a `Set[heap]` returns a
/// `List[heap]`; the predicate `set.contains` keys on its SUBJECT (arg 0) element type (its
/// result is Bool). Every one of these funcs relies on `__str_eq` (byte-level String-layout
/// equality, for membership/dedup) AND/OR `string.repeat` (a String-specific deep copy)
/// internally (set_str.almd) — BOTH are unsound for a non-String heap element: `__str_eq`
/// misreads a block's slot-count `len` as a byte count (a confirmed false-positive collision
/// past the first ~2 bytes), and `string.repeat` would corrupt a tuple/record/list block's
/// bytes. Restrict the `_str` route to an ACTUAL String element; any other heap element
/// (tuple, record, nested list, Value) falls through to a WALL — no correct Set variant
/// exists yet for those (the flat-scalar `_hshare` family list.contains just grew does not
/// cover Set's dedup-on-build/algebra ops).
///
/// Pattern-1 name-router (codopsy7 complexity sweep): each group below is an independent
/// classification, called in the SAME order via early return — pure text-move, no logic
/// change. The shared `result_elem_is_string`/`result_elem_is_heap`/`arg0_elem_is_*` derived
/// booleans are RECOMPUTED locally inside each helper that needs them (pure, read-only
/// functions of `arg_tys`/`result_ty` — cheap and side-effect-free to duplicate; this is NOT
/// the mutable cross-arm state-threading pattern that blocks a router split elsewhere).
fn set_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    if let Some(name) = set_call_name_srec_tuple(func, arg_tys, result_ty) {
        return Some(name);
    }
    if let Some(name) = set_call_name_result_keyed(func, result_ty) {
        return Some(name);
    }
    if let Some(name) = set_call_name_map(func, arg_tys, result_ty) {
        return Some(name);
    }
    if let Some(name) = set_call_name_passthrough(func, arg_tys) {
        return Some(name);
    }
    set_call_name_arg_membership(func, arg_tys)
}

/// Extracted from `set_call_name` (codopsy7 complexity sweep, group 1 of 5): FLAT all-scalar
/// TUPLE elements (`Set[(Int, Int)]` — the C-015 compound-eq class): each element normalizes
/// through `__srec_key` (map_vkey.almd — the tuple block is physically the all-scalar record)
/// into a backing `_str` set, so dedup/lookup are CONTENT equality. Build/lookup fns only —
/// an iteration-returning fn (to_list) would surface the normalized strings and keeps the
/// wall below. Verbatim.
fn set_call_name_srec_tuple(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
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
    None
}

/// Extracted from `set_call_name` (codopsy7 complexity sweep, group 2 of 5): RESULT-keyed
/// constructors / Set-returning algebra over heap elements. The bare `set.{func}` fallback is
/// NOT a safe wall for a non-String heap element here — it links against the Int-typed
/// generic (set_core.almd), which silently "succeeds" via raw i64-slot / pointer-identity
/// comparison (the OLD C-015 bug) instead of refusing to link (CONFIRMED by probe:
/// `set.from_list` over a `List[(Int,Int)]` silently mis-deduped, len 3 instead of 2, then
/// trapped later). Route explicitly to the UNREGISTERED `_x` wall name instead. Verbatim.
fn set_call_name_result_keyed(func: &str, result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(
        func,
        "from_list" | "to_list" | "union" | "intersection" | "difference"
            | "new" | "insert" | "remove" | "symmetric_difference" | "filter"
    ) {
        return None;
    }
    let result_elem_is_heap = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && is_heap_ty(&a[0])
    );
    if !result_elem_is_heap {
        return None;
    }
    let result_elem_is_string = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::String)
    );
    Some(if result_elem_is_string {
        format!("set.{func}_str")
    } else {
        format!("set.{func}_x")
    })
}

/// Extracted from `set_call_name` (codopsy7 complexity sweep, group 3 of 5): TYPE-CHANGING
/// `set.map` to a HEAP element result (C-039's `set.map(si, (x) => "n" + …)`): the generic's
/// i64-result closure table type traps on the i32-result closure. A scalar subject → String
/// result composes through the insert_str twin; any other heap result walls (`_x`). (A scalar
/// RESULT over any subject keeps the generic — its dedup and closure table are i64-correct
/// there.) Verbatim.
fn set_call_name_map(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "map" {
        return None;
    }
    let result_elem_is_heap = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && is_heap_ty(&a[0])
    );
    if !result_elem_is_heap {
        return None;
    }
    let result_elem_is_string = matches!(
        result_ty,
        Ty::Applied(TypeConstructorId::Set | TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::String)
    );
    let subj_scalar = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && !is_heap_ty(&a[0]));
    Some(if result_elem_is_string && subj_scalar {
        "set.map_i2s".to_string()
    } else {
        "set.map_x".to_string()
    })
}

/// Extracted from `set_call_name` (codopsy7 complexity sweep, group 4 of 5): `all`/`any`/
/// `fold` are pure closure-passthrough (no internal eq compare or deep copy) — safe for any
/// heap element type, same as the list-module analogue above. Verbatim.
fn set_call_name_passthrough(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let arg0_elem_is_heap = matches!(
        arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
    );
    if matches!(func, "all" | "any" | "fold") && arg0_elem_is_heap {
        return Some(format!("set.{func}_str"));
    }
    None
}
