
/// Extracted from `unwrap_or_call_name` (codopsy7 complexity sweep): `option.unwrap_or(o, d)`
/// over an `Option[String]` (the pipe/UFCS form `list.get(xs, i) |> option.unwrap_or("")`,
/// NOT the `??` operator that `try_lower_option_unwrap_or` desugars) must route to
/// `option.unwrap_or_str`: the generic `option.unwrap_or` takes its default as an i64 SCALAR
/// (`Option[Int]`), so a String fallback (an i32 handle) and a String result repr-mismatch it
/// — invalid wasm (`expected i64, found i32` in the call + the i64 result).
/// `option.unwrap_or_str` (param i32 i32) (result i32) is the rc-correct String variant
/// (deep-copies the kept payload so result + source can both drop). Keyed on the Option
/// payload being String. Verbatim.
fn unwrap_or_call_name_option(arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let Some(Ty::Applied(TypeConstructorId::Option, a)) = arg_tys.first() else { return None };
    if a.len() == 1 && matches!(a[0], Ty::String) {
        return Some("option.unwrap_or_str".to_string());
    }
    // The remaining heap payloads route to their rc-correct self-hosts
    // (already registered for the `??` desugar): Value / List[Value] /
    // List[String]. The generic unwrap_or takes an i64 scalar default,
    // so a handle default (`json.get_array(v,k) |> option.unwrap_or([])`,
    // json_gltf_walk's count_floats) repr-mismatched — invalid wasm.
    if a.len() == 1 && is_value_ty(&a[0]) {
        return Some("option.value_unwrap_or".to_string());
    }
    if a.len() == 1
        && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && is_value_ty(&e[0]))
    {
        return Some("option.listvalue_unwrap_or".to_string());
    }
    if a.len() == 1
        && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && matches!(e[0], Ty::String))
    {
        return Some("option.liststr_unwrap_or".to_string());
    }
    // A FLAT scalar-element list payload (`map.get(groups, "0") ?? []` —
    // Option[List[Int]], the group_by class): the rc-correct flat variant.
    // A scalar TUPLE or Bytes payload is the SAME flat block shape (uniform
    // slots / raw bytes @12, flat rc_dec drop) — bytes.chunks' element class.
    if a.len() == 1
        && (matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                if e.len() == 1 && !is_heap_ty(&e[0]))
            || matches!(&a[0], Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
            // A NESTED `Option[<scalar>]` payload (`option.unwrap_or(
            // result.to_option(v3), none)` — C-149) is the SAME flat block
            // (len-as-tag + one scalar slot, flat rc_dec).
            || matches!(&a[0], Ty::Applied(TypeConstructorId::Option, e)
                if e.len() == 1 && !is_heap_ty(&e[0]))
            || matches!(a[0], Ty::Bytes))
    {
        return Some("option.listint_unwrap_or".to_string());
    }
    // Any OTHER heap payload: no registered rc-correct variant — wall honestly
    // (the generic impl's i64 scalar default repr-mismatches a handle).
    if a.len() == 1 && is_heap_ty(&a[0]) {
        return Some("option.unwrap_or_hx".to_string());
    }
    None
}

/// Every `list.*` typed-variant route (extracted verbatim, order preserved;
/// `None` = no typed variant applies → the caller falls to the plain name).
fn list_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Split (2026-07-20, #781 cog>100 burn-down) into per-theme routers, called in the
    // SAME ORDER the original single if-chain evaluated them (order is load-bearing: e.g.
    // `enumerate` has two historical arms and the FIRST one — source_keyed — is exhaustive
    // for any single-type-arg List, making transform's `enumerate` arm dead by construction,
    // exactly as in the original unified function). Pure text move, no logic change.
    list_call_name_hof_combinators(func, arg_tys, result_ty)
        .or_else(|| list_call_name_source_keyed(func, arg_tys, result_ty))
        .or_else(|| list_call_name_ordering(func, arg_tys, result_ty))
        .or_else(|| list_call_name_transform(func, arg_tys, result_ty))
        .or_else(|| list_call_name_modifiers(func, arg_tys, result_ty))
        .or_else(|| list_call_name_accessors(func, arg_tys, result_ty))
}

fn list_call_name_hof_combinators(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1 name-router (codopsy8 complexity sweep): the 5 groups below are
    // independent, self-contained classifications (one per `func` name) — a pure
    // text-move split, called in the SAME order via early return. No logic change.
    list_call_name_shuffle_choice(func, arg_tys)
        .or_else(|| list_call_name_group_by(func, arg_tys, result_ty))
        .or_else(|| list_call_name_zip_with(func, arg_tys, result_ty))
        .or_else(|| list_call_name_scan(func, arg_tys))
        .or_else(|| list_call_name_unique_by(func, arg_tys))
}

/// Extracted from `list_call_name_hof_combinators` (codopsy8 complexity sweep, group 1 of
/// 5): `list.shuffle` / `list.choice` ARE `random.shuffle`/`random.choice` under a second
/// stdlib name (the same almide_rt intrinsics) — delegate to the random element-repr
/// router. The Entropy capability stays honest: the witness derives from the LINKED
/// self-host body (prim.random_get), not the call-site module name. Verbatim.
fn list_call_name_shuffle_choice(func: &str, arg_tys: &[Ty]) -> Option<String> {
    if matches!(func, "shuffle" | "choice") {
        return Some(random_call_name(func, arg_tys));
    }
    None
}

/// Extracted from `list_call_name_hof_combinators` (codopsy8 complexity sweep, group 2 of
/// 5): `list.group_by` — the hval-map builder (scalar elements, String keys). Any other
/// repr routes to the UNLINKED `_x` (a clean render wall, never a wrong-typed link). Verbatim.
fn list_call_name_group_by(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "group_by" {
        return None;
    }
    let ok = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
            if e.len() == 1 && !is_heap_ty(&e[0]))
        && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2
                && matches!(a[0], Ty::String)
                && matches!(&a[1], Ty::Applied(TypeConstructorId::List, b)
                    if b.len() == 1 && !is_heap_ty(&b[0])));
    Some(if ok { "list.group_by".to_string() } else { "list.group_by_x".to_string() })
}

/// Extracted from `list_call_name_hof_combinators` (codopsy8 complexity sweep, group 3 of
/// 5): `list.zip_with` keys on the RESULT element (= the closure's result repr, the
/// only axis of the CallIndirect table type — params ride the widened i64 slots
/// uniformly): a SCALAR result element rides the base impl (heap SOURCE elements
/// are passed as borrowed handles, never copied into the flat result — sound);
/// a (String, String → String) triple routes to the `_str` twin (move-in fill);
/// any other heap result element routes to the UNLINKED `_x` — the scalar impl's
/// $closure_fn2 table type TRAPS on a heap-result closure ("indirect call type
/// mismatch", fuzz G-65) and its raw copies would alias handles un-owned. Verbatim.
fn list_call_name_zip_with(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    if func != "zip_with" {
        return None;
    }
    fn elem(t: Option<&Ty>) -> Option<&Ty> {
        use almide_lang::types::constructor::TypeConstructorId;
        match t {
            Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 => Some(&e[0]),
            _ => None,
        }
    }
    let a = elem(arg_tys.first());
    let b = elem(arg_tys.get(1));
    let c = elem(Some(result_ty));
    Some(match (a, b, c) {
        (Some(_), Some(_), Some(z)) if !is_heap_ty(z) => "list.zip_with".to_string(),
        (Some(Ty::String), Some(Ty::String), Some(Ty::String)) => "list.zip_with_str".to_string(),
        _ => "list.zip_with_x".to_string(),
    })
}

/// Extracted from `list_call_name_hof_combinators` (codopsy8 complexity sweep, group 4 of
/// 5): `list.scan` keys on (element, ACC) reprs — the ACC is the closure's result
/// and the OUTPUT element (one table-type axis, one layout axis): scalar/scalar
/// rides the base impl, scalar/String the `_str` twin (move-in fill, borrow-back
/// threading), anything else the UNLINKED `_x` wall — the scalar impl's i64 init
/// param failed validation on a String acc ("expected i64, found i32", the v1
/// edition of fuzz seed-20260718 index 259). Verbatim.
fn list_call_name_scan(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "scan" {
        return None;
    }
    let elem_scalar = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
        if e.len() == 1 && !is_heap_ty(&e[0]));
    Some(match (elem_scalar, arg_tys.get(1)) {
        (true, Some(a)) if !is_heap_ty(a) => "list.scan".to_string(),
        (true, Some(Ty::String)) => "list.scan_str".to_string(),
        _ => "list.scan_x".to_string(),
    })
}

/// Extracted from `list_call_name_hof_combinators` (codopsy8 complexity sweep, group 5 of
/// 5): `list.unique_by` keys on (element, KEY) reprs — the KEY is the closure's
/// result (the CallIndirect table-type axis): scalar/scalar rides the base
/// impl, scalar/String the `_sk` twin (content equality via string.eq), and
/// anything else the UNLINKED `_x` wall — the scalar `(Int) -> Int` table
/// type TRAPPED on a String-key closure ("indirect call type mismatch",
/// fuzz seed-20260718 index 9, the unique_by edition of the zip_with class). Verbatim.
fn list_call_name_unique_by(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "unique_by" {
        return None;
    }
    let elem_scalar = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
        if e.len() == 1 && !is_heap_ty(&e[0]));
    let key_ty = match arg_tys.get(1) {
        Some(Ty::Fn { ret, .. }) => Some(ret.as_ref()),
        _ => None,
    };
    Some(match (elem_scalar, key_ty) {
        (true, Some(k)) if !is_heap_ty(k) => "list.unique_by".to_string(),
        (true, Some(Ty::String)) => "list.unique_by_sk".to_string(),
        _ => "list.unique_by_x".to_string(),
    })
}

fn list_call_name_source_keyed(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1 name-router (codopsy8 complexity sweep): the 6 groups below are
    // independent, self-contained classifications (one per `func` name), called in the
    // SAME order via early return — a pure text-move split, no logic change.
    list_call_name_drop_end(func, arg_tys)
        .or_else(|| list_call_name_enumerate_source(func, arg_tys))
        .or_else(|| list_call_name_pop(func, arg_tys))
        .or_else(|| list_call_name_zip(func, arg_tys))
        .or_else(|| list_call_name_repeat(func, arg_tys))
        .or_else(|| list_call_name_flatten(func, result_ty))
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 1 of 6):
/// `list.drop_end` keys on its SOURCE element: a String element routes to the CO-OWNED
/// rc-copy variant (`__copy_slots_rc`) — the raw slot copy aliases heap handles un-owned,
/// which double-frees under the nested Option[List[String]] drop (is_balanced's fold). Verbatim.
fn list_call_name_drop_end(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "drop_end" {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
        if a.len() == 1 && matches!(a[0], Ty::String) {
            return Some("list.drop_end_str".to_string());
        }
    }
    None
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 2 of 6):
/// `list.enumerate` keys on its SOURCE element: scalar → the flat-pair self-host;
/// String → the rc-share pair variant (`DropListIntStr` at the call site frees each
/// pair's key ref); any other heap element routes to an UNREGISTERED name (walls
/// cleanly — a flat pair drop would leak a rich element's children). Verbatim.
fn list_call_name_enumerate_source(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "enumerate" {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
        if a.len() == 1 {
            if !is_heap_ty(&a[0]) {
                return Some("list.enumerate".to_string());
            }
            if matches!(a[0], Ty::String) {
                return Some("list.enumerate_str".to_string());
            }
            return Some("list.enumerate_h".to_string());
        }
    }
    None
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 3 of 6):
/// `list.pop` keys on its SOURCE element: a SCALAR (8-byte-slot) element rides the
/// registered in-place self-host (`list_pop.almd` — Int/Bool/Float move bit-exactly);
/// a HEAP element routes to the UNREGISTERED `_x` name and walls at render (popping
/// an owned handle is an ownership transfer the flat impl cannot express). Verbatim.
fn list_call_name_pop(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "pop" {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
        if a.len() == 1 && !is_heap_ty(&a[0]) {
            return Some("list.pop".to_string());
        }
    }
    Some("list.pop_x".to_string())
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 4 of 6):
/// `list.zip` keys on BOTH sources: scalar/scalar → flat pairs; FLAT-heap/FLAT-heap
/// (String or List[scalar] each side — matrix rows) → the rc-share variant (the
/// call-site `DropListStrStr` releases both acquired refs); anything else walls. Verbatim.
fn list_call_name_zip(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "zip" || arg_tys.len() != 2 {
        return None;
    }
    let elem = |t: &Ty| match t {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(a[0].clone()),
        _ => None,
    };
    let (Some(ea), Some(eb)) = (elem(&arg_tys[0]), elem(&arg_tys[1])) else { return None };
    let flat_heap = |t: &Ty| matches!(t, Ty::String)
        || matches!(t, Ty::Applied(TypeConstructorId::List, b)
            if b.len() == 1 && !is_heap_ty(&b[0]));
    if !is_heap_ty(&ea) && !is_heap_ty(&eb) {
        return Some("list.zip".to_string());
    }
    if flat_heap(&ea) && flat_heap(&eb) {
        return Some("list.zip_rc".to_string());
    }
    // MIXED scalar/flat-heap: co-own only the heap side (`_sh`/`_hs`).
    if !is_heap_ty(&ea) && flat_heap(&eb) {
        return Some("list.zip_sh".to_string());
    }
    if flat_heap(&ea) && !is_heap_ty(&eb) {
        return Some("list.zip_hs".to_string());
    }
    Some("list.zip_h".to_string())
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 5 of 6):
/// `list.repeat` over a HEAP element (`list.repeat(h, n_rep)` where `h: Matrix` — the
/// nn repeat_kv GQA duplication): each result slot must CO-OWN the element (rc_inc per
/// copy) — the scalar impl's raw alias would make every slot an uncounted owner the
/// recursive result drop double-frees. Verbatim.
fn list_call_name_repeat(func: &str, arg_tys: &[Ty]) -> Option<String> {
    if func != "repeat" {
        return None;
    }
    if let Some(t) = arg_tys.first() {
        if is_heap_ty(t) {
            return Some("list.repeat_rc".to_string());
        }
    }
    None
}

/// Extracted from `list_call_name_source_keyed` (codopsy8 complexity sweep, group 6 of 6):
/// `list.flatten` over HEAP-element sublists: the copied slots are handles the
/// result must CO-OWN — route to the rc_inc-on-copy variant (the scalar variant's
/// raw copy would make the result a second uncounted owner = a double free). Verbatim.
fn list_call_name_flatten(func: &str, result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "flatten" {
        return None;
    }
    if let Ty::Applied(TypeConstructorId::List, inner) = result_ty {
        if inner.len() == 1 && is_heap_ty(&inner[0]) {
            return Some("list.flatten_rc".to_string());
        }
    }
    None
}

fn list_call_name_ordering(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    let _ = result_ty; // unused (kept for the shared `.or_else()` router signature)
    if matches!(func, "sort" | "min" | "max") {
        if let Some(r) = list_call_name_sort_min_max(func, arg_tys) {
            return Some(r);
        }
    }
    if func == "sort_by" {
        if let Some(r) = list_call_name_sort_by(arg_tys) {
            return Some(r);
        }
    }
    None
}

/// The `sort`/`min`/`max` arm of [`list_call_name_ordering`] — verbatim move.
fn list_call_name_sort_min_max(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // List[Float] ordering uses IEEE-754 totalOrder (f64::total_cmp), NOT a signed-int slot
    // compare. Float is SCALAR (is_heap_ty false), so the heap routes below never fire for it —
    // route sort/min/max explicitly on the element being Ty::Float (C-055). sort_by keys on the
    // CLOSURE (arg 1) RETURN type being Float — the element list may be any type (e.g. List[R]).
    if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
        if a.len() == 1 && a[0] == Ty::Float {
            return Some(format!("list.{func}_float"));
        }
        // list.sort/min/max over a List[String] compare by CONTENT, not the i64 handle the
        // generic impls compare (→ arbitrary/handle order or wrong element, a silent bug).
        // _str variants do a lexicographic byte compare.
        if a.len() == 1 && matches!(a[0], Ty::String) {
            return Some(format!("list.{func}_str"));
        }
        // COMPOUND elements (the C-053 lattice) — type-directed comparators
        // (list_ord_compound.almd): scalar/String 2-tuples, List[Int/Bool],
        // List[String], Option[scalar]. Float components are EXCLUDED (IEEE
        // totalOrder ≠ an i64 slot compare). Any OTHER heap element walls (`_x`):
        // the generic twin would compare handle addresses AND raw-copy without
        // rc_inc — a silent wrong order plus a double-free, never acceptable.
        if a.len() == 1 && is_heap_ty(&a[0]) {
            let suffix = list_call_name_sort_min_max_heap_suffix(&a[0]);
            return Some(match suffix {
                // sort_lstr has no twin yet — only min/max are implemented for
                // the List[String] element family; sort keeps the honest wall.
                Some("lstr") if func == "sort" => "list.sort_x".to_string(),
                Some(s) => format!("list.{func}_{s}"),
                None => format!("list.{func}_x"),
            });
        }
    }
    None
}

/// Extracted from `list_call_name_sort_min_max` (codopsy8 complexity sweep): the HEAP-element
/// suffix classification (the C-053 compound lattice: scalar/String 2-tuples, List[Int/Bool],
/// List[String], Option[scalar]) — a pure sub-match, no logic change. Verbatim.
fn list_call_name_sort_min_max_heap_suffix(elem: &Ty) -> Option<&'static str> {
    use almide_lang::types::constructor::TypeConstructorId;
    let scalar_nf = |t: &Ty| matches!(t, Ty::Int | Ty::Bool);
    match elem {
        Ty::Tuple(ts) if ts.len() == 2 && scalar_nf(&ts[0]) && scalar_nf(&ts[1]) => Some("tss"),
        Ty::Tuple(ts) if ts.len() == 2 && scalar_nf(&ts[0]) && matches!(ts[1], Ty::String) => {
            Some("tsstr")
        }
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && scalar_nf(&e[0]) => Some("lint"),
        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && matches!(e[0], Ty::String) => {
            Some("lstr")
        }
        Ty::Applied(TypeConstructorId::Option, o) if o.len() == 1 && scalar_nf(&o[0]) => {
            Some("oint")
        }
        _ => None,
    }
}

/// The `sort_by` arm of [`list_call_name_ordering`] — verbatim move.
fn list_call_name_sort_by(arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if let Some(Ty::Fn { ret, .. }) = arg_tys.get(1) {
        // A HEAP element (List[String]/List[R]) must be CO-OWNED by the
        // result list (rc_inc per copied handle) — the raw-copy variants
        // share without acquiring and the two recursive drops double-free.
        let heap_elem = matches!(arg_tys.first(),
            Some(Ty::Applied(TypeConstructorId::List, a)) if a.len() == 1 && is_heap_ty(&a[0]));
        if **ret == Ty::Float {
            return Some(if heap_elem {
                "list.sort_by_float_rc".to_string()
            } else {
                "list.sort_by_float".to_string()
            });
        }
        // A STRING-key sort_by over SCALAR elements routes to the cached-key
        // stable twin (byte-lexicographic via string.cmp — #560/C-055). A HEAP
        // element still walls (`_x`): the copied handles would need the rc_inc
        // co-own leg the scalar twin doesn't carry.
        if **ret == Ty::String {
            if !heap_elem {
                return Some("list.sort_by_str_key".to_string());
            }
            return Some("list.sort_by_str_key_x".to_string());
        }
        if heap_elem {
            return Some("list.sort_by_rc".to_string());
        }
    }
    None
}

fn list_call_name_transform(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    // Pattern-1 name-router (codopsy8 complexity sweep): the 3 groups below are independent,
    // self-contained classifications, called in the SAME order via early return — a pure
    // text-move split, no logic change. Order is load-bearing between the two combinator
    // groups (a String source routes to `_str` BEFORE the general heap-element wall check).
    list_call_name_map(func, arg_tys, result_ty)
        .or_else(|| list_call_name_enumerate_str_source(func, arg_tys))
        .or_else(|| list_call_name_str_elem_combinators(func, arg_tys))
        .or_else(|| list_call_name_heap_elem_combinators(func, arg_tys))
}

/// Extracted from `list_call_name_transform` (codopsy8 complexity sweep, group 1 of 3):
/// `list.map` is the one combinator whose SOURCE and RESULT element reprs may DIFFER (the
/// closure transforms the type). A heap RESULT over a SCALAR source (`float.to_string` over a
/// List[Float], `int.to_string` over a List[Int]) must read the source slot as a raw i64
/// scalar (load64), not as a String handle (load_str) — that is `map_s2h`; a heap result over
/// a heap source is the all-String `map_str`. Verbatim.
fn list_call_name_map(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "map" {
        return None;
    }
    if let Ty::Applied(TypeConstructorId::List, rargs) = result_ty {
        if rargs.len() == 1 && is_heap_ty(&rargs[0]) {
            let src_heap = matches!(
                arg_tys.first(),
                Some(Ty::Applied(TypeConstructorId::List, s)) if s.len() == 1 && is_heap_ty(&s[0])
            );
            return Some(if src_heap { "list.map_str".to_string() } else { "list.map_s2h".to_string() });
        }
    }
    None
}

/// Extracted from `list_call_name_transform` (codopsy8 complexity sweep, group 2 of 3):
/// `list.enumerate` over a List[String] → `list.enumerate_str` (result List[(Int, String)]).
/// Keyed on the SOURCE arg being List[String] (the yaml `lines |> list.enumerate` shape). This
/// arm is DEAD BY CONSTRUCTION (the `list_call_name_enumerate_source` group tried before
/// `list_call_name_transform` in the top-level `list_call_name` router is exhaustive for any
/// single-type-arg List, so `enumerate` never reaches here) — kept verbatim per the original
/// #781 note rather than dropped, to preserve the historical text-move fidelity. Verbatim.
fn list_call_name_enumerate_str_source(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if func != "enumerate" {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
        if s.len() == 1 && matches!(s[0], Ty::String) {
            return Some("list.enumerate_str".to_string());
        }
    }
    None
}

/// Extracted from `list_call_name_transform` (codopsy8 complexity sweep, group 3a of 3, the
/// String-element half): chunk/windows/window (build a NESTED List[List[heap]] whose recursive
/// drop is a separate gap) and the HIGHER-ORDER take_while/drop_while/reduce over a HEAP
/// element (String/Value): the generic i64 self-host impls copy element handles WITHOUT
/// rc_inc → a DOUBLE-FREE at scope end (the result and source both free the shared handle),
/// and the HO closure ABI is i64-scalar so a String/Value i32 handle mismatches the indirect
/// call. take_while/drop_while/reduce over a List[String] now have rc-correct `_str` variants
/// (each kept String DEEP-COPIED so result + source can both drop without a double-free);
/// route to them BEFORE the general heap-element wall
/// ([`list_call_name_heap_elem_combinators`]). Verbatim.
fn list_call_name_str_elem_combinators(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "take_while" | "drop_while" | "chunk" | "windows" | "window" | "reduce") {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
        if s.len() == 1 && matches!(s[0], Ty::String) {
            return Some(format!("list.{func}_str"));
        }
    }
    None
}

/// Extracted from `list_call_name_transform` (codopsy8 complexity sweep, group 3b of 3, the
/// general heap-element fallback): any OTHER heap element (List[Value], nested List[List[..]],
/// etc.) for the same func set as [`list_call_name_str_elem_combinators`] — both are memory-
/// safety bugs the prim-region ownership cert cannot see (it treats prim rc as a no-op), so
/// they slipped past corpus-wall and trapped only at runtime. Route to an UNREGISTERED name →
/// render walls cleanly (a controlled reject, never a miscompile or double-free) until the
/// rc-correct heap variants land. Scalar element lists (Int/Float/Bool) are unaffected. Verbatim.
fn list_call_name_heap_elem_combinators(func: &str, arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    if !matches!(func, "chunk" | "windows" | "window" | "take_while" | "drop_while" | "reduce") {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
        if s.len() == 1 && is_heap_ty(&s[0]) {
            return Some(format!("list.{func}_heapelem"));
        }
    }
    None
}
