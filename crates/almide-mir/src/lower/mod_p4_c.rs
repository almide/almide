
/// Route `result.partition` / `result.collect_map` by REPR: the self-host impls
/// (result_collect.almd) pin the Ok payload to a SCALAR raw-copy slot and the Err
/// to a deep-copied String. Any other repr routes to the UNLINKED `_x` suffix — a
/// clean render wall, never a wrong-typed link (the random/fan `_x` discipline).
/// The mapped-list ELEMENT (collect_map's `xs`) is unconstrained: its slot is
/// forwarded to the closure untouched (borrowed, any repr).
/// Extracted from `result_call_name` (codopsy7 complexity sweep, pattern-2 uniform-arm
/// split): the VALUE combinators over a HEAP-Ok Result — same cap-as-tag misread as
/// is_ok/is_err, but the scalar impls also REBUILT the wrong layout: every `ok(x)` took the
/// Err path and the result printed as a swapped/zeroed value (the fuzz C-904 silent `ok("")`
/// class; unwrap_or_else even emitted invalid wasm — an i64-result CallFn bound to an i32
/// String local). The exact `Result[String, String]` instantiation routes to the `_h` twins
/// (result_map.almd); any other heap-Ok instantiation routes to the UNLINKED `_x` — a
/// deterministic render wall, never a wrong-typed link. `unwrap_or` needs no arm
/// (unwrap_or_call_name already keys on the repr), and the display/eq families are chosen by
/// type at the call site. Verbatim (only re-narrowed via `use .. as TC` already in scope).
fn result_call_name_heap_ok_input(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    // `flatten`'s BASE impl already reads the heap-Ok OUTER (tag@16) and the
    // len-as-tag scalar INNER — `Result[Result[scalar, String], String]` is
    // its exact shape (option_result_symmetry); only other inners wall.
    if func == "flatten" {
        let base_ok = matches!(arg_tys.first(), Some(Ty::Applied(TC::Result, a))
            if matches!(&a[0], Ty::Applied(TC::Result, i)
                if i.len() == 2 && !is_heap_ty(&i[0]) && matches!(i[1], Ty::String))
                && matches!(a[1], Ty::String));
        return Some(if base_ok {
            "result.flatten".to_string()
        } else {
            "result.flatten_x".to_string()
        });
    }
    let ss_in = matches!(arg_tys.first(), Some(Ty::Applied(TC::Result, a))
        if matches!(a[0], Ty::String) && matches!(a[1], Ty::String));
    let ss_res = matches!(result_ty, Ty::Applied(TC::Result, a)
        if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String));
    let has_h = match func {
        "map" | "map_err" | "flat_map" => ss_in && ss_res,
        // The `_h` twin is payload-type-INDEPENDENT (one handle slot per side:
        // the Ok returns with the share discipline, the Err borrows into f, the
        // closure ABI is (i32) -> i32 for any heap in/out), so ANY heap-Ok /
        // heap-Err instantiation admits — `Result[List[Float], List[String]]`
        // (uoe_heap_ok_share via result.collect) rides the String routine. A
        // SCALAR Err stays walled: its slot holds an i64 value, not a handle,
        // and the twin's load_handle would misread it.
        "unwrap_or_else" => {
            matches!(arg_tys.first(), Some(Ty::Applied(TC::Result, a))
                if a.len() == 2 && is_heap_ty(&a[1]))
                && is_heap_ty(result_ty)
        }
        // `to_option`'s `_h` twin is payload-type-INDEPENDENT (Ok → the
        // shared handle into some(), Err → none — no Err read at all), so
        // ANY heap-Ok instantiation admits (`result.to_option(v3)` over
        // `Result[Option[Float], String]` — C-149).
        "to_option" => true,
        // `filter_h` is likewise payload-type-INDEPENDENT (the Ok payload is
        // borrowed into pred and shared back; keep re-shares, fail builds the
        // String err) — ANY heap-Ok admits. The E-repr guard in
        // `result_call_name` runs BEFORE this arm, so arg 2 is String here.
        "filter" => true,
        "to_err_option" => ss_in,
        _ => false,
    };
    Some(if has_h { format!("result.{func}_h") } else { format!("result.{func}_x") })
}

/// Extracted from `result_call_name` (codopsy7 complexity sweep, pattern-2 uniform-arm
/// split): the same combinators with a SCALAR-Ok INPUT but a HEAP-Ok RESULT
/// (`result.map(r, (v) => some(v))` — `Result[Int, String]` → `Result[Option[Int], String]`,
/// fuzz seed-20260718 index 647): the scalar impl's i64-result closure table type mismatches
/// the heap-result closure AND its len-as-tag rebuild is the wrong OUTPUT layout. `map`
/// routes to the `_s2h` twin (C-151 — Value-erased heap payload, cap-as-tag build);
/// map_err/flat_map keep the UNLINKED `_x`. (Reached only when the input arm above did not
/// match.) Verbatim.
fn result_call_name_scalar_in_heap_out(func: &str, result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let err_is_string = matches!(result_ty, Ty::Applied(TC::Result, a)
        if a.len() == 2 && matches!(a[1], Ty::String));
    if func == "map" && err_is_string {
        return Some("result.map_s2h".to_string());
    }
    Some(format!("result.{func}_x"))
}

fn result_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match func {
        // is_ok/is_err over a HEAP-Ok Result: the block is cap-as-tag (tag @16, len@4
        // always 1) — the len-as-tag base impls would call every heap-Ok value an Err.
        "is_ok" | "is_err"
            if matches!(arg_tys.first(), Some(Ty::Applied(TC::Result, a))
                if a.len() == 2 && is_heap_ty(&a[0])) =>
        {
            Some(format!("result.{func}_h"))
        }
        // `filter` is the ONE result combinator that TAKES an E-typed value, so
        // it is the one whose E instantiation is pinned by an ARGUMENT rather
        // than by an annotation — measured: every other member either needs an
        // annotation to name E (and then routes fine) or cannot express a
        // non-String E at all. The registry body fixes `E = String`
        // (result_map.almd's `err_val: String`), so no other E repr may link
        // it: a SCALAR E handed an i64 to that i32 handle param and the module
        // failed wasm VALIDATION — `almide check` accepted, native built, and
        // the artifact was invalid (#1431, the acceptance-gap class). The E
        // test comes FIRST, before the heap-Ok rows below, because every twin
        // they route to is likewise a String-E form. `_x` is unlinked: an
        // honest render wall, the same refusal this router already uses for
        // every wrong-typed link.
        "filter" if !matches!(arg_tys.get(2), Some(Ty::String)) => {
            Some("result.filter_x".to_string())
        }
        // zip reads BOTH results' tags, and the scalar shim is len-as-tag on
        // EACH side — so a heap-Ok SECOND argument misreads exactly like a
        // heap-Ok first: `zip(ok(1.0), ok("abc"))` linked the shim through the
        // first-arg-only guard below and took the err path on ok inputs (fuzz
        // seed 500705518628 index 738, #1154 — a wall-escape, silent wrong
        // output). EITHER side heap-Ok routes to the UNLINKED `_x` — a
        // deterministic render wall, never a wrong-typed link.
        // The MIXED-tag instantiation the #1527 frontier row pins:
        // `zip(Result[Float, String], Result[String, String])`. The `_fs`
        // twin reads side a len-as-tag (f64 bits @12) and side b cap-as-tag
        // (@16, handle @12) — each side by its OWN rule, which is exactly
        // what the shared scalar shim cannot do. Checked BEFORE the generic
        // heap-Ok wall arm below.
        "zip" if matches!(
            (arg_tys.first(), arg_tys.get(1)),
            (
                Some(Ty::Applied(TC::Result, a)),
                Some(Ty::Applied(TC::Result, b)),
            ) if a.len() == 2 && b.len() == 2
                && matches!(a[0], Ty::Float) && matches!(a[1], Ty::String)
                && matches!(b[0], Ty::String) && matches!(b[1], Ty::String)
        ) =>
        {
            Some("result.zip_fs".to_string())
        }
        // Its consuming half: `unwrap_or_else` over the zip_fs result type,
        // Result[(Float, String), String] — the `_fs` twin shares the pair
        // out on Ok and feeds the deep-copied message to the fallback on
        // Err. Also BEFORE the generic heap-Ok arm (a tuple Ok is heap).
        "unwrap_or_else" if matches!(
            arg_tys.first(),
            Some(Ty::Applied(TC::Result, a)) if a.len() == 2
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::Float) && matches!(ts[1], Ty::String))
                && matches!(a[1], Ty::String)
        ) =>
        {
            Some("result.unwrap_or_else_fs".to_string())
        }
        "zip" if arg_tys.iter().take(2).any(|t| matches!(t, Ty::Applied(TC::Result, a)
                if a.len() == 2 && is_heap_ty(&a[0]))) =>
        {
            Some("result.zip_x".to_string())
        }
        // The VALUE combinators over a HEAP-Ok Result — same cap-as-tag misread as
        // is_ok/is_err, but the scalar impls also REBUILT the wrong layout: every
        // `ok(x)` took the Err path and the result printed as a swapped/zeroed value
        // (the fuzz C-904 silent `ok("")` class; unwrap_or_else even emitted invalid
        // wasm — an i64-result CallFn bound to an i32 String local). The exact
        // `Result[String, String]` instantiation routes to the `_h` twins
        // (result_map.almd); any other heap-Ok instantiation routes to the UNLINKED
        // `_x` — a deterministic render wall, never a wrong-typed link. `unwrap_or`
        // needs no arm (unwrap_or_call_name already keys on the repr), and the
        // display/eq families are chosen by type at the call site.
        "map" | "map_err" | "flat_map" | "unwrap_or_else" | "to_option"
        | "to_err_option" | "filter" | "or_else" | "flatten" | "to_list" | "zip"
            if matches!(arg_tys.first(), Some(Ty::Applied(TC::Result, a))
                if a.len() == 2 && is_heap_ty(&a[0])) =>
        {
            result_call_name_heap_ok_input(func, arg_tys, result_ty)
        }
        // The same combinators with a SCALAR-Ok INPUT but a HEAP-Ok RESULT
        // (`result.map(r, (v) => some(v))` — `Result[Int, String]` →
        // `Result[Option[Int], String]`, fuzz seed-20260718 index 647): the
        // scalar impl's i64-result closure table type mismatches the
        // heap-result closure AND its len-as-tag rebuild is the wrong OUTPUT
        // layout. `map` routes to the `_s2h` twin (C-151 — Value-erased heap
        // payload, cap-as-tag build); map_err/flat_map keep the UNLINKED `_x`.
        // (Reached only when the input arm above did not match.)
        "map" | "map_err" | "flat_map"
            if matches!(result_ty, Ty::Applied(TC::Result, a)
                if a.len() == 2 && is_heap_ty(&a[0])) =>
        {
            result_call_name_scalar_in_heap_out(func, result_ty)
        }
        _ => result_collection_call_name(func, arg_tys, result_ty),
    }
}

/// The Result COLLECTION combinators — `partition` and `collect_map`, both keyed
/// on a whole list-of-Result shape rather than a single Ok payload. Split out of
/// [`result_call_name`] so neither half outgrows a readable arm table.
fn result_collection_call_name(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match func {
        // partition: List[Result[scalar, String]] → (List[scalar], List[String])
        "partition" => {
            let ok = matches!(arg_tys.first(), Some(Ty::Applied(TC::List, e))
                if e.len() == 1
                    && matches!(&e[0], Ty::Applied(TC::Result, re)
                        if re.len() == 2 && !is_heap_ty(&re[0]) && matches!(re[1], Ty::String)));
            Some(if ok { "result.partition".to_string() } else { "result.partition_x".to_string() })
        }
        // collect_map: (List[T], (T) -> Result[U, E]) → Result[List[U], List[E]]
        // with U scalar, E String (keyed on the RESULT type — the closure's own
        // repr is heap-result `_h` by construction for every instantiation).
        "collect_map" => {
            let ok = matches!(result_ty, Ty::Applied(TC::Result, oe)
                if oe.len() == 2
                    && matches!(&oe[0], Ty::Applied(TC::List, u) if u.len() == 1 && !is_heap_ty(&u[0]))
                    && matches!(&oe[1], Ty::Applied(TC::List, e) if e.len() == 1 && matches!(e[0], Ty::String)));
            Some(if ok {
                "result.collect_map".to_string()
            } else {
                "result.collect_map_x".to_string()
            })
        }
        _ => None,
    }
}

/// `random.choice`/`random.shuffle` routing (extracted from the monolithic
/// dispatch — #781). Always names a target: the `_x` tail is the honest wall.
fn random_call_name(func: &str, arg_tys: &[Ty]) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `random.choice` / `random.shuffle` key on the LIST ELEMENT: a scalar element uses the
    // flat self-host (i64 slots, flat drops), a String element the rc-aware `_str` variant
    // (random_choice.almd / random_shuffle.almd). Any other element class routes to the
    // UNLINKED `random.<func>_x` → a clean render wall (never a wrong-typed link).
    if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
        if a.len() == 1 && !is_heap_ty(&a[0]) {
            return format!("random.{func}");
        }
        if a.len() == 1 && matches!(a[0], Ty::String) {
            return format!("random.{func}_str");
        }
        // A `(String, String)` element (#1169's omikuji table) — `choice` shares
        // the picked pair by handle (`random.choice_pair`); `shuffle` swaps raw
        // slots within the COW copy via `list.swap` (`random.shuffle_pair` —
        // ownership-invariant, so no pair get/set twins needed).
        if matches!(func, "choice" | "shuffle")
            && matches!(&a[..], [Ty::Tuple(ts)]
                if ts.len() == 2 && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::String))
        {
            return format!("random.{func}_pair");
        }
    }
    format!("random.{func}_x")
}

/// `fan.map` monomorphic-variant routing (see the block comment).
///
/// COMPLETENESS RULE (the fan mapper matrix, shared with `fan_any_call_name`):
/// every (A, B) pairing with A, B ∈ {Int, Float, String} has a self-host —
/// the full 3×3, gated by `spec/lang/fan_mapper_matrix_test.almd` (a missing
/// link walls the wasm leg, so the gate cannot rot silently). Intentional
/// omissions: Bool payloads (ride Int 0/1 until a dogfood need appears) and
/// heap element types (the funcref-callback rails are scalar/String — a wall,
/// not a wrong value). `fan.settle(xs, f)` is NOT in this table by design:
/// it desugars to `list.map` in frontend lowering and is generic for free.
fn fan_map_call_name(arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `fan.map` selects a monomorphic self-host by (input element A, output element B) — `fan.map<sfx>`
    // where the suffix spells (A, B) initials. The input A is `arg_tys[0] = List[A]`; the output B is
    // `result_ty = Result[List[B], String]`. An unsupported pairing routes to the UNLINKED
    // `fan.map_x` → a clean render wall (never a wrong-typed link = never invalid wasm).
    fn elem_of(t: Option<&Ty>) -> Option<&Ty> {
        match t {
            Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 => Some(&e[0]),
            _ => None,
        }
    }
    let a = elem_of(arg_tys.first());
    let b = match result_ty {
        Ty::Applied(TypeConstructorId::Result, r) if r.len() == 2 => elem_of(Some(&r[0])),
        _ => None,
    };
    let sfx = fan_pair_suffix(a, b);
    format!("fan.map{sfx}")
}

/// The shared (A, B) → suffix table of the fan mapper matrix. `_x` is the
/// deliberate unlinked-name wall for pairings outside the completeness rule.
fn fan_pair_suffix(a: Option<&Ty>, b: Option<&Ty>) -> &'static str {
    match (a, b) {
        (Some(Ty::Int), Some(Ty::Int)) => "",
        (Some(Ty::Int), Some(Ty::String)) => "_is",
        (Some(Ty::String), Some(Ty::String)) => "_ss",
        (Some(Ty::String), Some(Ty::Int)) => "_si",
        (Some(Ty::Int), Some(Ty::Float)) => "_if",
        (Some(Ty::Float), Some(Ty::Int)) => "_fi",
        (Some(Ty::Float), Some(Ty::Float)) => "_ff",
        (Some(Ty::Float), Some(Ty::String)) => "_fs",
        (Some(Ty::String), Some(Ty::Float)) => "_sf",
        _ => "_x",
    }
}

/// `fan.any_map` monomorphic-variant routing — the T2-3 mapper form. Same
/// (input element A, output element B) keying as `fan_map_call_name`, but the
/// output B comes from `result_ty = Result[B, String]` DIRECTLY (any returns
/// one winner, not a list). An unsupported pairing routes to the UNLINKED
/// `fan.any_map_x` → a clean render wall.
fn fan_any_call_name(arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    fn elem_of(t: Option<&Ty>) -> Option<&Ty> {
        match t {
            Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 => Some(&e[0]),
            _ => None,
        }
    }
    let a = elem_of(arg_tys.first());
    let b = match result_ty {
        Ty::Applied(TypeConstructorId::Result, r) if r.len() == 2 => Some(&r[0]),
        _ => None,
    };
    // Same 3×3 completeness rule as `fan_map_call_name` — one shared table.
    let sfx = fan_pair_suffix(a, b);
    format!("fan.any_map{sfx}")
}

/// HEAP-accumulator `fold` routing for list/map/set (see the block comment). `fold` threads an
/// ACCUMULATOR (= the result type). A HEAP accumulator (e.g. a String built up across the
/// fold) needs the closure-result + accumulator to be an i32 handle, not the i64 the
/// scalar-accumulator fold variants hardcode — emitting an i32 there is invalid wasm. No
/// heap-accumulator fold variant is self-hosted yet, so route it to an UNREGISTERED name:
/// render walls it cleanly (a controlled reject) rather than emitting a repr-mismatched
/// module. (Soundness-preserving: a wall is never a miscompile.)
///
/// Pattern-1 name-router (codopsy7 complexity sweep): each group below is an independent
/// classification, called in the SAME order via early return — pure text-move, no logic
/// change. `src_is_list_str` is recomputed locally in each helper that needs it (pure,
/// read-only, cheap to duplicate).
fn heap_fold_call_name(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> String {
    if let Some(name) = heap_fold_call_name_ols(module, arg_tys, result_ty) {
        return name;
    }
    if let Some(name) = heap_fold_call_name_msi(module, arg_tys, result_ty) {
        return name;
    }
    if let Some(name) = heap_fold_call_name_map_str_acc(module, arg_tys, result_ty) {
        return name;
    }
    if let Some(name) = heap_fold_call_name_hrec(module, arg_tys, result_ty) {
        return name;
    }
    if let Some(name) = heap_fold_call_name_hsca(module, arg_tys, result_ty) {
        return name;
    }
    format!("{module}.fold_hacc")
}

/// Extracted from `heap_fold_call_name` (codopsy7 complexity sweep, group 1 of 5): the ONE
/// self-hosted heap-accumulator variant: `Option[List[String]]` acc over `List[String]`
/// elements (is_balanced's paren-stack fold) — `list.fold_ols`, the (heap, heap) -> heap
/// 2-arity CallIndirect (`list_reduce_str`'s proven closure shape). Every other heap-acc fold
/// keeps the unregistered `fold_hacc` wall. Verbatim.
fn heap_fold_call_name_ols(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_ols_acc = matches!(result_ty,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1
            && matches!(&a[0], Ty::Applied(TypeConstructorId::List, b)
                if b.len() == 1 && matches!(b[0], Ty::String)));
    let src_is_list_str = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && matches!(e[0], Ty::String));
    if module == "list" && is_ols_acc && src_is_list_str {
        return Some("list.fold_ols".to_string());
    }
    // Any OTHER heap accumulator over String elements takes the type-ERASED
    // sibling: the loop moves the accumulator handle into `f` and takes the
    // result back without inspecting it, so one variant serves every heap acc.
    // `Set[String]` shares it — a Set[String] IS a DynListStr block of unique,
    // insertion-ordered Strings, the same layout `List[String]` has.
    let src_is_set_str = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::Set, e)) if e.len() == 1 && matches!(e[0], Ty::String));
    if is_heap_ty(result_ty) {
        if module == "list" && src_is_list_str {
            return Some("list.fold_str_hacc".to_string());
        }
        if module == "set" && src_is_set_str {
            return Some("set.fold_str_hacc".to_string());
        }
    }
    None
}

/// Extracted from `heap_fold_call_name` (codopsy7 complexity sweep, group 2 of 5): the
/// `Map[String, Int]`-accumulator map.fold pair (the map_fold_heap_acc corpus shape): keyed
/// on the EXACT acc type + subject family — `Map[String, String]` (fold_str_msi) /
/// `Map[String, Int]` (fold_skv_msi). Other heap accs keep the wall. Verbatim.
fn heap_fold_call_name_msi(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let is_msi_acc = matches!(result_ty,
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2
            && matches!(a[0], Ty::String) && matches!(a[1], Ty::Int));
    if !is_msi_acc {
        return None;
    }
    // `list.fold` over a Map[String, Int] acc with String elements — the frequencies
    // shape (map_fold_hacc.almd's list_fold_str_msi; same closure ABI as fold_ols).
    let src_is_list_str = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && matches!(e[0], Ty::String));
    if module == "list" && src_is_list_str {
        return Some("list.fold_str_msi".to_string());
    }
    if module == "map" {
        if let Some(Ty::Applied(TypeConstructorId::Map, s)) = arg_tys.first() {
            if s.len() == 2 && matches!(s[0], Ty::String) {
                if matches!(s[1], Ty::String) {
                    return Some("map.fold_str_msi".to_string());
                }
                if matches!(s[1], Ty::Int) {
                    return Some("map.fold_skv_msi".to_string());
                }
            }
        }
    }
    None
}

/// Extracted from `heap_fold_call_name` (codopsy7 complexity sweep, group 3 of 5): a STRING
/// accumulator over a `Map[String, String]` subject (map_fold.almd's `map.fold(ms, "[", (acc,
/// k, v) => acc + k + "=" + v)` builder): all sides are handles, so the all-i32 3-arity
/// CallIndirect twin runs it faithfully — the same interleaved-pair walk as fold_str_msi with
/// a String acc. Verbatim.
fn heap_fold_call_name_map_str_acc(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // ANY heap accumulator, not just String: a heap accumulator is one i32
    // handle whatever block it points at, and the walk below never inspects it —
    // it only moves it into `f` and takes the result back. Restricting this to
    // `Ty::String` walled the `map.fold(m, [], (acc, k, v) => acc + [k])`
    // keys-collect shape for no reason the layout requires.
    if !(module == "map" && is_heap_ty(result_ty)) {
        return None;
    }
    if let Some(Ty::Applied(TypeConstructorId::Map, s)) = arg_tys.first() {
        if s.len() == 2 && matches!(s[0], Ty::String) {
            // The two String-keyed subject layouts need different walks: the
            // all-String map interleaves 16-byte pairs, the skv map splits keys
            // and values into two runs of 8-byte slots. A heap accumulator is
            // one i32 handle either way, so each layout needs exactly one
            // variant regardless of what the accumulator holds.
            return match s[1] {
                Ty::String => Some("map.fold_str_sacc".to_string()),
                _ if !is_heap_ty(&s[1]) => Some("map.fold_skv_hacc".to_string()),
                _ => None,
            };
        }
    }
    None
}

/// Extracted from `heap_fold_call_name` (codopsy7 complexity sweep, group 4 of 5): a
/// RECORD/VARIANT accumulator over a `List[record/variant]` (`list.fold(counters,
/// Counter.empty(), (acc, c) => Counter.merge(acc, c))` — the protocol-merge shape, and the
/// cross-module `Loc` pick fold): every side is a uniform heap HANDLE, so the TYPE-ERASED
/// `list.fold_hrec` (the fold_ols closure ABI) runs it faithfully — acc MOVES into f each
/// step, elements are borrowed, the final acc is the owned result. A Named alias to a scalar
/// fails `is_heap_ty` and keeps the wall. Verbatim.
fn heap_fold_call_name_hrec(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // ANY heap element with ANY heap accumulator (#883). `list_fold_hrec` is
    // fully TYPE-ERASED — both sides are uniform handles, the element is read
    // with `prim.load_handle` and the acc moves through the closure — so the
    // Named/Record spelling of its gate was narrower than its implementation.
    // A `(Value, Value)` tuple element with a `Value` acc (the lisp
    // `list.zip(names, vals) |> list.fold(env, …)` shape) fell past it to the
    // unregistered catch-all `list.fold_hacc` and walled. SCALAR elements stay
    // out: their i64 slots are values, not handles, and reading one as a
    // handle would be a wrong pointer (Int/Bool have their own `_hsca` twin
    // below; Float keeps walling, since an f64 closure param would ABI-mismatch).
    let elem_is_heap = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && is_heap_ty(&e[0]));
    if module == "list" && elem_is_heap && is_heap_ty(result_ty) {
        return Some("list.fold_hrec".to_string());
    }
    None
}

/// Extracted from `heap_fold_call_name` (codopsy7 complexity sweep, group 5 of 5): a heap
/// accumulator over SCALAR Int/Bool elements (`list.fold([true, false], tmp, (s, v) =>
/// ok(…))` with a Result acc — result_uoe_float): the elements are plain i64 slot copies, the
/// acc a handle, so the (heap, scalar) -> heap 2-arity CallIndirect twin runs it faithfully.
/// Float elements stay walled (an f64 closure param would ABI-mismatch the i64 slot load).
/// Verbatim.
fn heap_fold_call_name_hsca(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let elem_is_int_bool = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && matches!(e[0], Ty::Int | Ty::Bool));
    if module == "list" && elem_is_int_bool && is_heap_ty(result_ty) {
        return Some("list.fold_hsca".to_string());
    }
    None
}

/// Repr-poly `result.unwrap_or` / `option.unwrap_or` routing (see the block
/// comment). Caller guarantees `func == "unwrap_or"`.
fn unwrap_or_call_name(module: &str, arg_tys: &[Ty]) -> Option<String> {
    // Pattern-1 name-router (codopsy7 complexity sweep): the `result`/`option` branches are
    // mutually exclusive (keyed on `module`) and independent — a pure text-move split, called
    // in the SAME order via early return. No logic change.
    if module == "result" {
        return unwrap_or_call_name_result(arg_tys);
    }
    if module == "option" {
        return unwrap_or_call_name_option(arg_tys);
    }
    None
}

/// Extracted from `unwrap_or_call_name` (codopsy7 complexity sweep): the same repr-poly
/// routing for `result.unwrap_or` (the pipe/direct form — `result.unwrap_or(json.parse(s),
/// json.null())`, json_gltf_walk): the generic impl takes an i64 scalar default; a heap
/// Ok/default needs the rc-correct self-hosts registered for the `??` desugar. Verbatim.
/// A FLAT scalar block Ok payload — a scalar TUPLE (`result.zip`'s `(Int, Int)`),
/// a `List[<scalar>]`, `Bytes`, or an `Option[<scalar>]` (the C-149 nested-share
/// chain: len-as-tag plus one scalar slot, flat rc_dec). These route to the
/// rc-correct flat variant over the cap-as-tag layout (tag @16).
fn is_flat_scalar_ok_payload(ok: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    matches!(ok, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
        || matches!(ok, Ty::Applied(TC::List | TC::Option, e)
            if e.len() == 1 && !is_heap_ty(&e[0]))
        || matches!(ok, Ty::Bytes)
}

fn unwrap_or_call_name_result(arg_tys: &[Ty]) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    let Some(Ty::Applied(TypeConstructorId::Result, a)) = arg_tys.first() else { return None };
    if a.len() == 2 && is_value_ty(&a[0]) {
        return Some("result.value_unwrap_or".to_string());
    }
    if a.len() == 2
        && matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
            if e.len() == 1 && is_value_ty(&e[0]))
    {
        return Some("result.list_value_unwrap_or".to_string());
    }
    if a.len() == 2 && matches!(a[0], Ty::String) {
        return Some("result.str_unwrap_or".to_string());
    }
    if a.len() == 2 && is_flat_scalar_ok_payload(&a[0]) {
        return Some("result.flat_unwrap_or".to_string());
    }
    // Any OTHER heap Ok payload has no registered rc-correct variant: the
    // generic impl takes an i64 SCALAR default, so a handle payload/default
    // repr-mismatches (invalid wasm). Route to an unregistered `_x` name —
    // the caller WALLS honestly.
    if a.len() == 2 && is_heap_ty(&a[0]) {
        return Some("result.unwrap_or_hx".to_string());
    }
    None
}
