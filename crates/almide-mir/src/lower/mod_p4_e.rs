
/// Extracted from `set_call_name` (codopsy7 complexity sweep, group 5 of 5): ARG-keyed
/// eq/membership: a Bool-returning fn over a `Set[heap]` subject (arg 0) — String element
/// only (the `__str_eq`/`__set_has_str` unsoundness above); any other heap element routes to
/// the `_x` wall (the same fallthrough danger as the RESULT-keyed family). Verbatim.
fn set_call_name_arg_membership(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let arg0_elem_is_heap = matches!(
        arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && is_heap_ty(&a[0])
    );
    if !(matches!(func, "contains" | "is_subset" | "is_disjoint" | "eq") && arg0_elem_is_heap) {
        return None;
    }
    let arg0_elem_is_string = matches!(
        arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, a)) if a.len() == 1 && matches!(a[0], Ty::String)
    );
    Some(if arg0_elem_is_string {
        format!("set.{func}_str")
    } else {
        format!("set.{func}_x")
    })
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


