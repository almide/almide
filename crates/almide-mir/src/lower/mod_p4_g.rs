
/// The generated container-repr callee for a `${List[<record>]}` / `${Option[<record>]}` /
/// `${List[<variant>]}` interp part, or `None` for every other type (falls to the
/// `interp_to_string_call` table). Record-vs-variant discrimination mirrors the bare-part
/// arms: a `Named` that resolves in the record registry is a record, else a variant.
///
/// Split out of `mod_p4_b.rs` into its own part file (codopsy8 complexity sweep): the
/// pattern-2 decomposition below grew `mod_p4_b.rs` past the 800-line `max-lines` threshold,
/// so this self-contained family (no other function in `mod_p4_b.rs` calls it besides
/// `interp_part_leaf`, which resolves it through the crate's flat `include!` namespace same
/// as every other cross-part-file call in this module) moved here verbatim.
fn container_repr_name(ty: &Ty, registry: &RecordLayouts) -> Option<String> {
    use almide_lang::types::constructor::TypeConstructorId;
    // Pattern-2 uniform-match split (codopsy8 complexity sweep): the 3 container-kind arms
    // below (List/Option/Map) are independent, self-contained classifications with no
    // shared mutable state — a pure text-move split of the original `match ty { .. }`, no
    // logic change.
    match ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
            container_repr_name_list(&a[0], registry)
        }
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
            container_repr_name_option(&a[0], registry)
        }
        // `${Map[String, <record/variant>]}` — the paired-slot map repr (quoted keys,
        // element repr values, `[:]` when empty).
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String) =>
        {
            let (n, _is_rec) = container_repr_named(&a[1], registry)?;
            let n_fn = crate::lower::drop_fn_ident(&n);
            Some(format!("__repr_map_{n_fn}"))
        }
        _ => None,
    }
}

/// Shared by every arm of [`container_repr_name`]: is `t` a `Named` type, and if so, is it a
/// RECORD (resolves in the registry) or a custom VARIANT (does not)? Extracted from
/// `container_repr_name`'s original `named` closure (codopsy8 complexity sweep). Verbatim.
fn container_repr_named(t: &Ty, registry: &RecordLayouts) -> Option<(String, bool)> {
    let Ty::Named(n, _) = t else { return None };
    Some((n.as_str().to_string(), resolve_aggregate(t, registry).is_some()))
}

/// Extracted from `container_repr_name` (codopsy8 complexity sweep, the `List[<record/
/// variant>]` arm): a generic-variant INSTANTIATION element (`${forest}` over
/// `List[Tree[Int]]` — C-010) routes to the instantiation-KEYED walker; else a bare
/// record/variant element routes to `__repr_list_rec_<R>` / `__repr_list_<V>`. Verbatim.
fn container_repr_name_list(elem: &Ty, registry: &RecordLayouts) -> Option<String> {
    if let Ty::Named(n, args) = elem {
        if !args.is_empty() && resolve_aggregate(elem, registry).is_none() {
            return Some(format!("__repr_list_{}", crate::lower::repr_inst_ident(n.as_str(), args)));
        }
    }
    let (n, is_rec) = container_repr_named(elem, registry)?;
    let n_fn = crate::lower::drop_fn_ident(&n);
    Some(if is_rec { format!("__repr_list_rec_{n_fn}") } else { format!("__repr_list_{n_fn}") })
}

/// Extracted from `container_repr_name` (codopsy8 complexity sweep, the `Option[<record/
/// variant>]` arm): an instantiation payload (`${opt}` over `Option[Tree[String]]`) routes
/// to the instantiation-KEYED walker; a CUSTOM-variant payload (`${opt_tree}` over
/// `Option[Tree]` — C-009) to `__repr_opt_<V>` (Option/Result/Value payloads keep their
/// existing table routing — `None` here); a record payload to `__repr_opt_rec_<R>`. Verbatim.
fn container_repr_name_option(payload: &Ty, registry: &RecordLayouts) -> Option<String> {
    if let Ty::Named(n, args) = payload {
        if !args.is_empty() && resolve_aggregate(payload, registry).is_none() {
            return Some(format!("__repr_opt_{}", crate::lower::repr_inst_ident(n.as_str(), args)));
        }
    }
    let (n, is_rec) = container_repr_named(payload, registry)?;
    let n_fn = crate::lower::drop_fn_ident(&n);
    if !is_rec {
        if matches!(payload, Ty::Named(vn, _) if !matches!(vn.as_str(), "Option" | "Result" | "Value")) {
            return Some(format!("__repr_opt_{n_fn}"));
        }
        return None;
    }
    Some(format!("__repr_opt_rec_{n_fn}"))
}
