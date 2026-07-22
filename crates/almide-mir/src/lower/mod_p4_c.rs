
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
    }
    format!("random.{func}_x")
}

/// `fan.map` monomorphic-variant routing (see the block comment).
fn fan_map_call_name(arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `fan.map` selects a monomorphic self-host by (input element A, output element B) — `fan.map<sfx>`
    // where A/B in {Int (""), String ("s")}. The input A is `arg_tys[0] = List[A]`; the output B is
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
    let sfx = match (a, b) {
        (Some(Ty::Int), Some(Ty::Int)) => "",
        (Some(Ty::Int), Some(Ty::String)) => "_is",
        (Some(Ty::String), Some(Ty::String)) => "_ss",
        (Some(Ty::String), Some(Ty::Int)) => "_si",
        _ => "_x",
    };
    format!("fan.map{sfx}")
}

/// HEAP-accumulator `fold` routing for list/map/set (see the block comment).
fn heap_fold_call_name(module: &str, arg_tys: &[Ty], result_ty: &Ty) -> String {
    use almide_lang::types::constructor::TypeConstructorId;
    // `fold` threads an ACCUMULATOR (= the result type). A HEAP accumulator (e.g. a String built up
    // across the fold) needs the closure-result + accumulator to be an i32 handle, not the i64 the
    // scalar-accumulator fold variants hardcode — emitting an i32 there is invalid wasm. No heap-
    // accumulator fold variant is self-hosted yet, so route it to an UNREGISTERED name: render walls
    // it cleanly (a controlled reject) rather than emitting a repr-mismatched module. (Soundness-
    // preserving: a wall is never a miscompile.)
    // The ONE self-hosted heap-accumulator variant: `Option[List[String]]` acc over
    // `List[String]` elements (is_balanced's paren-stack fold) — `list.fold_ols`, the
    // (heap, heap) -> heap 2-arity CallIndirect (`list_reduce_str`'s proven closure
    // shape). Every other heap-acc fold keeps the unregistered `fold_hacc` wall.
    let is_ols_acc = matches!(result_ty,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1
            && matches!(&a[0], Ty::Applied(TypeConstructorId::List, b)
                if b.len() == 1 && matches!(b[0], Ty::String)));
    let src_is_list_str = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && matches!(e[0], Ty::String));
    if module == "list" && is_ols_acc && src_is_list_str {
        return "list.fold_ols".to_string();
    }
    // The `Map[String, Int]`-accumulator map.fold pair (the map_fold_heap_acc corpus
    // shape): keyed on the EXACT acc type + subject family — `Map[String, String]`
    // (fold_str_msi) / `Map[String, Int]` (fold_skv_msi). Other heap accs keep the wall.
    let is_msi_acc = matches!(result_ty,
        Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2
            && matches!(a[0], Ty::String) && matches!(a[1], Ty::Int));
    // `list.fold` over a Map[String, Int] acc with String elements — the frequencies
    // shape (map_fold_hacc.almd's list_fold_str_msi; same closure ABI as fold_ols).
    if module == "list" && is_msi_acc && src_is_list_str {
        return "list.fold_str_msi".to_string();
    }
    if module == "map" && is_msi_acc {
        if let Some(Ty::Applied(TypeConstructorId::Map, s)) = arg_tys.first() {
            if s.len() == 2 && matches!(s[0], Ty::String) {
                if matches!(s[1], Ty::String) {
                    return "map.fold_str_msi".to_string();
                }
                if matches!(s[1], Ty::Int) {
                    return "map.fold_skv_msi".to_string();
                }
            }
        }
    }
    // A STRING accumulator over a `Map[String, String]` subject (map_fold.almd's
    // `map.fold(ms, "[", (acc, k, v) => acc + k + "=" + v)` builder): all sides are
    // handles, so the all-i32 3-arity CallIndirect twin runs it faithfully — the same
    // interleaved-pair walk as fold_str_msi with a String acc.
    if module == "map" && matches!(result_ty, Ty::String) {
        if let Some(Ty::Applied(TypeConstructorId::Map, s)) = arg_tys.first() {
            if s.len() == 2 && matches!(s[0], Ty::String) && matches!(s[1], Ty::String) {
                return "map.fold_str_sacc".to_string();
            }
        }
    }
    // A RECORD/VARIANT accumulator over a `List[record/variant]` (`list.fold(counters,
    // Counter.empty(), (acc, c) => Counter.merge(acc, c))` — the protocol-merge shape,
    // and the cross-module `Loc` pick fold): every side is a uniform heap HANDLE, so the
    // TYPE-ERASED `list.fold_hrec` (the fold_ols closure ABI) runs it faithfully — acc
    // MOVES into f each step, elements are borrowed, the final acc is the owned result.
    // A Named alias to a scalar fails `is_heap_ty` and keeps the wall.
    let is_named_heap =
        |t: &Ty| matches!(t, Ty::Named(..) | Ty::Record { .. }) && is_heap_ty(t);
    let elem_is_named = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && is_named_heap(&e[0]));
    if module == "list" && elem_is_named && is_named_heap(result_ty) {
        return "list.fold_hrec".to_string();
    }
    // A heap accumulator over SCALAR Int/Bool elements (`list.fold([true, false],
    // tmp, (s, v) => ok(…))` with a Result acc — result_uoe_float): the elements are
    // plain i64 slot copies, the acc a handle, so the (heap, scalar) -> heap
    // 2-arity CallIndirect twin runs it faithfully. Float elements stay walled (an
    // f64 closure param would ABI-mismatch the i64 slot load).
    let elem_is_int_bool = matches!(arg_tys.first(),
        Some(Ty::Applied(TypeConstructorId::List, e)) if e.len() == 1 && matches!(e[0], Ty::Int | Ty::Bool));
    if module == "list" && elem_is_int_bool && is_heap_ty(result_ty) {
        return "list.fold_hsca".to_string();
    }
    format!("{module}.fold_hacc")
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
    // A FLAT scalar block payload — a scalar TUPLE (`result.zip`'s `(Int, Int)`),
    // a List[<scalar>], Bytes, or an `Option[<scalar>]` (the C-149
    // nested-share chain — len-as-tag + one scalar slot, flat rc_dec):
    // the rc-correct flat variant over the cap-as-tag layout (tag @16).
    if a.len() == 2
        && (matches!(&a[0], Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
            || matches!(&a[0], Ty::Applied(TypeConstructorId::List, e)
                if e.len() == 1 && !is_heap_ty(&e[0]))
            || matches!(&a[0], Ty::Applied(TypeConstructorId::Option, e)
                if e.len() == 1 && !is_heap_ty(&e[0]))
            || matches!(a[0], Ty::Bytes))
    {
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
    use almide_lang::types::constructor::TypeConstructorId;
    // `list.shuffle` / `list.choice` ARE `random.shuffle`/`random.choice` under a second
    // stdlib name (the same almide_rt intrinsics) — delegate to the random element-repr
    // router. The Entropy capability stays honest: the witness derives from the LINKED
    // self-host body (prim.random_get), not the call-site module name.
    if matches!(func, "shuffle" | "choice") {
        return Some(random_call_name(func, arg_tys));
    }
    // `list.group_by` — the hval-map builder (scalar elements, String keys). Any other
    // repr routes to the UNLINKED `_x` (a clean render wall, never a wrong-typed link).
    if func == "group_by" {
        let ok = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
                if e.len() == 1 && !is_heap_ty(&e[0]))
            && matches!(result_ty, Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2
                    && matches!(a[0], Ty::String)
                    && matches!(&a[1], Ty::Applied(TypeConstructorId::List, b)
                        if b.len() == 1 && !is_heap_ty(&b[0])));
        return Some(if ok { "list.group_by".to_string() } else { "list.group_by_x".to_string() });
    }
    // `list.zip_with` keys on the RESULT element (= the closure's result repr, the
    // only axis of the CallIndirect table type — params ride the widened i64 slots
    // uniformly): a SCALAR result element rides the base impl (heap SOURCE elements
    // are passed as borrowed handles, never copied into the flat result — sound);
    // a (String, String → String) triple routes to the `_str` twin (move-in fill);
    // any other heap result element routes to the UNLINKED `_x` — the scalar impl's
    // $closure_fn2 table type TRAPS on a heap-result closure ("indirect call type
    // mismatch", fuzz G-65) and its raw copies would alias handles un-owned.
    if func == "zip_with" {
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
        return Some(match (a, b, c) {
            (Some(_), Some(_), Some(z)) if !is_heap_ty(z) => "list.zip_with".to_string(),
            (Some(Ty::String), Some(Ty::String), Some(Ty::String)) => {
                "list.zip_with_str".to_string()
            }
            _ => "list.zip_with_x".to_string(),
        });
    }
    // `list.scan` keys on (element, ACC) reprs — the ACC is the closure's result
    // and the OUTPUT element (one table-type axis, one layout axis): scalar/scalar
    // rides the base impl, scalar/String the `_str` twin (move-in fill, borrow-back
    // threading), anything else the UNLINKED `_x` wall — the scalar impl's i64 init
    // param failed validation on a String acc ("expected i64, found i32", the v1
    // edition of fuzz seed-20260718 index 259).
    if func == "scan" {
        let elem_scalar = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
            if e.len() == 1 && !is_heap_ty(&e[0]));
        return Some(match (elem_scalar, arg_tys.get(1)) {
            (true, Some(a)) if !is_heap_ty(a) => "list.scan".to_string(),
            (true, Some(Ty::String)) => "list.scan_str".to_string(),
            _ => "list.scan_x".to_string(),
        });
    }
    // `list.unique_by` keys on (element, KEY) reprs — the KEY is the closure's
    // result (the CallIndirect table-type axis): scalar/scalar rides the base
    // impl, scalar/String the `_sk` twin (content equality via string.eq), and
    // anything else the UNLINKED `_x` wall — the scalar `(Int) -> Int` table
    // type TRAPPED on a String-key closure ("indirect call type mismatch",
    // fuzz seed-20260718 index 9, the unique_by edition of the zip_with class).
    if func == "unique_by" {
        let elem_scalar = matches!(arg_tys.first(), Some(Ty::Applied(TypeConstructorId::List, e))
            if e.len() == 1 && !is_heap_ty(&e[0]));
        let key_ty = match arg_tys.get(1) {
            Some(Ty::Fn { ret, .. }) => Some(ret.as_ref()),
            _ => None,
        };
        return Some(match (elem_scalar, key_ty) {
            (true, Some(k)) if !is_heap_ty(k) => "list.unique_by".to_string(),
            (true, Some(Ty::String)) => "list.unique_by_sk".to_string(),
            _ => "list.unique_by_x".to_string(),
        });
    }
    None
}

fn list_call_name_source_keyed(func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // `list.enumerate` keys on its SOURCE element: scalar → the flat-pair self-host;
    // String → the rc-share pair variant (`DropListIntStr` at the call site frees each
    // pair's key ref); any other heap element routes to an UNREGISTERED name (walls
    // cleanly — a flat pair drop would leak a rich element's children).
    // `list.drop_end` keys on its SOURCE element: a String element routes to the CO-OWNED
    // rc-copy variant (`__copy_slots_rc`) — the raw slot copy aliases heap handles un-owned,
    // which double-frees under the nested Option[List[String]] drop (is_balanced's fold).
    if func == "drop_end" {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1 && matches!(a[0], Ty::String) {
                return Some("list.drop_end_str".to_string());
            }
        }
    }
    if func == "enumerate" {
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
    }
    // `list.pop` keys on its SOURCE element: a SCALAR (8-byte-slot) element rides the
    // registered in-place self-host (`list_pop.almd` — Int/Bool/Float move bit-exactly);
    // a HEAP element routes to the UNREGISTERED `_x` name and walls at render (popping
    // an owned handle is an ownership transfer the flat impl cannot express).
    if func == "pop" {
        if let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() {
            if a.len() == 1 && !is_heap_ty(&a[0]) {
                return Some("list.pop".to_string());
            }
        }
        return Some("list.pop_x".to_string());
    }
    // `list.zip` keys on BOTH sources: scalar/scalar → flat pairs; FLAT-heap/FLAT-heap
    // (String or List[scalar] each side — matrix rows) → the rc-share variant (the
    // call-site `DropListStrStr` releases both acquired refs); anything else walls.
    if func == "zip" && arg_tys.len() == 2 {
        let elem = |t: &Ty| match t {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(a[0].clone()),
            _ => None,
        };
        if let (Some(ea), Some(eb)) = (elem(&arg_tys[0]), elem(&arg_tys[1])) {
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
            return Some("list.zip_h".to_string());
        }
    }
    // `list.repeat` over a HEAP element (`list.repeat(h, n_rep)` where `h: Matrix` — the
    // nn repeat_kv GQA duplication): each result slot must CO-OWN the element (rc_inc per
    // copy) — the scalar impl's raw alias would make every slot an uncounted owner the
    // recursive result drop double-frees.
    if func == "repeat" {
        if let Some(t) = arg_tys.first() {
            if is_heap_ty(t) {
                return Some("list.repeat_rc".to_string());
            }
        }
    }
    // `list.flatten` over HEAP-element sublists: the copied slots are handles the
    // result must CO-OWN — route to the rc_inc-on-copy variant (the scalar variant's
    // raw copy would make the result a second uncounted owner = a double free).
    if func == "flatten" {
        if let Ty::Applied(TypeConstructorId::List, inner) = result_ty {
            if inner.len() == 1 && is_heap_ty(&inner[0]) {
                return Some("list.flatten_rc".to_string());
            }
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
            let scalar_nf = |t: &Ty| matches!(t, Ty::Int | Ty::Bool);
            let suffix = match &a[0] {
                Ty::Tuple(ts) if ts.len() == 2 && scalar_nf(&ts[0]) && scalar_nf(&ts[1]) => {
                    Some("tss")
                }
                Ty::Tuple(ts)
                    if ts.len() == 2 && scalar_nf(&ts[0]) && matches!(ts[1], Ty::String) =>
                {
                    Some("tsstr")
                }
                Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && scalar_nf(&e[0]) =>
                {
                    Some("lint")
                }
                Ty::Applied(TypeConstructorId::List, e)
                    if e.len() == 1 && matches!(e[0], Ty::String) =>
                {
                    Some("lstr")
                }
                Ty::Applied(TypeConstructorId::Option, o)
                    if o.len() == 1 && scalar_nf(&o[0]) =>
                {
                    Some("oint")
                }
                _ => None,
            };
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
    use almide_lang::types::constructor::TypeConstructorId;
    // `list.map` is the one combinator whose SOURCE and RESULT element reprs may DIFFER (the
    // closure transforms the type). A heap RESULT over a SCALAR source (`float.to_string` over a
    // List[Float], `int.to_string` over a List[Int]) must read the source slot as a raw i64
    // scalar (load64), not as a String handle (load_str) — that is `map_s2h`; a heap result over
    // a heap source is the all-String `map_str`.
    if func == "map" {
        if let Ty::Applied(TypeConstructorId::List, rargs) = result_ty {
            if rargs.len() == 1 && is_heap_ty(&rargs[0]) {
                let src_heap = matches!(
                    arg_tys.first(),
                    Some(Ty::Applied(TypeConstructorId::List, s)) if s.len() == 1 && is_heap_ty(&s[0])
                );
                return Some(if src_heap {
                    "list.map_str".to_string()
                } else {
                    "list.map_s2h".to_string()
                });
            }
        }
    }
    // `list.enumerate` over a List[String] → `list.enumerate_str` (result List[(Int, String)]).
    // Keyed on the SOURCE arg being List[String] (the yaml `lines |> list.enumerate` shape).
    if func == "enumerate" {
        if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
            if s.len() == 1 && matches!(s[0], Ty::String) {
                return Some("list.enumerate_str".to_string());
            }
        }
    }
    // chunk/windows/window (build a NESTED List[List[heap]] whose recursive drop is a separate
    // gap) and the HIGHER-ORDER take_while/drop_while/reduce over a HEAP element (String/Value):
    // the generic i64 self-host impls copy element handles WITHOUT rc_inc → a DOUBLE-FREE at
    // scope end (the result and source both free the shared handle), and the HO closure ABI is
    // i64-scalar so a String/Value i32 handle mismatches the indirect call. Both are memory-
    // safety bugs the prim-region ownership cert cannot see (it treats prim rc as a no-op), so
    // they slipped past corpus-wall and trapped only at runtime. Route to an UNREGISTERED name →
    // render walls cleanly (a controlled reject, never a miscompile or double-free) until the
    // rc-correct heap variants land. Scalar element lists (Int/Float/Bool) are unaffected.
    // take_while/drop_while over a List[String] now have rc-correct _str variants (each kept
    // String DEEP-COPIED so result + source can both drop without a double-free); route to them
    // BEFORE the heap-element wall below. (List[Value] / the other combinators still wall.)
    // `reduce` over a List[String] joins them too: `list.reduce_str` is the self-host tail-`if`
    // (None on empty, else Some) whose loop is the proven closure-call heap-accumulator (#738 Path
    // C). Routed to the rc-correct _str variant BEFORE the heap-element wall.
    if matches!(func, "take_while" | "drop_while" | "chunk" | "windows" | "window" | "reduce") {
        if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
            if s.len() == 1 && matches!(s[0], Ty::String) {
                return Some(format!("list.{func}_str"));
            }
        }
    }
    if matches!(func, "chunk" | "windows" | "window" | "take_while" | "drop_while" | "reduce") {
        if let Some(Ty::Applied(TypeConstructorId::List, s)) = arg_tys.first() {
            if s.len() == 1 && is_heap_ty(&s[0]) {
                return Some(format!("list.{func}_heapelem"));
            }
        }
    }
    None
}